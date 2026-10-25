use super::{input, tensors};
use am_sdk_support::{abi::*, Result};
use am_vision_core::sdk::{ImageTransform, TensorData};
pub type TensorSignature = Vec<(String, u32, Vec<i64>)>;

pub enum Plan {
    Image(Box<input::Plan>),
    Tensors(tensors::Plan),
}
impl Plan {
    pub fn cost(&self) -> usize {
        match self {
            Self::Image(p) => p.cost,
            Self::Tensors(p) => p.cost,
        }
    }
    pub fn params(&self) -> am_infer_params_t {
        match self {
            Self::Image(p) => p.params,
            Self::Tensors(p) => p.params,
        }
    }
    pub fn native_extra(&self) -> usize {
        match self {
            Self::Image(_) => 0,
            Self::Tensors(p) => p.native_extra(),
        }
    }
    pub fn signature(&self, name: &str) -> TensorSignature {
        match self {
            Self::Tensors(p) => p.signature(),
            Self::Image(p) => {
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
    pub unsafe fn capture(&self) -> Result<Input> {
        match self {
            Self::Image(p) => Ok(Input::Image(Box::new(unsafe { p.capture() }?))),
            Self::Tensors(p) => Ok(Input::Tensors(unsafe { p.capture() }?)),
        }
    }
}
pub enum Input {
    Image(Box<input::Input>),
    Tensors(tensors::Input),
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
        }
    }
    pub fn prepare(&self, name: &str, budget: usize) -> Result<Prepared<'_>> {
        match self {
            Self::Image(v) => {
                let (tensor, mapping) = v.prepare(name, budget)?;
                Ok(Prepared::Image(tensor, mapping))
            }
            Self::Tensors(v) => Ok(Prepared::Tensors(&v.tensors)),
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
