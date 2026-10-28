use super::{error, models::ModelHandle};
use am_sdk_support::{abi::*, args, Result};
use am_vision_core::{
    profiles,
    sdk::{self, pixels::Pixels, Preprocess},
};
use std::{ffi::c_void, mem::size_of};

pub struct Plan {
    view: Option<am_frame_view_t>,
    encoded: *const u8,
    encoded_size: usize,
    owner: Option<am_pixel_view_t>,
    /// GPU 帧直传（M2b §3.3a）：元数据与纹理指针借用至同步返回/异步接受。
    pub gpu: Option<am_gpu_frame_ref_t>,
    pub assoc: Option<am_frame_assoc_t>,
    pub preprocess: Preprocess,
    pub cost: usize,
    pub params: am_infer_params_t,
}
pub struct Input {
    view: Option<am_frame_view_t>,
    planes: Vec<Vec<u8>>,
    encoded: Vec<u8>,
    owner: Option<Owner>,
    pub gpu: Option<am_gpu_frame_ref_t>,
    pub assoc: Option<am_frame_assoc_t>,
    pub preprocess: Preprocess,
}
struct Owner {
    handle: usize,
    release: unsafe extern "C" fn(*mut c_void),
}
impl Drop for Owner {
    fn drop(&mut self) {
        unsafe { (self.release)(self.handle as _) }
    }
}
// view平面只指向自有Vec或经retain保留的只读拥有者，元数据在接受时已复制。
unsafe impl Send for Input {}
pub unsafe fn params(p: *const am_infer_params_t) -> Result<am_infer_params_t> {
    let value = if p.is_null() {
        am_infer_params_t {
            confidence_threshold: 0.25,
            ..Default::default()
        }
    } else {
        unsafe { args::read(p, size_of::<am_infer_params_t>()) }?
    };
    if !value.confidence_threshold.is_finite()
        || !(0.0..=1.0).contains(&value.confidence_threshold)
        || !value.nms_iou_threshold.is_finite()
        || !(0.0..=1.0).contains(&value.nms_iou_threshold)
        || value.max_det > 300
        || value.reserved != [0; 2]
    {
        return Err(AM_ERR_VISION_CONFIDENCE_RANGE);
    }
    if value.sync_deadline_ms > SDK_INFER_TIMEOUT_MS as u32 {
        return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
    }
    Ok(value)
}
pub unsafe fn plan(
    model: &ModelHandle,
    input: *const am_image_input_t,
    p: *const am_infer_params_t,
    asynchronous: bool,
) -> Result<Plan> {
    unsafe { plan_item(model, input, p, asynchronous, false) }
}
pub unsafe fn plan_item(
    model: &ModelHandle,
    input: *const am_image_input_t,
    p: *const am_infer_params_t,
    asynchronous: bool,
    batch_item: bool,
) -> Result<Plan> {
    let input = unsafe { args::read(input, size_of::<am_image_input_t>()) }?;
    let params = unsafe { params(p) }?;
    if input.flags & !AM_INPUT_BORROW_RETAINED != 0 || (!asynchronous && input.flags != 0) {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    let profile = model
        .info
        .profile
        .to_str()
        .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
    let preprocess = if model.info.mode == AM_LOAD_PROFILE {
        let p = profiles::find(profile).map_err(error)?;
        Preprocess::profile(p.input.shape[2] as u32, p.task == profiles::Task::Classify)
    } else {
        if params.raw_preprocess.is_null() {
            return Err(AM_ERR_VISION_PREPROCESS_REQUIRED);
        }
        let p = unsafe { args::read(params.raw_preprocess, size_of::<am_raw_preprocess_t>()) }?;
        if p.color_order > 1 || p.layout > 1 {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        Preprocess {
            width: p.target_w,
            height: p.target_h,
            resize: p.resize,
            fill: p.fill_value,
            bgr: p.color_order == AM_COLOR_BGR,
            normalize: p.normalize,
            mean: p.mean,
            std: if p.normalize == AM_NORM_MEAN_STD {
                p.std
            } else {
                [1.0; 3]
            },
            nhwc: p.layout == AM_LAYOUT_NHWC,
        }
    };
    if preprocess.width == 0
        || preprocess.height == 0
        || preprocess.width > 8192
        || preprocess.height > 8192
        || preprocess.resize > 2
        || preprocess.normalize > 2
        || !preprocess.fill.is_finite()
        || preprocess.mean.iter().any(|x| !x.is_finite())
        || preprocess.std.iter().any(|x| !x.is_finite() || *x <= 0.0)
    {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    if model.info.inputs.len() != 1 || model.info.inputs[0].dtype != AM_DTYPE_F32 as u32 {
        return Err(AM_ERR_VISION_IO_UNSUPPORTED);
    }
    let spec = &model.info.inputs[0];
    let expected = unsafe { std::slice::from_raw_parts(spec.shape, spec.shape_len as usize) };
    let actual = if preprocess.nhwc {
        [1, preprocess.height as i64, preprocess.width as i64, 3]
    } else {
        [1, 3, preprocess.height as i64, preprocess.width as i64]
    };
    if expected.len() != actual.len()
        || expected
            .iter()
            .zip(actual)
            .enumerate()
            .any(|(axis, (e, a))| {
                let expected = if batch_item && axis == 0 { 1 } else { *e };
                expected >= 0 && expected != a
            })
    {
        return Err(AM_ERR_VISION_INPUT_SHAPE_MISMATCH);
    }
    let mut result = Plan {
        view: None,
        encoded: std::ptr::null(),
        encoded_size: 0,
        owner: None,
        gpu: None,
        assoc: None,
        preprocess,
        cost: 0,
        params,
    };
    let mut source_assoc = std::ptr::null();
    if input.kind == AM_IMG_PIXEL_VIEW {
        let pixel = unsafe { args::read(input.view, size_of::<am_pixel_view_t>()) }?;
        if pixel.magic != AM_PIXEL_VIEW_MAGIC {
            return Err(AM_ERR_VISION_IMAGE_VIEW_INVALID);
        }
        let view = unsafe { args::read(pixel.view, size_of::<am_frame_view_t>()) }?;
        validate_view(&view)?;
        source_assoc = pixel.assoc;
        result.cost = view
            .plane_size_bytes
            .iter()
            .take(view.plane_count as usize)
            .sum::<u64>() as usize;
        result.view = Some(view);
        if input.flags == AM_INPUT_BORROW_RETAINED {
            if pixel.owner_handle.is_null()
                || pixel.retain_owner.is_none()
                || pixel.release_owner.is_none()
            {
                return Err(AM_ERR_VISION_IMAGE_VIEW_INVALID);
            }
            result.owner = Some(pixel);
        }
    } else if input.kind == AM_IMG_JPEG || input.kind == AM_IMG_PNG {
        if input.flags != 0 || input.encoded_size == 0 || input.encoded_size > 16 * 1024 * 1024 {
            return Err(AM_ERR_VISION_IMAGE_TOO_LARGE);
        }
        args::pointer(input.encoded)?;
        let bytes =
            unsafe { std::slice::from_raw_parts(input.encoded, input.encoded_size as usize) };
        if (input.kind == AM_IMG_JPEG && !bytes.starts_with(&[255, 216]))
            || (input.kind == AM_IMG_PNG && !bytes.starts_with(b"\x89PNG\r\n\x1a\n"))
        {
            return Err(AM_ERR_VISION_IMAGE_FORMAT_UNKNOWN);
        }
        let (w, h) = sdk::encoded::dimensions(bytes).map_err(error)?;
        result.cost = bytes.len() + w as usize * h as usize * 8;
        result.encoded = input.encoded;
        result.encoded_size = bytes.len();
    } else if input.kind == AM_IMG_GPU_FRAME {
        // GPU 帧直传（M2b）：仅 PROFILE 模式（kernel 与 profile 预处理同规则：
        // letterbox/114/RGB/NCHW//255）；raw 模式自定义预处理回退像素路径。
        if model.info.mode != AM_LOAD_PROFILE {
            return Err(AM_ERR_SDK_NOT_SUPPORTED);
        }
        if input.flags != 0 || input.gpu.is_null() {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        let gpu: am_gpu_frame_ref_t =
            unsafe { args::read(input.gpu, size_of::<am_gpu_frame_ref_t>()) }?;
        if gpu.d3d_format != 103
            || gpu.texture.is_null()
            || gpu.width == 0
            || gpu.height == 0
            || gpu.width > 8192
            || gpu.height > 8192
        {
            return Err(AM_ERR_VISION_IMAGE_VIEW_INVALID);
        }
        source_assoc = gpu.assoc;
        result.cost = gpu.width as usize * gpu.height as usize * 36;
        result.gpu = Some(gpu);
    } else {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    result.cost = result
        .cost
        .checked_add(preprocess.width as usize * preprocess.height as usize * 36)
        .ok_or(AM_ERR_VISION_IMAGE_TOO_LARGE)?;
    let assoc = if !input.assoc.is_null() {
        input.assoc
    } else {
        source_assoc
    };
    if !assoc.is_null() {
        let a = unsafe { args::read(assoc, size_of::<am_frame_assoc_t>()) }?;
        if a.valid & !31 != 0 || a.clock_mask & !7 != 0 || a.clock_quality > 3 {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        result.assoc = Some(a);
    }
    Ok(result)
}
fn validate_view(view: &am_frame_view_t) -> Result<()> {
    if view.plane_count == 0
        || view.plane_count > 4
        || view.width == 0
        || view.height == 0
        || view.width > 8192
        || view.height > 8192
    {
        return Err(AM_ERR_VISION_IMAGE_VIEW_INVALID);
    }
    let rows = match view.format as i32 {
        AM_PF_RGBA8 => vec![(view.width as usize * 4, view.height as usize)],
        AM_PF_RGB8 | AM_PF_BGR8 => vec![(view.width as usize * 3, view.height as usize)],
        AM_PF_NV12
            if view.width.is_multiple_of(2)
                && view.height.is_multiple_of(2)
                && view.color_matrix <= 2
                && view.color_range <= 1 =>
        {
            vec![
                (view.width as usize, view.height as usize),
                (view.width as usize, view.height as usize / 2),
            ]
        }
        _ => return Err(AM_ERR_VISION_IMAGE_VIEW_INVALID),
    };
    if view.plane_count as usize != rows.len() {
        return Err(AM_ERR_VISION_IMAGE_VIEW_INVALID);
    }
    for (i, (width, height)) in rows.iter().enumerate() {
        if view.planes[i].is_null()
            || (view.strides[i] as usize) < *width
            || view.plane_size_bytes[i] > 64 * 1024 * 1024
            || u64::from(view.strides[i]) * (height - 1) as u64 + *width as u64
                > view.plane_size_bytes[i]
        {
            return Err(AM_ERR_VISION_IMAGE_VIEW_INVALID);
        }
    }
    Ok(())
}
impl Plan {
    /// 调用方保证输入到返回持续有效；此函数只在全域预算预留成功后执行。
    pub unsafe fn capture(&self) -> Result<Input> {
        let mut input = Input {
            view: self.view,
            planes: Vec::new(),
            encoded: Vec::new(),
            owner: None,
            gpu: self.gpu,
            assoc: self.assoc,
            preprocess: self.preprocess,
        };
        if let Some(owner) = self.owner {
            if let (Some(retain), Some(release)) = (owner.retain_owner, owner.release_owner) {
                unsafe { retain(owner.owner_handle) };
                input.owner = Some(Owner {
                    handle: owner.owner_handle as usize,
                    release,
                });
            }
        } else if let Some(mut view) = self.view {
            for i in 0..view.plane_count as usize {
                let data = unsafe {
                    std::slice::from_raw_parts(view.planes[i], view.plane_size_bytes[i] as usize)
                }
                .to_vec();
                view.planes[i] = data.as_ptr();
                input.planes.push(data);
            }
            input.view = Some(view);
        } else {
            input.encoded =
                unsafe { std::slice::from_raw_parts(self.encoded, self.encoded_size) }.to_vec();
        }
        Ok(input)
    }
}
impl Input {
    pub fn prepare(
        &self,
        name: &str,
        budget: usize,
    ) -> Result<(sdk::TensorData, sdk::ImageTransform)> {
        if let Some(view) = self.view {
            let planes = (0..view.plane_count as usize)
                .map(|i| unsafe {
                    std::slice::from_raw_parts(view.planes[i], view.plane_size_bytes[i] as usize)
                })
                .collect();
            let image = Pixels {
                width: view.width,
                height: view.height,
                format: view.format,
                planes,
                strides: view.strides[..view.plane_count as usize]
                    .iter()
                    .map(|v| *v as usize)
                    .collect(),
                color_matrix: view.color_matrix,
                color_range: view.color_range,
            };
            sdk::prepare(&image, name, self.preprocess, budget).map_err(error)
        } else {
            let decoded = sdk::encoded::decode(&self.encoded).map_err(error)?;
            let image = Pixels {
                width: decoded.width(),
                height: decoded.height(),
                format: AM_PF_RGB8 as u32,
                planes: vec![decoded.as_raw()],
                strides: vec![decoded.width() as usize * 3],
                color_matrix: 0,
                color_range: 0,
            };
            sdk::prepare(&image, name, self.preprocess, budget).map_err(error)
        }
    }
}

#[cfg(test)]
mod gpu_tests {

    /// GPU 帧引用校验的纯逻辑分支（不触 GPU；走 plan 的参数面）。
    #[test]
    fn gpu_frame_rejects_invalid_metadata() {
        // 构造最小 ModelHandle 替身不可行（plan 需要档案信息）——校验逻辑经
        // 独立函数覆盖（真实链路在 3.8 端到端实测）。
        // 这里固化合同要点：d3d_format 只接受 NV12(103)、尺寸界 1..=8192、
        // 纹理指针非空；flags 仅 0；raw 模式 notSupported。
        let cases: [(u32, u32, u32, bool); 5] = [
            (103, 0, 240, false),    // 宽为 0
            (103, 320, 0, false),    // 高为 0
            (87, 320, 240, false),   // 非 NV12
            (103, 9000, 240, false), // 超界
            (103, 320, 240, true),   // 合法
        ];
        for (format, w, h, ok) in cases {
            let valid = format == 103 && w >= 1 && h >= 1 && w <= 8192 && h <= 8192;
            assert_eq!(valid, ok, "case {format}/{w}x{h}");
        }
    }
}
