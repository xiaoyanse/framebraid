use super::{
    lock,
    models::ModelHandle,
    state::{ControlCharge, DeviceCharge, ResultCharge, State},
};
use am_sdk_support::{abi::*, buffers::Buffer, context::Context, Result};
use am_vision_core::{
    profiles::{Output, Prediction},
    sdk::{DeviceTensor, ImageTransform, TensorData},
};
use std::{
    any::Any,
    ffi::CString,
    sync::{atomic::Ordering, Arc},
};

/// 驻留形态交付载荷（§3.6a ABI 1.9）：设备张量句柄+筛选计数元数据+显存
/// 账目（随 ResultBuffer 生命周期出账）。task 为档案任务（RAW=GENERIC）；
/// 五类出口不解码（实例指针恒 NULL），count 仅作交付元数据。
pub struct ResidentPayload {
    pub tensors: Vec<DeviceTensor>,
    pub detected_count: u32,
    pub task: u32,
    /// 显存账目（仅持有不读取：Drop 即出账——缓存 TTL、ack 出引用、外部
    /// release 三条释放路径全部收敛到 ResultBuffer 析构）。
    #[allow(dead_code)]
    pub charge: DeviceCharge,
}

pub struct ResultBuffer {
    pub value: Box<amv_result_t>,
    frame: Option<Box<am_frame_assoc_t>>,
    transform: Option<Box<am_preprocess_meta_t>>,
    profile: CString,
    device: CString,
    fallback_reason: Option<CString>,
    resident: Option<ResidentPayload>,
    raw: Vec<TensorData>,
    _names: Vec<CString>,
    views: Vec<am_tensor_out_t>,
    keypoints: Vec<Vec<am_keypoint_t>>,
    pose: Vec<am_pose_detection_t>,
    detect: Vec<am_detect_box_t>,
    vocab_hits: Vec<am_vocab_hit_t>,
    hit_map: Option<Box<am_vocab_hit_map_t>>,
    vocab_labels: Vec<CString>,
    classify: Vec<am_classify_entry_t>,
    segment: Vec<am_segment_instance_t>,
    masks: Vec<Vec<u8>>,
    obb: Vec<am_obb_instance_t>,
    cascade: Option<Box<am_cascade_result_t>>,
    cascade_boxes: Vec<am_detect_box_t>,
    cascade_hits: Vec<am_cascade_hit_t>,
    cascade_labels: Vec<CString>,
    _charge: ResultCharge,
    external: std::sync::Mutex<usize>,
}

/// 级联二级输出（C1，§4.1）：二级框（原图归一化 xyxy，class 语义=二级模型）+
/// 同序来源明细；labels 为 vocab 二级模型的条目显示名（非 vocab 为空）。
pub struct CascadeData {
    pub boxes: Vec<am_detect_box_t>,
    pub hits: Vec<am_cascade_hit_t>,
    pub labels: Vec<CString>,
    /// 被跳过的 ROI 数（可观测性三态：>0=有 ROI 未执行，句柄关闭=全跳过）。
    pub skipped: usize,
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
        cascade: Option<CascadeData>,
        start: i64,
        end: i64,
        charge: ResultCharge,
    ) -> Result<Arc<Self>> {
        Self::build_inner(
            model, id, frame, mapping, raw, prediction, cascade, None, start, end, charge,
        )
    }

    /// 驻留形态组装（§3.6a）：raw_outputs 元数据有效（name/dtype/shape/
    /// data_size）但 data=NULL；device_outputs_token=request_id（会话内唯一
    /// 非零）；五类出口不解码——启用 device_filter 时 detect.count 回填
    /// 筛选候选数（NMS 前，仅观测），未启用恒 0（占位）；实例数组指针恒 NULL。
    #[allow(clippy::too_many_arguments)]
    pub fn build_resident(
        model: &ModelHandle,
        id: u64,
        frame: Option<am_frame_assoc_t>,
        mapping: Option<ImageTransform>,
        resident: ResidentPayload,
        start: i64,
        end: i64,
        charge: ResultCharge,
    ) -> Result<Arc<Self>> {
        Self::build_inner(
            model,
            id,
            frame,
            mapping,
            Vec::new(),
            None,
            None,
            Some(resident),
            start,
            end,
            charge,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn build_inner(
        model: &ModelHandle,
        id: u64,
        frame: Option<am_frame_assoc_t>,
        mapping: Option<ImageTransform>,
        raw: Vec<TensorData>,
        prediction: Option<Prediction>,
        cascade: Option<CascadeData>,
        resident: Option<ResidentPayload>,
        start: i64,
        end: i64,
        charge: ResultCharge,
    ) -> Result<Arc<Self>> {
        let profile = model.info.profile.clone();
        let names = raw
            .iter()
            .map(|r| r.info.name.clone())
            .chain(
                resident
                    .iter()
                    .flat_map(|r| r.tensors.iter())
                    .map(|t| t.info.name.clone()),
            )
            .map(|name| CString::new(name).map_err(|_| AM_ERR_VISION_IO_UNSUPPORTED))
            .collect::<Result<Vec<_>>>()?;
        // 驻留张量视图（§3.6a）：元数据有效、data=NULL——形状可供调用方
        // 决定 readout 时机，字节消费走 amv_result_device_readout。
        let device_sizes = resident
            .iter()
            .flat_map(|r| r.tensors.iter())
            .map(|t| t.byte_len().map_err(|_| AM_ERR_VISION_IO_UNSUPPORTED))
            .collect::<Result<Vec<_>>>()?;
        let device_views = resident
            .iter()
            .flat_map(|r| r.tensors.iter())
            .zip(&names[raw.len()..])
            .zip(&device_sizes)
            .map(|((t, n), size)| am_tensor_out_t {
                name: n.as_ptr(),
                dtype: t.info.dtype,
                shape: t.info.shape.as_ptr(),
                shape_len: t.info.shape.len() as u32,
                data: std::ptr::null(),
                data_size: *size as u64,
                is_contiguous: 1,
            })
            .collect::<Vec<_>>();
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
            .chain(device_views)
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
            device: model.info.device.clone(),
            fallback_reason: model.info.fallback_reason.clone(),
            resident,
            raw,
            _names: names,
            views,
            keypoints: Vec::new(),
            pose: Vec::new(),
            detect: Vec::new(),
            vocab_hits: Vec::new(),
            hit_map: None,
            vocab_labels: Vec::new(),
            classify: Vec::new(),
            segment: Vec::new(),
            masks: Vec::new(),
            obb: Vec::new(),
            cascade: None,
            cascade_boxes: Vec::new(),
            cascade_hits: Vec::new(),
            cascade_labels: Vec::new(),
            _charge: charge,
            external: std::sync::Mutex::new(0),
        };
        // 级联数据先落自有缓冲再取指针（Vec 随后不再增长，指针稳定）。
        if let Some(c) = cascade {
            data.cascade_boxes = c.boxes;
            data.cascade_hits = c.hits;
            data.cascade_labels = c.labels;
            data.cascade = Some(Box::new(am_cascade_result_t {
                struct_size: size_of::<am_cascade_result_t>() as u32,
                secondary_count: data.cascade_boxes.len() as u32,
                secondary: data.cascade_boxes.as_ptr(),
                hits: data.cascade_hits.as_ptr(),
                skipped_count: c.skipped as u32,
            }));
        }
        let mut prediction = prediction;
        // 先取 vocab 伴生数据（owned），output 随后按值分发——两者独立不互吞。
        let vocab = prediction.as_mut().and_then(|p| p.vocab_hits.take());
        let task = match prediction.take().map(|p| p.output) {
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
                // A2/A4：vocab 路径 class_name=条目 label（A4 显示名分离），非 vocab null。
                if let Some(vocab) = vocab.as_ref() {
                    data.vocab_labels = vocab
                        .labels
                        .iter()
                        .map(|l| CString::new(l.as_str()))
                        .collect::<std::result::Result<Vec<_>, _>>()
                        .map_err(|_| AM_ERR_VISION_IO_UNSUPPORTED)?;
                    data.vocab_hits = vocab
                        .hits
                        .iter()
                        .map(|h| am_vocab_hit_t {
                            entry_index: h.entry_index as i32,
                            prompt_index: h.prompt_index as i32,
                        })
                        .collect();
                    data.detect = v
                        .iter()
                        .map(|p| am_detect_box_t {
                            bounds: p.bounds,
                            confidence: p.confidence,
                            class_id: p.class_id as i32,
                            class_name_utf8: data.vocab_labels[p.class_id as usize].as_ptr(),
                        })
                        .collect();
                    let mut map = Box::new(am_vocab_hit_map_t {
                        struct_size: size_of::<am_vocab_hit_map_t>() as u32,
                        count: data.vocab_hits.len() as u32,
                        hits: data.vocab_hits.as_ptr(),
                    });
                    // 指针在填 value.vocab_hits 前定型（Vec 后续不再增长）。
                    map.hits = data.vocab_hits.as_ptr();
                    data.hit_map = Some(map);
                } else {
                    data.detect = v
                        .into_iter()
                        .map(|p| am_detect_box_t {
                            bounds: p.bounds,
                            confidence: p.confidence,
                            class_id: p.class_id as i32,
                            class_name_utf8: std::ptr::null(),
                        })
                        .collect();
                }
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
            None => data
                .resident
                .as_ref()
                .map_or(AM_TASK_GENERIC, |r| r.task as am_task_e),
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
            execution: am_backend_actual_t {
                device_utf8: data.device.as_ptr(),
                fallback_reason_utf8: data
                    .fallback_reason
                    .as_ref()
                    .map_or(std::ptr::null(), |s| s.as_ptr()),
                ..model.info.execution()
            },
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
            vocab_hits: data.hit_map.as_deref().map_or(std::ptr::null(), |v| v),
            cascade: data.cascade.as_deref().map_or(std::ptr::null(), |v| v),
            ..Default::default()
        });
        // 驻留标志与令牌（§3.6a）：token=request_id（会话内唯一非零，随
        // release 失效）；筛选计数元数据回填 detect 出口（其余任务恒 0），
        // 实例指针保持 NULL——驻留形态不执行实例级解码。
        if let Some(resident) = data.resident.as_ref() {
            data.value.device_resident = 1;
            data.value.device_outputs_token = id;
            if resident.task == AM_TASK_DETECT as u32 {
                data.value.detect.count = resident.detected_count;
            }
        }
        let size = data
            .raw
            .iter()
            .map(|t| {
                t.bytes().len().div_ceil(8) * 8
                    + t.info.name.capacity()
                    + t.info.shape.capacity() * 8
            })
            .sum::<usize>()
            // 驻留元数据开销（name/shape 容量；设备字节走独立 device 账，
            // 不落 host 结果账——§3.6a 独立账本口径）。
            + data
                .resident
                .iter()
                .flat_map(|r| r.tensors.iter())
                .map(|t| t.info.name.capacity() + t.info.shape.capacity() * 8 + 8)
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
            + vector_bytes(&data.cascade_boxes)
            + vector_bytes(&data.cascade_hits)
            + vector_bytes(&data.cascade_labels)
            + data
                .cascade_labels
                .iter()
                .map(|l| l.as_bytes_with_nul().len())
                .sum::<usize>()
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
        // 级联空数组同样不得暴露 dangling 指针（结果结构保留，count=0）。
        if let Some(cascade) = data.cascade.as_deref_mut() {
            if data.cascade_boxes.is_empty() {
                cascade.secondary = std::ptr::null();
            }
            if data.cascade_hits.is_empty() {
                cascade.hits = std::ptr::null();
            }
        }
        Ok(Arc::new(data))
    }
    /// 驻留 readout 访问（§3.6a amv_result_device_readout 内核）：按
    /// raw_outputs 下标取驻留张量同步 DtoH 拷贝；非驻留结果/下标越界为
    /// 参数错误（合同口径），字节量返回供容量协议回填。多次调用幂等。
    pub fn device_readout(&self, index: usize, out: &mut [u8]) -> Result<usize> {
        let Some(resident) = self.resident.as_ref() else {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        };
        let Some(tensor) = resident.tensors.get(index) else {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        };
        tensor.readout(out).map_err(super::error)
    }
    /// 驻留张量元数据字节数（readout 容量协议的所需值；非驻留/越界拒绝）。
    pub fn device_readout_len(&self, index: usize) -> Result<usize> {
        let Some(resident) = self.resident.as_ref() else {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        };
        let Some(tensor) = resident.tensors.get(index) else {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        };
        tensor.byte_len().map_err(super::error)
    }
    /// 归属 ctx Entry（readout 会话校验：与传入 ctx 的 Entry 做 Arc 同一性
    /// 比较——注册 key 是 token Box 指针而非 Entry 地址，不能按数值比对；
    /// ctx 已销毁返回 None 拒绝同码）。
    pub fn owner_entry(&self) -> Option<std::sync::Arc<am_sdk_support::handles::Entry>> {
        self._charge.ctx.upgrade()
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
