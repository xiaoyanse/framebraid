use super::*;
use crate::frames::{camera_state, CameraState, ControlCharge};
use crate::reader::BudgetedReader;
use am_camera_core::{
    credentials::Secret,
    media::sdk_io::{self, IoError},
};
use am_sdk_support::{args, buffers::Buffer, clock, context::Context, Result};
use std::{
    any::{Any, TypeId},
    collections::hash_map::RandomState,
    ffi::CString,
    hash::{BuildHasher, Hash, Hasher},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

const MAX_CANDIDATES: usize = 16;
const CACHE_ITEMS: usize = 64;
const CACHE_BYTES: usize = 1024 * 1024;

#[derive(Clone)]
struct Observation {
    status: i32,
    codec: Option<&'static std::ffi::CStr>,
    width: u32,
    height: u32,
    fps: f64,
    observed: i64,
    valid_until: i64,
    expires: Instant,
}
impl Observation {
    fn failed(status: i32) -> Self {
        let observed = clock::monotonic_ms();
        Self {
            status,
            codec: None,
            width: 0,
            height: 0,
            fps: -1.0,
            observed,
            valid_until: observed.saturating_add(30_000),
            expires: Instant::now() + Duration::from_secs(30),
        }
    }
    fn success(codec: &'static std::ffi::CStr, width: u32, height: u32, fps: f64) -> Self {
        let observed = clock::monotonic_ms();
        Self {
            status: AM_PROBE_OK,
            codec: Some(codec),
            width,
            height,
            fps: if fps.is_finite() && fps > 0.0 {
                fps
            } else {
                -1.0
            },
            observed,
            valid_until: observed.saturating_add(300_000),
            expires: Instant::now() + Duration::from_secs(300),
        }
    }
}
mod planning;
mod storage;
#[cfg(test)]
mod tests;
use storage::Cache;

pub(crate) fn invalidate(ctx: usize) -> Result<()> {
    let state = camera_state(ctx)?;
    let cache = cache(&state)?;
    cache.generation.fetch_add(1, Ordering::AcqRel);
    cache.clear(false)
}
fn cache(state: &CameraState) -> Result<Arc<Cache>> {
    let ctx = state.ctx.data::<Context>()?;
    let mut services = ctx.services.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
    services
        .get_or_insert_with(TypeId::of::<Cache>(), || Arc::new(Cache::new()))?
        .downcast()
        .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)
}

struct ProbeResult {
    abi: Box<am_probe_result_t>,
    _rows: Vec<am_probe_candidate_t>,
    _references: Vec<CString>,
    _charge: ControlCharge,
}
// 指针仅指向本对象拥有的不可变堆数据或静态编码名。
unsafe impl Send for ProbeResult {}
unsafe impl Sync for ProbeResult {}
impl Buffer for ProbeResult {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn probe(state: &Arc<CameraState>, url: &str, secret: &Secret, timeout: Duration) -> Observation {
    let result = if url.starts_with("http://") || url.starts_with("https://") {
        let charge = state
            .charge(4 * 1024 * 1024, false)
            .map_err(|_| IoError::Resource);
        charge.and_then(|_charge| {
            sdk_io::capture(
                url,
                secret,
                timeout,
                4 * 1024 * 1024,
                16 * 1024 * 1024,
                false,
            )
            .map(|image| {
                let codec = if image.encoded.starts_with(&[0x89, b'P', b'N', b'G']) {
                    c"png"
                } else {
                    c"mjpeg"
                };
                Observation::success(codec, image.width, image.height, -1.0)
            })
        })
    } else if url.starts_with("rtsp://") || url.starts_with("rtsps://") {
        match crate::decoder::config::runtime(state) {
            Ok(api) => BudgetedReader::open(
                state.clone(),
                api,
                url,
                secret,
                Arc::new(AtomicBool::new(false)),
                timeout,
            )
            .and_then(|reader| {
                let info = reader.info();
                let codec = match info.codec {
                    AM_CODEC_H264 => c"h264",
                    AM_CODEC_H265 => c"h265",
                    AM_CODEC_MJPEG => c"mjpeg",
                    _ => return Err(IoError::Unsupported),
                };
                Ok(Observation::success(
                    codec,
                    info.width,
                    info.height,
                    info.fps,
                ))
            }),
            Err(_) => Err(IoError::Unsupported),
        }
    } else {
        Err(IoError::Unsupported)
    };
    result.unwrap_or_else(|error| {
        Observation::failed(match error {
            IoError::Timeout => AM_PROBE_TIMEOUT,
            IoError::Auth => AM_PROBE_AUTH_FAILED,
            IoError::Unsupported | IoError::Invalid => AM_PROBE_UNSUPPORTED,
            IoError::Resource | IoError::Closed => AM_PROBE_PENDING,
            IoError::Network => AM_PROBE_UNREACHABLE,
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn amc_probe_streams(
    ctx: *mut amc_context_t,
    req: *const am_probe_request_t,
    out: *mut *mut am_probe_result_t,
) -> i32 {
    status(domain(), ctx as usize, || unsafe {
        am_sdk_support::callbacks::reject_reentry()?;
        args::pointer(out)?;
        out.write(std::ptr::null_mut());
        let _active = domain().active(ctx as usize)?;
        let state = camera_state(ctx as usize)?;
        let plan = planning::Plan::read(req)?;
        let req = plan.request;
        let cache = cache(&state)?;
        let generation = cache.generation.load(Ordering::Acquire);
        let mut charge = planning::reserve(&state, &cache, plan.reservation())?;
        let secret = plan.secret()?;
        let references = plan.references()?;
        let mut rows = planning::exact_vec::<am_probe_candidate_t>(references.len())?;
        let identity = cache.identity(&secret);
        let mut values: [Option<Observation>; MAX_CANDIDATES] = std::array::from_fn(|_| None);
        let mut misses = [0usize; MAX_CANDIDATES];
        let mut miss_count = 0;
        for (i, reference) in references.iter().enumerate() {
            let url = reference.to_str().map_err(|_| AM_ERR_SDK_UTF8_INVALID)?;
            let hit = if req.force_refresh == 0 {
                cache.get(url, identity)?
            } else {
                None
            };
            if hit.is_none() {
                misses[miss_count] = i;
                miss_count += 1;
            }
            values[i] = hit;
        }
        // 全进程至多四个探测工作线程；并发调用排队，不随候选或调用量膨胀线程。
        static RUN: OnceLock<Mutex<()>> = OnceLock::new();
        let _run = RUN
            .get_or_init(Mutex::default)
            .lock()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        let timeout = Duration::from_millis(u64::from(if req.timeout_ms == 0 {
            5000
        } else {
            req.timeout_ms
        }));
        let width = state.metrics.threads.limit()?.min(4) as usize;
        if width == 0 && miss_count != 0 {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        for group in misses[..miss_count].chunks(width.max(1)) {
            let results = std::thread::scope(|scope| {
                let mut threads: [Option<(usize, std::thread::ScopedJoinHandle<'_, Observation>)>;
                    4] = std::array::from_fn(|_| None);
                for (slot, &index) in group.iter().enumerate() {
                    let url = references[index]
                        .to_str()
                        .map_err(|_| AM_ERR_SDK_UTF8_INVALID)?;
                    let state = &state;
                    let secret = &secret;
                    let handle = state
                        .metrics
                        .threads
                        .spawn_scoped(scope, "am-sdk-probe", move || {
                            probe(state, url, secret, timeout)
                        })
                        .map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
                    threads[slot] = Some((index, handle));
                }
                let mut result: [Option<(usize, Observation)>; 4] = std::array::from_fn(|_| None);
                for (slot, (index, handle)) in threads.into_iter().flatten().enumerate() {
                    result[slot] =
                        Some((index, handle.join().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?));
                }
                Ok::<_, i32>(result)
            })?;
            for (index, value) in results.into_iter().flatten() {
                cache.put(
                    &state,
                    generation,
                    references[index]
                        .to_str()
                        .map_err(|_| AM_ERR_SDK_UTF8_INVALID)?,
                    identity,
                    value.clone(),
                )?;
                values[index] = Some(value);
            }
        }
        for (reference, value) in references.iter().zip(values) {
            let value = value.ok_or(AM_ERR_SDK_INTERNAL_ERROR)?;
            rows.push(am_probe_candidate_t {
                struct_size: size_of::<am_probe_candidate_t>() as u32,
                reference_utf8: reference.as_ptr(),
                status: value.status,
                codec_utf8: value.codec.map_or(std::ptr::null(), |codec| codec.as_ptr()),
                width: value.width,
                height: value.height,
                fps: value.fps,
                observed_at_ms: value.observed,
                valid_until_ms: value.valid_until,
            });
        }
        drop(secret);
        charge.shrink_to(plan.result_bytes())?;
        let body = Arc::new(ProbeResult {
            abi: Box::new(am_probe_result_t {
                struct_size: size_of::<am_probe_result_t>() as u32,
                candidate_count: rows.len() as u32,
                candidates: rows.as_ptr(),
            }),
            _rows: rows,
            _references: references,
            _charge: charge,
        });
        let pointer = (&*body.abi as *const am_probe_result_t).cast_mut();
        domain().buffers.insert(pointer as usize, body)?;
        out.write(pointer);
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn amc_probe_result_release(result: *mut am_probe_result_t) -> i32 {
    status(domain(), 0, || {
        let body = domain().buffers.get(result as usize)?;
        body.as_any()
            .downcast_ref::<ProbeResult>()
            .ok_or(AM_ERR_SDK_HANDLE_INVALID)?;
        domain().buffers.release(result as usize)
    })
}
