//! OpenVINO 原生推理引擎：内存加载 → 按目标设备编译（CPU 默认/GPU 定向）→ 同步推理；
//! 候选失败回退 ORT CPU。GPU 定向（§3.5b）：DXGI 命中适配器 → OV GPU 设备域可用性
//! 检查 + FULL_DEVICE_NAME 身份核对（厂商级防静默换卡），多 GPU 域内按 needle+instance
//! 消歧；OV C API 无 DXGI↔OV 设备级映射，边界见 resolve_gpu_device。
#![allow(non_snake_case)]
use super::ov_sys::{
    self, ov_compiled_model_t, ov_core_t, ov_element_type_e, ov_infer_request_t, ov_model_t,
    ov_output_const_port_t, ov_status_e, ov_tensor_t, ElementLine, OvApi, OV_OK, OV_TYPE_F16,
    OV_TYPE_F32, OV_TYPE_I32, OV_TYPE_I64, OV_TYPE_U8,
};
use crate::sdk::device::DeviceSelector;
use crate::sdk::{TensorData, TensorInfo};
use crate::{Error, Result};
use std::ffi::{c_char, c_void, CStr, CString};
use std::ptr::null_mut;
use std::sync::{Arc, Mutex, OnceLock};

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

/// 编译目标设备：Cpu 与 E1 首版行为完全一致；Gpu 为 §3.5b 定向（selector 的
/// needle/instance 进 OV 设备域消歧，vendor 为 DXGI 命中适配器的厂商核对词）。
pub(crate) enum CompileDevice<'a> {
    Cpu,
    Gpu {
        selector: &'a DeviceSelector,
        vendor: Option<&'static str>,
    },
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
    /// 实际编译设备（OV 设备名："CPU" 或 GPU 域设备名）；定向核对与标签用。
    pub(crate) device: String,
}

impl OpenvinoEngine {
    /// 是否编译在 GPU 设备域（am_backend_actual_t.device_utf8 "OpenVINO:GPU" 语义）。
    pub(crate) fn is_gpu(&self) -> bool {
        self.device == "GPU" || self.device.starts_with("GPU.")
    }
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

/// ov 元素类型 → TensorData dtype 合同值（sdk/model.rs byte_count 同套编码）；
/// 序数按运行库线选表（2025.0.x 旧序数经 ElementLine 适配）。
fn map_element_type(element: ov_element_type_e, line: ElementLine) -> Result<u32> {
    let mapped = match line {
        ElementLine::Current => match element {
            OV_TYPE_F32 => Some(0),
            OV_TYPE_F16 => Some(1),
            OV_TYPE_I64 => Some(2),
            OV_TYPE_I32 => Some(3),
            OV_TYPE_U8 => Some(4),
            _ => None,
        },
        ElementLine::Legacy2025_0 => match element {
            ov_sys::OV_TYPE_F32_2025_0 => Some(0),
            ov_sys::OV_TYPE_F16_2025_0 => Some(1),
            ov_sys::OV_TYPE_I64_2025_0 => Some(2),
            ov_sys::OV_TYPE_I32_2025_0 => Some(3),
            ov_sys::OV_TYPE_U8_2025_0 => Some(4),
            _ => None,
        },
    };
    mapped.ok_or_else(|| Error("vision.tensorTypeUnsupported".into()))
}

/// TensorData dtype → ov 元素类型（输入张量创建用，map_element_type 的逆映射）。
fn element_type_of(dtype: u32, line: ElementLine) -> Result<ov_element_type_e> {
    let element = match dtype {
        0 => match line {
            ElementLine::Current => OV_TYPE_F32,
            ElementLine::Legacy2025_0 => ov_sys::OV_TYPE_F32_2025_0,
        },
        1 => match line {
            ElementLine::Current => OV_TYPE_F16,
            ElementLine::Legacy2025_0 => ov_sys::OV_TYPE_F16_2025_0,
        },
        2 => match line {
            ElementLine::Current => OV_TYPE_I64,
            ElementLine::Legacy2025_0 => ov_sys::OV_TYPE_I64_2025_0,
        },
        3 => match line {
            ElementLine::Current => OV_TYPE_I32,
            ElementLine::Legacy2025_0 => ov_sys::OV_TYPE_I32_2025_0,
        },
        4 => match line {
            ElementLine::Current => OV_TYPE_U8,
            ElementLine::Legacy2025_0 => ov_sys::OV_TYPE_U8_2025_0,
        },
        _ => return Err(Error("vision.tensorTypeUnsupported".into())),
    };
    Ok(element)
}

/// 端口规格校验：拒绝空/超长/含非正维（动态 shape 首版不支持，走 ORT 候选）。
fn tensor_spec(
    name: &str,
    dims: &[i64],
    element: ov_element_type_e,
    line: ElementLine,
) -> Result<TensorInfo> {
    if dims.is_empty() || dims.len() > 8 || dims.iter().any(|d| *d <= 0) {
        return Err(Error("vision.ioUnsupported".into()));
    }
    Ok(TensorInfo {
        name: name.into(),
        dtype: map_element_type(element, line)?,
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

/// 期望的运行库线（失败详情用；符号集/结构逐线核对一致，元素序数 2025.0.x 经线适配）。
const EXPECTED_LINE: &str = "expected OpenVINO C API 2025.x/2026.x line (openvino_c.dll)";

/// 编译并装配引擎；候选失败统一 backendStrictUnavailable 触发 selection 回退。
/// 每次尝试先清零旧失败详情；装载/构造失败时记录检测版本与期望线（进 last_error 详情）。
pub(crate) fn create(
    bytes: &[u8],
    target: CompileDevice<'_>,
) -> std::result::Result<OpenvinoEngine, CreateFailure> {
    ov_sys::clear_load_failure();
    let api = OvApi::load().map_err(|failure| {
        ov_sys::note_load_failure(format!(
            "openvino native engine unavailable: {}; {EXPECTED_LINE}",
            failure.reason
        ));
        fail("runtime")
    })?;
    let built = unsafe { build(api.clone(), bytes, target) };
    if let Err(failure) = &built {
        let version = api.version();
        let detected = if version.is_empty() {
            String::from("runtime version unknown")
        } else {
            format!("detected runtime {version}")
        };
        let gpu_note = match (failure.stage, gpu_domain_detail()) {
            ("gpu_device", Some(detail)) => format!("; {detail}"),
            _ => String::new(),
        };
        ov_sys::note_load_failure(format!(
            "openvino native engine failed at {} (native_code={:?}, {detected}){gpu_note}; {EXPECTED_LINE}",
            failure.stage, failure.native_code
        ));
    }
    built
}

/// GPU 域解析失败详情（最近一次枚举的设备域快照；仅 gpu_device 阶段附带）。
fn gpu_domain_detail() -> Option<String> {
    GPU_DOMAIN_SNAPSHOT
        .lock()
        .ok()
        .and_then(|slot| slot.clone())
}

/// GPU 定向解析的设备域快照（诊断用；resolve_gpu_device 写入，create 失败详情读）。
static GPU_DOMAIN_SNAPSHOT: Mutex<Option<String>> = Mutex::new(None);

/// OV remote 直传可用性探测（§3.3a ABI 1.8 能力位置位面）：openvino_c 可载 + remote
/// 六符号齐备 + 设备枚举域含 GPU 设备；进程级 OnceLock 缓存（能力位查询语义）。
/// 仅枚举设备域——不建 remote context、不触发推理编译；失败静默返回 false，
/// 不写装载失败详情（探测属能力查询，非模型构造尝试）。
pub(crate) fn remote_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| unsafe { probe_remote_once() })
}

unsafe fn probe_remote_once() -> bool {
    let Ok(api) = OvApi::load() else {
        return false;
    };
    api.remote_symbols_present() && unsafe { gpu_in_device_domain(&api) }
}

/// 设备枚举域是否含 GPU 设备（"GPU"/"GPU.N"；resolve_gpu_device 同款过滤口径）。
unsafe fn gpu_in_device_domain(api: &OvApi) -> bool {
    let mut core: *mut ov_core_t = null_mut();
    if (api.core_create)(&mut core) != OV_OK {
        return false;
    }
    let mut devices = ov_sys::ov_available_devices_t {
        devices: null_mut(),
        size: 0,
    };
    let enumerated = (api.core_get_available_devices)(core, &mut devices) == OV_OK;
    let gpu = enumerated
        && !devices.devices.is_null()
        && (0..devices.size).any(|index| {
            let name = *devices.devices.add(index);
            !name.is_null() && {
                let name = CStr::from_ptr(name).to_string_lossy();
                name == "GPU" || name.starts_with("GPU.")
            }
        });
    if enumerated {
        (api.available_devices_free)(&mut devices);
    }
    (api.core_free)(core);
    gpu
}

unsafe fn build(
    api: Arc<OvApi>,
    bytes: &[u8],
    target: CompileDevice<'_>,
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
        device: String::new(),
    };
    ov_try((api.core_create)(&mut engine.core), "core_create")?;
    let device = match target {
        CompileDevice::Cpu => c"CPU".to_owned(),
        CompileDevice::Gpu { selector, vendor } => {
            resolve_gpu_device(&api, engine.core, selector, vendor)?
        }
    };
    engine.device = device.to_string_lossy().into_owned();
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
            device.as_ptr(),
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

/// GPU 定向解析：枚举 OV 设备域 → 取 GPU 设备（"GPU"/"GPU.N"）→ 纯函数选定 →
/// 编译设备名。无 GPU 设备（CPU 插件运行库）或定向不可达（needle/instance 在 OV
/// 域无消歧命中且非单设备回退，或厂商核对不符）按候选失败（stage=gpu_device）回退，
/// 不静默换设备编译。
unsafe fn resolve_gpu_device(
    api: &OvApi,
    core: *mut ov_core_t,
    selector: &DeviceSelector,
    vendor: Option<&'static str>,
) -> std::result::Result<CString, CreateFailure> {
    let mut devices = ov_sys::ov_available_devices_t {
        devices: null_mut(),
        size: 0,
    };
    ov_try(
        (api.core_get_available_devices)(core, &mut devices),
        "gpu_device",
    )?;
    let names: Vec<String> = if devices.devices.is_null() {
        Vec::new()
    } else {
        std::slice::from_raw_parts(devices.devices, devices.size)
            .iter()
            .filter(|p| !p.is_null())
            .map(|p| CStr::from_ptr(*p).to_string_lossy().into_owned())
            .collect()
    };
    (api.available_devices_free)(&mut devices);
    let gpu_devices: Vec<(String, String)> = names
        .iter()
        .filter(|name| name.as_str() == "GPU" || name.starts_with("GPU."))
        .map(|name| (name.clone(), full_device_name(api, core, name)))
        .collect();
    if let Ok(mut slot) = GPU_DOMAIN_SNAPSHOT.lock() {
        let listing = gpu_devices
            .iter()
            .map(|(name, full)| format!("{name}=\"{full}\""))
            .collect::<Vec<_>>()
            .join(", ");
        *slot = Some(format!("ov gpu device domain [{listing}]"));
    }
    match pick_gpu_device(&gpu_devices, selector.needle(), selector.instance(), vendor) {
        Some(name) => CString::new(name.as_str()).map_err(|_| fail("gpu_device")),
        None => Err(fail("gpu_device")),
    }
}

/// 设备全名读取（FULL_DEVICE_NAME）；失败返回空串（该设备仍可作为单设备回退候选，
/// 仅失去 needle 命中资格）。
unsafe fn full_device_name(api: &OvApi, core: *mut ov_core_t, device: &str) -> String {
    let Ok(key) = CString::new(device) else {
        return String::new();
    };
    let mut value: *mut c_char = null_mut();
    if (api.core_get_property)(core, key.as_ptr(), c"FULL_DEVICE_NAME".as_ptr(), &mut value)
        != OV_OK
        || value.is_null()
    {
        return String::new();
    }
    let name = CStr::from_ptr(value).to_string_lossy().into_owned();
    (api.free)(value);
    name
}

/// 纯函数设备选定（无 FFI，单测直测）：优先 needle 在 FULL_DEVICE_NAME 域的
/// 子串命中（第 instance 个命中消歧）；无命中时仅当 OV 域恰一个 GPU 设备才回退
/// （OV 设备全名与 DXGI Description 文本不同源，单设备域无歧义）；最终经厂商
/// 核对词把关（DXGI 命中适配器厂商须出现在选定设备全名，防跨厂商静默换卡）。
/// 返回编译用 OV 设备名。
fn pick_gpu_device(
    gpu_devices: &[(String, String)],
    needle: &str,
    instance: u32,
    vendor: Option<&'static str>,
) -> Option<String> {
    let needle = needle.to_lowercase();
    let matched = gpu_devices
        .iter()
        .filter(|(_, full)| full.to_lowercase().contains(&needle))
        .nth(instance as usize);
    let candidate = match matched {
        Some((name, _)) => Some(name.clone()),
        None if gpu_devices.len() == 1 => Some(gpu_devices[0].0.clone()),
        None => None,
    }?;
    if let Some(token) = vendor {
        let full = gpu_devices
            .iter()
            .find(|(name, _)| *name == candidate)
            .map(|(_, full)| full.to_lowercase())
            .unwrap_or_default();
        if !full.contains(token) {
            return None;
        }
    }
    Some(candidate)
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

/// 端口句柄 RAII 守卫（F07）：获取即建立，任意查询失败提前返回经 Drop 释放，
/// 成功路径同样统一走 Drop——不再依赖末尾显式 free。
struct PortGuard<'a> {
    api: &'a OvApi,
    port: *mut ov_output_const_port_t,
}

impl Drop for PortGuard<'_> {
    fn drop(&mut self) {
        if !self.port.is_null() {
            unsafe { (self.api.output_const_port_free)(self.port) };
        }
    }
}

/// 读端口规格并释放句柄；名称/shape/元素类型读序固定，free 前全部取值。
unsafe fn port_info(
    api: &OvApi,
    port: *mut ov_output_const_port_t,
    stage: &'static str,
) -> std::result::Result<TensorInfo, CreateFailure> {
    let _port = PortGuard { api, port };
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
    tensor_spec(&name, &dims, element, api.element_line()).map_err(|_| fail(stage))
}

/// 输入张量按端口静态 shape 预建；shape 局部 Vec 撑住，tensor_create 内部复制。
unsafe fn create_input_tensor(
    api: &OvApi,
    info: &TensorInfo,
    stage: &'static str,
) -> std::result::Result<*mut ov_tensor_t, CreateFailure> {
    let element = element_type_of(info.dtype, api.element_line()).map_err(|_| fail(stage))?;
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
                // 剩余预算传入读取：复制前检查（F06，对齐 ORT 先扣后复制语义），
                // 外层扣减维护多输出累加。
                let built = self.read_output(index, spec, remaining)?;
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
    /// remaining：本输出复制前的剩余预算（F06：TensorData 分配前检查）。
    unsafe fn read_output(
        &self,
        index: usize,
        name_spec: &TensorInfo,
        remaining: usize,
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
        let built = self.read_output_data(tensor, name_spec, remaining);
        (api.tensor_free)(tensor);
        built
    }

    unsafe fn read_output_data(
        &self,
        tensor: *mut ov_tensor_t,
        name_spec: &TensorInfo,
        remaining: usize,
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
        let dtype =
            map_element_type(element, self.api.element_line()).map_err(|error| OvInferError {
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
        // 预算检查在复制之前（F06）：按原生 byte size 对齐 8 扣减，不足即耗尽，
        // 不再先 TensorData::new 分配复制后才报（零预算/差一字节均不得先复制）。
        let allocated = size.checked_next_multiple_of(8).ok_or_else(exhaustion)?;
        remaining.checked_sub(allocated).ok_or_else(exhaustion)?;
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
    use crate::sdk::{byte_count, selection::BackendTarget, Model, ModelSource};
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, MutexGuard, OnceLock};

    const PARITY_MODEL: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../.local/tests/met2-s3/yolo26n-pose.onnx"
    );
    const PARITY_REL_TOLERANCE: f32 = 5.0e-3;
    const PARITY_STATS_REL_TOLERANCE: f32 = 1.0e-4;
    const PARITY_ABS_FLOOR: f32 = 1.0e-5;
    // AM_SDK_OPENVINO_DIR 是进程级环境变量；对拍与回退各档互斥执行，
    // 且每测试先恢复进程启动时的原始值，避免回退档遗留状态污染后续测试。
    static ENV_LOCK: Mutex<()> = Mutex::new(());
    static ORIGINAL_OV_DIR: OnceLock<Option<OsString>> = OnceLock::new();

    fn env_lock() -> MutexGuard<'static, ()> {
        ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn restore_ov_dir() {
        let original = ORIGINAL_OV_DIR
            .get_or_init(|| std::env::var_os("AM_SDK_OPENVINO_DIR"))
            .clone();
        match &original {
            Some(dir) => std::env::set_var("AM_SDK_OPENVINO_DIR", dir),
            None => std::env::remove_var("AM_SDK_OPENVINO_DIR"),
        }
        // E1 runtime_dirs 登记与失败详情同为进程级状态：每测试恢复到进程启动时
        // 的干净值，避免登记残留让「环境未设→回退」等档位误找到运行库。
        super::ov_sys::reset_runtime_dir_for_tests();
        super::ov_sys::clear_load_failure();
    }

    fn ensure_ort_ready() {
        let dll = crate::backend::locate_runtime()
            .or_else(|| std::env::var_os("AM_RT_ORT_GPU_LIBRARY").map(PathBuf::from))
            .unwrap_or_else(|| {
                panic!("ORT 运行库未定位：设 AM_RT_ORT_LIBRARY 或 AM_RT_ORT_GPU_LIBRARY")
            });
        crate::sdk::initialize(&dll).expect("ORT 运行库初始化");
    }

    /// 固定种子 xorshift32 → [0,1) f32 NCHW [1,3,640,640]；两次调用产出相同字节。
    fn parity_input_bytes() -> Vec<u8> {
        let mut state: u32 = 0x5EED_1234;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        let mut bytes = Vec::with_capacity(3 * 640 * 640 * 4);
        for _ in 0..3 * 640 * 640 {
            bytes.extend_from_slice(&((next() >> 8) as f32 / 16_777_216.0).to_le_bytes());
        }
        bytes
    }

    fn row_max_rel(a: &[f32], b: &[f32]) -> f32 {
        a.iter()
            .zip(b)
            .map(|(&x, &y)| {
                let diff = (x - y).abs();
                if diff <= PARITY_ABS_FLOOR {
                    0.0
                } else {
                    diff / x.abs().max(y.abs()).max(1e-6)
                }
            })
            .fold(0.0f32, f32::max)
    }

    fn row_l2_sq(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b).map(|(&x, &y)| (x - y) * (x - y)).sum()
    }

    fn moment_max_rel(a: &[f32], b: &[f32]) -> f32 {
        moment_rels(a, b).into_iter().fold(0.0f32, f32::max)
    }

    /// 四统计量（min/max/mean/std）各自相对差：mean/std 稳健、min/max 近零敏感。
    fn moment_rels(a: &[f32], b: &[f32]) -> [f32; 4] {
        fn moments(v: &[f32]) -> (f32, f32, f32, f32) {
            let n = v.len() as f32;
            let mean = v.iter().sum::<f32>() / n;
            let var = v.iter().map(|x| (x - mean) * (x - mean)).sum::<f32>() / n;
            (
                v.iter().fold(f32::INFINITY, |acc, &x| acc.min(x)),
                v.iter().fold(f32::NEG_INFINITY, |acc, &x| acc.max(x)),
                mean,
                var.sqrt(),
            )
        }
        let (min_a, max_a, mean_a, std_a) = moments(a);
        let (min_b, max_b, mean_b, std_b) = moments(b);
        [
            (min_a, min_b),
            (max_a, max_b),
            (mean_a, mean_b),
            (std_a, std_b),
        ]
        .into_iter()
        .map(|(x, y)| {
            let diff = (x - y).abs();
            diff / x.abs().max(y.abs()).max(1e-6)
        })
        .collect::<Vec<_>>()
        .try_into()
        .expect("四统计量定长")
    }

    #[test]
    fn element_type_round_trip_covers_supported_dtypes_on_both_lines() {
        for line in [ElementLine::Current, ElementLine::Legacy2025_0] {
            for (ov_value, dtype) in match line {
                ElementLine::Current => [
                    (OV_TYPE_F32, 0u32),
                    (OV_TYPE_F16, 1),
                    (OV_TYPE_I64, 2),
                    (OV_TYPE_I32, 3),
                    (OV_TYPE_U8, 4),
                ],
                ElementLine::Legacy2025_0 => [
                    (ov_sys::OV_TYPE_F32_2025_0, 0u32),
                    (ov_sys::OV_TYPE_F16_2025_0, 1),
                    (ov_sys::OV_TYPE_I64_2025_0, 2),
                    (ov_sys::OV_TYPE_I32_2025_0, 3),
                    (ov_sys::OV_TYPE_U8_2025_0, 4),
                ],
            } {
                assert_eq!(map_element_type(ov_value, line).unwrap(), dtype);
                assert_eq!(element_type_of(dtype, line).unwrap(), ov_value);
            }
        }
        // 跨线序数互不认账：两线仅 4/10 数值重叠（语义不同，按线选表正是关键），
        // 其余各自独有值在对侧线必须拒收（如当前线 F16=3/U8=16、旧线 F32=5/U8=17）。
        assert!(map_element_type(OV_TYPE_F16, ElementLine::Legacy2025_0).is_err());
        assert!(map_element_type(OV_TYPE_I32, ElementLine::Legacy2025_0).is_err());
        assert!(map_element_type(OV_TYPE_U8, ElementLine::Legacy2025_0).is_err());
        assert!(map_element_type(ov_sys::OV_TYPE_F32_2025_0, ElementLine::Current).is_err());
        assert!(map_element_type(ov_sys::OV_TYPE_I64_2025_0, ElementLine::Current).is_err());
        assert!(map_element_type(ov_sys::OV_TYPE_U8_2025_0, ElementLine::Current).is_err());
        // STRING 不在 TensorData 合同内，两线均拒收。
        assert!(map_element_type(OV_TYPE_STRING, ElementLine::Current).is_err());
        assert!(map_element_type(
            super::ov_sys::OV_TYPE_STRING_2025_0,
            ElementLine::Legacy2025_0
        )
        .is_err());
        assert!(element_type_of(9, ElementLine::Current).is_err());
        assert!(element_type_of(9, ElementLine::Legacy2025_0).is_err());
    }

    #[test]
    fn element_line_follows_build_number_prefix() {
        use super::ov_sys::element_line_of_version;
        assert_eq!(
            element_line_of_version("2025.0.0-17942-1f68be9f594-releases/2025/0"),
            ElementLine::Legacy2025_0
        );
        for build in [
            "2025.1.0-…",
            "2025.3.0-…",
            "2026.1.0-12852c161c.clean",
            "2026.2.1-…",
            "",
        ] {
            assert_eq!(element_line_of_version(build), ElementLine::Current);
        }
    }

    #[test]
    fn tensor_spec_rejects_dynamic_or_oversized_shapes() {
        let ok = tensor_spec(
            "images",
            &[1, 3, 640, 640],
            OV_TYPE_F32,
            ElementLine::Current,
        )
        .unwrap();
        assert_eq!(ok.name, "images");
        assert_eq!(ok.dtype, 0);
        assert_eq!(ok.shape, vec![1, 3, 640, 640]);
        for dims in [&[][..], &[0][..], &[-1][..], &[1, 3, -1, 3][..]] {
            assert!(tensor_spec("x", dims, OV_TYPE_F32, ElementLine::Current).is_err());
        }
        assert!(tensor_spec("x", &[1i64; 9], OV_TYPE_F32, ElementLine::Current).is_err());
    }

    #[test]
    fn pick_gpu_device_matches_needle_then_single_fallback_and_guards_vendor() {
        let domain = vec![
            (
                "GPU".to_owned(),
                "Intel(R) Arc(TM) Graphics (iGPU)".to_owned(),
            ),
            (
                "GPU.1".to_owned(),
                "Intel(R) Arc(TM) A770 Graphics".to_owned(),
            ),
        ];
        // needle 命中（大小写不敏感）+instance 消歧。
        assert_eq!(
            pick_gpu_device(&domain, "arc", 0, None).as_deref(),
            Some("GPU")
        );
        assert_eq!(
            pick_gpu_device(&domain, "a770", 0, None).as_deref(),
            Some("GPU.1")
        );
        assert_eq!(
            pick_gpu_device(&domain, "arc", 1, None).as_deref(),
            Some("GPU.1")
        );
        // 无命中且多设备域：拒绝（不猜枚举序）。
        assert_eq!(pick_gpu_device(&domain, "rtx", 0, None), None);
        // 厂商核对：DXGI 命中 NVIDIA 但 OV 域全 Intel——拒绝（不静默换卡）。
        assert_eq!(pick_gpu_device(&domain, "arc", 0, Some("nvidia")), None);
        assert_eq!(
            pick_gpu_device(&domain, "arc", 0, Some("intel")).as_deref(),
            Some("GPU")
        );
        // 单设备域回退：needle 未命中（OV 名与 DXGI 文本不同源）仍可用唯一 GPU。
        let single = vec![(
            "GPU".to_owned(),
            "Intel(R) Arc(TM) Graphics (iGPU)".to_owned(),
        )];
        assert_eq!(
            pick_gpu_device(&single, "140t", 0, Some("intel")).as_deref(),
            Some("GPU")
        );
        assert_eq!(pick_gpu_device(&single, "140t", 0, Some("nvidia")), None);
        // 单设备域 instance 越界仍按回退可用（消歧只在命中域内生效）。
        assert_eq!(
            pick_gpu_device(&single, "140t", 3, Some("intel")).as_deref(),
            Some("GPU")
        );
        // 空域：拒绝。
        assert_eq!(pick_gpu_device(&[], "arc", 0, None), None);
        // 未知厂商（None 核对词）：不做厂商筛。
        assert_eq!(
            pick_gpu_device(&single, "140t", 0, None).as_deref(),
            Some("GPU")
        );
    }

    /// remote 六符号 Option 化：任一缺失即探测面不可用（旧运行库装载不受影响）；
    /// 全在才判可用。variadic 桥接桩永不调用（被调即 panic）。
    #[test]
    fn remote_symbol_presence_requires_all_six_symbols() {
        let bare = port_release_injection::stub_api(true, true, true);
        assert!(!bare.remote_symbols_present());
        let full = port_release_injection::stub_api_remote_present();
        assert!(full.remote_symbols_present());
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
        let _env = env_lock();
        restore_ov_dir();
        let bytes = std::fs::read(PARITY_MODEL).expect("模型文件缺失");
        let api = match OvApi::load() {
            Ok(api) => api,
            Err(failure) => panic!("运行库加载失败: {}", failure.reason),
        };
        let engine = match unsafe { build(api.clone(), &bytes, CompileDevice::Cpu) } {
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

    #[test]
    #[ignore = "真实双引擎对拍：AM_SDK_OPENVINO_DIR + AM_RT_ORT_LIBRARY/AM_RT_ORT_GPU_LIBRARY（E1.6）"]
    fn parity_matches_ort_cpu() {
        let _env = env_lock();
        restore_ov_dir();
        ensure_ort_ready();
        let source = ModelSource::Path(Path::new(PARITY_MODEL));
        let mut ort_model = Model::load(source, None, 0, false, 2).expect("ORT CPU 加载");
        let ort_input = TensorData::new(ort_model.inputs[0].clone(), &parity_input_bytes())
            .expect("ORT 输入构造");
        let ort_out = ort_model
            .run(std::slice::from_ref(&ort_input), 64 << 20)
            .expect("ORT CPU 推理");
        let mut ov_model = Model::load(source, None, 4, false, 2).expect("OpenVINO 原生加载");
        assert_eq!(ov_model.actual_backend, 4, "必须真实落在 OpenVINO 原生后端");
        let ov_input = TensorData::new(ov_model.inputs[0].clone(), &parity_input_bytes())
            .expect("OV 输入构造");
        let ov_out = ov_model
            .run(std::slice::from_ref(&ov_input), 64 << 20)
            .expect("OpenVINO 推理");
        assert_eq!(ov_out.len(), ort_out.len(), "输出张量数不一致");
        for (index, (ort, ov)) in ort_out.iter().zip(&ov_out).enumerate() {
            assert_eq!(ort.info.shape, ov.info.shape, "输出{index} shape 不一致");
            let left = ort.floats().expect("对拍输出应为 f32");
            let right = ov.floats().expect("对拍输出应为 f32");
            assert_eq!(left.len(), right.len());
            let stats_rel = moment_max_rel(left, right);
            assert!(
                stats_rel <= PARITY_STATS_REL_TOLERANCE,
                "输出{index} 统计量 max_rel={stats_rel:.3e} 超容差 1e-4"
            );
            let cols = usize::try_from(*ort.info.shape.last().expect("输出 shape 非空"))
                .expect("维度非负");
            let rows = left.len() / cols;
            let mut direct_bad_rows = 0usize;
            let mut matched_max_rel = 0f32;
            let mut worst: Option<(usize, usize)> = None;
            for r in 0..rows {
                let ov_row = &right[r * cols..(r + 1) * cols];
                if row_max_rel(&left[r * cols..(r + 1) * cols], ov_row) > PARITY_REL_TOLERANCE {
                    direct_bad_rows += 1;
                }
                let (nearest_row, _) = (0..rows)
                    .map(|s| (s, row_l2_sq(&left[s * cols..(s + 1) * cols], ov_row)))
                    .min_by(|a, b| a.1.partial_cmp(&b.1).expect("统计量断言已排除 NaN"))
                    .expect("行数大于 0");
                let matched = &left[nearest_row * cols..(nearest_row + 1) * cols];
                let row_rel = row_max_rel(matched, ov_row);
                if row_rel > matched_max_rel {
                    matched_max_rel = row_rel;
                    worst = Some((r, nearest_row));
                }
            }
            println!(
                "parity output[{index}] elements={} rows={} direct-mismatch-rows={} matched-max-rel={matched_max_rel:.3e}",
                left.len(),
                rows,
                direct_bad_rows
            );
            if let Some((r, s)) = worst {
                println!("parity worst row {r} matched to ref row {s}");
            }
            assert!(
                matched_max_rel <= PARITY_REL_TOLERANCE,
                "输出{index} 匹配后 max_rel={matched_max_rel:.3e} 超容差 5e-3"
            );
        }
    }

    fn assert_openvino_fallback(model: &Model) {
        assert_eq!(model.actual_backend, 0, "实际后端应为回退的 ORT CPU");
        assert!(model.fallback, "公开字段 fallback 应为 true");
        let report = model.selection_report();
        assert_eq!(
            report.selected,
            Some(BackendTarget {
                backend: 0,
                device_id: 0
            })
        );
        assert_eq!(
            report.attempts.len(),
            2,
            "应记录 OpenVINO 失败 + CPU 成功两级"
        );
        let ov = &report.attempts[0];
        assert_eq!(ov.target.backend, 4);
        assert_eq!(ov.stage, "runtime");
        assert_eq!(ov.error.as_deref(), Some("vision.backendStrictUnavailable"));
        assert_eq!(report.attempts[1].target.backend, 0);
        assert_eq!(report.attempts[1].stage, "session_created");
        assert_eq!(report.attempts[1].error, None);
    }

    #[test]
    #[ignore = "真实回退矩阵：AM_RT_ORT_LIBRARY/AM_RT_ORT_GPU_LIBRARY（E1.7）"]
    fn fallback_matrix() {
        let _env = env_lock();
        restore_ov_dir();
        ensure_ort_ready();
        let source = ModelSource::Path(Path::new(PARITY_MODEL));
        let missing =
            std::env::temp_dir().join(format!("am-vision-e17-missing-{}", std::process::id()));

        std::env::set_var("AM_SDK_OPENVINO_DIR", &missing);
        let mut model = Model::load(source, None, 4, false, 2).expect("目录不存在应回退 ORT CPU");
        assert_openvino_fallback(&model);
        let bytes = vec![0u8; byte_count(model.inputs[0].dtype, &model.inputs[0].shape).unwrap()];
        let input = TensorData::new(model.inputs[0].clone(), &bytes).expect("输入构造");
        model
            .run(std::slice::from_ref(&input), 64 << 20)
            .expect("回退后的 ORT CPU 模型应可真实推理");
        println!(
            "fallback-matrix missing-dir -> fallback=ort-cpu attempts=2 report={}",
            serde_json::to_string(model.selection_report()).unwrap()
        );

        std::env::remove_var("AM_SDK_OPENVINO_DIR");
        let model = Model::load(source, None, 4, false, 2).expect("环境变量未设应回退 ORT CPU");
        assert_openvino_fallback(&model);
        println!(
            "fallback-matrix env-unset -> fallback=ort-cpu attempts=2 report={}",
            serde_json::to_string(model.selection_report()).unwrap()
        );

        let Err(error) = Model::load(source, None, 4, true, 2) else {
            panic!("strict=true 且 OpenVINO 不可用应报错，不得静默回退");
        };
        assert_eq!(error.0, "vision.backendStrictUnavailable");
        println!("fallback-matrix strict=true -> error=vision.backendStrictUnavailable（不回退）");
    }

    /// P0-1 接线：runtime_dirs 扫描登记 + 环境变量显式覆盖 + 首次登记优先。
    /// 用空假文件驱动发现路径，不加载真实 DLL（LoadLibrary 对空文件必失败）。
    #[test]
    fn ov_discovery_prefers_env_override_then_first_registered_dir() {
        let _env = env_lock();
        restore_ov_dir();
        let original = std::env::var_os("AM_SDK_OPENVINO_DIR");
        let base = std::env::temp_dir().join(format!("am-ov-disc-{}", std::process::id()));
        let first = base.join("first");
        let second = base.join("second");
        std::fs::create_dir_all(&first).expect("建目录 first");
        std::fs::create_dir_all(&second).expect("建目录 second");
        std::fs::write(first.join(super::ov_sys::library_name()), b"").expect("假库文件 first");
        std::fs::write(second.join(super::ov_sys::library_name()), b"").expect("假库文件 second");
        let assert_dir_channel = |reason: &str, dir: &Path, channel: &str| {
            assert!(reason.contains(channel), "缺渠道标记 {channel}: {reason}");
            assert!(
                reason.contains(dir.to_string_lossy().as_ref()),
                "缺目录 {}: {reason}",
                dir.display()
            );
        };
        let load_failure = || match OvApi::load() {
            Ok(_) => panic!("假库文件必然装载失败"),
            Err(failure) => failure,
        };
        std::env::remove_var("AM_SDK_OPENVINO_DIR");
        // 扫描登记：无库目录跳过，取第一个含 openvino_c 的目录（次序敏感）。
        crate::sdk::runtime::note_openvino_dirs(&[
            base.join("absent").to_string_lossy().into_owned(),
            second.to_string_lossy().into_owned(),
        ]);
        let failure = load_failure();
        assert_dir_channel(&failure.reason, &second, "runtime_dirs");
        // 环境变量显式覆盖：优先于登记目录。
        std::env::set_var("AM_SDK_OPENVINO_DIR", &first);
        let failure = load_failure();
        assert_dir_channel(&failure.reason, &first, "AM_SDK_OPENVINO_DIR");
        // 首次登记优先：已登记目录不被后续登记覆盖。
        std::env::remove_var("AM_SDK_OPENVINO_DIR");
        super::ov_sys::register_runtime_dir(&first);
        let failure = load_failure();
        assert_dir_channel(&failure.reason, &second, "runtime_dirs");
        match original {
            Some(dir) => std::env::set_var("AM_SDK_OPENVINO_DIR", dir),
            None => std::env::remove_var("AM_SDK_OPENVINO_DIR"),
        }
        super::ov_sys::reset_runtime_dir_for_tests();
        super::ov_sys::clear_load_failure();
        let _ = std::fs::remove_dir_all(&base);
    }

    /// P0-1 痛点：候选失败详情必须携带定位原因与期望版本线（无运行库环境可跑）。
    #[test]
    fn engine_failure_detail_carries_expected_line() {
        let _env = env_lock();
        restore_ov_dir();
        let original = std::env::var_os("AM_SDK_OPENVINO_DIR");
        std::env::remove_var("AM_SDK_OPENVINO_DIR");
        super::ov_sys::reset_runtime_dir_for_tests();
        let failure = match create(b"onnx-bytes", CompileDevice::Cpu) {
            Ok(_) => panic!("无运行库应候选失败"),
            Err(failure) => failure,
        };
        assert_eq!(failure.error.0, "vision.backendStrictUnavailable");
        assert_eq!(failure.stage, "runtime");
        let detail =
            crate::sdk::runtime::openvino_load_failure().expect("装载失败必须记录诊断详情");
        assert!(
            detail.contains("AM_SDK_OPENVINO_DIR unset"),
            "缺定位原因: {detail}"
        );
        assert!(detail.contains("2025.x/2026.x"), "缺期望版本线: {detail}");
        // 详情随每次引擎构造清零：成功构造不存在，这里验证读取侧可再清空。
        crate::sdk::runtime::clear_openvino_load_failure();
        assert!(crate::sdk::runtime::openvino_load_failure().is_none());
        match original {
            Some(dir) => std::env::set_var("AM_SDK_OPENVINO_DIR", dir),
            None => std::env::remove_var("AM_SDK_OPENVINO_DIR"),
        }
    }

    /// P0-1 接线复现接入方场景：运行库只经 runtime_dirs 提供（环境变量不设），
    /// 原生引擎应可装载并真实推理。
    #[test]
    #[ignore = "真实 OpenVINO 运行库经 runtime_dirs 登记（进程启动时 AM_SDK_OPENVINO_DIR 指向运行库目录）；E1 接线复现"]
    fn runtime_dirs_registered_engine_loads_and_infers() {
        let _env = env_lock();
        restore_ov_dir();
        let original = ORIGINAL_OV_DIR
            .get()
            .cloned()
            .flatten()
            .map(PathBuf::from)
            .unwrap_or_else(|| panic!("进程启动时须设 AM_SDK_OPENVINO_DIR 指向真实运行库目录"));
        std::env::remove_var("AM_SDK_OPENVINO_DIR");
        crate::sdk::runtime::note_openvino_dirs(&[
            std::env::temp_dir()
                .join(format!("am-ov-absent-{}", std::process::id()))
                .to_string_lossy()
                .into_owned(),
            original.to_string_lossy().into_owned(),
        ]);
        let bytes = std::fs::read(PARITY_MODEL).expect("模型文件缺失");
        let engine =
            create(&bytes, CompileDevice::Cpu).expect("runtime_dirs 登记后应能装载原生引擎");
        assert_eq!(engine.inputs.len(), 1);
        let len = crate::sdk::byte_count(engine.inputs[0].dtype, &engine.inputs[0].shape).unwrap();
        let input = TensorData::new(engine.inputs[0].clone(), &vec![0u8; len]).unwrap();
        let outputs = engine
            .infer(std::slice::from_ref(&input), 64 << 20)
            .expect("原生引擎推理");
        assert!(!outputs.is_empty());
        println!(
            "runtime-dirs wiring: outputs={} runtime={}",
            outputs.len(),
            engine.api.version()
        );
    }

    /// GPU 定向选择器政策（§3.5b）：候选 (4,0)+回退 (0,0)，device=needle。
    fn gpu_policy(needle: &str) -> crate::sdk::selection::BackendSelectionPolicy {
        crate::sdk::selection::BackendSelectionPolicy {
            priority: vec![
                BackendTarget {
                    backend: 4,
                    device_id: 0,
                },
                BackendTarget {
                    backend: 0,
                    device_id: 0,
                },
            ],
            allow_fallback: true,
            budget_ms: 1500,
            device: Some(crate::sdk::device::DeviceSelector::new(needle, 0)),
        }
    }

    /// GPU 定向真机门（OV 2025.0.0 运行库=ort-ov-124-final 实测）：DXGI 命中核显 →
    /// OV GPU 域可用性+身份核对 → GPU 编译+真实推理；同进程与 CPU 引擎统计对拍
    /// （OV 设备全名文本与 DXGI 不同源，needle 用 "arc" 命中两侧）。
    #[test]
    #[ignore = "GPU 定向真机门：AM_SDK_OPENVINO_DIR 指向含 GPU 插件的 OpenVINO 运行库（ort-ov-124-final）"]
    fn gpu_selector_compiles_and_infers_on_openvino_gpu() {
        let _env = env_lock();
        restore_ov_dir();
        let source = ModelSource::Path(Path::new(PARITY_MODEL));
        let mut gpu_model =
            Model::load_selected(source, None, &gpu_policy("arc"), 2).expect("GPU 定向加载");
        assert_eq!(gpu_model.actual_backend, 4, "必须真实落在 OV 原生 GPU");
        assert_eq!(gpu_model.device_label, "OpenVINO:GPU");
        let input = TensorData::new(gpu_model.inputs[0].clone(), &parity_input_bytes())
            .expect("GPU 输入构造");
        let gpu_out = gpu_model
            .run(std::slice::from_ref(&input), 64 << 20)
            .expect("OV 原生 GPU 推理");
        assert!(!gpu_out.is_empty());
        // 同进程双引擎（GPU 定向 + 无 selector 的 CPU 默认路径）零变化对照：
        // 旧入口（无 selector）仍编译 CPU，两侧输出统计对拍。
        let mut cpu_model = Model::load(source, None, 4, false, 2).expect("OV 原生 CPU 加载");
        assert_eq!(cpu_model.actual_backend, 4);
        assert_eq!(cpu_model.device_label, "OpenVINO:CPU");
        let cpu_input = TensorData::new(cpu_model.inputs[0].clone(), &parity_input_bytes())
            .expect("CPU 输入构造");
        let cpu_out = cpu_model
            .run(std::slice::from_ref(&cpu_input), 64 << 20)
            .expect("OV 原生 CPU 推理");
        assert_eq!(gpu_out.len(), cpu_out.len(), "GPU/CPU 输出张量数不一致");
        for (index, (gpu, cpu)) in gpu_out.iter().zip(&cpu_out).enumerate() {
            assert_eq!(gpu.info.shape, cpu.info.shape, "输出{index} shape 不一致");
            let left = gpu.floats().expect("GPU 输出应为 f32");
            let right = cpu.floats().expect("CPU 输出应为 f32");
            assert_eq!(left.len(), right.len());
            // 分统计量对拍：OV Intel GPU 插件对 f32 模型做隐式 FP16 精度变换（官方
            // 默认行为），GPU/CPU 漂移类 ~1e-3——稳健统计量（mean/std）按 1e-2 门，
            // min/max 在随机尾部分布上对近零值极敏感（相对差被 1e-6 地板放大）按 5e-2。
            // 实测（2026-11-15，Arc 140T/OV 2025.0.0）：min=1.4e-2 max=1.7e-4
            // mean=3.2e-3 std=2.0e-3。
            let [min_rel, max_rel, mean_rel, std_rel] = moment_rels(left, right);
            println!(
                "gpu-parity output[{index}] elements={} min={min_rel:.3e} max={max_rel:.3e} mean={mean_rel:.3e} std={std_rel:.3e}",
                left.len()
            );
            assert!(
                mean_rel <= 1.0e-2 && std_rel <= 1.0e-2,
                "输出{index} GPU/CPU 稳健统计量超差 mean={mean_rel:.3e} std={std_rel:.3e}"
            );
            assert!(
                min_rel <= 5.0e-2 && max_rel <= 5.0e-2,
                "输出{index} GPU/CPU 极值统计量超差 min={min_rel:.3e} max={max_rel:.3e}"
            );
        }
        println!(
            "ovgpu gate: device_label={} backend={} outputs={}",
            gpu_model.device_label,
            gpu_model.actual_backend,
            gpu_out.len()
        );
    }

    /// GPU 定向不可达真机门：needle 命中 DXGI 独显（本机 NVIDIA，OV GPU 域仅 Intel）
    /// → 候选失败 stage=gpu_device（厂商核对拒绝，不静默换 Intel 卡）→ 回退 ORT CPU
    /// 真实推理；strict 单候选正确报错不回退。
    #[test]
    #[ignore = "GPU 定向不可达真机门：AM_SDK_OPENVINO_DIR 同上 + AM_RT_ORT_LIBRARY/AM_RT_ORT_GPU_LIBRARY（回退臂）"]
    fn gpu_selector_unreachable_adapter_fails_candidate_and_falls_back() {
        let _env = env_lock();
        restore_ov_dir();
        ensure_ort_ready();
        let source = ModelSource::Path(Path::new(PARITY_MODEL));
        let mut model = Model::load_selected(source, None, &gpu_policy("rtx"), 2)
            .expect("不可达定向应回退 ORT CPU");
        assert_eq!(model.actual_backend, 0, "应回退 ORT CPU");
        assert!(model.fallback);
        let report = model.selection_report();
        assert_eq!(report.attempts.len(), 2);
        let gpu = &report.attempts[0];
        assert_eq!(gpu.target.backend, 4);
        assert_eq!(gpu.stage, "gpu_device");
        assert_eq!(
            gpu.error.as_deref(),
            Some("vision.backendStrictUnavailable")
        );
        assert_eq!(report.attempts[1].target.backend, 0);
        assert_eq!(report.attempts[1].stage, "session_created");
        let bytes = vec![0u8; byte_count(model.inputs[0].dtype, &model.inputs[0].shape).unwrap()];
        let input = TensorData::new(model.inputs[0].clone(), &bytes).expect("输入构造");
        model
            .run(std::slice::from_ref(&input), 64 << 20)
            .expect("回退后的 ORT CPU 模型应可真实推理");
        let mut strict = gpu_policy("rtx");
        strict.priority.truncate(1);
        strict.allow_fallback = false;
        let failure = match Model::load_selected(source, None, &strict, 2) {
            Err(failure) => failure,
            Ok(_) => panic!("strict 不可达定向应报错"),
        };
        assert_eq!(failure.error.0, "vision.backendStrictUnavailable");
        assert_eq!(failure.report.attempts.len(), 1);
        assert_eq!(failure.report.attempts[0].stage, "gpu_device");
        println!(
            "ovgpu unreachable: fallback=ort-cpu strict-error=vision.backendStrictUnavailable report={}",
            serde_json::to_string(model.selection_report()).unwrap()
        );
    }

    /// F07 失败注入：port_info 三个端口查询任意失败必须释放 port（RAII 守卫）。
    /// 假符号表按字段注入（OvApi 裸函数指针表）；port_info 不消费的符号一律
    /// panic stub——被调即说明路径走偏。释放计数用 thread_local（测试线程天然隔离）。
    mod port_release_injection {
        use super::ov_sys::{
            ov_available_devices_t, ov_compiled_model_t, ov_core_t, ov_infer_request_t, ov_model_t,
            ov_shape_t, ov_tensor_t, ov_version_t, OvApi, OV_OK, OV_TYPE_F32,
        };
        use super::*;

        thread_local! {
            static PORT_FREES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
        }

        fn port_frees() -> usize {
            PORT_FREES.with(|cell| cell.get())
        }

        static STUB_DIMS: [i64; 4] = [1, 3, 640, 640];
        static STUB_NAME: &[u8; 7] = b"images\0";
        /// 注入失败码（任意非 OV_OK；断言经 native_code 回显）。
        const FAIL_CODE: i32 = 9;

        unsafe extern "C" fn shape_ok(
            _port: *const ov_output_const_port_t,
            shape: *mut ov_shape_t,
        ) -> i32 {
            *shape = ov_shape_t {
                rank: 4,
                dims: STUB_DIMS.as_ptr() as *mut i64,
            };
            OV_OK
        }
        unsafe extern "C" fn shape_fail(
            _port: *const ov_output_const_port_t,
            _shape: *mut ov_shape_t,
        ) -> i32 {
            FAIL_CODE
        }
        unsafe extern "C" fn element_ok(
            _port: *const ov_output_const_port_t,
            element: *mut ov_element_type_e,
        ) -> i32 {
            *element = OV_TYPE_F32;
            OV_OK
        }
        unsafe extern "C" fn element_fail(
            _port: *const ov_output_const_port_t,
            _element: *mut ov_element_type_e,
        ) -> i32 {
            FAIL_CODE
        }
        unsafe extern "C" fn name_ok(
            _port: *const ov_output_const_port_t,
            name: *mut *mut c_char,
        ) -> i32 {
            *name = STUB_NAME.as_ptr() as *mut c_char;
            OV_OK
        }
        unsafe extern "C" fn name_fail(
            _port: *const ov_output_const_port_t,
            _name: *mut *mut c_char,
        ) -> i32 {
            FAIL_CODE
        }
        unsafe extern "C" fn shape_free_noop(_shape: *mut ov_shape_t) -> i32 {
            OV_OK
        }
        unsafe extern "C" fn char_free_noop(_text: *const c_char) {}
        unsafe extern "C" fn port_free_count(_port: *mut ov_output_const_port_t) {
            PORT_FREES.with(|cell| cell.set(cell.get() + 1));
        }

        macro_rules! panic_stub_v {
            ($name:ident($($arg:ident: $ty:ty),* $(,)?)) => {
                unsafe extern "C" fn $name($($arg: $ty),*) {
                    panic!("port_info 注入测试不应调用 {}", stringify!($name));
                }
            };
        }
        macro_rules! panic_stub_r {
            ($name:ident($($arg:ident: $ty:ty),* $(,)?) -> $ret:ty) => {
                unsafe extern "C" fn $name($($arg: $ty),*) -> $ret {
                    panic!("port_info 注入测试不应调用 {}", stringify!($name));
                }
            };
        }

        panic_stub_r!(stub_get_openvino_version(_v: *mut ov_version_t) -> i32);
        panic_stub_v!(stub_version_free(_v: *mut ov_version_t));
        panic_stub_r!(stub_core_create(_c: *mut *mut ov_core_t) -> i32);
        panic_stub_v!(stub_core_free(_c: *mut ov_core_t));
        panic_stub_r!(stub_read_model(
            _core: *const ov_core_t,
            _bytes: *const c_char,
            _len: usize,
            _weights: *const ov_tensor_t,
            _model: *mut *mut ov_model_t,
        ) -> i32);
        panic_stub_r!(stub_compile_model(
            _core: *const ov_core_t,
            _model: *const ov_model_t,
            _device: *const c_char,
            _props: usize,
            _compiled: *mut *mut ov_compiled_model_t,
        ) -> i32);
        panic_stub_v!(stub_compiled_model_free(_c: *mut ov_compiled_model_t));
        panic_stub_r!(stub_create_infer_request(
            _c: *const ov_compiled_model_t,
            _r: *mut *mut ov_infer_request_t,
        ) -> i32);
        panic_stub_r!(stub_inputs_size(_c: *const ov_compiled_model_t, _n: *mut usize) -> i32);
        panic_stub_r!(stub_input_by_index(
            _c: *const ov_compiled_model_t,
            _i: usize,
            _p: *mut *mut ov_output_const_port_t,
        ) -> i32);
        panic_stub_r!(stub_outputs_size(_c: *const ov_compiled_model_t, _n: *mut usize) -> i32);
        panic_stub_r!(stub_output_by_index(
            _c: *const ov_compiled_model_t,
            _i: usize,
            _p: *mut *mut ov_output_const_port_t,
        ) -> i32);
        panic_stub_v!(stub_infer_request_free(_r: *mut ov_infer_request_t));
        panic_stub_r!(stub_set_input_tensor(
            _r: *mut ov_infer_request_t,
            _t: *const ov_tensor_t,
        ) -> i32);
        panic_stub_r!(stub_infer(_r: *mut ov_infer_request_t) -> i32);
        panic_stub_r!(stub_get_output_tensor(
            _r: *const ov_infer_request_t,
            _i: usize,
            _t: *mut *mut ov_tensor_t,
        ) -> i32);
        panic_stub_v!(stub_tensor_free(_t: *mut ov_tensor_t));
        panic_stub_r!(stub_tensor_create(
            _e: ov_element_type_e,
            _s: ov_shape_t,
            _t: *mut *mut ov_tensor_t,
        ) -> i32);
        panic_stub_r!(stub_tensor_data(_t: *const ov_tensor_t, _d: *mut *mut c_void) -> i32);
        panic_stub_r!(stub_tensor_get_shape(
            _t: *const ov_tensor_t,
            _s: *mut ov_shape_t,
        ) -> i32);
        panic_stub_r!(stub_tensor_get_byte_size(
            _t: *const ov_tensor_t,
            _n: *mut usize,
        ) -> i32);
        panic_stub_r!(stub_tensor_get_element_type(
            _t: *const ov_tensor_t,
            _e: *mut ov_element_type_e,
        ) -> i32);
        panic_stub_v!(stub_model_free(_m: *mut ov_model_t));
        panic_stub_r!(stub_available_devices(
            _c: *const ov_core_t,
            _d: *mut ov_available_devices_t,
        ) -> i32);
        panic_stub_v!(stub_available_devices_free(_d: *mut ov_available_devices_t));
        panic_stub_r!(stub_core_get_property(
            _c: *const ov_core_t,
            _k: *const c_char,
            _n: *const c_char,
            _v: *mut *mut c_char,
        ) -> i32);
        panic_stub_r!(stub_get_last_err_msg() -> *const c_char);

        pub(super) fn stub_api(
            shape_ok_path: bool,
            element_ok_path: bool,
            name_ok_path: bool,
        ) -> OvApi {
            OvApi {
                get_openvino_version: stub_get_openvino_version,
                version_free: stub_version_free,
                core_create: stub_core_create,
                core_free: stub_core_free,
                read_model_from_memory_buffer: stub_read_model,
                // compile_model 为 C variadic 签名，Rust 稳定版无法声明 variadic
                // stub：经 transmute 桥接（panic stub 永不被调，仅满足指针类型）。
                compile_model: {
                    #[allow(clippy::missing_transmute_annotations)]
                    let bridged = unsafe {
                        std::mem::transmute::<
                            unsafe extern "C" fn(
                                *const ov_core_t,
                                *const ov_model_t,
                                *const c_char,
                                usize,
                                *mut *mut ov_compiled_model_t,
                            ) -> i32,
                            _,
                        >(stub_compile_model)
                    };
                    bridged
                },
                compiled_model_free: stub_compiled_model_free,
                compiled_model_create_infer_request: stub_create_infer_request,
                compiled_model_inputs_size: stub_inputs_size,
                compiled_model_input_by_index: stub_input_by_index,
                compiled_model_outputs_size: stub_outputs_size,
                compiled_model_output_by_index: stub_output_by_index,
                infer_request_free: stub_infer_request_free,
                infer_request_set_input_tensor: stub_set_input_tensor,
                infer_request_infer: stub_infer,
                infer_request_get_output_tensor_by_index: stub_get_output_tensor,
                tensor_free: stub_tensor_free,
                tensor_create: stub_tensor_create,
                tensor_data: stub_tensor_data,
                tensor_get_shape: stub_tensor_get_shape,
                tensor_get_byte_size: stub_tensor_get_byte_size,
                tensor_get_element_type: stub_tensor_get_element_type,
                shape_free: shape_free_noop,
                model_free: stub_model_free,
                const_port_get_shape: if shape_ok_path { shape_ok } else { shape_fail },
                port_get_element_type: if element_ok_path {
                    element_ok
                } else {
                    element_fail
                },
                port_get_any_name: if name_ok_path { name_ok } else { name_fail },
                output_const_port_free: port_free_count,
                free: char_free_noop,
                core_get_available_devices: stub_available_devices,
                available_devices_free: stub_available_devices_free,
                core_get_property: stub_core_get_property,
                get_last_err_msg: stub_get_last_err_msg,
                core_create_context: None,
                compile_model_with_context: None,
                remote_context_create_tensor: None,
                remote_context_get_params: None,
                remote_tensor_get_params: None,
                remote_context_free: None,
                version: String::new(),
                element_line: ElementLine::Current,
            }
        }

        /// remote 族全 Some 桩（variadic 签名 Rust 稳定版无法直接声明 stub：
        /// 零参 panic stub 经 transmute 桥接；presence 检查只读 is_some 永不调用）。
        pub(super) fn stub_api_remote_present() -> OvApi {
            macro_rules! variadic_panic_stub {
                ($name:ident) => {{
                    unsafe extern "C" fn $name() {
                        panic!("remote presence 桩不应被调用");
                    }
                    #[allow(clippy::missing_transmute_annotations)]
                    let bridged =
                        unsafe { std::mem::transmute::<unsafe extern "C" fn(), _>($name) };
                    bridged
                }};
            }
            let mut api = stub_api(true, true, true);
            api.core_create_context = Some(variadic_panic_stub!(stub_ov_create_context));
            api.compile_model_with_context = Some(variadic_panic_stub!(stub_ov_compile_ctx));
            api.remote_context_create_tensor = Some(variadic_panic_stub!(stub_ov_remote_tensor));
            api.remote_context_get_params = Some(variadic_panic_stub!(stub_ov_ctx_params));
            api.remote_tensor_get_params = Some(variadic_panic_stub!(stub_ov_tensor_params));
            api.remote_context_free = Some(variadic_panic_stub!(stub_ov_ctx_free));
            api
        }

        fn sentinel_port() -> *mut ov_output_const_port_t {
            // 非空哨兵：注入 stub 不解引用，仅计数释放。
            0x1234 as *mut ov_output_const_port_t
        }

        #[test]
        fn port_info_frees_port_when_shape_query_fails() {
            let api = stub_api(false, true, true);
            let failure = unsafe { port_info(&api, sentinel_port(), "model_session") }
                .expect_err("shape 查询注入失败应返回候选失败");
            assert_eq!(failure.error.0, "vision.backendStrictUnavailable");
            assert_eq!(failure.stage, "model_session");
            assert_eq!(failure.native_code, Some(FAIL_CODE));
            assert_eq!(port_frees(), 1, "查询失败提前返回也必须释放 port");
        }

        #[test]
        fn port_info_frees_port_when_element_query_fails() {
            let api = stub_api(true, false, true);
            let failure = unsafe { port_info(&api, sentinel_port(), "model_session") }
                .expect_err("element 查询注入失败应返回候选失败");
            assert_eq!(failure.error.0, "vision.backendStrictUnavailable");
            assert_eq!(failure.native_code, Some(FAIL_CODE));
            assert_eq!(port_frees(), 1, "查询失败提前返回也必须释放 port");
        }

        #[test]
        fn port_info_frees_port_when_name_query_fails() {
            let api = stub_api(true, true, false);
            let failure = unsafe { port_info(&api, sentinel_port(), "model_session") }
                .expect_err("name 查询注入失败应返回候选失败");
            assert_eq!(failure.error.0, "vision.backendStrictUnavailable");
            assert_eq!(failure.native_code, Some(FAIL_CODE));
            assert_eq!(port_frees(), 1, "查询失败提前返回也必须释放 port");
        }

        #[test]
        fn port_info_frees_port_exactly_once_on_success() {
            let api = stub_api(true, true, true);
            let info = unsafe { port_info(&api, sentinel_port(), "model_session") }
                .expect("全成功路径应产出端口规格");
            assert_eq!(info.name, "images");
            assert_eq!(info.shape, vec![1, 3, 640, 640]);
            assert_eq!(info.dtype, 0);
            assert_eq!(port_frees(), 1, "成功路径统一走守卫释放，恰一次");
        }
    }

    /// F06 真机门：输出预算在 SDK 复制之前检查。三形态：零预算 / 仅差一个对齐
    /// 字节 / 多输出累加边界。「复制前 vs 复制后」的时序差异由实现结构保证
    /// （检查位于 TensorData::new 之前），本门锚定的是预算边界语义与错误键。
    #[test]
    #[ignore = "真实 OpenVINO 运行库与模型（AM_SDK_OPENVINO_DIR）；F06 输出预算时序门"]
    fn output_budget_boundaries_reject_before_copy() {
        let _env = env_lock();
        restore_ov_dir();
        let bytes = std::fs::read(PARITY_MODEL).expect("模型文件缺失");
        let api = match OvApi::load() {
            Ok(api) => api,
            Err(failure) => panic!("OpenVINO 运行库装载失败: {}", failure.reason),
        };
        let engine = unsafe { build(api.clone(), &bytes, CompileDevice::Cpu) }
            .unwrap_or_else(|failure| panic!("引擎构造失败 stage={}", failure.stage));
        let len = crate::sdk::byte_count(engine.inputs[0].dtype, &engine.inputs[0].shape).unwrap();
        let input = TensorData::new(engine.inputs[0].clone(), &vec![0u8; len]).unwrap();
        let outputs = engine
            .infer(std::slice::from_ref(&input), usize::MAX)
            .expect("充足预算基线推理");
        assert!(!outputs.is_empty());
        let allocs: Vec<usize> = outputs
            .iter()
            .map(|output| output.bytes().len().next_multiple_of(8))
            .collect();
        assert!(allocs.iter().all(|a| *a > 0), "输出分配额非正：{allocs:?}");
        println!("budget-gate outputs={} allocs={allocs:?}", outputs.len());

        // 形态1：零预算——第一个输出前即耗尽。
        let error = match engine.infer(std::slice::from_ref(&input), 0) {
            Err(error) => error,
            Ok(_) => panic!("零预算必须耗尽"),
        };
        assert_eq!(error.error.0, "sdk.resourceExhausted");
        assert!(error.native_ok);

        // 形态2：仅差一个对齐字节（alloc-1 拒；单输出模型恰好 alloc 应通过）。
        let error = match engine.infer(std::slice::from_ref(&input), allocs[0] - 1) {
            Err(error) => error,
            Ok(_) => panic!("差一个对齐字节必须耗尽"),
        };
        assert_eq!(error.error.0, "sdk.resourceExhausted");
        if outputs.len() == 1 {
            engine
                .infer(std::slice::from_ref(&input), allocs[0])
                .expect("单输出模型恰好预算应通过");
        }

        // 形态3：多输出累加——总量差一字节拒；恰好总量过；仅够前一输出时
        // 在下一输出拒绝（修复点：不再越额复制完整输出后才报）。
        if outputs.len() >= 2 {
            let total: usize = allocs.iter().sum();
            let error = match engine.infer(std::slice::from_ref(&input), total - 1) {
                Err(error) => error,
                Ok(_) => panic!("总量差一字节必须耗尽"),
            };
            assert_eq!(error.error.0, "sdk.resourceExhausted");
            let delivered = engine
                .infer(std::slice::from_ref(&input), total)
                .expect("恰好全部输出预算应通过");
            assert_eq!(delivered.len(), outputs.len());
            let error = match engine.infer(std::slice::from_ref(&input), allocs[0]) {
                Err(error) => error,
                Ok(_) => panic!("仅够前一输出时后续输出必须耗尽"),
            };
            assert_eq!(error.error.0, "sdk.resourceExhausted");
        } else {
            println!(
                "单输出模型：多输出累加形态不适用（outputs={}）",
                outputs.len()
            );
        }
    }
}
