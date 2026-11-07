use super::{cascade, input, models::ModelHandle, tensors};
use am_sdk_support::{abi::*, Result};
use am_vision_core::sdk::{cascade::MAX_ROIS, ImageTransform, TensorData};
pub type TensorSignature = Vec<(String, u32, Vec<i64>)>;

pub enum Plan {
    Image(Box<input::Plan>),
    Tensors(tensors::Plan),
    /// ROI 级联（C1 §4.1）：一级图像计划 + 二级执行载荷；图像只 capture 一次
    /// （BORROW_RETAINED 语义沿用一级计划）。
    Cascade(Box<input::Plan>, cascade::CascadeJob),
}
impl Plan {
    pub fn cost(&self) -> usize {
        match self {
            Self::Image(p) => p.cost,
            Self::Tensors(p) => p.cost,
            // 一级 + 二级输入面 + 二级 preprocess 尺寸×ROI 上限粗估（逐 ROI
            // 顺序复用同一份中转缓冲，估算不叠满 16 份张量）。
            Self::Cascade(p, job) => p.cost.saturating_add(job.secondary_cost).saturating_add(
                job.secondary_preprocess.width as usize
                    * job.secondary_preprocess.height as usize
                    * MAX_ROIS,
            ),
        }
    }
    pub fn params(&self) -> am_infer_params_t {
        match self {
            Self::Image(p) => p.params,
            Self::Tensors(p) => p.params,
            Self::Cascade(p, _) => p.params,
        }
    }
    /// 批 E 分期计费（仅攒批窗口挂起项使用）：挂起期入账 capture 实驻留，
    /// 执行期补账组批峰值份额（合并缓冲+ORT 输入副本+编码解码面；GPU 帧
    /// 设备侧组装不落 CPU 账）。两项之和即执行期同源核心组批预算基线；
    /// 非攒批路径仍按 cost() 一次性全额入账。
    pub fn stage(&self) -> (usize, usize) {
        match self {
            Self::Image(p) => (p.pending, p.exec_charge),
            Self::Tensors(p) => (p.pending, p.exec_charge),
            // 级联不进攒批窗口（scheduler::submit 已按计划类型关断）；此臂
            // 保持一次性全额语义，不分期。
            Self::Cascade(..) => (self.cost(), 0),
        }
    }
    pub fn native_extra(&self) -> usize {
        match self {
            Self::Image(_) | Self::Cascade(..) => 0,
            Self::Tensors(p) => p.native_extra(),
        }
    }
    pub fn signature(&self, name: &str) -> TensorSignature {
        match self {
            Self::Tensors(p) => p.signature(),
            // 级联签名取一级前处理（攒批窗口已按计划类型关断，此臂仅为穷尽）。
            Self::Image(p) | Self::Cascade(p, _) => {
                let v = p.preprocess;
                let shape = if v.nhwc {
                    vec![1, v.height as i64, v.width as i64, 3]
                } else {
                    vec![1, 3, v.height as i64, v.width as i64]
                };
                vec![(name.into(), 0, shape)]
            }
        }
    }
    pub unsafe fn capture(&self, model: &ModelHandle) -> Result<Input> {
        match self {
            Self::Image(p) => Ok(Input::Image(Box::new(unsafe { p.capture(model) }?))),
            Self::Tensors(p) => Ok(Input::Tensors(unsafe { p.capture() }?)),
            Self::Cascade(p, job) => Ok(Input::Cascade(
                Box::new(unsafe { p.capture(model) }?),
                job.clone(),
            )),
        }
    }
}
pub enum Input {
    Image(Box<input::Input>),
    Tensors(tensors::Input),
    Cascade(Box<input::Input>, cascade::CascadeJob),
}
pub enum Prepared<'a> {
    Image(TensorData, ImageTransform),
    Tensors(&'a [TensorData]),
}
impl Input {
    pub fn assoc(&self) -> Option<am_frame_assoc_t> {
        match self {
            Self::Image(v) => v.assoc,
            Self::Tensors(v) => v.assoc,
            Self::Cascade(v, _) => v.assoc,
        }
    }
    pub fn prepare(&self, name: &str, budget: usize) -> Result<Prepared<'_>> {
        match self {
            Self::Image(v) => {
                let (tensor, mapping) = v.prepare(name, budget)?;
                Ok(Prepared::Image(tensor, mapping))
            }
            Self::Tensors(v) => Ok(Prepared::Tensors(&v.tensors)),
            // 级联在 run() 入口即分派到 run_cascade（自带一级 prepare），不经此处。
            Self::Cascade(..) => Err(AM_ERR_SDK_INTERNAL_ERROR),
        }
    }
}
impl Prepared<'_> {
    pub fn tensors(&self) -> &[TensorData] {
        match self {
            Self::Image(t, _) => std::slice::from_ref(t),
            Self::Tensors(t) => t,
        }
    }
    pub fn transform(&self) -> Option<ImageTransform> {
        match self {
            Self::Image(_, m) => Some(*m),
            Self::Tensors(_) => None,
        }
    }
}
