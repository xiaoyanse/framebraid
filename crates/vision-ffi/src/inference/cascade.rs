//! ROI 级联推理（C1，合同 §4.1）FFI 侧：请求解析校验与提交计划组装；
//! ROI 逐项执行编排在 scheduler/execution.rs 的 run_cascade。
use super::{input, models::ModelHandle, submission::Plan};
use am_sdk_support::{abi::*, args, handles::Entry, Result};
use am_vision_core::{
    profiles,
    sdk::{cascade, Preprocess},
};
use std::{
    mem::size_of,
    sync::{atomic::Ordering, Arc},
};

/// 解析后的级联请求（安全内部形态；float 已过有限性与范围校验）。
#[derive(Clone)]
pub struct CascadeSpec {
    pub static_rois: Vec<[f32; 4]>,
    pub rules: Vec<cascade::Rule>,
    /// 生效二级置信度（请求 0=继承时此处已落定为 params.confidence_threshold）。
    pub secondary_confidence: f32,
}

/// 级联执行载荷：随 Job 跨线程；secondary entry 经 domain active 校验后快照。
pub struct CascadeJob {
    pub secondary: Arc<Entry>,
    /// 二级档案前处理（input::plan 对二级校验的产物快照）。
    pub secondary_preprocess: Preprocess,
    /// 二级输入面成本（准入估算用）。
    pub secondary_cost: usize,
    pub spec: CascadeSpec,
}

impl Clone for CascadeJob {
    fn clone(&self) -> Self {
        Self {
            secondary: self.secondary.clone(),
            secondary_preprocess: self.secondary_preprocess,
            secondary_cost: self.secondary_cost,
            spec: self.spec.clone(),
        }
    }
}

/// 解析校验 am_cascade_request_t：计数上限（静态/规则各 ≤16）、count>0 时指针
/// 非空、静态数组逐 4 float 读取且有限、规则逐条 struct_size 协商且
/// expand/min_roi_size 有限非负、secondary_confidence 有限且 0..=1（0=继承）。
/// 违规返回 sdk.invalidArgument。
pub(super) unsafe fn parse(
    request: *const am_cascade_request_t,
    params: &am_infer_params_t,
) -> Result<CascadeSpec> {
    let request = unsafe { args::read(request, size_of::<am_cascade_request_t>()) }?;
    if request.static_roi_count as usize > cascade::MAX_STATIC
        || request.rule_count as usize > cascade::MAX_RULES
    {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    if !request.secondary_confidence.is_finite()
        || !(0.0..=1.0).contains(&request.secondary_confidence)
    {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    let mut static_rois = Vec::with_capacity(request.static_roi_count as usize);
    if request.static_roi_count > 0 {
        if request.static_rois.is_null() {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        let floats = unsafe {
            std::slice::from_raw_parts(request.static_rois, request.static_roi_count as usize * 4)
        };
        for roi in floats.chunks_exact(4) {
            if roi.iter().any(|v| !v.is_finite()) {
                return Err(AM_ERR_SDK_INVALID_ARGUMENT);
            }
            static_rois.push([roi[0], roi[1], roi[2], roi[3]]);
        }
    }
    let mut rules = Vec::with_capacity(request.rule_count as usize);
    if request.rule_count > 0 {
        if request.rules.is_null() {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        for index in 0..request.rule_count as usize {
            let raw =
                unsafe { args::read(request.rules.add(index), size_of::<am_cascade_rule_t>()) }?;
            if !raw.expand.is_finite()
                || raw.expand < 0.0
                || !raw.min_roi_size.is_finite()
                || raw.min_roi_size < 0.0
            {
                return Err(AM_ERR_SDK_INVALID_ARGUMENT);
            }
            rules.push(cascade::Rule {
                source_class_id: raw.source_class_id,
                expand: raw.expand,
                min_roi_size: raw.min_roi_size,
            });
        }
    }
    Ok(CascadeSpec {
        static_rois,
        rules,
        secondary_confidence: if request.secondary_confidence == 0.0 {
            params.confidence_threshold
        } else {
            request.secondary_confidence
        },
    })
}

fn detect_profile(model: &ModelHandle) -> Result<()> {
    if model.info.mode != AM_LOAD_PROFILE {
        return Err(AM_ERR_VISION_PROFILE_REQUIRED);
    }
    let id = model
        .info
        .profile
        .to_str()
        .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
    let profile = profiles::find(id).map_err(super::error)?;
    if profile.task != profiles::Task::Detect {
        return Err(AM_ERR_VISION_PROFILE_UNSUPPORTED);
    }
    Ok(())
}

/// 组装级联提交计划：一级/二级图像面各跑一次 input::plan（只读校验不 capture，
/// 调用两次安全）；两级均须 PROFILE detect 档案且句柄独立；GPU 帧直传无原图
/// 像素访问（ROI 裁剪需要），显式 notSupported。
pub(super) unsafe fn plan(
    primary: &ModelHandle,
    secondary_entry: &Arc<Entry>,
    secondary: &ModelHandle,
    input: *const am_image_input_t,
    cascade_request: *const am_cascade_request_t,
    p: *const am_infer_params_t,
    asynchronous: bool,
) -> Result<Plan> {
    if primary.info.key.load(Ordering::Acquire) == secondary.info.key.load(Ordering::Acquire) {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    detect_profile(primary)?;
    detect_profile(secondary)?;
    let primary_plan = unsafe { input::plan(primary, input, p, asynchronous) }?;
    if primary_plan.gpu.is_some() {
        return Err(AM_ERR_SDK_NOT_SUPPORTED);
    }
    let secondary_plan = unsafe { input::plan(secondary, input, p, asynchronous) }?;
    if secondary_plan.gpu.is_some() {
        return Err(AM_ERR_SDK_NOT_SUPPORTED);
    }
    let spec = unsafe { parse(cascade_request, &primary_plan.params) }?;
    let job = CascadeJob {
        secondary: secondary_entry.clone(),
        secondary_preprocess: secondary_plan.preprocess,
        secondary_cost: secondary_plan.cost,
        spec,
    };
    Ok(Plan::Cascade(Box::new(primary_plan), job))
}
