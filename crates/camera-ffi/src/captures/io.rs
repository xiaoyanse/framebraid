use super::*;
use crate::{frames::Charge, sources::SharedSource};
use am_camera_core::media::sdk_io::{self, IoError};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Condvar,
};
static FRAME_IDS: AtomicU64 = AtomicU64::new(1);
pub struct Raw {
    pub encoded: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub meta: Box<am_frame_meta_t>,
    _charge: Charge,
}
pub struct Flight {
    result: Mutex<Option<Result<Arc<Raw>>>>,
    changed: Condvar,
}
pub fn continuous(
    state: &Arc<state::State>,
    source: &Arc<SharedSource>,
    timeout: Duration,
    interval: u32,
) -> Result<Arc<Raw>> {
    let key = {
        let recent = lock(&state.recent)?;
        let candidate = recent
            .iter()
            .filter(|((id, cadence), _)| *id == source.source_id && *cadence <= interval)
            .min_by_key(|((_, cadence), _)| *cadence);
        if let Some((key, image)) = candidate {
            if monotonic_ms().saturating_sub(image.meta.received_at_ms) < interval as i64
                && image.meta.source_generation == source.generation.load(Ordering::Acquire)
                && image.meta.config_generation == source.config_generation.load(Ordering::Acquire)
            {
                return Ok(image.clone());
            }
            *key
        } else {
            (source.source_id, interval)
        }
    };
    let image = acquire_for(state, source, timeout, Some((key, interval)))?;
    if state.closed.load(Ordering::Acquire) || source.stop.load(Ordering::Acquire) {
        return Err(AM_ERR_SDK_HANDLE_CLOSED);
    }
    let mut recent = lock(&state.recent)?;
    if state.closed.load(Ordering::Acquire) || source.stop.load(Ordering::Acquire) {
        return Err(AM_ERR_SDK_HANDLE_CLOSED);
    }
    if !recent.contains_key(&key) && recent.len() >= 4 * source.state.limits()?.max_sources as usize
    {
        return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
    }
    recent.insert(key, image.clone());
    Ok(image)
}
pub fn error(error: IoError) -> i32 {
    match error {
        IoError::Invalid => AM_ERR_CAMERA_URL_INVALID,
        IoError::Timeout => AM_ERR_CAMERA_TIMEOUT,
        IoError::Auth => AM_ERR_CAMERA_AUTH_FAILED,
        IoError::Network => AM_ERR_CAMERA_UNREACHABLE,
        IoError::Unsupported => AM_ERR_CAMERA_CODEC_UNSUPPORTED,
        IoError::Resource => AM_ERR_SDK_RESOURCE_EXHAUSTED,
        IoError::Closed => AM_ERR_SDK_HANDLE_CLOSED,
    }
}
pub fn acquire(
    state: &Arc<state::State>,
    source: &Arc<SharedSource>,
    timeout: Duration,
) -> Result<Arc<Raw>> {
    acquire_for(state, source, timeout, None)
}
fn acquire_for(
    state: &Arc<state::State>,
    source: &Arc<SharedSource>,
    timeout: Duration,
    freshness: Option<((u64, u32), u32)>,
) -> Result<Arc<Raw>> {
    if !source.isapi {
        return Err(AM_ERR_CAMERA_MODE_CONFLICT);
    }
    if source.stop.load(Ordering::Acquire) || state.closed.load(Ordering::Acquire) {
        return Err(AM_ERR_SDK_HANDLE_CLOSED);
    }
    let (flight, leader) = {
        let mut flights = lock(&state.flights)?;
        if let Some((key, interval)) = freshness {
            if let Some(image) = lock(&state.recent)?.get(&key) {
                if monotonic_ms().saturating_sub(image.meta.received_at_ms) < interval as i64
                    && image.meta.source_generation == source.generation.load(Ordering::Acquire)
                    && image.meta.config_generation
                        == source.config_generation.load(Ordering::Acquire)
                {
                    return Ok(image.clone());
                }
            }
        }
        match flights.get(&source.source_id) {
            Some(existing) => (existing.clone(), false),
            None => {
                let f = Arc::new(Flight {
                    result: Mutex::default(),
                    changed: Condvar::new(),
                });
                flights.insert(source.source_id, f.clone());
                (f, true)
            }
        }
    };
    if leader {
        let result = capture(source, timeout);
        if let (Some((key, _)), Ok(image)) = (freshness, result.as_ref()) {
            let mut recent = lock(&state.recent)?;
            if !state.closed.load(Ordering::Acquire) && !source.stop.load(Ordering::Acquire) {
                recent.insert(key, image.clone());
            }
        }
        *lock(&flight.result)? = Some(result.clone());
        flight.changed.notify_all();
        let mut flights = lock(&state.flights)?;
        if flights
            .get(&source.source_id)
            .is_some_and(|f| Arc::ptr_eq(f, &flight))
        {
            flights.remove(&source.source_id);
        }
        result
    } else {
        let until = Instant::now() + timeout;
        let mut result = lock(&flight.result)?;
        loop {
            if let Some(value) = result.as_ref() {
                return value.clone();
            }
            if source.stop.load(Ordering::Acquire) || state.closed.load(Ordering::Acquire) {
                return Err(AM_ERR_SDK_HANDLE_CLOSED);
            }
            let remaining = until.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(AM_ERR_SDK_DEADLINE_EXCEEDED);
            }
            result = flight
                .changed
                .wait_timeout(result, remaining.min(Duration::from_millis(20)))
                .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?
                .0;
        }
    }
}
fn capture(source: &Arc<SharedSource>, timeout: Duration) -> Result<Arc<Raw>> {
    let charge = source.state.charge(4 * 1024 * 1024, false)?;
    let (secret, credential, generation, configuration) = source.credential_snapshot()?;
    let image = sdk_io::capture(
        &source.url,
        &secret,
        timeout,
        4 * 1024 * 1024,
        16_777_216,
        false,
    )
    .map_err(|e| {
        let code = error(e);
        if code == AM_ERR_CAMERA_AUTH_FAILED {
            source.report(
                AM_SRC_AUTH_WAITING as u32,
                AM_FAULT_AUTH as u32,
                c"camera.authFailed",
            );
        } else if code == AM_ERR_CAMERA_TIMEOUT || code == AM_ERR_CAMERA_UNREACHABLE {
            source.report(
                AM_SRC_RECONNECT_WAIT as u32,
                AM_FAULT_NETWORK as u32,
                c"camera.captureFailed",
            );
        }
        code
    })?;
    if source.stop.load(Ordering::Acquire) {
        return Err(AM_ERR_SDK_HANDLE_CLOSED);
    }
    if generation != source.generation.load(Ordering::Acquire)
        || configuration != source.config_generation.load(Ordering::Acquire)
        || credential != source.credential_version()
    {
        return Err(AM_ERR_SDK_GENERATION_MISMATCH);
    }
    let received = monotonic_ms();
    source.received(received);
    source
        .state
        .metrics
        .newest_received
        .store(received, Ordering::Release);
    source.state.metrics.accepted.fetch_add(1, Ordering::AcqRel);
    let id = FRAME_IDS.fetch_add(1, Ordering::AcqRel);
    let mut meta = Box::<am_frame_meta_t>::default();
    unsafe {
        args::write(
            &mut *meta,
            am_frame_meta_t {
                source_id: source.source_id,
                frame_id: crate::frames::next_frame_id()?,
                seq: id,
                source_generation: generation,
                config_generation: configuration,
                pts: i64::MIN,
                received_at_ms: received,
                captured_at_ms: i64::MIN,
                clock_mask: AM_CLK_RECEIVED,
                clock_quality: AM_CQ_LOCAL_RECEIPT as u32,
                transform: am_spatial_transform_t {
                    scale_x: 1.0,
                    scale_y: 1.0,
                    valid: 1,
                    ..Default::default()
                },
                ..Default::default()
            },
        )?;
    }
    Ok(Arc::new(Raw {
        encoded: image.encoded,
        width: image.width,
        height: image.height,
        meta,
        _charge: charge,
    }))
}
