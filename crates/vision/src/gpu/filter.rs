//! GPU 侧交付筛选（ABI 1.9 §3.6a device_filter）：检测头阈值计数 kernel
//! （filter.ptx，手写 PTX——工具链先例同 bgra_letterbox）。驻留形态下零目标
//! 帧只回读 4 字节计数张量（不回读输出本体）；谓词与宿主解码器首段逐字对齐
//! （严格大于 argmax + `best <= conf 跳过`），计数口径为 NMS 前候选数——驻留
//! 形态不执行实例级解码（合同 §3.6a），detected_count 即筛选后候选交付元数据。
use super::cuda_sys::CudaApi;
use super::driver_sys::{DriverApi, KernelParams};
use std::ffi::c_void;
use std::sync::{Arc, OnceLock};

/// JIT 加载的筛选模块（进程级缓存：首次调用加载，失败原因一并缓存——
/// CUDA 不可用进程内不重复试探）。句柄创建后只读，cuLaunchKernel 线程安全。
pub struct FilterModule {
    api: Arc<DriverApi>,
    /// CUmodule 句柄保活（function 依赖其存活；进程生命周期不卸载，同
    /// KernelModule 惯例）。
    #[allow(dead_code)]
    module: *mut c_void,
    function: *mut c_void,
}
unsafe impl Send for FilterModule {}
unsafe impl Sync for FilterModule {}

impl FilterModule {
    fn load(api: Arc<DriverApi>) -> Result<Self, crate::Error> {
        const PTX: &str = include_str!("filter.ptx");
        let mut module: *mut c_void = std::ptr::null_mut();
        let text = format!("{PTX}\0");
        unsafe {
            api.check(
                (api.module_load_data)(&mut module, text.as_ptr().cast()),
                "筛选模块JIT加载",
            )?
        };
        let mut function: *mut c_void = std::ptr::null_mut();
        let name = b"filter_count\0";
        unsafe {
            api.check(
                (api.module_get_function)(&mut function, module, name.as_ptr()),
                "筛选kernel句柄获取",
            )?
        };
        Ok(Self {
            api,
            module,
            function,
        })
    }
}

static FILTER: OnceLock<Result<FilterModule, String>> = OnceLock::new();

fn filter_module() -> Result<&'static FilterModule, crate::Error> {
    let cached = FILTER.get_or_init(|| {
        let api = DriverApi::load().map_err(|e| e.to_string())?;
        FilterModule::load(api).map_err(|e| e.0)
    });
    cached
        .as_ref()
        .map_err(|e| crate::Error(format!("CUDA筛选kernel不可用|{e}")))
}

/// 检测头筛选计数（§3.6a）：`head` 为驻留检测头张量设备地址；`rows`=
/// 4+classes（row-major 寻址）；`transposed`=B1 布局声明；`class_sel`<0 为
/// 全类（-1 语义同档案惯例），否则要求 argmax 类命中；`min_confidence`
/// 严格小于才计数（与宿主解码同式）。计数张量在本函数内分配/清零/回读/
/// 释放，调用方只拿到结果标量。
pub fn count_head(
    head: u64,
    anchors: u32,
    rows: u32,
    classes: u32,
    transposed: bool,
    class_sel: i32,
    min_confidence: f32,
    ordinal: u32,
) -> Result<u32, crate::Error> {
    let module = filter_module()?;
    let runtime = CudaApi::load().map_err(|e| crate::Error(format!("cudart不可用|{e}")))?;
    runtime.check(
        unsafe { (runtime.set_device)(ordinal.min(i32::MAX as u32) as i32) },
        "设备激活",
    )?;
    let mut counter: *mut c_void = std::ptr::null_mut();
    runtime.check(unsafe { (runtime.malloc)(&mut counter, 4) }, "计数缓冲分配")?;
    let zero: u32 = 0;
    let outcome = (|| {
        runtime.check(
            unsafe { (runtime.memcpy_htod)(counter, &zero as *const u32 as *const c_void, 4, 1) },
            "计数清零",
        )?;
        let mut params = KernelParams::new();
        params.push_u64(head);
        params.push_u64(counter as u64);
        params.push_u32(anchors);
        params.push_u32(rows);
        params.push_u32(classes);
        params.push_u32(u32::from(transposed));
        params.push_u32(class_sel as u32);
        params.push_f32(min_confidence);
        let mut pointers: Vec<*mut c_void> = params
            .pointers()
            .into_iter()
            .map(|p| p as *mut c_void)
            .collect();
        let blocks = anchors.div_ceil(256);
        let code = unsafe {
            (module.api.launch_kernel)(
                module.function,
                blocks,
                1,
                1,
                256,
                1,
                1,
                0,
                std::ptr::null_mut(),
                pointers.as_mut_ptr(),
                std::ptr::null_mut(),
            )
        };
        module.api.check(code, "筛选kernel启动")?;
        // null-stream launch 与 ORT EP 内部 stream 跨流：设备级同步排干全部
        // 在途工作后再回读（readout 路径同款前置）。
        runtime.check(unsafe { (runtime.device_synchronize)() }, "设备同步")?;
        let mut count: u32 = 0;
        runtime.check(
            unsafe { (runtime.memcpy_dtoh)(&mut count as *mut u32 as *mut c_void, counter, 4, 2) },
            "计数回读",
        )?;
        Ok(count)
    })();
    let _free = unsafe { (runtime.free)(counter) };
    outcome
}
