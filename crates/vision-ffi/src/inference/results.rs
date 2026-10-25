use super::{
    lock,
    models::{execution, ModelHandle},
    state::{ControlCharge, ResultCharge, State},
};
use am_sdk_support::{abi::*, buffers::Buffer, context::Context, Result};
use am_vision_core::{
    profiles::{Output, Prediction},
    sdk::{ImageTransform, TensorData},
};
use std::{
    any::Any,
    ffi::CString,
    sync::{atomic::Ordering, Arc},
};

pub struct ResultBuffer {
    pub value: Box<amv_result_t>,
    frame: Option<Box<am_frame_assoc_t>>,
    transform: Option<Box<am_preprocess_meta_t>>,
    profile: CString,
    raw: Vec<TensorData>,
    _names: Vec<CString>,
    views: Vec<am_tensor_out_t>,
    keypoints: Vec<Vec<am_keypoint_t>>,
    pose: Vec<am_pose_detection_t>,
    detect: Vec<am_detect_box_t>,
    classify: Vec<am_classify_entry_t>,
    segment: Vec<am_segment_instance_t>,
    masks: Vec<Vec<u8>>,
    obb: Vec<am_obb_instance_t>,
    _charge: ResultCharge,
    external: std::sync::Mutex<usize>,
}
// 出口只读，所有内部指针均由同一Arc中的堆缓冲保持。
unsafe impl Send for ResultBuffer {}
unsafe impl Sync for ResultBuffer {}
impl Buffer for ResultBuffer {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
impl ResultBuffer {
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        model: &ModelHandle,
        id: u64,
        frame: Option<am_frame_assoc_t>,
        mapping: Option<ImageTransform>,
        raw: Vec<TensorData>,
        prediction: Option<Prediction>,
        start: i64,
        end: i64,
        charge: ResultCharge,
    ) -> Result<Arc<Self>> {
        let profile = model.info.profile.clone();
        let names = raw
            .iter()
            .map(|r| CString::new(r.info.name.clone()).map_err(|_| AM_ERR_VISION_IO_UNSUPPORTED))
            .collect::<Result<Vec<_>>>()?;
        let views = raw
            .iter()
            .zip(&names)
            .map(|(r, n)| am_tensor_out_t {
                name: n.as_ptr(),
                dtype: r.info.dtype,
                shape: r.info.shape.as_ptr(),
                shape_len: r.info.shape.len() as u32,
                data: r.bytes().as_ptr().cast(),
                data_size: r.bytes().len() as u64,
                is_contiguous: 1,
            })
            .collect::<Vec<_>>();
        let mut data = Self {
            value: Box::default(),
            frame: frame.map(Box::new),
            transform: mapping.map(|mapping| {
                Box::new(am_preprocess_meta_t {
                    src_w: mapping.width,
                    src_h: mapping.height,
                    dst_w: mapping.dst_w,
                    dst_h: mapping.dst_h,
                    scale_x: mapping.scale_x,
                    scale_y: mapping.scale_y,
                    crop_x: mapping.crop_x,
                    crop_y: mapping.crop_y,
                    pad_left: mapping.left,
                    pad_top: mapping.top,
                    valid: 1,
                    ..Default::default()
                })
            }),
            profile,
            raw,
            _names: names,
            views,
            keypoints: Vec::new(),
            pose: Vec::new(),
            detect: Vec::new(),
            classify: Vec::new(),
            segment: Vec::new(),
            masks: Vec::new(),
            obb: Vec::new(),
            _charge: charge,
            external: std::sync::Mutex::new(0),
        };
        let task = match prediction.map(|p| p.output) {
            Some(Output::Pose(v)) => {
                for p in v {
                    let points = p
                        .keypoints
                        .iter()
                        .map(|k| am_keypoint_t {
                            x: k.x,
                            y: k.y,
                            confidence: k.confidence,
                        })
                        .collect::<Vec<_>>();
                    data.pose.push(am_pose_detection_t {
                        bounds: p.bounds,
                        confidence: p.confidence,
                        kpt_count: points.len() as u32,
                        keypoints: points.as_ptr(),
                    });
                    data.keypoints.push(points);
                }
                AM_TASK_POSE
            }
            Some(Output::Detect(v)) => {
                data.detect = v
                    .into_iter()
                    .map(|p| am_detect_box_t {
                        bounds: p.bounds,
                        confidence: p.confidence,
                        class_id: p.class_id as i32,
                        class_name_utf8: std::ptr::null(),
                    })
                    .collect();
                AM_TASK_DETECT
            }
            Some(Output::Classify(v)) => {
                data.classify = v
                    .into_iter()
                    .map(|p| am_classify_entry_t {
                        class_id: p.class_id as i32,
                        class_name_utf8: std::ptr::null(),
                        logit: f32::NAN,
                        probability: p.probability,
                        has_probability: 1,
                    })
                    .collect();
                AM_TASK_CLASSIFY
            }
            Some(Output::Segment(v)) => {
                for p in v {
                    data.segment.push(am_segment_instance_t {
                        bounds: p.detection.bounds,
                        confidence: p.detection.confidence,
                        class_id: p.detection.class_id as i32,
                        mask_width: p.mask_width,
                        mask_height: p.mask_height,
                        mask_stride_bytes: p.mask_stride as u32,
                        mask_bits: p.mask_bits.as_ptr(),
                        roi_map: am_roi_map_t {
                            scale_x: 1.0,
                            scale_y: 1.0,
                            offset_x: 0.0,
                            offset_y: 0.0,
                            valid: 1,
                        },
                    });
                    data.masks.push(p.mask_bits);
                }
                AM_TASK_SEGMENT
            }
            Some(Output::Obb(v)) => {
                data.obb = v
                    .into_iter()
                    .map(|p| am_obb_instance_t {
                        vertices_xy: std::array::from_fn(|i| p.vertices[i / 2][i % 2]),
                        center_x: p.center[0],
                        center_y: p.center[1],
                        width: p.width,
                        height: p.height,
                        angle: p.angle,
                        confidence: p.confidence,
                        class_id: p.class_id as i32,
                        angle_normalized: 1,
                    })
                    .collect();
                AM_TASK_OBB
            }
            None => AM_TASK_GENERIC,
        };
        data.value = Box::new(amv_result_t {
            request_id: id,
            task,
            frame: data.frame.as_deref().map_or(std::ptr::null(), |v| v),
            model: am_model_identity_t {
                model_uid: model.info.uid,
                model_version: 1,
                profile_id: data.profile.as_ptr(),
            },
            execution: execution(model.info.actual, model.info.fallback),
            transform: data.transform.as_deref().map_or(std::ptr::null(), |v| v),
            pose: am_pose_result_t {
                count: data.pose.len() as u32,
                detections: optional_ptr(&data.pose),
            },
            detect: am_detect_result_t {
                count: data.detect.len() as u32,
                boxes: optional_ptr(&data.detect),
            },
            classify: am_classify_result_t {
                count: data.classify.len() as u32,
                entries: optional_ptr(&data.classify),
            },
            segment: am_segment_result_t {
                count: data.segment.len() as u32,
                instances: optional_ptr(&data.segment),
            },
            obb: am_obb_result_t {
                count: data.obb.len() as u32,
                instances: optional_ptr(&data.obb),
            },
            raw_output_count: data.views.len() as u32,
            raw_outputs: data.views.as_ptr(),
            infer_start_ms: start,
            infer_end_ms: end,
            ..Default::default()
        });
        let size = data
            .raw
            .iter()
            .map(|t| {
                t.bytes().len().div_ceil(8) * 8
                    + t.info.name.capacity()
                    + t.info.shape.capacity() * 8
            })
            .sum::<usize>()
            + data
                ._names
                .iter()
                .map(|n| n.as_bytes_with_nul().len())
                .sum::<usize>()
            + data.masks.iter().map(Vec::capacity).sum::<usize>()
            + data
                .keypoints
                .iter()
                .map(|v| v.capacity() * std::mem::size_of::<am_keypoint_t>())
                .sum::<usize>()
            + vector_bytes(&data.raw)
            + vector_bytes(&data._names)
            + vector_bytes(&data.views)
            + vector_bytes(&data.keypoints)
            + vector_bytes(&data.masks)
            + vector_bytes(&data.pose)
            + vector_bytes(&data.detect)
            + vector_bytes(&data.classify)
            + vector_bytes(&data.segment)
            + vector_bytes(&data.obb)
            + data.profile.as_bytes_with_nul().len()
            + std::mem::size_of::<Self>()
            + 1024;
        if size > data._charge.bytes {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        data._charge
            .usage
            .bytes
            .fetch_sub(data._charge.bytes - size, Ordering::AcqRel);
        data._charge.bytes = size;
        // 非匹配出口必须为空指针，不能暴露Vec空缓冲的dangling地址。
        if data.pose.is_empty() {
            data.value.pose.detections = std::ptr::null();
        }
        if data.detect.is_empty() {
            data.value.detect.boxes = std::ptr::null();
        }
        if data.classify.is_empty() {
            data.value.classify.entries = std::ptr::null();
        }
        if data.segment.is_empty() {
            data.value.segment.instances = std::ptr::null();
        }
        if data.obb.is_empty() {
            data.value.obb.instances = std::ptr::null();
        }
        Ok(Arc::new(data))
    }
    pub fn transfer(self: &Arc<Self>) -> Result<*mut amv_result_t> {
        let key = &*self.value as *const _ as usize;
        let mut references = lock(&self.external)?;
        self.acquire_external(*references)?;
        if let Err(e) = crate::domain().buffers.transfer(key, self.clone()) {
            self.undo_external(*references);
            return Err(e);
        }
        *references += 1;
        Ok(key as _)
    }
    fn acquire_external(&self, references: usize) -> Result<()> {
        if references == usize::MAX {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        if references != 0 {
            return Ok(());
        }
        let ctx = self._charge.ctx.upgrade().ok_or(AM_ERR_SDK_HANDLE_CLOSED)?;
        let limits = *lock(&ctx.data::<Context>()?.limits)?;
        let usage = &self._charge.usage;
        if usage.external_items.fetch_add(1, Ordering::AcqRel) >= limits.result_max_items as usize {
            usage.external_items.fetch_sub(1, Ordering::AcqRel);
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        let mut bytes = usage.external_bytes.load(Ordering::Acquire);
        loop {
            let Some(next) = bytes
                .checked_add(self._charge.bytes)
                .filter(|v| *v <= limits.external_hold_max_bytes as usize)
            else {
                usage.external_items.fetch_sub(1, Ordering::AcqRel);
                return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
            };
            match usage.external_bytes.compare_exchange_weak(
                bytes,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(n) => bytes = n,
            }
        }
        Ok(())
    }
    fn undo_external(&self, references: usize) {
        if references == 0 {
            self._charge
                .usage
                .external_items
                .fetch_sub(1, Ordering::AcqRel);
            self._charge
                .usage
                .external_bytes
                .fetch_sub(self._charge.bytes, Ordering::AcqRel);
        }
    }
    pub fn change_reference(&self, retain: bool) -> Result<()> {
        let key = &*self.value as *const _ as usize;
        let mut references = lock(&self.external)?;
        if *references == 0 {
            return Err(AM_ERR_SDK_HANDLE_INVALID);
        }
        if retain {
            self.acquire_external(*references)?;
            crate::domain().buffers.retain(key)?;
            *references += 1;
        } else {
            crate::domain().buffers.release(key)?;
            *references -= 1;
            self.undo_external(*references);
        }
        Ok(())
    }
}
pub struct Snapshot {
    pub value: Box<am_request_snapshot_t>,
    _error: Box<am_error_info_t>,
    _charge: ControlCharge,
}
unsafe impl Send for Snapshot {}
unsafe impl Sync for Snapshot {}
impl Buffer for Snapshot {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
pub fn snapshot(state: &State, id: u64) -> Result<*mut am_request_snapshot_t> {
    let charge = state.control(
        std::mem::size_of::<Snapshot>()
            + std::mem::size_of::<am_request_snapshot_t>()
            + std::mem::size_of::<am_error_info_t>(),
    )?;
    let mut records = lock(&state.records)?;
    state.purge(&mut records);
    let record = records.get(&id).ok_or_else(|| state.unknown(id))?;
    let error = Box::new(am_error_info_t {
        code: record.code,
        message_utf8: am_sdk_support::common::name(record.code),
        source_utf8: c"vision.request".as_ptr(),
        ..Default::default()
    });
    let value = Box::new(am_request_snapshot_t {
        state: record.state,
        terminal: (record.state >= AM_REQ_DONE as u32) as u32,
        error: &*error,
        created_at_ms: record.created,
        finished_at_ms: record.finished,
        ..Default::default()
    });
    let data = Arc::new(Snapshot {
        value,
        _error: error,
        _charge: charge,
    });
    let key = &*data.value as *const _ as usize;
    crate::domain().buffers.insert(key, data)?;
    Ok(key as _)
}

fn optional_ptr<T>(v: &[T]) -> *const T {
    if v.is_empty() {
        std::ptr::null()
    } else {
        v.as_ptr()
    }
}
fn vector_bytes<T>(v: &Vec<T>) -> usize {
    v.capacity() * std::mem::size_of::<T>()
}
