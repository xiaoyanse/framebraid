use super::*;
use am_vision_core::{
    gpu::{iobinding::InferenceInput, pipeline::Nv12Snapshot},
    sdk::{Model, TensorData},
};

pub(super) fn roi_pixels(roi: [f32; 4], width: u32, height: u32) -> [u32; 4] {
    let x = ((roi[0] * width as f32).floor() as u32).min(width - 1);
    let y = ((roi[1] * height as f32).floor() as u32).min(height - 1);
    let end_x = ((roi[2] * width as f32).ceil() as u32).clamp(x + 1, width);
    let end_y = ((roi[3] * height as f32).ceil() as u32).clamp(y + 1, height);
    [x, y, end_x - x, end_y - y]
}

pub(super) fn should_stop(
    primary: &ModelHandle,
    secondary: Option<&ModelHandle>,
    id: u64,
) -> Result<bool> {
    Ok(primary.stopping.load(Ordering::Acquire)
        || secondary.is_some_and(|model| model.stopping.load(Ordering::Acquire))
        || lock(&primary.state.records)?
            .get(&id)
            .is_none_or(|record| record.canceled))
}

pub(super) fn output_bytes(
    model: &Model,
    vocab: Option<&profiles::VocabLayout>,
    fallback: usize,
) -> usize {
    model
        .profile
        .zip(vocab)
        .and_then(|(profile, layout)| sdk::vocab::affinity_row_bytes(profile, layout))
        .unwrap_or_else(|| outputs_bytes(&model.outputs, fallback))
}

pub(super) fn bind_vocab(binding: &mut InferenceInput, vocab: Option<&TensorData>) -> Result<()> {
    if let Some(vocab) = vocab {
        binding.bind_vocab(vocab).map_err(error)?;
        binding
            .binding
            .synchronize_inputs()
            .map_err(|_| AM_ERR_VISION_INFERENCE_FAILED)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn run_secondary(
    primary: &ModelHandle,
    secondary: Option<&ModelHandle>,
    id: u64,
    snapshot: &Nv12Snapshot,
    session: &mut Model,
    name: &str,
    rois: &[([f32; 4], i32)],
    group: usize,
    size: u32,
    params: am_infer_params_t,
    max_det: usize,
    reserve: usize,
    vocab: Option<&(TensorData, profiles::VocabLayout)>,
    labels: Vec<CString>,
) -> Result<CascadeData> {
    let (w, h) = snapshot.dimensions();
    let mut result = CascadeData {
        boxes: Vec::new(),
        hits: Vec::new(),
        labels,
        skipped: 0,
    };
    let layout = vocab.map(|(_, layout)| layout);
    let raw_bytes = output_bytes(session, layout, reserve);
    for chunk in rois.chunks(group) {
        if should_stop(primary, secondary, id)? {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        let regions: Vec<_> = chunk
            .iter()
            .map(|(roi, _)| roi_pixels(*roi, w, h))
            .collect();
        let run = (|| {
            let (mut binding, maps) = snapshot
                .bind_regions(session.ort_session(), name, size, &regions, true)
                .map_err(|_| AM_ERR_VISION_INFERENCE_FAILED)?;
            let tiled = vocab
                .map(|(tensor, _)| sdk::vocab::replicate_frames(tensor, chunk.len()))
                .transpose()
                .map_err(error)?;
            bind_vocab(&mut binding, tiled.as_ref())?;
            let output = session.run_gpu(&binding, reserve);
            if let Some(handle) = secondary {
                handle.refresh_selection(session);
            }
            let raw = output.map_err(error)?;
            let rows = if chunk.len() == 1 {
                vec![raw]
            } else {
                native_execution::split_batch_rows(raw, chunk.len())?
            };
            Ok::<_, i32>((rows, maps))
        })();
        let Ok((rows, maps)) = run else {
            result.skipped += chunk.len();
            continue;
        };
        if should_stop(primary, secondary, id)? {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        for (((roi, primary_index), row), mapping) in chunk.iter().zip(rows).zip(maps) {
            let Ok(Some(prediction)) = decode_profile(
                session, &row, &mapping, params, max_det, reserve, raw_bytes, layout,
            ) else {
                result.skipped += 1;
                continue;
            };
            let profiles::Output::Detect(boxes) = prediction.output else {
                result.skipped += 1;
                continue;
            };
            for d in boxes {
                result.boxes.push(am_detect_box_t {
                    bounds: cascade::map_box_to_original(d.bounds, *roi),
                    confidence: d.confidence,
                    class_id: d.class_id as i32,
                    class_name_utf8: result
                        .labels
                        .get(d.class_id as usize)
                        .map_or(std::ptr::null(), |label| label.as_ptr()),
                });
                result.hits.push(am_cascade_hit_t {
                    primary_index: *primary_index,
                    roi: *roi,
                });
            }
        }
    }
    Ok(result)
}
