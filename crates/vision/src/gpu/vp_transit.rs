//! D3D11 VideoProcessor 平面转换中转（P1 UV 修复，2026-11-20）。
//!
//! 背景（实验定谳，`uv-transit-probe`/`nv12-array-probe` 双探针在档）：CUDA 对
//! NV12 数组纹理的互操作映射为**扁平 slice 口径且仅暴露 luma 平面**（runtime+
//! driver 双 API 实证）；本机 NVIDIA 驱动（621.14/CUDA 13.5）上
//! CopySubresourceRegion 平面拆拷（NV12→R8/R8G8 六种口径形式）与 cast SRV
//! 平面视图（R8G8/R8 恒指平面 0）均不可用——UV 平面在 D3D/CUDA 两侧都无
//! 直接寻址路径。唯一零 host 路径：**VideoProcessorBlt 把 NV12 slice 转为
//! B8G8R8A8 中转纹理**（标准格式 CUDA 完全支持映射，BT.601 limited 定点转换+
//! 色度上采样在 VP 内完成），kernel 消费 BGRA（RGB 域重采样，与 CPU 参考
//! 像素路径同域）。
//!
//! 注册纪律（探针 PRODSEQ 定界，2026-11-20）：**VP Blt 目标不注册 CUDA**——
//! 图形队列读已注册纹理而 VP Blt（视频队列）尚在队列时，本驱动直接设备移除
//! （0x887a0005）。注册面为独立整帧暂存纹理：VP 写未注册 output → 图形队列
//! 整帧拷 output→暂存（兼作 blt 完成序：EVENT 查询只覆盖图形队列，拷贝的
//! 跨队列依赖把 Blt 纳入查询屏障）→ CUDA 映射暂存。视频队列永不触碰注册面。
//!
//! 槽位依据 Windows SDK 10.0.26100 d3d11.h 逐项核对（与本仓既有核对链一致）：
//! 设备（IUnknown 直承）CreateTexture2D=5/CreateRenderTargetView=9/
//! CreateQuery=24/GetImmediateContext=40；ID3D11VideoDevice（QI）：
//! CreateVideoProcessor=4/CreateVideoProcessorInputView=8/
//! CreateVideoProcessorOutputView=9/CreateVideoProcessorEnumerator=10；
//! ID3D11VideoContext（QI 自立即上下文）：SetStreamColorSpace=28（指针参）/
//! SetStreamAutoProcessingMode=37/VideoProcessorBlt=53（实证）；
//! 上下文 Flush=111/End=28/GetData=29/CopySubresourceRegion=46（ov_remote.rs
//! 先例同源）。SetOutputColorSpace 不调用（槽 15 归属未实证，默认输出即 RGB）。
use super::cuda_sys::CudaApi;
use super::interop::{Com, RegisteredTexture};
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Arc, Mutex};

type Hresult = i32;

const IID_ID3D11_VIDEO_DEVICE: Guid = Guid {
    a: 0x10EC_4D5B,
    b: 0x975A,
    c: 0x4689,
    d: [0xB9, 0xE4, 0xD0, 0xAA, 0xC3, 0x0F, 0xE3, 0x33],
};

const IID_ID3D11_VIDEO_CONTEXT: Guid = Guid {
    a: 0x61F2_1C45,
    b: 0x3C0E,
    c: 0x4A74,
    d: [0x9C, 0xEA, 0x67, 0x10, 0x0D, 0x9A, 0xD5, 0xE4],
};

#[repr(C)]
struct Guid {
    a: u32,
    b: u16,
    c: u16,
    d: [u8; 8],
}

#[repr(C)]
struct ContentDesc {
    input_frame_format: u32,
    input_rate_num: u32,
    input_rate_den: u32,
    input_width: u32,
    input_height: u32,
    output_rate_num: u32,
    output_rate_den: u32,
    output_width: u32,
    output_height: u32,
    usage: u32,
}

/// 头文件序：FourCC(0)/ViewDimension(4)/union{MipSlice,ArraySlice}(8)。
#[repr(C)]
struct VpivDesc {
    four_cc: u32,
    dimension: u32,
    mip_slice: u32,
    array_slice: u32,
}

/// 头文件序：ViewDimension(0)/union{MipSlice,FirstArraySlice,ArraySize}(4)。
#[repr(C)]
struct VpovDesc {
    dimension: u32,
    mip_slice: u32,
    first_array_slice: u32,
    array_size: u32,
}

/// D3D11_VIDEO_PROCESSOR_STREAM（72B；头文件序 ppPastSurfaces 先于
/// pInputSurface——槽位错配即驱动空指针崩溃，探针实证）。
#[repr(C)]
struct VpStream {
    enable: u32,
    output_index: u32,
    input_frame_or_field: u32,
    past_frames: u32,
    future_frames: u32,
    pad: u32,
    pp_past: *mut c_void,
    p_input_surface: *mut c_void,
    pp_future: *mut c_void,
    pp_past_right: *mut c_void,
    p_input_right: *mut c_void,
    pp_future_right: *mut c_void,
}

#[repr(C)]
struct QueryDesc {
    query: u32, // D3D11_QUERY_EVENT
    misc: u32,
}

unsafe fn vtable(object: *mut c_void) -> *const *const c_void {
    unsafe { *object.cast::<*const *const c_void>() }
}

unsafe fn qi(object: *mut c_void, iid: &Guid) -> Result<Com, crate::Error> {
    let query_interface: unsafe extern "system" fn(
        *mut c_void,
        *const Guid,
        *mut *mut c_void,
    ) -> Hresult = unsafe { std::mem::transmute(*vtable(object).add(0)) };
    let mut out: *mut c_void = std::ptr::null_mut();
    let rc = unsafe { query_interface(object, iid, &mut out) };
    if rc < 0 || out.is_null() {
        return Err(crate::Error(format!(
            "vision.gpuUnavailable|VideoProcessor 接口查询失败 hr={rc:#x}"
        )));
    }
    Ok(Com(out))
}

/// 纹理→所属 D3D11 设备（ID3D11DeviceChild::GetDevice 槽 3；借用指针，
/// AddRef 保活由条目持有）。ov_remote.rs:145 同源。
unsafe fn device_of_texture(texture: *mut c_void) -> Result<*mut c_void, crate::Error> {
    if texture.is_null() {
        return Err(crate::Error("vision.gpuFrameInvalid".into()));
    }
    let get_device: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) =
        unsafe { std::mem::transmute(*vtable(texture).add(3)) };
    let mut device: *mut c_void = std::ptr::null_mut();
    unsafe { get_device(texture, &mut device) };
    if device.is_null() {
        return Err(crate::Error(
            "vision.gpuUnavailable|GPU 帧 GetDevice 为空".into(),
        ));
    }
    Ok(device)
}

/// 立即上下文（借用指针归设备所有，不 AddRef；ov_remote.rs:232 同源）。
unsafe fn immediate_context(device: *mut c_void) -> Result<*mut c_void, crate::Error> {
    let get_ctx: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) =
        unsafe { std::mem::transmute(*vtable(device).add(40)) };
    let mut context: *mut c_void = std::ptr::null_mut();
    unsafe { get_ctx(device, &mut context) };
    if context.is_null() {
        return Err(crate::Error(
            "vision.gpuUnavailable|GetImmediateContext 为空".into(),
        ));
    }
    Ok(context)
}

/// GPU 完成同步：EVENT 查询 End+GetData 自旋（ov_remote.rs:286 同源——VP Blt
/// 完成对 CUDA 映射可见的显式同步点）。
unsafe fn wait_gpu_idle(device: *mut c_void, context: *mut c_void) -> Result<(), crate::Error> {
    let create_query: unsafe extern "system" fn(
        *mut c_void,
        *const QueryDesc,
        *mut *mut c_void,
    ) -> Hresult = unsafe { std::mem::transmute(*vtable(device).add(24)) };
    let desc = QueryDesc { query: 0, misc: 0 };
    let mut query: *mut c_void = std::ptr::null_mut();
    let rc = unsafe { create_query(device, &desc, &mut query) };
    if rc < 0 || query.is_null() {
        return Err(crate::Error(format!(
            "vision.inferenceFailed|VP 同步查询创建失败 hr={rc:#x}"
        )));
    }
    let end: unsafe extern "system" fn(*mut c_void, *mut c_void) =
        unsafe { std::mem::transmute(*vtable(context).add(28)) };
    let get_data: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        *mut c_void,
        u32,
        u32,
    ) -> Hresult = unsafe { std::mem::transmute(*vtable(context).add(29)) };
    let flush: unsafe extern "system" fn(*mut c_void) =
        unsafe { std::mem::transmute(*vtable(context).add(111)) };
    unsafe {
        // 先显式提交再 End/GetData（VP Blt 经 video context 提交，EVENT 查询
        // 对其完成态有竞态——Flush 强制提交；探针 VPFENCE/PRODSEQ 实证序）。
        flush(context);
        end(context, query);
    }
    let mut spins = 0u32;
    loop {
        let hr = unsafe { get_data(context, query, std::ptr::null_mut(), 0, 0) };
        if hr == 0 {
            release(query);
            return Ok(());
        }
        if hr != 1 {
            release(query);
            return Err(crate::Error(format!(
                "vision.inferenceFailed|VP GPU 同步查询失败 hr={hr:#x}"
            )));
        }
        spins = spins.saturating_add(1);
        if spins > 120_000_000 {
            release(query);
            return Err(crate::Error(
                "vision.inferenceFailed|VP GPU 同步超时（10s 量级）".into(),
            ));
        }
        if spins.is_multiple_of(1024) {
            std::thread::yield_now();
        }
    }
}

unsafe fn release(ptr: *mut c_void) {
    let rel: unsafe extern "system" fn(*mut c_void) -> u32 =
        unsafe { std::mem::transmute(*vtable(ptr).add(2)) };
    unsafe { rel(ptr) };
}

/// 一个（D3D 设备×帧尺寸）的中转条目：VP 对象、VP 专属 BGRA 输出纹理（**不
/// 注册**）、CUDA 注册的整帧暂存纹理（见 transit 注释）与条目级串行锁（多
/// lane 共用同尺寸条目时 blt→copy→map→搬运原子化）。
pub(crate) struct TransitEntry {
    _device: Com,
    video_device: Com,
    video_context: Com,
    enumerator: Com,
    processor: Com,
    /// VP Blt 目标（未注册——VPOV 持有期间不销毁）。
    #[allow(dead_code)]
    output: Com,
    output_view: Com,
    /// CUDA 注册的整帧暂存纹理 D3D 句柄（图形队列拷贝的落点）。
    staging: Com,
    /// 暂存纹理的 CUDA 注册（映射源；保活引用共享 staging）。
    texture: RegisteredTexture,
    lock: Mutex<()>,
}

unsafe impl Send for TransitEntry {}
unsafe impl Sync for TransitEntry {}

/// 条目上限：设备级 8 个（设备×尺寸组合；8 路 lane 常态单尺寸=1 条，超限逐出
/// 最旧——registry.rs 16 条先例同策略）。
const MAX_ENTRIES: usize = 8;

/// 进程内 VP 中转池（GpuPipeline 成员；按 D3D 设备身份+帧尺寸键控）。
pub(crate) struct TransitPool {
    entries: HashMap<(usize, u32, u32), Arc<TransitEntry>>,
    order: std::collections::VecDeque<(usize, u32, u32)>,
}

impl TransitPool {
    pub(crate) fn new() -> Self {
        Self {
            entries: HashMap::new(),
            order: std::collections::VecDeque::new(),
        }
    }

    /// 显式关闭路径清理（pipeline::release_for_shutdown 调用；条目 Drop =
    /// CUDA unregister → COM Release，调用方线程执行，registry.drain 同语义）。
    pub(crate) fn drain(&mut self) {
        let entries = std::mem::take(&mut self.entries);
        self.order.clear();
        for (_, entry) in entries {
            drop(entry);
        }
    }

    /// 取（或建）条目并完成一帧转换：VP Blt NV12 slice→BGRA + GPU 同步 +
    /// CUDA 映射。返回（条目引用， 映射的 BGRA CUDA array）——调用方完成
    /// D2D 搬运后 drop 映射守卫（计数归零 unmap）。
    pub(crate) fn acquire(
        &mut self,
        api: &Arc<CudaApi>,
        frame: &super::pipeline::GpuFrameData,
    ) -> Result<(Arc<TransitEntry>, super::interop::MappedArray<'static>), crate::Error> {
        let device = unsafe { device_of_texture(frame.texture)? };
        let key = (device as usize, frame.width, frame.height);
        if let Some(entry) = self.entries.get(&key) {
            self.order.retain(|k| k != &key);
            self.order.push_back(key);
            let mapped = Self::transit(entry, frame)?;
            return Ok((entry.clone(), mapped));
        }
        let entry = Arc::new(unsafe { build_entry(api, device, frame.width, frame.height)? });
        self.entries.insert(key, entry.clone());
        self.order.push_back(key);
        while self.entries.len() > MAX_ENTRIES {
            if let Some(old) = self.order.pop_front() {
                self.entries.remove(&old);
            }
        }
        let mapped = Self::transit(&entry, frame)?;
        Ok((entry.clone(), mapped))
    }

    /// 帧转换（条目锁内）：VPIV 每帧创建（解码纹理随租约轮换，缓存键空间
    /// 不稳定；VP 视图创建为轻量内核对象，探针实测顺序复用无异常）→ Blt →
    /// GPU 同步 → CUDA map idx0。
    fn transit(
        entry: &TransitEntry,
        frame: &super::pipeline::GpuFrameData,
    ) -> Result<super::interop::MappedArray<'static>, crate::Error> {
        let _guard = entry.lock.lock().unwrap_or_else(|e| e.into_inner());
        unsafe {
            let vpiv = Com(create_input_view(
                entry.video_device.0,
                frame.texture,
                entry.enumerator.0,
                frame.subresource,
            )?);
            let mut stream = std::mem::zeroed::<VpStream>();
            stream.enable = 1;
            stream.p_input_surface = vpiv.0;
            let blt: unsafe extern "system" fn(
                *mut c_void,
                *mut c_void,
                *mut c_void,
                u32,
                u32,
                *const VpStream,
            ) -> Hresult = std::mem::transmute(*vtable(entry.video_context.0).add(53));
            let rc = blt(
                entry.video_context.0,
                entry.processor.0,
                entry.output_view.0,
                0,
                1,
                &stream,
            );
            if rc < 0 {
                return Err(crate::Error(format!(
                    "vision.inferenceFailed|VideoProcessorBlt 失败 hr={rc:#x}"
                )));
            }
            // 完成序+暂存合一（探针 PRODSEQ/PROD_OPT4 定界，2026-11-20）：
            // EVENT 查询对本驱动（621.14）的 VP Blt 不构成屏障（视频队列与
            // 图形队列分离，查询只覆盖图形队列）；且**图形队列读已注册纹理而
            // VP Blt 尚在队列**的组合会让本驱动设备移除（0x887a0005）。解法：
            // VP 只写未注册的 output；图形队列整帧拷 output→已注册暂存纹理
            // ——该拷贝对 Blt 输出建立跨队列依赖（未注册面安全），EVENT 查询
            // 完成即拷贝与 Blt 均完成；CUDA 只映射暂存纹理（视频队列永不触碰
            // 注册面）。
            let context = immediate_context(entry._device.0)?;
            copy_region(context, entry.staging.0, 0, entry.output.0, 0);
            wait_gpu_idle(entry._device.0, context)?;
        }
        // CUDA 映射（'static 生命周期契约见 map_static 注释）
        unsafe { map_static(&entry.texture) }
    }
}

/// 建条目：QI 视频接口→枚举器/处理器→BGRA 输出纹理+输出视图→色彩空间/
/// 自动处理设置→CUDA 注册（条目生命周期内保活）。
unsafe fn build_entry(
    api: &Arc<CudaApi>,
    device: *mut c_void,
    width: u32,
    height: u32,
) -> Result<TransitEntry, crate::Error> {
    let video_device = unsafe { qi(device, &IID_ID3D11_VIDEO_DEVICE)? };
    let context = unsafe { immediate_context(device)? };
    let video_context = unsafe { qi(context, &IID_ID3D11_VIDEO_CONTEXT)? };
    // 设备引用保活（条目生命周期内宿主可释放其设备子对象）
    let keepalive = unsafe { super::interop::com_add_ref(device) };
    let device = Com(device);
    let _ = keepalive;
    let content = ContentDesc {
        input_frame_format: 0, // PROGRESSIVE
        input_rate_num: 30,
        input_rate_den: 1,
        input_width: width,
        input_height: height,
        output_rate_num: 30,
        output_rate_den: 1,
        output_width: width,
        output_height: height,
        usage: 0, // OPTIMAL_SPEED
    };
    let create_enum: unsafe extern "system" fn(
        *mut c_void,
        *const ContentDesc,
        *mut *mut c_void,
    ) -> Hresult = unsafe { std::mem::transmute(*vtable(video_device.0).add(10)) };
    let mut enumerator: *mut c_void = std::ptr::null_mut();
    let rc = unsafe { create_enum(video_device.0, &content, &mut enumerator) };
    if rc < 0 || enumerator.is_null() {
        return Err(crate::Error(format!(
            "vision.gpuUnavailable|CreateVideoProcessorEnumerator 失败 hr={rc:#x}"
        )));
    }
    let enumerator = Com(enumerator);
    let create_vp: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        u32,
        *mut *mut c_void,
    ) -> Hresult = unsafe { std::mem::transmute(*vtable(video_device.0).add(4)) };
    let mut processor: *mut c_void = std::ptr::null_mut();
    let rc = unsafe { create_vp(video_device.0, enumerator.0, 0, &mut processor) };
    if rc < 0 || processor.is_null() {
        return Err(crate::Error(format!(
            "vision.gpuUnavailable|CreateVideoProcessor 失败 hr={rc:#x}"
        )));
    }
    let processor = Com(processor);
    // BGRA 输出纹理（RTV 绑定；CUDA 完全支持映射——探针 R8G8MAP/VPBLIT 实证）
    let desc = super::interop::TexDesc {
        width,
        height,
        mip_levels: 1,
        array_size: 1,
        format: 87, // DXGI_FORMAT_B8G8R8A8_UNORM
        sample_count: 1,
        sample_quality: 0,
        usage: 0,
        bind_flags: 0x20 | 0x8, // RENDER_TARGET | SHADER_RESOURCE
        cpu_access: 0,
        misc_flags: 0,
    };
    let output = unsafe { create_texture(device.0, &desc)? };
    let vpov_desc = VpovDesc {
        dimension: 1, // TEXTURE2D
        mip_slice: 0,
        first_array_slice: 0,
        array_size: 1,
    };
    let create_vpov: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        *mut c_void,
        *const VpovDesc,
        *mut *mut c_void,
    ) -> Hresult = unsafe { std::mem::transmute(*vtable(video_device.0).add(9)) };
    let mut output_view: *mut c_void = std::ptr::null_mut();
    let rc = unsafe {
        create_vpov(
            video_device.0,
            output.0,
            enumerator.0,
            &vpov_desc,
            &mut output_view,
        )
    };
    if rc < 0 || output_view.is_null() {
        return Err(crate::Error(format!(
            "vision.gpuUnavailable|CreateVideoProcessorOutputView 失败 hr={rc:#x}"
        )));
    }
    let output_view = Com(output_view);
    // 流色彩空间（legacy {Usage} 指针参）+流自动处理禁用（画质增强会改动
    // 像素——探针 VP2 定界：auto-off 后样点字节级吻合 BT.601 full）。输出色
    // 彩空间不设：默认即标准 RGB（探针全臂未设且颜色已验证），且槽 15 的
    // 精确归属未在探针实证过——多打一个未验证槽位曾致首次 blt 提交崩溃。
    let usage: u32 = 1;
    let set_stream_cs: unsafe extern "system" fn(*mut c_void, *mut c_void, u32, *const u32) =
        unsafe { std::mem::transmute(*vtable(video_context.0).add(28)) };
    set_stream_cs(video_context.0, processor.0, 0, &usage);
    let set_auto: unsafe extern "system" fn(*mut c_void, *mut c_void, u32, u32) =
        unsafe { std::mem::transmute(*vtable(video_context.0).add(37)) };
    set_auto(video_context.0, processor.0, 0, 1);
    // CUDA 注册的整帧暂存纹理（VP 专属 output 不注册——注册面避开视频队列，
    // 见 transit 注释；探针 PROD_OPT4 实证）
    let staging_desc = super::interop::TexDesc {
        width,
        height,
        mip_levels: 1,
        array_size: 1,
        format: 87, // B8G8R8A8_UNORM
        sample_count: 1,
        sample_quality: 0,
        usage: 0,
        bind_flags: 0x8, // SHADER_RESOURCE
        cpu_access: 0,
        misc_flags: 0,
    };
    let staging = unsafe { create_texture(device.0, &staging_desc)? };
    // CUDA 注册（COM AddRef 保活+unregister 先于 Release——interop.rs 语义）
    let texture = unsafe { RegisteredTexture::register_keepalive(api.clone(), staging.0)? };
    Ok(TransitEntry {
        _device: device,
        video_device,
        video_context,
        processor,
        enumerator,
        output,
        output_view,
        staging,
        texture,
        lock: Mutex::new(()),
    })
}

/// VPIV（每帧；FourCC=0 → 2D 形态 + ArraySlice=扁平 slice）。
unsafe fn create_input_view(
    video_device: *mut c_void,
    texture: *mut c_void,
    enumerator: *mut c_void,
    slice: u32,
) -> Result<*mut c_void, crate::Error> {
    let desc = VpivDesc {
        four_cc: 0,
        dimension: 1, // TEXTURE2D
        mip_slice: 0,
        array_slice: slice,
    };
    let create_vpiv: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        *mut c_void,
        *const VpivDesc,
        *mut *mut c_void,
    ) -> Hresult = unsafe { std::mem::transmute(*vtable(video_device).add(8)) };
    let mut view: *mut c_void = std::ptr::null_mut();
    let rc = unsafe { create_vpiv(video_device, texture, enumerator, &desc, &mut view) };
    if rc < 0 || view.is_null() {
        return Err(crate::Error(format!(
            "vision.inferenceFailed|CreateVideoProcessorInputView(slice={slice}) 失败 hr={rc:#x}"
        )));
    }
    Ok(view)
}

unsafe fn create_texture(
    device: *mut c_void,
    desc: &super::interop::TexDesc,
) -> Result<Com, crate::Error> {
    let create_tex2d: unsafe extern "system" fn(
        *mut c_void,
        *const super::interop::TexDesc,
        *const c_void,
        *mut *mut c_void,
    ) -> Hresult = unsafe { std::mem::transmute(*vtable(device).add(5)) };
    let mut texture: *mut c_void = std::ptr::null_mut();
    let rc = unsafe { create_tex2d(device, desc, std::ptr::null(), &mut texture) };
    if rc < 0 || texture.is_null() {
        return Err(crate::Error(format!(
            "vision.gpuUnavailable|BGRA 中转纹理创建失败 hr={rc:#x}"
        )));
    }
    Ok(Com(texture))
}

/// CUDA 映射为 'static：MappedArray<'a> 的 Drop 只做计数递减（unmap 走
/// resource 句柄，不触碰 RegisteredTexture 借用），条目由池/调用方 Arc
/// 保活到映射 drop 之后，生命周期背书成立（Safety 由 TransitPool::acquire
/// 的返回契约约束：调用方必须同时持有条目 Arc 直到映射 drop）。
unsafe fn map_static(
    texture: &RegisteredTexture,
) -> Result<super::interop::MappedArray<'static>, crate::Error> {
    let mapped = texture.map_subresource(0)?;
    Ok(unsafe {
        std::mem::transmute::<super::interop::MappedArray<'_>, super::interop::MappedArray<'static>>(
            mapped,
        )
    })
}

// ---- 测试辅助：NV12 字节 → D3D 纹理 slice 装载（生产纹理来自解码器）。
// P1 修复后 CUDA 无法写 NV12 的 UV 平面（仅 luma 可映射），测试图案必须走
// D3D：staging（array=1，创建期初始数据——实验 D20 大数组+初始数据 SIGSEGV
// 坑规避：初始数据仅用于单 slice staging）→ CopySubresourceRegion 整 slice
// （扁平口径，RELAY 臂字节闭环先例）→ GPU 完成同步。

#[repr(C)]
struct SubresourceData {
    sys_mem: *const c_void,
    pitch: usize,
    slice_pitch: usize,
}

/// NV12 字节装载进纹理的指定 slice（尺寸取自 nv12 长度推断需 width/height
/// 显式）。ov_remote.rs upload_nv12 同参先例（array=1+两平面初始数据）。
///
/// # Safety
/// `texture` 为有效 NV12 纹理（ArraySize > slice），借用期内有效。
pub unsafe fn upload_nv12_slice(
    texture: *mut c_void,
    slice: u32,
    nv12: &[u8],
    width: u32,
    height: u32,
) -> Result<(), crate::Error> {
    let expected = (width as usize) * (height as usize) * 3 / 2;
    if nv12.len() != expected {
        return Err(crate::Error("vision.gpuFrameInvalid|图案长度不符".into()));
    }
    let device = unsafe { device_of_texture(texture)? };
    let context = unsafe { immediate_context(device)? };
    let desc = super::interop::TexDesc {
        width,
        height,
        mip_levels: 1,
        array_size: 1,
        format: 103, // NV12
        sample_count: 1,
        sample_quality: 0,
        usage: 0,
        bind_flags: 0x200 | 0x8, // DECODER | SHADER_RESOURCE（ov_remote 同参）
        cpu_access: 0,
        misc_flags: 0x2, // MISC_SHARED（探针 staging 同参）
    };
    let initial = [
        SubresourceData {
            sys_mem: nv12.as_ptr().cast(),
            pitch: width as usize,
            slice_pitch: 0,
        },
        SubresourceData {
            sys_mem: unsafe { nv12.as_ptr().add(width as usize * height as usize).cast() },
            pitch: width as usize,
            slice_pitch: 0,
        },
    ];
    let staging = unsafe { create_texture_init(device, &desc, &initial)? };
    unsafe {
        copy_region(context, texture, slice, staging.0, 0);
        wait_gpu_idle(device, context)?;
    }
    Ok(())
}

unsafe fn create_texture_init(
    device: *mut c_void,
    desc: &super::interop::TexDesc,
    initial: &[SubresourceData],
) -> Result<Com, crate::Error> {
    let create_tex2d: unsafe extern "system" fn(
        *mut c_void,
        *const super::interop::TexDesc,
        *const c_void,
        *mut *mut c_void,
    ) -> Hresult = unsafe { std::mem::transmute(*vtable(device).add(5)) };
    let mut texture: *mut c_void = std::ptr::null_mut();
    let rc = unsafe { create_tex2d(device, desc, initial.as_ptr().cast(), &mut texture) };
    if rc < 0 || texture.is_null() {
        return Err(crate::Error(format!(
            "vision.gpuFrameInvalid|staging 纹理创建失败 hr={rc:#x}"
        )));
    }
    Ok(Com(texture))
}

/// CopySubresourceRegion（上下文槽 46；box=null 整子资源——扁平 slice 口径；
/// 调用方保证 dst 与 src 子资源同尺寸）。
unsafe fn copy_region(
    context: *mut c_void,
    dst: *mut c_void,
    dst_sub: u32,
    src: *mut c_void,
    src_sub: u32,
) {
    let copy: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        u32,
        u32,
        u32,
        u32,
        *mut c_void,
        u32,
        *const c_void,
    ) = unsafe { std::mem::transmute(*vtable(context).add(46)) };
    unsafe {
        copy(
            context,
            dst,
            dst_sub,
            0,
            0,
            0,
            src,
            src_sub,
            std::ptr::null(),
        )
    };
}
