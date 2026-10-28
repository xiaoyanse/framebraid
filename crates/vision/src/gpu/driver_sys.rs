//! CUDA driver API 最小 FFI（M2b 3.4）：模块 JIT 加载/kernel launch/显存管理。
//! nvcuda.dll 由 NVIDIA 驱动提供（系统加载）；PTX 经 cuModuleLoadData JIT。
use std::ffi::c_void;
use std::sync::Arc;

pub type CuResult = i32;
const CUDA_SUCCESS: CuResult = 0;

/// kernel 参数块（cuLaunchKernel 的 void**）。
pub struct KernelParams {
    slots: Vec<usize>,
}

impl Default for KernelParams {
    fn default() -> Self {
        Self::new()
    }
}

impl KernelParams {
    pub fn new() -> Self {
        Self { slots: Vec::new() }
    }
    pub fn push_u32(&mut self, value: u32) {
        self.slots.push(value as usize);
    }
    pub fn push_u64(&mut self, value: u64) {
        self.slots.push(value as usize);
    }
    /// 组装参数指针数组：每个槽是值的地址。
    pub fn pointers(&self) -> Vec<*const c_void> {
        self.slots
            .iter()
            .map(|s| s as *const _ as *const c_void)
            .collect()
    }
}

pub struct DriverApi {
    pub cu_init: unsafe extern "C" fn(u32) -> CuResult,
    pub device_get: unsafe extern "C" fn(*mut i32, i32) -> CuResult,
    pub ctx_create: unsafe extern "C" fn(*mut *mut c_void, u32, i32) -> CuResult,
    pub ctx_set_current: unsafe extern "C" fn(*mut c_void) -> CuResult,
    pub ctx_synchronize: unsafe extern "C" fn() -> CuResult,
    pub module_load_data: unsafe extern "C" fn(*mut *mut c_void, *const c_void) -> CuResult,
    pub module_get_function:
        unsafe extern "C" fn(*mut *mut c_void, *mut c_void, *const u8) -> CuResult,
    pub launch_kernel: unsafe extern "C" fn(
        *mut c_void,
        u32,
        u32,
        u32,
        u32,
        u32,
        u32,
        u32,
        *mut c_void,
        *mut *mut c_void,
        *mut c_void,
    ) -> CuResult,
    pub mem_alloc: unsafe extern "C" fn(*mut *mut c_void, usize) -> CuResult,
    pub mem_free: unsafe extern "C" fn(*mut c_void) -> CuResult,
    pub memcpy_dtoh: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> CuResult,
    pub memcpy_htod: unsafe extern "C" fn(*mut c_void, *const c_void, usize) -> CuResult,
}

impl DriverApi {
    /// 初始化并建立（或复位）driver 当前上下文；后续 driver 调用须同线程或显式
    /// 重新 set。与 python 探针验证路径一致：cuInit → cuCtxCreate。
    pub fn establish_context(self: &Arc<Self>) -> Result<*mut c_void, crate::Error> {
        self.check(unsafe { (self.cu_init)(0) }, "cuInit")?;
        let mut device: i32 = 0;
        self.check(unsafe { (self.device_get)(&mut device, 0) }, "cuDeviceGet")?;
        let mut context: *mut c_void = std::ptr::null_mut();
        self.check(
            unsafe { (self.ctx_create)(&mut context, 0, device) },
            "cuCtxCreate",
        )?;
        Ok(context)
    }
    #[allow(clippy::missing_transmute_annotations)]
    pub fn load() -> Result<Arc<Self>, &'static str> {
        let library =
            unsafe { libloading::Library::new("nvcuda.dll").map_err(|_| "nvcuda 加载失败")? };
        macro_rules! cast {
            ($name:expr) => {
                unsafe { std::mem::transmute::<*mut c_void, _>(sym($name)?) }
            };
        }
        fn sym(name: &[u8]) -> Result<*mut c_void, &'static str> {
            unsafe {
                // nvcuda 进程内常驻（驱动加载），符号查后库句柄保活。
                let library = libloading::Library::new("nvcuda.dll").map_err(|_| "nvcuda")?;
                let address = library.get(name).map(|s| *s).map_err(|_| "驱动符号缺失")?;
                std::mem::forget(library);
                Ok(address)
            }
        }
        let api = Self {
            cu_init: cast!(b"cuInit\0"),
            device_get: cast!(b"cuDeviceGet\0"),
            ctx_create: cast!(b"cuCtxCreate\0"),
            ctx_set_current: cast!(b"cuCtxSetCurrent\0"),
            ctx_synchronize: cast!(b"cuCtxSynchronize\0"),
            module_load_data: cast!(b"cuModuleLoadData\0"),
            module_get_function: cast!(b"cuModuleGetFunction\0"),
            launch_kernel: cast!(b"cuLaunchKernel\0"),
            // 本机驱动上 _v2 符号在纯 driver context 下报 INVALID_CONTEXT（2026-10-29
            // 实测：同 context 非 v2 符号成功）；与探针验证一致使用旧符号。
            mem_alloc: cast!(b"cuMemAlloc\0"),
            mem_free: cast!(b"cuMemFree\0"),
            memcpy_dtoh: cast!(b"cuMemcpyDtoH\0"),
            memcpy_htod: cast!(b"cuMemcpyHtoD\0"),
        };
        std::mem::forget(library);
        Ok(Arc::new(api))
    }

    pub fn check(&self, code: CuResult, context: &str) -> Result<(), crate::Error> {
        if code == CUDA_SUCCESS {
            Ok(())
        } else {
            Err(crate::Error(format!("CUDA驱动 {context} 失败({code})")))
        }
    }
}

/// JIT 加载 PTX 文本并取 kernel 句柄。
pub struct KernelModule {
    pub api: Arc<DriverApi>,
    /// CUmodule 句柄：function 的生命周期依赖它存活（进程内保活，不显式卸载）。
    #[allow(dead_code)]
    module: *mut c_void,
    function: *mut c_void,
}

impl KernelModule {
    pub fn load_nv12_letterbox(api: Arc<DriverApi>) -> Result<Self, crate::Error> {
        const PTX: &str = include_str!("nv12_letterbox.ptx");
        let mut module: *mut c_void = std::ptr::null_mut();
        // PTX 文本需 NUL 结尾。
        let text = format!("{PTX}\0");
        unsafe {
            api.check(
                (api.module_load_data)(&mut module, text.as_ptr().cast()),
                "模块JIT加载",
            )?
        };
        let mut function: *mut c_void = std::ptr::null_mut();
        let name = b"nv12_letterbox\0";
        unsafe {
            api.check(
                (api.module_get_function)(&mut function, module, name.as_ptr()),
                "kernel句柄获取",
            )?
        };
        Ok(Self {
            api,
            module,
            function,
        })
    }

    /// 启动 NV12 letterbox kernel；grid 覆盖 dst 全像素（block 16x16）。
    /// 参数与 PTX 入口一一对应（内核入口签名决定，非 SDK 公开接口）。
    #[allow(clippy::too_many_arguments)]
    pub fn launch(
        &self,
        y_ptr: u64,
        uv_ptr: u64,
        src_w: u32,
        src_h: u32,
        dst_w: u32,
        dst_h: u32,
        rw: u32,
        rh: u32,
        out_ptr: u64,
    ) -> Result<(), crate::Error> {
        let mut params = KernelParams::new();
        params.push_u64(y_ptr);
        params.push_u64(uv_ptr);
        params.push_u32(src_w);
        params.push_u32(src_h);
        params.push_u32(dst_w);
        params.push_u32(dst_h);
        params.push_u32(rw);
        params.push_u32(rh);
        params.push_u64(out_ptr);
        let mut pointers: Vec<*mut c_void> = params
            .pointers()
            .into_iter()
            .map(|p| p as *mut c_void)
            .collect();
        let blocks_x = dst_w.div_ceil(16);
        let blocks_y = dst_h.div_ceil(16);
        let code = unsafe {
            (self.api.launch_kernel)(
                self.function,
                blocks_x,
                blocks_y,
                1,
                16,
                16,
                1,
                0,
                std::ptr::null_mut(),
                pointers.as_mut_ptr(),
                std::ptr::null_mut(),
            )
        };
        self.api.check(code, "kernel启动")
    }
}

impl Drop for KernelModule {
    fn drop(&mut self) {
        // 模块卸载省略：进程生命周期内复用，cuModuleUnload 非必需。
    }
}
