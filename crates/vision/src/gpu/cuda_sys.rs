//! CUDA 运行库最小 FFI（M2b 3.3）：互操作五入口 + 设备初始化，显式加载。
//! 布局与错误码依据 CUDA 13 运行库（cudart64_13.dll）；升级大版本须复核。
// transmute 从裸符号地址到具体函数签名：按名解析后类型由字段声明固定。
#![allow(clippy::missing_safety_doc)]
use std::ffi::c_void;
use std::sync::Arc;

pub type CudaResult = i32;

/// cudaGraphicsResource_t / cudaArray_t 均为不透明指针。
pub type GraphicsResource = *mut c_void;
pub type CudaArray = *mut c_void;

const CUDA_SUCCESS: CudaResult = 0;
/// cudaGraphicsRegisterFlagsNone（只读采样默认）。
pub const REGISTER_DEFAULT: u32 = 0;

/// 已解析符号表；裸函数指针天然线程安全（ffmpeg_sys 同惯例）。
pub struct CudaApi {
    pub get_device_count: unsafe extern "C" fn(*mut i32) -> CudaResult,
    pub set_device: unsafe extern "C" fn(i32) -> CudaResult,
    pub get_error_string: unsafe extern "C" fn(CudaResult) -> *const u8,
    pub register_d3d11: unsafe extern "C" fn(*mut GraphicsResource, *mut c_void, u32) -> CudaResult,
    pub unregister: unsafe extern "C" fn(GraphicsResource) -> CudaResult,
    pub map_resources:
        unsafe extern "C" fn(i32, *const GraphicsResource, *mut c_void) -> CudaResult,
    pub unmap_resources:
        unsafe extern "C" fn(i32, *const GraphicsResource, *mut c_void) -> CudaResult,
    pub mapped_array:
        unsafe extern "C" fn(*mut CudaArray, GraphicsResource, u32, u32) -> CudaResult,
}

impl CudaApi {
    /// 显式加载 cudart64_*.dll：优先 AM_SDK_CUDA_DIR，其次 ort 运行库目录
    /// AM_SDK_ORT_DIR（互操作入口由 GPU 版 ORT 同款 CUDA 运行库提供）。
    pub fn load() -> Result<Arc<Self>, &'static str> {
        let candidates: Vec<std::path::PathBuf> = ["AM_SDK_CUDA_DIR", "AM_SDK_ORT_DIR"]
            .iter()
            .filter_map(|key| std::env::var_os(key).map(std::path::PathBuf::from))
            .collect();
        let mut last = "未配置 CUDA 运行库目录";
        for dir in &candidates {
            for name in ["cudart64_13.dll", "cudart64_12.dll"] {
                let path = dir.join(name);
                if !path.is_file() {
                    continue;
                }
                match unsafe { libloading::Library::new(&path) } {
                    Ok(library) => return unsafe { Ok(Arc::new(Self::resolve(library)?)) },
                    Err(_) => last = "cudart 加载失败",
                }
            }
        }
        Err(last)
    }

    /// 符号存在性校验装载；库句柄由静态池保活（Library drop 即卸载）。
    #[allow(clippy::missing_transmute_annotations)]
    unsafe fn resolve(library: libloading::Library) -> Result<Self, &'static str> {
        let sym = |name: &[u8]| -> Result<*mut c_void, &'static str> {
            unsafe { library.get(name).map(|s| *s).map_err(|_| "CUDA 符号缺失") }
        };
        // transmute 源类型标注为 *mut c_void（符号地址），目标由字段声明固定。
        #[allow(clippy::missing_transmute_annotations)]
        macro_rules! cast {
            ($name:expr) => {
                unsafe { std::mem::transmute::<*mut c_void, _>(sym($name)?) }
            };
        }
        let api = Self {
            get_device_count: cast!(b"cudaGetDeviceCount"),
            set_device: cast!(b"cudaSetDevice"),
            get_error_string: cast!(b"cudaGetErrorString"),
            register_d3d11: cast!(b"cudaGraphicsD3D11RegisterResource"),
            unregister: cast!(b"cudaGraphicsUnregisterResource"),
            map_resources: cast!(b"cudaGraphicsMapResources"),
            unmap_resources: cast!(b"cudaGraphicsUnmapResources"),
            mapped_array: cast!(b"cudaGraphicsSubResourceGetMappedArray"),
        };
        // 库保活：泄漏一个持有句柄的 guard（进程生命周期，与 FFmpeg 运行库同策略）。
        std::mem::forget(library);
        Ok(api)
    }

    pub fn check(&self, code: CudaResult, context: &str) -> Result<(), crate::Error> {
        if code == CUDA_SUCCESS {
            return Ok(());
        }
        let text = unsafe {
            let ptr = (self.get_error_string)(code);
            if ptr.is_null() {
                "unknown"
            } else {
                std::ffi::CStr::from_ptr(ptr.cast())
                    .to_str()
                    .unwrap_or("unknown")
            }
        };
        Err(crate::Error(format!("CUDA {context} 失败({code}): {text}")))
    }
}
