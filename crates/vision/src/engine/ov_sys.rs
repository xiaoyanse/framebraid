//! OpenVINO C API 最小 FFI：显式加载 openvino_c 运行库，符号存在性校验后逐名解析。
//! 签名依据 vcpkg openvino 头文件（openvino/c/*.h）；升级运行库大版本须复核。
#![allow(non_camel_case_types)]
use std::ffi::{c_char, c_void, CStr};
use std::path::Path;
use std::sync::Arc;

pub type ov_status_e = i32;
pub type ov_element_type_e = u32;
pub const OV_OK: ov_status_e = 0;

/// ov_common.h 元素类型序数：仅收录可映射到 TensorData 合同的五类。
pub const OV_TYPE_F16: ov_element_type_e = 3;
pub const OV_TYPE_F32: ov_element_type_e = 4;
pub const OV_TYPE_I32: ov_element_type_e = 9;
pub const OV_TYPE_I64: ov_element_type_e = 10;
pub const OV_TYPE_U8: ov_element_type_e = 16;
/// 仅测试引用：STRING 张量不在 TensorData 合同内，map_element_type 按 `_` 拒收。
#[cfg(test)]
pub const OV_TYPE_STRING: ov_element_type_e = 23;

pub enum ov_core_t {}
pub enum ov_model_t {}
pub enum ov_compiled_model_t {}
pub enum ov_infer_request_t {}
pub enum ov_tensor_t {}
pub enum ov_output_const_port_t {}

/// ov_shape.h：按值传参；API 返回的 shape 须 ov_shape_free，自建 shape 由局部 Vec 撑住。
#[derive(Clone, Copy)]
#[repr(C)]
pub struct ov_shape_t {
    pub rank: i64,
    pub dims: *mut i64,
}

/// ov_core.h ov_version_t：字符串归 OpenVINO 所有，经 ov_version_free 释放。
#[repr(C)]
pub struct ov_version_t {
    pub build_number: *const c_char,
    pub description: *const c_char,
}

/// 已解析符号表；裸函数指针天然线程安全（cuda_sys 同惯例）。
pub struct OvApi {
    pub get_openvino_version: unsafe extern "C" fn(*mut ov_version_t) -> ov_status_e,
    pub version_free: unsafe extern "C" fn(*mut ov_version_t),
    pub core_create: unsafe extern "C" fn(*mut *mut ov_core_t) -> ov_status_e,
    pub core_free: unsafe extern "C" fn(*mut ov_core_t),
    pub read_model_from_memory_buffer: unsafe extern "C" fn(
        *const ov_core_t,
        *const c_char,
        usize,
        *const ov_tensor_t,
        *mut *mut ov_model_t,
    ) -> ov_status_e,
    pub compile_model: unsafe extern "C" fn(
        *const ov_core_t,
        *const ov_model_t,
        *const c_char,
        usize,
        *mut *mut ov_compiled_model_t,
        ...
    ) -> ov_status_e,
    pub compiled_model_free: unsafe extern "C" fn(*mut ov_compiled_model_t),
    pub compiled_model_create_infer_request: unsafe extern "C" fn(
        *const ov_compiled_model_t,
        *mut *mut ov_infer_request_t,
    ) -> ov_status_e,
    pub compiled_model_inputs_size:
        unsafe extern "C" fn(*const ov_compiled_model_t, *mut usize) -> ov_status_e,
    pub compiled_model_input_by_index: unsafe extern "C" fn(
        *const ov_compiled_model_t,
        usize,
        *mut *mut ov_output_const_port_t,
    ) -> ov_status_e,
    pub compiled_model_outputs_size:
        unsafe extern "C" fn(*const ov_compiled_model_t, *mut usize) -> ov_status_e,
    pub compiled_model_output_by_index: unsafe extern "C" fn(
        *const ov_compiled_model_t,
        usize,
        *mut *mut ov_output_const_port_t,
    ) -> ov_status_e,
    pub infer_request_free: unsafe extern "C" fn(*mut ov_infer_request_t),
    pub infer_request_set_input_tensor:
        unsafe extern "C" fn(*mut ov_infer_request_t, *const ov_tensor_t) -> ov_status_e,
    pub infer_request_infer: unsafe extern "C" fn(*mut ov_infer_request_t) -> ov_status_e,
    pub infer_request_get_output_tensor_by_index: unsafe extern "C" fn(
        *const ov_infer_request_t,
        usize,
        *mut *mut ov_tensor_t,
    ) -> ov_status_e,
    pub tensor_free: unsafe extern "C" fn(*mut ov_tensor_t),
    pub tensor_create:
        unsafe extern "C" fn(ov_element_type_e, ov_shape_t, *mut *mut ov_tensor_t) -> ov_status_e,
    pub tensor_data: unsafe extern "C" fn(*const ov_tensor_t, *mut *mut c_void) -> ov_status_e,
    pub tensor_get_shape: unsafe extern "C" fn(*const ov_tensor_t, *mut ov_shape_t) -> ov_status_e,
    pub tensor_get_byte_size: unsafe extern "C" fn(*const ov_tensor_t, *mut usize) -> ov_status_e,
    pub tensor_get_element_type:
        unsafe extern "C" fn(*const ov_tensor_t, *mut ov_element_type_e) -> ov_status_e,
    pub shape_free: unsafe extern "C" fn(*mut ov_shape_t) -> ov_status_e,
    pub model_free: unsafe extern "C" fn(*mut ov_model_t),
    pub const_port_get_shape:
        unsafe extern "C" fn(*const ov_output_const_port_t, *mut ov_shape_t) -> ov_status_e,
    pub port_get_element_type:
        unsafe extern "C" fn(*const ov_output_const_port_t, *mut ov_element_type_e) -> ov_status_e,
    pub port_get_any_name:
        unsafe extern "C" fn(*const ov_output_const_port_t, *mut *mut c_char) -> ov_status_e,
    pub output_const_port_free: unsafe extern "C" fn(*mut ov_output_const_port_t),
    pub free: unsafe extern "C" fn(*const c_char),
    /// 诊断用：错误消息只进冒烟/联调日志，不进 selection 报告（同 ORT 键纪律）。
    #[allow(dead_code)]
    pub get_last_err_msg: unsafe extern "C" fn() -> *const c_char,
    /// 装载即取的运行库版本（诊断用：运行库冒烟/E1.6 对拍读，正常推理不读）。
    #[allow(dead_code)]
    version: String,
}

impl OvApi {
    /// 显式加载 openvino_c 运行库：目录取 AM_SDK_OPENVINO_DIR；缺目录/缺文件/
    /// 符号缺失都作为候选失败返回，由 selection 回退下一候选。
    pub fn load() -> Result<Arc<Self>, &'static str> {
        let dir = std::env::var_os("AM_SDK_OPENVINO_DIR").ok_or("未配置 OpenVINO 运行库目录")?;
        let path = Path::new(&dir).join(library_name());
        if !path.is_file() {
            return Err("openvino_c 运行库文件缺失");
        }
        let library = unsafe { open_library(&path) }.map_err(|_| "openvino_c 加载失败")?;
        unsafe { Self::resolve(library) }
    }
}

fn library_name() -> &'static str {
    if cfg!(windows) {
        "openvino_c.dll"
    } else if cfg!(target_os = "macos") {
        "libopenvino_c.dylib"
    } else {
        "libopenvino_c.so"
    }
}

#[cfg(windows)]
unsafe fn open_library(path: &Path) -> Result<libloading::Library, libloading::Error> {
    // LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR|DEFAULT_DIRS(0x1100)：openvino.dll、tbb12.dll
    // 与插件从 DLL 同目录解析；ORT preflight 同参（sdk/runtime/windows.rs 0x1100）。
    const SEARCH_DLL_LOAD_DIR: u32 = 0x100;
    const SEARCH_DEFAULT_DIRS: u32 = 0x1000;
    let inner = unsafe {
        libloading::os::windows::Library::load_with_flags(
            path,
            SEARCH_DLL_LOAD_DIR | SEARCH_DEFAULT_DIRS,
        )
    }?;
    Ok(inner.into())
}

#[cfg(not(windows))]
unsafe fn open_library(path: &Path) -> Result<libloading::Library, libloading::Error> {
    unsafe { libloading::Library::new(path) }
}

impl OvApi {
    /// 符号存在性校验装载；库句柄 forget 保活（进程生命周期，cuda_sys 同策略）。
    #[allow(clippy::missing_transmute_annotations)]
    unsafe fn resolve(library: libloading::Library) -> Result<Arc<Self>, &'static str> {
        let sym = |name: &[u8]| -> Result<*mut c_void, &'static str> {
            unsafe {
                library
                    .get(name)
                    .map(|s| *s)
                    .map_err(|_| "OpenVINO 符号缺失")
            }
        };
        // transmute 源类型标注为 *mut c_void（符号地址），目标由字段声明固定。
        macro_rules! cast {
            ($name:expr) => {
                unsafe { std::mem::transmute::<*mut c_void, _>(sym($name)?) }
            };
        }
        let mut api = Self {
            get_openvino_version: cast!(b"ov_get_openvino_version"),
            version_free: cast!(b"ov_version_free"),
            core_create: cast!(b"ov_core_create"),
            core_free: cast!(b"ov_core_free"),
            read_model_from_memory_buffer: cast!(b"ov_core_read_model_from_memory_buffer"),
            compile_model: cast!(b"ov_core_compile_model"),
            compiled_model_free: cast!(b"ov_compiled_model_free"),
            compiled_model_create_infer_request: cast!(b"ov_compiled_model_create_infer_request"),
            compiled_model_inputs_size: cast!(b"ov_compiled_model_inputs_size"),
            compiled_model_input_by_index: cast!(b"ov_compiled_model_input_by_index"),
            compiled_model_outputs_size: cast!(b"ov_compiled_model_outputs_size"),
            compiled_model_output_by_index: cast!(b"ov_compiled_model_output_by_index"),
            infer_request_free: cast!(b"ov_infer_request_free"),
            infer_request_set_input_tensor: cast!(b"ov_infer_request_set_input_tensor"),
            infer_request_infer: cast!(b"ov_infer_request_infer"),
            infer_request_get_output_tensor_by_index: cast!(
                b"ov_infer_request_get_output_tensor_by_index"
            ),
            tensor_free: cast!(b"ov_tensor_free"),
            tensor_create: cast!(b"ov_tensor_create"),
            tensor_data: cast!(b"ov_tensor_data"),
            tensor_get_shape: cast!(b"ov_tensor_get_shape"),
            tensor_get_byte_size: cast!(b"ov_tensor_get_byte_size"),
            tensor_get_element_type: cast!(b"ov_tensor_get_element_type"),
            shape_free: cast!(b"ov_shape_free"),
            model_free: cast!(b"ov_model_free"),
            const_port_get_shape: cast!(b"ov_const_port_get_shape"),
            port_get_element_type: cast!(b"ov_port_get_element_type"),
            port_get_any_name: cast!(b"ov_port_get_any_name"),
            output_const_port_free: cast!(b"ov_output_const_port_free"),
            free: cast!(b"ov_free"),
            get_last_err_msg: cast!(b"ov_get_last_err_msg"),
            version: String::new(),
        };
        api.version = api.read_version();
        std::mem::forget(library);
        Ok(Arc::new(api))
    }

    /// 诊断用途：装载即取 buildNumber，失败留空串不阻断引擎可用性。
    fn read_version(&self) -> String {
        unsafe {
            let mut info = ov_version_t {
                build_number: std::ptr::null(),
                description: std::ptr::null(),
            };
            if (self.get_openvino_version)(&mut info) != OV_OK || info.build_number.is_null() {
                return String::new();
            }
            let text = CStr::from_ptr(info.build_number)
                .to_string_lossy()
                .into_owned();
            (self.version_free)(&mut info);
            text
        }
    }

    /// 线程局部最近一次 ov 调用的错误消息；仅诊断读，不进 selection 报告（同 ORT 键纪律）。
    #[allow(dead_code)]
    pub fn last_error_message(&self) -> String {
        unsafe {
            let ptr = (self.get_last_err_msg)();
            if ptr.is_null() {
                return String::new();
            }
            CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    }
}
