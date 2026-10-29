//! OpenVINO 原生推理引擎：内存加载 → CPU 编译 → 同步推理；候选失败回退 ORT CPU。
#![allow(non_snake_case)]
use super::ov_sys::{
    self, ov_compiled_model_t, ov_core_t, ov_element_type_e, ov_infer_request_t, ov_model_t,
    ov_output_const_port_t, ov_status_e, ov_tensor_t, OvApi, OV_OK, OV_TYPE_F16, OV_TYPE_F32,
    OV_TYPE_I32, OV_TYPE_I64, OV_TYPE_U8,
};
use crate::sdk::{TensorData, TensorInfo};
use crate::{Error, Result};
use std::ffi::{c_char, c_void, CStr};
use std::ptr::null_mut;
use std::sync::Arc;

/// 候选失败：错误键须精确 "vision.backendStrictUnavailable"（selection 回退按精确 key 匹配）。
#[derive(Debug)]
pub(crate) struct CreateFailure {
    pub error: Error,
    pub stage: &'static str,
    pub native_code: Option<i32>,
}

/// 推理失败分层：Some=原生 infer 失败（selection 记码），None=交付类失败（镜像 ORT 语义）。
#[derive(Debug)]
pub(crate) struct OvInferError {
    pub native_ok: bool,
    pub native_code: Option<i32>,
    pub error: Error,
}

/// 持有 core→model→compiled→request→tensor 全链；字段顺序即释放顺序。
pub(crate) struct OpenvinoEngine {
    request: *mut ov_infer_request_t,
    input_tensor: *mut ov_tensor_t,
    compiled: *mut ov_compiled_model_t,
    model: *mut ov_model_t,
    core: *mut ov_core_t,
    api: Arc<OvApi>,
    pub(crate) inputs: Vec<TensorInfo>,
    pub(crate) outputs: Vec<TensorInfo>,
}

impl Drop for OpenvinoEngine {
    fn drop(&mut self) {
        unsafe {
            if !self.request.is_null() {
                (self.api.infer_request_free)(self.request);
            }
            if !self.input_tensor.is_null() {
                (self.api.tensor_free)(self.input_tensor);
            }
            if !self.compiled.is_null() {
                (self.api.compiled_model_free)(self.compiled);
            }
            if !self.model.is_null() {
                (self.api.model_free)(self.model);
            }
            if !self.core.is_null() {
                (self.api.core_free)(self.core);
            }
        }
    }
}

/// 句柄为独占所有（Box 语义），OpenVINO 运行时对象无线程亲和，跨线程移动与 Drop 安全；
/// 并发使用仍由 ModelHandle 的 Mutex 串行化（同 ORT Session 的 Send 语义）。
unsafe impl Send for OpenvinoEngine {}

/// ov 元素类型 → TensorData dtype 合同值（sdk/model.rs byte_count 同套编码）。
fn map_element_type(element: ov_element_type_e) -> Result<u32> {
    match element {
        OV_TYPE_F32 => Ok(0),
        OV_TYPE_F16 => Ok(1),
        OV_TYPE_I64 => Ok(2),
        OV_TYPE_I32 => Ok(3),
        OV_TYPE_U8 => Ok(4),
        _ => Err(Error("vision.tensorTypeUnsupported".into())),
    }
}

/// TensorData dtype → ov 元素类型（输入张量创建用，map_element_type 的逆映射）。
fn element_type_of(dtype: u32) -> Result<ov_element_type_e> {
    match dtype {
        0 => Ok(OV_TYPE_F32),
        1 => Ok(OV_TYPE_F16),
        2 => Ok(OV_TYPE_I64),
        3 => Ok(OV_TYPE_I32),
        4 => Ok(OV_TYPE_U8),
        _ => Err(Error("vision.tensorTypeUnsupported".into())),
    }
}

/// 端口规格校验：拒绝空/超长/含非正维（动态 shape 首版不支持，走 ORT 候选）。
fn tensor_spec(name: &str, dims: &[i64], element: ov_element_type_e) -> Result<TensorInfo> {
    if dims.is_empty() || dims.len() > 8 || dims.iter().any(|d| *d <= 0) {
        return Err(Error("vision.ioUnsupported".into()));
    }
    Ok(TensorInfo {
        name: name.into(),
        dtype: map_element_type(element)?,
        shape: dims.to_vec(),
    })
}

fn fail(stage: &'static str) -> CreateFailure {
    CreateFailure {
        error: Error("vision.backendStrictUnavailable".into()),
        stage,
        native_code: None,
    }
}

/// ov 状态码守卫：非零转候选失败并保留码值；逻辑拒绝（数量/规格不符）native_code 为 None。
fn ov_try(status: ov_status_e, stage: &'static str) -> std::result::Result<(), CreateFailure> {
    if status == OV_OK {
        Ok(())
    } else {
        Err(CreateFailure {
            error: Error("vision.backendStrictUnavailable".into()),
            stage,
            native_code: Some(status),
        })
    }
}

/// 编译并装配引擎；候选失败统一 backendStrictUnavailable 触发 selection 回退。
pub(crate) fn create(bytes: &[u8]) -> std::result::Result<OpenvinoEngine, CreateFailure> {
    let api = OvApi::load().map_err(|_| fail("runtime"))?;
    unsafe { build(api, bytes) }
}

unsafe fn build(
    api: Arc<OvApi>,
    bytes: &[u8],
) -> std::result::Result<OpenvinoEngine, CreateFailure> {
    let stage = "model_session";
    let mut engine = OpenvinoEngine {
        request: null_mut(),
        input_tensor: null_mut(),
        compiled: null_mut(),
        model: null_mut(),
        core: null_mut(),
        api: api.clone(),
        inputs: Vec::new(),
        outputs: Vec::new(),
    };
    ov_try((api.core_create)(&mut engine.core), "core_create")?;
    // ONNX 内存加载不使用 weights（仅 IR 需要）；中途失败由 Drop 兜住已建句柄。
    ov_try(
        (api.read_model_from_memory_buffer)(
            engine.core,
            bytes.as_ptr().cast(),
            bytes.len(),
            null_mut(),
            &mut engine.model,
        ),
        "model_read",
    )?;
    ov_try(
        (api.compile_model)(
            engine.core,
            engine.model,
            c"CPU".as_ptr(),
            0,
            &mut engine.compiled,
        ),
        "model_compile",
    )?;
    ov_try(
        (api.compiled_model_create_infer_request)(engine.compiled, &mut engine.request),
        "infer_request",
    )?;
    let (inputs, outputs) = collect_io(&api, engine.compiled, stage)?;
    (engine.inputs, engine.outputs) = (inputs, outputs);
    if engine.inputs.len() != 1 {
        return Err(fail(stage));
    }
    engine.input_tensor = create_input_tensor(&api, &engine.inputs[0], stage)?;
    ov_try(
        (api.infer_request_set_input_tensor)(engine.request, engine.input_tensor),
        stage,
    )?;
    Ok(engine)
}

/// 收集单输入/多输出的端口规格；输出数超 SDK 合同（1..=32）按候选失败回退。
unsafe fn collect_io(
    api: &OvApi,
    compiled: *mut ov_compiled_model_t,
    stage: &'static str,
) -> std::result::Result<(Vec<TensorInfo>, Vec<TensorInfo>), CreateFailure> {
    let mut inputs_count = 0usize;
    ov_try(
        (api.compiled_model_inputs_size)(compiled, &mut inputs_count),
        stage,
    )?;
    let mut outputs_count = 0usize;
    ov_try(
        (api.compiled_model_outputs_size)(compiled, &mut outputs_count),
        stage,
    )?;
    if inputs_count != 1 || outputs_count == 0 || outputs_count > 32 {
        return Err(fail(stage));
    }
    let input_port = port_by_index(api, compiled, true, 0, stage)?;
    let inputs = vec![port_info(api, input_port, stage)?];
    let mut outputs = Vec::with_capacity(outputs_count);
    for index in 0..outputs_count {
        let port = port_by_index(api, compiled, false, index, stage)?;
        outputs.push(port_info(api, port, stage)?);
    }
    Ok((inputs, outputs))
}

unsafe fn port_by_index(
    api: &OvApi,
    compiled: *mut ov_compiled_model_t,
    input: bool,
    index: usize,
    stage: &'static str,
) -> std::result::Result<*mut ov_output_const_port_t, CreateFailure> {
    let mut port: *mut ov_output_const_port_t = null_mut();
    let status = if input {
        (api.compiled_model_input_by_index)(compiled, index, &mut port)
    } else {
        (api.compiled_model_output_by_index)(compiled, index, &mut port)
    };
    ov_try(status, stage)?;
    Ok(port)
}

/// 读端口规格并释放句柄；名称/shape/元素类型读序固定，free 前全部取值。
unsafe fn port_info(
    api: &OvApi,
    port: *mut ov_output_const_port_t,
    stage: &'static str,
) -> std::result::Result<TensorInfo, CreateFailure> {
    let mut shape = ov_sys::ov_shape_t {
        rank: 0,
        dims: null_mut(),
    };
    ov_try((api.const_port_get_shape)(port, &mut shape), stage)?;
    let dims: Vec<i64> = if shape.rank > 0 && !shape.dims.is_null() {
        std::slice::from_raw_parts(shape.dims, shape.rank as usize).to_vec()
    } else {
        Vec::new()
    };
    let _ = (api.shape_free)(&mut shape);
    let mut element: ov_element_type_e = 0;
    ov_try((api.port_get_element_type)(port, &mut element), stage)?;
    let mut name_ptr: *mut c_char = null_mut();
    ov_try((api.port_get_any_name)(port, &mut name_ptr), stage)?;
    let name = if name_ptr.is_null() {
        String::new()
    } else {
        CStr::from_ptr(name_ptr).to_string_lossy().into_owned()
    };
    (api.free)(name_ptr);
    let built = tensor_spec(&name, &dims, element);
    (api.output_const_port_free)(port);
    built.map_err(|_| fail(stage))
}

/// 输入张量按端口静态 shape 预建；shape 局部 Vec 撑住，tensor_create 内部复制。
unsafe fn create_input_tensor(
    api: &OvApi,
    info: &TensorInfo,
    stage: &'static str,
) -> std::result::Result<*mut ov_tensor_t, CreateFailure> {
    let element = element_type_of(info.dtype).map_err(|_| fail(stage))?;
    let mut dims = info.shape.clone();
    let shape = ov_sys::ov_shape_t {
        rank: dims.len() as i64,
        dims: dims.as_mut_ptr(),
    };
    let mut tensor: *mut ov_tensor_t = null_mut();
    ov_try((api.tensor_create)(element, shape, &mut tensor), stage)?;
    Ok(tensor)
}

impl OpenvinoEngine {
    /// 同步推理：输入字节直拷进预建张量 → infer → 按索引读输出为 TensorData。
    /// 原生 infer 失败记 Some(code)；输入/输出搬运错误为交付类（native_code=None）。
    pub(crate) fn infer(
        &self,
        inputs: &[TensorData],
        output_budget: usize,
    ) -> std::result::Result<Vec<TensorData>, OvInferError> {
        let api = &self.api;
        unsafe {
            let mut data: *mut c_void = null_mut();
            if (api.tensor_data)(self.input_tensor, &mut data) != OV_OK || data.is_null() {
                return Err(OvInferError {
                    native_ok: false,
                    native_code: None,
                    error: Error("vision.inferenceFailed".into()),
                });
            }
            let mut size = 0usize;
            if (api.tensor_get_byte_size)(self.input_tensor, &mut size) != OV_OK
                || size != inputs[0].bytes().len()
            {
                return Err(OvInferError {
                    native_ok: false,
                    native_code: None,
                    error: Error("vision.inputBytesMismatch".into()),
                });
            }
            std::ptr::copy_nonoverlapping(inputs[0].bytes().as_ptr(), data.cast(), size);
            let status = (api.infer_request_infer)(self.request);
            if status != OV_OK {
                return Err(OvInferError {
                    native_ok: false,
                    native_code: Some(status),
                    error: Error("vision.inferenceFailed".into()),
                });
            }
            let mut remaining = output_budget;
            let mut outputs = Vec::with_capacity(self.outputs.len());
            for (index, spec) in self.outputs.iter().enumerate() {
                let built = self.read_output(index, spec)?;
                let allocated = built
                    .bytes()
                    .len()
                    .checked_next_multiple_of(8)
                    .ok_or_else(exhaustion)?;
                remaining = remaining.checked_sub(allocated).ok_or_else(exhaustion)?;
                outputs.push(built);
            }
            Ok(outputs)
        }
    }

    /// 读单个输出张量并释放句柄；shape/类型/尺寸/数据在 free 前全部取值。
    unsafe fn read_output(
        &self,
        index: usize,
        name_spec: &TensorInfo,
    ) -> std::result::Result<TensorData, OvInferError> {
        let api = &self.api;
        let mut tensor: *mut ov_tensor_t = null_mut();
        if (api.infer_request_get_output_tensor_by_index)(self.request, index, &mut tensor) != OV_OK
            || tensor.is_null()
        {
            return Err(OvInferError {
                native_ok: true,
                native_code: None,
                error: Error("vision.inferenceFailed".into()),
            });
        }
        let built = self.read_output_data(tensor, name_spec);
        (api.tensor_free)(tensor);
        built
    }

    unsafe fn read_output_data(
        &self,
        tensor: *mut ov_tensor_t,
        name_spec: &TensorInfo,
    ) -> std::result::Result<TensorData, OvInferError> {
        let api = &self.api;
        let mut shape = ov_sys::ov_shape_t {
            rank: 0,
            dims: null_mut(),
        };
        if (api.tensor_get_shape)(tensor, &mut shape) != OV_OK {
            return Err(OvInferError {
                native_ok: true,
                native_code: None,
                error: Error("vision.inferenceFailed".into()),
            });
        }
        let dims: Vec<i64> = if shape.rank > 0 && !shape.dims.is_null() {
            std::slice::from_raw_parts(shape.dims, shape.rank as usize).to_vec()
        } else {
            Vec::new()
        };
        let _ = (api.shape_free)(&mut shape);
        if dims.is_empty() || dims.iter().any(|d| *d <= 0) {
            return Err(OvInferError {
                native_ok: true,
                native_code: None,
                error: Error("vision.inferenceFailed".into()),
            });
        }
        let mut element: ov_element_type_e = 0;
        if (api.tensor_get_element_type)(tensor, &mut element) != OV_OK {
            return Err(OvInferError {
                native_ok: true,
                native_code: None,
                error: Error("vision.inferenceFailed".into()),
            });
        }
        let dtype = map_element_type(element).map_err(|error| OvInferError {
            native_ok: true,
            native_code: None,
            error,
        })?;
        let info = TensorInfo {
            name: name_spec.name.clone(),
            dtype,
            shape: dims,
        };
        let mut size = 0usize;
        if (api.tensor_get_byte_size)(tensor, &mut size) != OV_OK {
            return Err(OvInferError {
                native_ok: true,
                native_code: None,
                error: Error("vision.inferenceFailed".into()),
            });
        }
        let mut data: *mut c_void = null_mut();
        if (api.tensor_data)(tensor, &mut data) != OV_OK || data.is_null() {
            return Err(OvInferError {
                native_ok: true,
                native_code: None,
                error: Error("vision.inferenceFailed".into()),
            });
        }
        let raw = std::slice::from_raw_parts(data.cast(), size);
        TensorData::new(info, raw).map_err(|error| OvInferError {
            native_ok: true,
            native_code: None,
            error,
        })
    }
}

fn exhaustion() -> OvInferError {
    OvInferError {
        native_ok: true,
        native_code: None,
        error: Error("sdk.resourceExhausted".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::ov_sys::OV_TYPE_STRING;
    use super::*;

    #[test]
    fn element_type_round_trip_covers_supported_dtypes() {
        for (ov_value, dtype) in [
            (OV_TYPE_F32, 0u32),
            (OV_TYPE_F16, 1),
            (OV_TYPE_I64, 2),
            (OV_TYPE_I32, 3),
            (OV_TYPE_U8, 4),
        ] {
            assert_eq!(map_element_type(ov_value).unwrap(), dtype);
            assert_eq!(element_type_of(dtype).unwrap(), ov_value);
        }
        assert!(map_element_type(OV_TYPE_STRING).is_err());
        assert!(element_type_of(9).is_err());
    }

    #[test]
    fn tensor_spec_rejects_dynamic_or_oversized_shapes() {
        let ok = tensor_spec("images", &[1, 3, 640, 640], OV_TYPE_F32).unwrap();
        assert_eq!(ok.name, "images");
        assert_eq!(ok.dtype, 0);
        assert_eq!(ok.shape, vec![1, 3, 640, 640]);
        for dims in [&[][..], &[0][..], &[-1][..], &[1, 3, -1, 3][..]] {
            assert!(tensor_spec("x", dims, OV_TYPE_F32).is_err());
        }
        assert!(tensor_spec("x", &[1i64; 9], OV_TYPE_F32).is_err());
    }

    #[test]
    fn profile_layout_boundary_is_nchw_byte_identity() {
        // E1.5 档案适配边界：NCHW f32 预处理张量按字节直传引擎，无布局变换。
        let values: Vec<f32> = (0..12).map(|i| i as f32).collect();
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_ne_bytes()).collect();
        let tensor = TensorData::new(
            TensorInfo {
                name: "images".into(),
                dtype: 0,
                shape: [1, 3, 2, 2].map(i64::from).to_vec(),
            },
            &bytes,
        )
        .unwrap();
        assert_eq!(tensor.bytes().len(), 48);
        assert_eq!(tensor.floats().unwrap(), values.as_slice());
    }

    #[test]
    #[ignore = "显式 OpenVINO 运行库与真实 ONNX 模型；E1.6 冒烟入口"]
    fn real_runtime_smoke_loads_compiles_and_infers() {
        let model_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../.local/tests/met2-s3/yolo26n-pose.onnx"
        );
        let bytes = std::fs::read(model_path).expect("模型文件缺失");
        let api = match OvApi::load() {
            Ok(api) => api,
            Err(reason) => panic!("运行库加载失败: {reason}"),
        };
        let engine = match unsafe { build(api.clone(), &bytes) } {
            Ok(engine) => engine,
            Err(failure) => panic!(
                "构造失败 stage={} code={:?} msg={}",
                failure.stage,
                failure.native_code,
                api.last_error_message()
            ),
        };
        assert_eq!(engine.inputs.len(), 1);
        assert_eq!(engine.inputs[0].shape, [1, 3, 640, 640].map(i64::from));
        assert_eq!(engine.inputs[0].dtype, 0);
        let len = crate::sdk::byte_count(engine.inputs[0].dtype, &engine.inputs[0].shape).unwrap();
        let zeros = vec![0u8; len];
        let input = TensorData::new(engine.inputs[0].clone(), &zeros).unwrap();
        let outputs = engine
            .infer(std::slice::from_ref(&input), 64 << 20)
            .unwrap();
        for output in &outputs {
            println!(
                "output: name={} dtype={} shape={:?}",
                output.info.name, output.info.dtype, output.info.shape
            );
        }
        assert!(!outputs.is_empty());
        for output in &outputs {
            assert_eq!(output.info.shape.first(), Some(&1));
        }
    }
}
