use super::*;
use crate::sources::SharedSource;
use am_camera_core::media::sdk_io::StreamInfo;

#[allow(clippy::too_many_arguments)]
pub fn fanout(
    source: &Arc<SharedSource>,
    subscribers: &[Arc<Subscription>],
    rgba: Vec<u8>,
    w: u32,
    h: u32,
    pts: i64,
    info: &StreamInfo,
    received: i64,
    seq: u64,
    source_generation: u32,
    config_generation: u32,
) -> Result<()> {
    if source.stop.load(Ordering::Acquire)
        || source.generation.load(Ordering::Acquire) != source_generation
    {
        return Ok(());
    }
    let frame_id = crate::frames::next_frame_id()?;
    let now = Instant::now();
    let due: Vec<_> = subscribers.iter().filter(|s| s.due(now)).collect();
    source.received(received);
    if due.is_empty() {
        return Ok(());
    }
    let meta = am_frame_meta_t {
        source_id: source.source_id,
        frame_id,
        seq,
        source_generation,
        config_generation,
        pts,
        time_base_num: info.time_base_num,
        time_base_den: info.time_base_den,
        received_at_ms: received,
        captured_at_ms: i64::MIN,
        clock_mask: AM_CLK_RECEIVED | if pts == i64::MIN { 0 } else { AM_CLK_PTS },
        clock_quality: AM_CQ_LOCAL_RECEIPT as u32,
        transform: am_spatial_transform_t {
            scale_x: 1.,
            scale_y: 1.,
            valid: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut frames: [Option<Arc<Frame>>; 4] = std::array::from_fn(|_| None);
    let mut wanted = [false; 4];
    for sub in &due {
        if let Kind::Frames {
            format, presets, ..
        } = &sub.kind
        {
            wanted[*format as usize] = true;
            debug_assert!(presets.len() as u64 <= SDK_MAX_SCALED_FRAMES);
        }
    }
    for format in 1..4 {
        if !wanted[format] {
            continue;
        }
        let Ok(size) = crate::frames::scale::bytes(w, h, format as u32) else {
            continue;
        };
        let Ok(charge) = source.state.charge(size as u64, true) else {
            continue;
        };
        let data = match format {
            1 => rgba
                .chunks_exact(4)
                .flat_map(|p| [p[0], p[1], p[2]])
                .collect(),
            2 => rgba
                .chunks_exact(4)
                .flat_map(|p| [p[2], p[1], p[0]])
                .collect(),
            _ => match am_camera_core::media::pixel_ops::rgba_to_nv12(&rgba, w, h) {
                Ok(p) => p,
                Err(_) => continue,
            },
        };
        frames[format] = Some(Frame::with_format(data, w, h, format as u32, meta, charge)?);
    }
    if wanted[0] {
        if let Ok(charge) = source.state.charge(rgba.len() as u64, true) {
            frames[0] = Some(Frame::with_format(
                rgba,
                w,
                h,
                AM_PF_RGBA8 as u32,
                meta,
                charge,
            )?);
        }
    }
    for sub in due {
        if let Kind::Frames { format, .. } = sub.kind {
            if let Some(frame) = &frames[format as usize] {
                sub.enqueue(Item::Frame(frame.clone()));
            } else {
                source
                    .state
                    .metrics
                    .frames_dropped
                    .fetch_add(1, Ordering::AcqRel);
                source.event(
                    AM_EVENT_STATE_CHANGED,
                    AM_EVENT_SCOPE_SINGLE_SUB,
                    sub.key.load(Ordering::Acquire),
                    AM_FAULT_RESOURCE as u32,
                    c"camera.outputFrameBudgetExceeded",
                );
            }
        }
    }
    Ok(())
}
