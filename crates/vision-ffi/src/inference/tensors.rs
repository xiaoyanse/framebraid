use super::{error, input, models::ModelHandle};
use am_sdk_support::{abi::*, args, Result};
use am_vision_core::sdk::{self, TensorData, TensorInfo};
use std::mem::size_of;

struct Source {
    info: TensorInfo,
    data: *const u8,
    len: usize,
}
pub struct Plan {
    sources: Vec<Source>,
    pub assoc: Option<am_frame_assoc_t>,
    pub cost: usize,
    pub params: am_infer_params_t,
}
pub struct Input {
    pub tensors: Vec<TensorData>,
    pub assoc: Option<am_frame_assoc_t>,
}

pub unsafe fn plan(
    model: &ModelHandle,
    count: u32,
    tensors: *const am_tensor_in_t,
    params: *const am_infer_params_t,
) -> Result<Plan> {
    unsafe { plan_item(model, count, tensors, params, false) }
}
pub unsafe fn plan_item(
    model: &ModelHandle,
    count: u32,
    tensors: *const am_tensor_in_t,
    params: *const am_infer_params_t,
    batch_item: bool,
) -> Result<Plan> {
    if count == 0 || count > 32 || count as usize != model.info.input_specs.len() {
        return Err(AM_ERR_VISION_INPUT_NAME_UNKNOWN);
    }
    args::pointer(tensors)?;
    let params = unsafe { input::params(params) }?;
    let mut sources = Vec::with_capacity(count as usize);
    let mut assoc = None;
    let mut cost = 0usize;
    for value in unsafe { std::slice::from_raw_parts(tensors, count as usize) } {
        if value.flags != AM_TENSOR_CONTIGUOUS {
            return Err(AM_ERR_VISION_IO_UNSUPPORTED);
        }
        if value.shape_len > 8 {
            return Err(AM_ERR_VISION_INPUT_SHAPE_MISMATCH);
        }
        let shape = if value.shape_len == 0 {
            Vec::new()
        } else {
            args::pointer(value.shape)?;
            unsafe { std::slice::from_raw_parts(value.shape, value.shape_len as usize) }.to_vec()
        };
        let info = TensorInfo {
            name: unsafe { args::string(value.name) }?,
            dtype: value.dtype,
            shape,
        };
        let len = sdk::byte_count(info.dtype, &info.shape).map_err(error)?;
        if value.data_size != len as u64 {
            return Err(AM_ERR_VISION_INPUT_BYTES_MISMATCH);
        }
        args::pointer(value.data.cast::<u8>())?;
        // 自有对齐副本与ORT输入各计一份；准入前不读取或复制大块调用方数据。
        cost = cost
            .checked_add(len.div_ceil(8).saturating_mul(16))
            .and_then(|n| {
                n.checked_add(
                    info.name.capacity() + info.shape.capacity() * 8 + size_of::<Source>(),
                )
            })
            .ok_or(AM_ERR_SDK_QUEUE_FULL)?;
        if !value.assoc.is_null() {
            let incoming = unsafe { args::read(value.assoc, size_of::<am_frame_assoc_t>()) }?;
            if incoming.valid & !31 != 0
                || incoming.clock_mask & !7 != 0
                || incoming.clock_quality > 3
            {
                return Err(AM_ERR_SDK_INVALID_ARGUMENT);
            }
            if assoc
                .as_ref()
                .is_some_and(|old| !same_assoc(old, &incoming))
            {
                return Err(AM_ERR_SDK_INVALID_ARGUMENT);
            }
            assoc = Some(incoming);
        }
        sources.push(Source {
            info,
            data: value.data.cast(),
            len,
        });
    }
    let mut expected = model.info.input_specs.clone();
    if batch_item {
        for spec in &mut expected {
            *spec
                .shape
                .first_mut()
                .ok_or(AM_ERR_VISION_BATCH_UNSUPPORTED)? = 1;
        }
    }
    sdk::validate_input_infos(
        &expected,
        &sources.iter().map(|s| s.info.clone()).collect::<Vec<_>>(),
    )
    .map_err(error)?;
    sdk::validate_batch_count(
        if batch_item {
            0
        } else {
            model.info.native_batch
        },
        &sources.iter().map(|s| s.info.clone()).collect::<Vec<_>>(),
    )
    .map_err(error)?;
    Ok(Plan {
        sources,
        assoc,
        cost,
        params,
    })
}
fn same_assoc(a: &am_frame_assoc_t, b: &am_frame_assoc_t) -> bool {
    a.source_id == b.source_id
        && a.frame_id == b.frame_id
        && a.source_generation == b.source_generation
        && a.config_generation == b.config_generation
        && a.pts == b.pts
        && a.time_base_num == b.time_base_num
        && a.time_base_den == b.time_base_den
        && a.captured_at_ms == b.captured_at_ms
        && a.received_at_ms == b.received_at_ms
        && a.clock_mask == b.clock_mask
        && a.clock_quality == b.clock_quality
        && a.valid == b.valid
}
impl Plan {
    pub fn native_extra(&self) -> usize {
        self.sources.iter().map(|s| s.len.div_ceil(8) * 8).sum()
    }
    pub fn signature(&self) -> Vec<(String, u32, Vec<i64>)> {
        let mut value = self
            .sources
            .iter()
            .map(|s| (s.info.name.clone(), s.info.dtype, s.info.shape.clone()))
            .collect::<Vec<_>>();
        value.sort_by(|a, b| a.0.cmp(&b.0));
        value
    }
    pub unsafe fn capture(&self) -> Result<Input> {
        let tensors = self
            .sources
            .iter()
            .map(|s| {
                let data = unsafe { std::slice::from_raw_parts(s.data, s.len) };
                TensorData::new(s.info.clone(), data).map_err(error)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Input {
            tensors,
            assoc: self.assoc,
        })
    }
}
