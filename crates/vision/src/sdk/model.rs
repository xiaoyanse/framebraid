use crate::{profiles, Error, Result};
use ort::{
    memory::Allocator,
    session::Session,
    value::{DynTensor, TensorElementType, ValueType},
};
use std::path::Path;

#[derive(Clone, Debug)]
pub struct TensorInfo {
    pub name: String,
    pub dtype: u32,
    pub shape: Vec<i64>,
}

/// 以八字节对齐的拥有者保存五种数值类型，避免跨语言未对齐数组被当作Rust切片。
pub struct TensorData {
    pub info: TensorInfo,
    words: Vec<u64>,
    len: usize,
}
impl TensorData {
    pub fn new(info: TensorInfo, bytes: &[u8]) -> Result<Self> {
        let expected = byte_count(info.dtype, &info.shape)?;
        if expected != bytes.len() {
            return Err(Error("vision.inputBytesMismatch".into()));
        }
        let mut data = Self::zeroed(info)?;
        // 拥有者按u64对齐，复制长度已按shape和dtype核对。
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), data.words.as_mut_ptr().cast(), expected);
        }
        Ok(data)
    }
    pub(super) fn zeroed(info: TensorInfo) -> Result<Self> {
        let len = byte_count(info.dtype, &info.shape)?;
        Ok(Self {
            info,
            words: vec![0; len.div_ceil(8)],
            len,
        })
    }
    pub(super) fn bytes_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.words.as_mut_ptr().cast(), self.len) }
    }
    pub fn bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.words.as_ptr().cast(), self.len) }
    }
    pub fn floats(&self) -> Result<&[f32]> {
        if self.info.dtype != 0 {
            return Err(Error("vision.dtypeMismatch".into()));
        }
        Ok(unsafe { std::slice::from_raw_parts(self.words.as_ptr().cast(), self.len / 4) })
    }
    pub(super) fn floats_mut(&mut self) -> Result<&mut [f32]> {
        if self.info.dtype != 0 {
            return Err(Error("vision.dtypeMismatch".into()));
        }
        if !self.len.is_multiple_of(4) || byte_count(self.info.dtype, &self.info.shape)? != self.len
        {
            return Err(Error("vision.inputBytesMismatch".into()));
        }
        // words独占、全初始化且八字节对齐；只暴露私有有效长度，不访问尾部填充。
        Ok(unsafe { std::slice::from_raw_parts_mut(self.words.as_mut_ptr().cast(), self.len / 4) })
    }
}
pub fn byte_count(dtype: u32, shape: &[i64]) -> Result<usize> {
    let width = match dtype {
        0 | 3 => 4,
        1 => 2,
        2 => 8,
        4 => 1,
        _ => return Err(Error("vision.tensorTypeUnsupported".into())),
    };
    if shape.len() > 8 || shape.iter().any(|d| *d <= 0) {
        return Err(Error("vision.inputShapeMismatch".into()));
    }
    shape
        .iter()
        .try_fold(width, |n: usize, d| n.checked_mul(*d as usize))
        .ok_or_else(|| Error("vision.inputBytesMismatch".into()))
}
/// 接受前只校验小型元数据，不等待正在运行的会话或读取张量内容。
pub fn validate_input_infos(expected: &[TensorInfo], inputs: &[TensorInfo]) -> Result<()> {
    if inputs.len() != expected.len() {
        return Err(Error("vision.inputNameUnknown".into()));
    }
    for spec in expected {
        let mut matches = inputs.iter().filter(|v| v.name == spec.name);
        let actual = matches
            .next()
            .ok_or_else(|| Error("vision.inputNameUnknown".into()))?;
        if matches.next().is_some() {
            return Err(Error("vision.inputNameUnknown".into()));
        }
        if actual.dtype != spec.dtype {
            return Err(Error("vision.dtypeMismatch".into()));
        }
        if actual.shape.len() != spec.shape.len() {
            return Err(Error("vision.inputShapeMismatch".into()));
        }
        for (&a, &e) in actual.shape.iter().zip(&spec.shape) {
            if a <= 0 || (e >= 0 && a != e) {
                return Err(Error("vision.inputShapeMismatch".into()));
            }
            if e < 0 && a > 8192 {
                return Err(Error("vision.dynamicRangeExceeded".into()));
            }
        }
    }
    Ok(())
}
pub(super) fn validate_input_data(expected: &[TensorInfo], inputs: &[TensorData]) -> Result<()> {
    validate_input_infos(
        expected,
        &inputs.iter().map(|v| v.info.clone()).collect::<Vec<_>>(),
    )?;
    for value in inputs {
        // Rust调用方也可改公开元数据；不能按新shape分配后按旧长度复制。
        if byte_count(value.info.dtype, &value.info.shape)? != value.bytes().len() {
            return Err(Error("vision.inputBytesMismatch".into()));
        }
    }
    Ok(())
}
fn element(dtype: u32) -> Result<TensorElementType> {
    match dtype {
        0 => Ok(TensorElementType::Float32),
        1 => Ok(TensorElementType::Float16),
        2 => Ok(TensorElementType::Int64),
        3 => Ok(TensorElementType::Int32),
        4 => Ok(TensorElementType::Uint8),
        _ => Err(Error("vision.tensorTypeUnsupported".into())),
    }
}
fn info(name: &str, value: &ValueType) -> Result<TensorInfo> {
    if name.len() > 4096 || name.contains('\0') {
        return Err(Error("vision.ioUnsupported".into()));
    }
    let ValueType::Tensor { ty, shape, .. } = value else {
        return Err(Error("vision.ioUnsupported".into()));
    };
    let dtype = (0..5)
        .find(|d| element(*d).ok() == Some(*ty))
        .ok_or_else(|| Error("vision.tensorTypeUnsupported".into()))?;
    if shape.len() > 8 {
        return Err(Error("vision.ioUnsupported".into()));
    }
    Ok(TensorInfo {
        name: name.into(),
        dtype,
        shape: shape.to_vec(),
    })
}
#[derive(Clone, Copy)]
pub enum ModelSource<'a> {
    Path(&'a Path),
    Memory(&'a [u8]),
}
pub struct Model {
    session: Session,
    pub inputs: Vec<TensorInfo>,
    pub outputs: Vec<TensorInfo>,
    pub profile: Option<&'static profiles::Profile>,
    pub actual_backend: u32,
    pub fallback: bool,
    pub(super) batch: u32,
    selection: super::selection::BackendSelectionReport,
}
impl Model {
    pub fn load(
        source: ModelSource<'_>,
        profile: Option<&str>,
        backend: u32,
        strict: bool,
        threads: usize,
    ) -> Result<Self> {
        use super::selection::{BackendSelectionPolicy, BackendTarget};
        let mut priority = vec![BackendTarget {
            backend,
            device_id: 0,
        }];
        if backend != 0 && !strict {
            priority.push(BackendTarget {
                backend: 0,
                device_id: 0,
            });
        }
        let policy = BackendSelectionPolicy {
            priority,
            allow_fallback: !strict,
            budget_ms: 1500,
        };
        Self::load_selected_inner(source, profile, &policy, threads, true)
            .map_err(|failure| failure.error)
    }
    pub fn load_selected(
        source: ModelSource<'_>,
        profile: Option<&str>,
        policy: &super::selection::BackendSelectionPolicy,
        threads: usize,
    ) -> super::selection::SelectionResult<Self> {
        Self::load_selected_inner(source, profile, policy, threads, false)
    }
    pub fn selection_report(&self) -> &super::selection::BackendSelectionReport {
        &self.selection
    }
    fn load_selected_inner(
        source: ModelSource<'_>,
        profile: Option<&str>,
        policy: &super::selection::BackendSelectionPolicy,
        threads: usize,
        legacy: bool,
    ) -> super::selection::SelectionResult<Self> {
        use super::selection::{self, BackendSelectionError, BackendSelectionReport};
        policy.validate().map_err(|error| BackendSelectionError {
            error,
            report: Box::new(BackendSelectionReport::default()),
        })?;
        let mut selection = selection::report(policy, legacy);
        let result = (|| -> Result<Self> {
            if threads == 0 {
                return Err(Error("sdk.invalidArgument".into()));
            }
            let p = profile.map(profiles::find).transpose()?;
            let length = match source {
                ModelSource::Path(path) => std::fs::metadata(path)
                    .map_err(|_| Error("vision.modelLoadFailed".into()))?
                    .len(),
                ModelSource::Memory(b) => b.len() as u64,
            };
            if length > 256 * 1024 * 1024 {
                return Err(Error("vision.modelTooLarge".into()));
            }
            let bytes = match source {
                ModelSource::Path(path) => {
                    std::fs::read(path).map_err(|_| Error("vision.modelLoadFailed".into()))?
                }
                ModelSource::Memory(b) => b.to_vec(),
            };
            if bytes.len() > 256 * 1024 * 1024 {
                return Err(Error("vision.modelTooLarge".into()));
            }
            super::onnx::validate_external(
                &bytes,
                match source {
                    ModelSource::Path(path) => Some(path),
                    _ => None,
                },
            )?;
            let session = selection::choose(policy, legacy, &mut selection, |target| {
                super::session::create(source, &bytes, target, threads)
            })?;
            let actual_backend = selection.selected.map_or(0, |target| target.backend);
            let inputs = session
                .inputs()
                .iter()
                .map(|v| info(v.name(), v.dtype()))
                .collect::<Result<Vec<_>>>()?;
            let outputs = session
                .outputs()
                .iter()
                .map(|v| info(v.name(), v.dtype()))
                .collect::<Result<Vec<_>>>()?;
            if inputs.is_empty() || outputs.is_empty() || inputs.len() > 32 || outputs.len() > 32 {
                return Err(Error("vision.ioUnsupported".into()));
            }
            if let Some(profile) = p {
                let matches = |actual: &[TensorInfo], expected: &[profiles::TensorSpec]| {
                    actual.len() == expected.len()
                        && expected.iter().all(|e| {
                            actual.iter().any(|a| {
                                a.name == e.name
                                    && a.dtype == 0
                                    && a.shape
                                        .iter()
                                        .copied()
                                        .eq(e.shape.iter().map(|d| *d as i64))
                            })
                        })
                };
                if !matches(&inputs, std::slice::from_ref(&profile.input))
                    || !matches(&outputs, &profile.outputs)
                {
                    return Err(Error("vision.formatIncompatible".into()));
                }
            }
            let batch = super::batch::from_session(&session, &inputs, &outputs)?;
            selection.model_accepted = true;
            Ok(Self {
                session,
                inputs,
                outputs,
                profile: p,
                actual_backend,
                fallback: selection.attempts.len() > 1,
                batch,
                selection: selection.clone(),
            })
        })();
        result.map_err(|error| BackendSelectionError {
            error,
            report: Box::new(selection),
        })
    }
    pub fn validate_inputs(&self, inputs: &[TensorData]) -> Result<()> {
        validate_input_data(&self.inputs, inputs)?;
        super::batch::validate_count(
            self.batch,
            &inputs.iter().map(|v| v.info.clone()).collect::<Vec<_>>(),
        )
    }
    pub fn run(&mut self, inputs: &[TensorData], output_budget: usize) -> Result<Vec<TensorData>> {
        self.validate_inputs(inputs)?;
        let values = inputs
            .iter()
            .map(|v| {
                let mut tensor = DynTensor::new(
                    &Allocator::default(),
                    element(v.info.dtype)?,
                    v.info.shape.clone(),
                )
                .map_err(|_| Error("vision.inferenceFailed".into()))?;
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        v.bytes().as_ptr(),
                        tensor.data_ptr_mut().cast(),
                        v.bytes().len(),
                    );
                }
                Ok((v.info.name.as_str(), tensor.into_dyn()))
            })
            .collect::<Result<Vec<_>>>()?;
        let result = match self.session.run(values) {
            Ok(result) => {
                self.selection.native_runs = self.selection.native_runs.saturating_add(1);
                self.selection.last_native_run_succeeded = Some(true);
                result
            }
            Err(error) => {
                self.selection.native_failures = self.selection.native_failures.saturating_add(1);
                self.selection.last_native_run_succeeded = Some(false);
                let code: ort::sys::OrtErrorCode = error.code().into();
                self.selection.last_native_error_code = Some(code as i32);
                return Err(Error("vision.inferenceFailed".into()));
            }
        };
        let mut remaining = output_budget;
        let mut output = Vec::new();
        for (name, value) in result.iter() {
            let spec = info(name, value.dtype())?;
            let bytes = byte_count(spec.dtype, &spec.shape)?;
            let allocated = bytes
                .checked_next_multiple_of(8)
                .ok_or_else(|| Error("sdk.resourceExhausted".into()))?;
            remaining = remaining
                .checked_sub(allocated)
                .ok_or_else(|| Error("sdk.resourceExhausted".into()))?;
            let tensor = value
                .downcast_ref::<ort::value::DynTensorValueType>()
                .map_err(|_| Error("vision.ioUnsupported".into()))?;
            if !tensor.memory_info().is_cpu_accessible() {
                return Err(Error("vision.ioUnsupported".into()));
            }
            let raw = unsafe { std::slice::from_raw_parts(tensor.data_ptr().cast(), bytes) };
            output.push(TensorData::new(spec, raw)?);
        }
        if self.batch != 0
            && output
                .iter()
                .any(|v| v.info.shape.first() != inputs[0].info.shape.first())
        {
            return Err(Error("vision.batchUnsupported".into()));
        }
        self.selection.delivered_runs = self.selection.delivered_runs.saturating_add(1);
        super::runtime::executed(self.selection.selected.map_or(0, |target| target.backend));
        Ok(output)
    }
}

#[cfg(test)]
mod tensor_tests {
    use super::*;
    #[test]
    fn numerical_shape_and_byte_contract_precedes_native_copy() {
        let spec = TensorInfo {
            name: "x".into(),
            dtype: 0,
            shape: vec![-1],
        };
        let mut tensor = TensorData::new(
            TensorInfo {
                shape: vec![65],
                ..spec.clone()
            },
            &[0; 260],
        )
        .unwrap();
        assert!(
            validate_input_data(std::slice::from_ref(&spec), std::slice::from_ref(&tensor)).is_ok()
        );
        tensor.info.shape[0] = 1;
        assert_eq!(
            validate_input_data(std::slice::from_ref(&spec), &[tensor])
                .unwrap_err()
                .0,
            "vision.inputBytesMismatch"
        );
        assert_eq!(byte_count(2, &[]).unwrap(), 8);
        assert!(byte_count(0, &[i64::MAX, i64::MAX]).is_err());
        assert!(byte_count(0, &[0]).is_err());
    }
}
