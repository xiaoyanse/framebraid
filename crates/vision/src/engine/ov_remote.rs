//! OV remote 直传（ABI 1.8 §3.3a 设备域 OV 侧条款）：OV GPU 会话的 GPU 帧处理链——
//! D3D11 纹理→设备获取与厂商核对→VA_SHARED remote context→NV12 双平面 remote
//! 张量→OV preprocess 图（NV12→RGB→pad→resize→f32→scale）→compile_model_with_context
//! →绑定推理。纹理借用在接受段（capture 返回）闭合：帧内容先 CopySubresourceRegion
//! 进 SDK 自有 NV12 纹理并经事件查询同步（D3D11 写对 CL 可见的显式同步点，
//! 原型 sdk-ovproto-20261116 实证结论的工程化），此后推理只引用自有纹理；
//! 全程零 host 上传（搬运审计 copies 按零计，合同 L790-792）。
//!
//! 几何语义：detect 族 letterbox 以「pad 成方形画布（114 填边，帧居中）→resize
//! 到模型输入」表达——与像素路径 resize-then-pad 同比例同中心（边界差为 ≤1px
//! 的双线性混合带，与 CUDA 线 YUV 域重采样同类已登记偏差）；classify center
//! crop（F02）无 OV preprocess C 算子可表达，显式拒绝不降级。
//! OV scale 为除法语义（归一化传 255.0）、pad 传归一化前 114.0——两处原型实证
//! 陷阱，GRAPH_PARAMS 常量钉死并单测锁定。
#![allow(non_camel_case_types)]
use super::openvino::OpenvinoEngine;
use super::ov_sys::{
    ov_compiled_model_t, ov_infer_request_t, ov_layout_t, ov_model_t, ov_output_const_port_t,
    ov_remote_context_t, ov_shape_t, ov_tensor_t, ElementLine, OvApi, OV_COLOR_NV12_TWO_PLANES,
    OV_COLOR_RGB, OV_PAD_CONSTANT, OV_RESIZE_BILINEAR, OV_TYPE_U8_2025_0,
};
use crate::gpu::interop::{com_add_ref, Com};
use crate::sdk::{ImageTransform, TensorData};
use crate::{Error, Result};
use std::collections::HashMap;
use std::ffi::{c_void, CStr, CString};
use std::ptr::null_mut;
use std::sync::{Arc, Mutex};

/// preprocess 图构造常量（两坑钉死，单测锁定）：
/// scale 语义为**除以**（C++ `scale({255.})`=归一化；误传 1/255 会 ×255 喂模型，
/// FP16 溢出全 NaN——原型实测陷阱）；pad 值用归一化前的 114.0（pad 后除 255）。
pub(crate) const SCALE_DIVISOR: f32 = 255.0;
pub(crate) const PAD_VALUE: f32 = 114.0;

/// letterbox 几何（纯函数产物；图 pad 与解码变换共用同一套数字）。
pub(crate) struct LetterboxPads {
    /// NHWC 帧张量 H 轴 begin/end pad（行）。
    pub top: u32,
    pub bottom: u32,
    /// W 轴 begin/end pad（列）。
    pub left: u32,
    pub right: u32,
    /// 统一缩放比例（dst 边 / 画布边）。
    pub scale: f32,
    /// 帧内容在模型输入平面的落点（解码变换 left/top 用）。
    pub model_left: u32,
    pub model_top: u32,
}

/// letterbox pad 几何（与 sdk/prepare.rs 的 min 比例+中心同式；无 FFI，单测直测）：
/// 帧先 pad 成方形画布（114，帧居中）再 resize 到方形模型输入。非偶数维（NV12
/// UV 平面合同）或非方形目标返回 None（调用方显式拒绝，不静默改几何）。
pub(crate) fn letterbox_pads(
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
) -> Option<LetterboxPads> {
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
        return None;
    }
    if !src_w.is_multiple_of(2) || !src_h.is_multiple_of(2) || dst_w != dst_h {
        return None;
    }
    let side = src_w.max(src_h);
    let top = (side - src_h) / 2;
    let left = (side - src_w) / 2;
    Some(LetterboxPads {
        top,
        bottom: side - src_h - top,
        left,
        right: side - src_w - left,
        scale: dst_w as f32 / side as f32,
        model_left: dst_w * left / side,
        model_top: dst_h * top / side,
    })
}

/// 厂商核对词一致性（§3.5b 机制镜像，纯函数）：纹理适配器 vendor 核对词须出现
/// 在会话 OV 设备全名中；未知厂商无核对词——镜像装载期语义不做厂商筛（边界
/// 见 device.rs vendor_token）。
pub(crate) fn vendor_consistent(token: Option<&str>, device_full_name: &str) -> bool {
    match token {
        Some(token) => device_full_name
            .to_lowercase()
            .contains(&token.to_lowercase()),
        None => true,
    }
}

// ---- D3D11 最小面：裸 vtable 声明（零新增依赖）。
// 槽位依据 Windows SDK 10.0.26100 d3d11.h/dxgi.h 逐项核对（2026-11-19）：
// IUnknown QI=0/AddRef=1/Release=2；ID3D11DeviceChild::GetDevice=3；
// ID3D11Device CreateTexture2D=5/CreateQuery=24/GetImmediateContext=40；
// ID3D11DeviceContext End=28/GetData=29/CopySubresourceRegion=46；
// IDXGIDevice::GetAdapter=7（QI IID_IDXGIDevice）；IDXGIAdapter::GetDesc=8。
// DXGI_ADAPTER_DESC 布局：Description[128] wchar 后 offset 256 = VendorId。

const IID_IDXGI_DEVICE: Guid = Guid {
    a: 0x54EC_77FA,
    b: 0x1377,
    c: 0x44E6,
    d: [0x8C, 0x32, 0x88, 0xFD, 0x5F, 0x44, 0xC8, 0x4C],
};

#[repr(C)]
struct Guid {
    a: u32,
    b: u16,
    c: u16,
    d: [u8; 8],
}

#[repr(C)]
struct TexDesc {
    width: u32,
    height: u32,
    mip_levels: u32,
    array_size: u32,
    format: u32,
    sample_count: u32,
    sample_quality: u32,
    usage: u32,
    bind_flags: u32,
    cpu_access: u32,
    misc_flags: u32,
}

#[repr(C)]
struct QueryDesc {
    query: u32, // D3D11_QUERY_EVENT = 0（d3d11.h 枚举首位）
    misc: u32,
}

/// Win32 HRESULT（COM 调用返回码；>=0 成功惯例）。
type Hresult = i32;

unsafe fn vtable(object: *mut c_void) -> *const *const c_void {
    unsafe { *object.cast::<*const *const c_void>() }
}

/// 纹理→所属 D3D11 设备（ID3D11DeviceChild::GetDevice，槽 3）。
/// 返回借用指针：帧纹理借用期内设备必然存活（D3D11 子对象持设备引用）。
unsafe fn device_of_texture(texture: *mut c_void) -> Result<*mut c_void> {
    if texture.is_null() {
        return Err(Error("sdk.notSupported|ov remote: 纹理指针为空".into()));
    }
    let get_device: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) =
        unsafe { std::mem::transmute(*vtable(texture).add(3)) };
    let mut device: *mut c_void = null_mut();
    unsafe { get_device(texture, &mut device) };
    if device.is_null() {
        return Err(Error(
            "sdk.notSupported|ov remote: 纹理 GetDevice 为空".into(),
        ));
    }
    Ok(device)
}

/// 设备→适配器 VendorId（QI IDXGIDevice→GetAdapter→GetDesc；失败返回 None，
/// 厂商核对按未知厂商语义处理）。
unsafe fn adapter_vendor_of_device(device: *mut c_void) -> Option<u32> {
    let query_interface: unsafe extern "system" fn(
        *mut c_void,
        *const Guid,
        *mut *mut c_void,
    ) -> Hresult = unsafe { std::mem::transmute(*vtable(device).add(0)) };
    let mut dxgi_device: *mut c_void = null_mut();
    if unsafe { query_interface(device, &IID_IDXGI_DEVICE, &mut dxgi_device) } < 0
        || dxgi_device.is_null()
    {
        return None;
    }
    let get_adapter: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> Hresult =
        unsafe { std::mem::transmute(*vtable(dxgi_device).add(7)) };
    let mut adapter: *mut c_void = null_mut();
    let adapter_ok = unsafe { get_adapter(dxgi_device, &mut adapter) } >= 0 && !adapter.is_null();
    unsafe { com_release_raw(dxgi_device) };
    if !adapter_ok {
        return None;
    }
    let get_desc: unsafe extern "system" fn(*mut c_void, *mut u8) -> Hresult =
        unsafe { std::mem::transmute(*vtable(adapter).add(8)) };
    let mut desc = [0u8; 304];
    let ok = unsafe { get_desc(adapter, desc.as_mut_ptr()) } >= 0;
    unsafe { com_release_raw(adapter) };
    ok.then(|| u32::from_le_bytes(desc[256..260].try_into().unwrap()))
}

unsafe fn com_release_raw(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    let release: unsafe extern "system" fn(*mut c_void) -> u32 =
        unsafe { std::mem::transmute(*vtable(ptr).add(2)) };
    unsafe { release(ptr) };
}

/// SDK 自有 NV12 纹理（帧内容快照；生产 interop.rs 同款绑定标志）。
unsafe fn create_nv12_texture(device: *mut c_void, width: u32, height: u32) -> Result<Com> {
    let create_tex2d: unsafe extern "system" fn(
        *mut c_void,
        *const TexDesc,
        *const c_void,
        *mut *mut c_void,
    ) -> Hresult = unsafe { std::mem::transmute(*vtable(device).add(5)) };
    let desc = TexDesc {
        width,
        height,
        mip_levels: 1,
        array_size: 1,
        format: 103, // DXGI_FORMAT_NV12
        sample_count: 1,
        sample_quality: 0,
        usage: 0,                // DEFAULT
        bind_flags: 0x200 | 0x8, // DECODER | SHADER_RESOURCE
        cpu_access: 0,
        misc_flags: 0x2, // MISC_SHARED
    };
    let mut texture: *mut c_void = null_mut();
    let rc = unsafe { create_tex2d(device, &desc, null_mut(), &mut texture) };
    if rc < 0 || texture.is_null() {
        return Err(Error(format!(
            "vision.inferenceFailed|ov remote NV12 纹理创建失败 hr={rc:#x}"
        )));
    }
    Ok(Com(texture))
}

/// 立即上下文（借用指针，归设备所有不 AddRef——原型同注释）。
unsafe fn immediate_context(device: *mut c_void) -> Result<*mut c_void> {
    let get_ctx: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) =
        unsafe { std::mem::transmute(*vtable(device).add(40)) };
    let mut context: *mut c_void = null_mut();
    unsafe { get_ctx(device, &mut context) };
    if context.is_null() {
        return Err(Error(
            "vision.inferenceFailed|ov remote GetImmediateContext 为空".into(),
        ));
    }
    Ok(context)
}

/// 帧内容快照：Y/UV 两子资源 CopySubresourceRegion（数组纹理 subresource 非零
/// 亦可；目标子资源恒 0/1——自有单 slice 纹理）。方法返回 void（26100 头核对；
/// 拷贝失败经设备移除状态在后续同步/读取点暴露）。
unsafe fn copy_nv12_subresources(
    context: *mut c_void,
    dst: *mut c_void,
    src: *mut c_void,
    subresource: u32,
) -> Result<()> {
    let copy: unsafe extern "system" fn(
        *mut c_void,   // context
        *mut c_void,   // dst resource
        u32,           // dst subresource
        u32,           // dst x
        u32,           // dst y
        u32,           // dst z
        *mut c_void,   // src resource
        u32,           // src subresource
        *const c_void, // src box（null=整子资源）
    ) = unsafe { std::mem::transmute(*vtable(context).add(46)) };
    for (dst_sub, src_sub) in [(0, subresource), (1, subresource + 1)] {
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
    Ok(())
}

/// GPU 完成同步点：EVENT 查询 End+GetData 自旋（CPU 阻塞至此前全部立即上下文
/// 命令完成）。作用有二：拷贝完成前不闭合纹理借用窗口；D3D11 写对 CL 可见的
/// 显式同步（原型实证：创建期上传无同步点时 VA_SURFACE 读到恒定垃圾）。
unsafe fn wait_gpu_idle(device: *mut c_void, context: *mut c_void) -> Result<()> {
    let create_query: unsafe extern "system" fn(
        *mut c_void,
        *const QueryDesc,
        *mut *mut c_void,
    ) -> Hresult = unsafe { std::mem::transmute(*vtable(device).add(24)) };
    let desc = QueryDesc {
        query: 0, // D3D11_QUERY_EVENT
        misc: 0,
    };
    let mut query: *mut c_void = null_mut();
    let rc = unsafe { create_query(device, &desc, &mut query) };
    if rc < 0 || query.is_null() {
        return Err(Error(format!(
            "vision.inferenceFailed|ov remote 查询创建失败 hr={rc:#x}"
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
    unsafe { end(context, query) };
    let mut spins = 0u32;
    loop {
        // flags=0 允许运行时按需 Flush；S_FALSE(1)=未完成。
        let hr = unsafe { get_data(context, query, null_mut(), 0, 0) };
        if hr == 0 {
            break;
        }
        if hr != 1 {
            unsafe { com_release_raw(query) };
            return Err(Error(format!(
                "vision.inferenceFailed|ov remote GPU 同步查询失败 hr={hr:#x}"
            )));
        }
        spins = spins.saturating_add(1);
        if spins > 120_000_000 {
            unsafe { com_release_raw(query) };
            return Err(Error(
                "vision.inferenceFailed|ov remote GPU 同步超时（10s 量级）".into(),
            ));
        }
        if spins.is_multiple_of(1024) {
            std::thread::yield_now();
        }
    }
    unsafe { com_release_raw(query) };
    Ok(())
}

/// 会话级 remote 编译缓存键：D3D11 设备身份（COM 规范指针）+帧尺寸
/// （preprocess 图的 set_spatial_static_shape 按帧尺寸入图，尺寸变则重编）。
type SessionKey = (usize, u32, u32);

/// 一个（设备×帧尺寸）的 remote 直传编译产物：preprocess 图模型在 remote
/// context 上编译后的请求。字段声明序即 Drop 序：request→compiled→
/// preprocessed→context，逐级先于所依赖对象释放。
pub(crate) struct OvRemoteSession {
    api: Arc<OvApi>,
    request: *mut ov_infer_request_t,
    compiled: *mut ov_compiled_model_t,
    preprocessed: *mut ov_model_t,
    context: *mut ov_remote_context_t,
    y_name: CString,
    uv_name: CString,
    y_dims: Vec<i64>,
    uv_dims: Vec<i64>,
    /// D3D11 设备引用保活（context 的 VA_DEVICE 交付物；帧纹理全部释放后
    /// 设备仍存活到会话尾）。仅 Drop 语义，不读取。
    _device: Com,
}

// OV 运行时对象无线程域约束（openvino.rs/GpuPipeline 同惯例）：会话内
// remote 编译/请求由持有会话锁的调用方串行使用；Arc 经 ModelEngine 进入
// Model 的 Send 链，需 Send+Sync 双标记（GpuPipeline 先例）。
unsafe impl Send for OvRemoteSession {}
unsafe impl Sync for OvRemoteSession {}

impl Drop for OvRemoteSession {
    fn drop(&mut self) {
        unsafe {
            if !self.request.is_null() {
                (self.api.infer_request_free)(self.request);
            }
            if !self.compiled.is_null() {
                (self.api.compiled_model_free)(self.compiled);
            }
            if !self.preprocessed.is_null() {
                (self.api.model_free)(self.preprocessed);
            }
            if !self.context.is_null() {
                if let Some(free) = self.api.remote_context_free {
                    free(self.context);
                }
            }
        }
    }
}

/// 接受段产物：SDK 自有纹理内容快照 + 包裹它的双平面 remote 张量。
/// Arc 保活链（帧可能活得比会话关闭晚）：tensors→remote 会话（context/
/// compiled）→引擎（core/model）——Drop 按声明序先行释放张量，其后才是
/// 会话与引擎，OV shared_ptr 语义下全程依赖对象存活。
pub struct OvRemoteFrame {
    y: *mut ov_tensor_t,
    uv: *mut ov_tensor_t,
    /// 帧内容快照纹理（remote 张量所裹的 VA_SURFACE 底层对象；保活到执行完）。
    _texture: Com,
    remote: Arc<OvRemoteSession>,
    _engine: Arc<OpenvinoEngine>,
    /// 预处理几何（解码还原原图用；与图 pad 数字同源）。
    pub transform: ImageTransform,
}

impl Drop for OvRemoteFrame {
    fn drop(&mut self) {
        let api = &self.remote.api;
        unsafe {
            if !self.y.is_null() {
                (api.tensor_free)(self.y);
            }
            if !self.uv.is_null() {
                (api.tensor_free)(self.uv);
            }
        }
    }
}

// OV 运行时对象无线程域约束（openvino.rs 同惯例）：捕获线程构造、调度线程
// 串行消费（模型级执行互斥），Drop 只触 ov_*_free。
unsafe impl Send for OvRemoteFrame {}

/// OV 引擎的 remote 会话缓存入口（engine 持有；键=设备×帧尺寸）。
pub(crate) struct RemoteSessions(Mutex<HashMap<SessionKey, Arc<OvRemoteSession>>>);

impl RemoteSessions {
    pub(crate) fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }
}

/// GPU 帧接受段处理（§3.3a：借用窗口在返回时闭合）。center_crop=true 为
/// classify 几何——OV preprocess 无裁剪算子，显式拒绝。
pub(crate) fn capture(
    engine: &Arc<OpenvinoEngine>,
    width: u32,
    height: u32,
    subresource: u32,
    texture: *mut c_void,
    input_hw: (u32, u32),
    center_crop: bool,
) -> Result<(OvRemoteFrame, ImageTransform)> {
    if center_crop {
        return Err(Error(
            "sdk.notSupported|ov remote: classify center crop 几何暂不支持（OV preprocess C API 无裁剪算子，显式拒绝不降级）".into(),
        ));
    }
    if !width.is_multiple_of(2) || !height.is_multiple_of(2) {
        return Err(Error(format!(
            "sdk.notSupported|ov remote: NV12 帧须偶数维，实测 {width}x{height}"
        )));
    }
    let Some(pads) = letterbox_pads(width, height, input_hw.1, input_hw.0) else {
        return Err(Error(format!(
            "sdk.notSupported|ov remote: 几何不可表达（帧 {width}x{height} → 目标 {}x{}）",
            input_hw.1, input_hw.0
        )));
    };
    let device = unsafe { device_of_texture(texture)? };
    // 厂商核对（§3.5b 机制）：纹理适配器 vs 会话 OV 设备；不一致显式拒绝，
    // 错误详情含设备域快照（合同 L776-779）。
    let adapter_vendor = unsafe { adapter_vendor_of_device(device) };
    let token = crate::sdk::device::vendor_token_of(adapter_vendor);
    let full_name = engine.device_full_name();
    if !vendor_consistent(token, full_name) {
        return Err(Error(format!(
            "sdk.notSupported|ov remote: 纹理适配器厂商({:#x}→{})与会话设备不一致(\"{full_name}\")；{}",
            adapter_vendor.unwrap_or(0),
            token.unwrap_or("未知厂商"),
            engine.gpu_domain_snapshot().unwrap_or_default()
        )));
    }
    let session = session_for(engine, device, width, height, input_hw)?;
    // 帧内容快照：自有纹理 + 拷贝 + GPU 完成同步（借用窗口内的全部设备操作）。
    let snapshot = unsafe { create_nv12_texture(device, width, height)? };
    let context = unsafe { immediate_context(device)? };
    unsafe {
        copy_nv12_subresources(context, snapshot.0, texture, subresource)?;
        wait_gpu_idle(device, context)?;
    }
    let (y, uv) = unsafe { create_remote_tensors(&session, snapshot.0)? };
    let transform = ImageTransform {
        width,
        height,
        dst_w: input_hw.1,
        dst_h: input_hw.0,
        scale_x: pads.scale,
        scale_y: pads.scale,
        crop_x: 0,
        crop_y: 0,
        left: pads.model_left,
        top: pads.model_top,
    };
    crate::gpu::pipeline::audit::note_remote_input();
    Ok((
        OvRemoteFrame {
            y,
            uv,
            _texture: snapshot,
            remote: session,
            _engine: engine.clone(),
            transform,
        },
        transform,
    ))
}

/// 取或建（设备×帧尺寸）的 remote 编译会话。同引擎的**第二个帧尺寸**显式
/// 拒绝：preprocess 图按 set_spatial_static_shape 入图须逐尺寸重建，而同一
/// ov_model_t 的二次 prepostprocessor 构建在 2025.0.0 运行库实测失败
/// （get_input_info rc=-1）——首版限单帧尺寸流（视频流常态）；宿主可对
/// notSupported 改走像素路径（合同显式回退条款）。
fn session_for(
    engine: &Arc<OpenvinoEngine>,
    device: *mut c_void,
    width: u32,
    height: u32,
    input_hw: (u32, u32),
) -> Result<Arc<OvRemoteSession>> {
    let key: SessionKey = (device as usize, width, height);
    let mut cache = engine
        .remote_sessions
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(cached) = cache.get(&key) {
        return Ok(cached.clone());
    }
    if !cache.is_empty() {
        let known: Vec<String> = cache.keys().map(|(_, w, h)| format!("{w}x{h}")).collect();
        return Err(Error(format!(
            "sdk.notSupported|ov remote: 同模型多帧尺寸暂不支持（已建 {known:?}，新帧 {width}x{height} 需二次 preprocess 构建，OV 2025.0.0 实测失败）；请改用像素路径"
        )));
    }
    let built = Arc::new(unsafe { build_session(engine, device, width, height, input_hw)? });
    cache.insert(key, built.clone());
    Ok(built)
}

/// 建链：context（VA_SHARED/VA_DEVICE）→preprocess 图→compile_with_context→
/// request→y/uv 输入名与 shape。中途失败由 Drop 兜住已建句柄。
unsafe fn build_session(
    engine: &Arc<OpenvinoEngine>,
    device: *mut c_void,
    width: u32,
    height: u32,
    input_hw: (u32, u32),
) -> Result<OvRemoteSession> {
    let api = engine.api();
    let device_name = CString::new(engine.device_name())
        .map_err(|_| Error("vision.inferenceFailed|ov remote: 会话设备名含内嵌 NUL".into()))?;
    let create_context = api
        .core_create_context
        .ok_or_else(|| Error("vision.inferenceFailed|ov remote: create_context 符号缺失".into()))?;
    let compile_with_context = api.compile_model_with_context.ok_or_else(|| {
        Error("vision.inferenceFailed|ov remote: compile_model_with_context 符号缺失".into())
    })?;
    let mut session = OvRemoteSession {
        api: api.clone(),
        request: null_mut(),
        compiled: null_mut(),
        preprocessed: null_mut(),
        context: null_mut(),
        y_name: CString::new("y").unwrap(),
        uv_name: CString::new("uv").unwrap(),
        y_dims: Vec::new(),
        uv_dims: Vec::new(),
        _device: {
            // context 的 VA_DEVICE 交付物保活：会话期间宿主可释放其全部设备子对象。
            unsafe { com_add_ref(device) };
            Com(device)
        },
    };
    let k_ctx_type = CString::new("CONTEXT_TYPE").unwrap();
    let v_va_shared = CString::new(super::ov_sys::va_keys::VA_SHARED).unwrap();
    let k_va_device = CString::new(super::ov_sys::va_keys::VA_DEVICE).unwrap();
    let rc = unsafe {
        create_context(
            engine.core(),
            device_name.as_ptr(),
            4,
            &mut session.context,
            k_ctx_type.as_ptr(),
            v_va_shared.as_ptr(),
            k_va_device.as_ptr(),
            device,
        )
    };
    if rc != 0 {
        return Err(Error(format!(
            "vision.inferenceFailed|ov remote context 创建失败 rc={rc}（{}）",
            domain_note(engine)
        )));
    }
    session.preprocessed =
        unsafe { build_preprocessed(api, engine.model(), width, height, input_hw)? };
    let rc = unsafe {
        compile_with_context(
            engine.core(),
            session.preprocessed,
            session.context,
            0,
            &mut session.compiled,
        )
    };
    if rc != 0 {
        return Err(Error(format!(
            "vision.inferenceFailed|ov remote compile_model_with_context 失败 rc={rc}（{}）",
            domain_note(engine)
        )));
    }
    let rc = unsafe {
        (api.compiled_model_create_infer_request)(session.compiled, &mut session.request)
    };
    if rc != 0 {
        return Err(Error(format!(
            "vision.inferenceFailed|ov remote 请求创建失败 rc={rc}"
        )));
    }
    let (y_name, y_dims) = unsafe { model_input(api, session.preprocessed, 0)? };
    let (uv_name, uv_dims) = unsafe { model_input(api, session.preprocessed, 1)? };
    session.y_name = CString::new(y_name)
        .map_err(|_| Error("vision.inferenceFailed|ov remote: y 输入名含内嵌 NUL".into()))?;
    session.uv_name = CString::new(uv_name)
        .map_err(|_| Error("vision.inferenceFailed|ov remote: uv 输入名含内嵌 NUL".into()))?;
    session.y_dims = y_dims;
    session.uv_dims = uv_dims;
    Ok(session)
}

fn domain_note(engine: &OpenvinoEngine) -> String {
    format!(
        "device=\"{}\" {}",
        engine.device_name(),
        engine.gpu_domain_snapshot().unwrap_or_default()
    )
}

/// preprocess 图构造（NV12 双平面→RGB→pad(114)→resize(LINEAR)→f32→scale(/255)）。
/// pad 置于 resize 前：帧先 pad 成方形画布再统一缩放，几何等价像素路径
/// letterbox（同式见 letterbox_pads）。
unsafe fn build_preprocessed(
    api: &OvApi,
    model: *mut ov_model_t,
    frame_w: u32,
    frame_h: u32,
    input_hw: (u32, u32),
) -> Result<*mut ov_model_t> {
    let pads = letterbox_pads(frame_w, frame_h, input_hw.1, input_hw.0)
        .ok_or_else(|| Error("sdk.notSupported|ov remote: 几何不可表达".into()))?;
    let missing = |what: &str| Error(format!("vision.inferenceFailed|ov remote 符号缺失: {what}"));
    let create = api
        .prepostprocessor_create
        .ok_or_else(|| missing("prepostprocessor_create"))?;
    let free_pp = api
        .prepostprocessor_free
        .ok_or_else(|| missing("prepostprocessor_free"))?;
    let build = api
        .prepostprocessor_build
        .ok_or_else(|| missing("prepostprocessor_build"))?;
    let get_info = api
        .prepostprocessor_get_input_info
        .ok_or_else(|| missing("get_input_info"))?;
    let free_info = api
        .input_info_free
        .ok_or_else(|| missing("input_info_free"))?;
    let get_tensor_info = api
        .input_info_get_tensor_info
        .ok_or_else(|| missing("get_tensor_info"))?;
    let free_tensor_info = api
        .tensor_info_free
        .ok_or_else(|| missing("tensor_info_free"))?;
    let set_element = api
        .tensor_info_set_element_type
        .ok_or_else(|| missing("set_element_type"))?;
    let set_color = api
        .tensor_info_set_color_format_with_subname
        .ok_or_else(|| missing("set_color_format_with_subname"))?;
    let set_memory = api
        .tensor_info_set_memory_type
        .ok_or_else(|| missing("set_memory_type"))?;
    let set_spatial = api
        .tensor_info_set_spatial_static_shape
        .ok_or_else(|| missing("set_spatial_static_shape"))?;
    let set_tensor_layout = api
        .tensor_info_set_layout
        .ok_or_else(|| missing("set_layout"))?;
    let get_steps = api
        .input_info_get_preprocess_steps
        .ok_or_else(|| missing("get_preprocess_steps"))?;
    let free_steps = api.steps_free.ok_or_else(|| missing("steps_free"))?;
    let steps_convert_color = api
        .steps_convert_color
        .ok_or_else(|| missing("convert_color"))?;
    let steps_resize = api.steps_resize.ok_or_else(|| missing("resize"))?;
    let steps_convert_element = api
        .steps_convert_element_type
        .ok_or_else(|| missing("convert_element_type"))?;
    let steps_pad = api.steps_pad.ok_or_else(|| missing("pad"))?;
    let steps_scale = api.steps_scale.ok_or_else(|| missing("scale"))?;
    let get_model_info = api
        .input_info_get_model_info
        .ok_or_else(|| missing("get_model_info"))?;
    let free_model_info = api
        .model_info_free
        .ok_or_else(|| missing("model_info_free"))?;
    let set_model_layout = api
        .model_info_set_layout
        .ok_or_else(|| missing("model_info_set_layout"))?;
    let layout_create = api.layout_create.ok_or_else(|| missing("layout_create"))?;
    let layout_free = api.layout_free.ok_or_else(|| missing("layout_free"))?;

    let ck = |rc: i32, what: &str| -> Result<()> {
        (rc == 0).then_some(()).ok_or_else(|| {
            Error(format!(
                "vision.inferenceFailed|ov remote preprocess {what} 失败 rc={rc}（{}）",
                api.last_error_message()
            ))
        })
    };
    unsafe {
        let mut pp: *mut super::ov_sys::ov_prepostprocessor_t = null_mut();
        ck(create(model, &mut pp), "prepostprocessor_create")?;
        let mut info: *mut super::ov_sys::ov_preprocess_input_info_t = null_mut();
        let info_rc = get_info(pp, &mut info);
        if info_rc != 0 {
            free_pp(pp);
            return Err(Error(format!(
                "vision.inferenceFailed|ov remote preprocess get_input_info 失败 rc={info_rc}"
            )));
        }
        let mut tensor_info: *mut super::ov_sys::ov_preprocess_input_tensor_info_t = null_mut();
        let ti_rc = get_tensor_info(info, &mut tensor_info);
        if ti_rc != 0 {
            free_info(info);
            free_pp(pp);
            return Err(Error(format!(
                "vision.inferenceFailed|ov remote preprocess get_tensor_info 失败 rc={ti_rc}"
            )));
        }
        // 元素序数按运行库线（2025.0.x 旧线 U8=17）。
        let u8_element = match api.element_line() {
            ElementLine::Current => super::ov_sys::OV_TYPE_U8,
            ElementLine::Legacy2025_0 => OV_TYPE_U8_2025_0,
        };
        let y_name = CString::new("y").unwrap();
        let uv_name = CString::new("uv").unwrap();
        let memory = CString::new(super::ov_sys::OV_MEM_TYPE_GPU_SURFACE).unwrap();
        let nhwc = CString::new("NHWC").unwrap();
        let nchw = CString::new("NCHW").unwrap();
        // pad：NHWC [N,H,W,C] 各轴 begin/end（H 轴 top/bottom、W 轴 left/right）。
        let pad_begin = [0i32, pads.top as i32, pads.left as i32, 0];
        let pad_end = [0i32, pads.bottom as i32, pads.right as i32, 0];
        let mut tensor_layout: *mut ov_layout_t = null_mut();
        let mut model_layout: *mut ov_layout_t = null_mut();
        let mut steps: *mut super::ov_sys::ov_preprocess_preprocess_steps_t = null_mut();
        let mut model_info: *mut super::ov_sys::ov_preprocess_input_model_info_t = null_mut();
        let mut preprocessed: *mut ov_model_t = null_mut();
        let staged: Result<()> = (|| {
            ck(set_element(tensor_info, u8_element), "set_element_type(u8)")?;
            ck(
                set_color(
                    tensor_info,
                    OV_COLOR_NV12_TWO_PLANES,
                    2,
                    y_name.as_ptr(),
                    uv_name.as_ptr(),
                ),
                "set_color_format(NV12_TWO_PLANES)",
            )?;
            ck(
                set_memory(tensor_info, memory.as_ptr()),
                "set_memory_type(GPU_SURFACE)",
            )?;
            ck(
                set_spatial(tensor_info, frame_h as usize, frame_w as usize),
                "set_spatial_static_shape",
            )?;
            ck(
                layout_create(nhwc.as_ptr(), &mut tensor_layout),
                "layout_create(NHWC)",
            )?;
            ck(
                set_tensor_layout(tensor_info, tensor_layout),
                "set_layout(NHWC)",
            )?;
            ck(get_steps(info, &mut steps), "get_preprocess_steps")?;
            ck(
                steps_convert_color(steps, OV_COLOR_RGB),
                "convert_color(RGB)",
            )?;
            // f32 先行：OV preprocess 的 pad 步要求浮点元素类型（2025.0.0 实测
            // is_real 断言），pad 值语义为归一化前 114.0（f32 域 114 后除 255，
            // 两坑常量同源锚定）。pad→resize 序实现 letterbox：帧先 pad 成方形
            // 画布（114，居中）再统一缩放到模型输入。
            let f32_element = super::openvino::element_of_dtype(0, api.element_line())?;
            ck(
                steps_convert_element(steps, f32_element),
                "convert_element_type(f32)",
            )?;
            ck(
                steps_pad(
                    steps,
                    pad_begin.as_ptr(),
                    pad_begin.len(),
                    pad_end.as_ptr(),
                    pad_end.len(),
                    PAD_VALUE,
                    OV_PAD_CONSTANT,
                ),
                "pad(CONSTANT,114)",
            )?;
            ck(steps_resize(steps, OV_RESIZE_BILINEAR), "resize(LINEAR)")?;
            // OV scale 为除法语义：归一化传 255.0（原型实证陷阱，禁改 1/255）。
            ck(steps_scale(steps, SCALE_DIVISOR), "scale(/255)")?;
            ck(get_model_info(info, &mut model_info), "get_model_info")?;
            ck(
                layout_create(nchw.as_ptr(), &mut model_layout),
                "layout_create(NCHW)",
            )?;
            ck(
                set_model_layout(model_info, model_layout),
                "model_info_set_layout(NCHW)",
            )?;
            ck(build(pp, &mut preprocessed), "prepostprocessor_build")?;
            Ok(())
        })();
        // 中间句柄按建立逆序统一释放（preprocessed 交调用方持有）。
        if !model_info.is_null() {
            free_model_info(model_info);
        }
        if !model_layout.is_null() {
            layout_free(model_layout);
        }
        if !steps.is_null() {
            free_steps(steps);
        }
        if !tensor_layout.is_null() {
            layout_free(tensor_layout);
        }
        free_tensor_info(tensor_info);
        free_info(info);
        free_pp(pp);
        staged?;
        Ok(preprocessed)
    }
}

/// 模型第 index 输入的（名称, shape）——preprocess 产物的 y/uv 端口读取。
unsafe fn model_input(
    api: &OvApi,
    model: *mut ov_model_t,
    index: usize,
) -> Result<(String, Vec<i64>)> {
    let inputs_size = api.model_inputs_size.ok_or_else(|| {
        Error("vision.inferenceFailed|ov remote 符号缺失: model_inputs_size".into())
    })?;
    let input_by_index = api.model_const_input_by_index.ok_or_else(|| {
        Error("vision.inferenceFailed|ov remote 符号缺失: model_const_input_by_index".into())
    })?;
    unsafe {
        let mut total = 0usize;
        let rc = (inputs_size)(model, &mut total);
        if rc != 0 || index >= total {
            return Err(Error(format!(
                "vision.inferenceFailed|ov remote 模型输入数异常 rc={rc} total={total}"
            )));
        }
        let mut port: *mut ov_output_const_port_t = null_mut();
        let rc = (input_by_index)(model, index, &mut port);
        if rc != 0 || port.is_null() {
            return Err(Error(format!(
                "vision.inferenceFailed|ov remote 输入端口读取失败 index={index} rc={rc}"
            )));
        }
        // 取值完成后统一释放（openvino.rs PortGuard 同语义，内联短路径）。
        let mut shape = ov_shape_t {
            rank: 0,
            dims: null_mut(),
        };
        let shape_rc = (api.const_port_get_shape)(port, &mut shape);
        let dims: Vec<i64> = if shape_rc == 0 && shape.rank > 0 && !shape.dims.is_null() {
            std::slice::from_raw_parts(shape.dims, shape.rank as usize).to_vec()
        } else {
            Vec::new()
        };
        if shape_rc == 0 {
            let _ = (api.shape_free)(&mut shape);
        }
        let mut name_ptr: *mut std::ffi::c_char = null_mut();
        let name_rc = (api.port_get_any_name)(port, &mut name_ptr);
        let name = if name_rc == 0 && !name_ptr.is_null() {
            CStr::from_ptr(name_ptr).to_string_lossy().into_owned()
        } else {
            String::new()
        };
        if name_rc == 0 {
            (api.free)(name_ptr);
        }
        (api.output_const_port_free)(port);
        if shape_rc != 0 || name_rc != 0 || name.is_empty() {
            return Err(Error(format!(
                "vision.inferenceFailed|ov remote 输入端口规格读取失败 shape_rc={shape_rc} name_rc={name_rc}"
            )));
        }
        Ok((name, dims))
    }
}

/// 自有纹理上的双平面 remote 张量（VA_SURFACE + DEV_OBJECT_HANDLE + VA_PLANE）。
unsafe fn create_remote_tensors(
    session: &OvRemoteSession,
    texture: *mut c_void,
) -> Result<(*mut ov_tensor_t, *mut ov_tensor_t)> {
    let api = &session.api;
    let create = api.remote_context_create_tensor.ok_or_else(|| {
        Error("vision.inferenceFailed|ov remote 符号缺失: remote_context_create_tensor".into())
    })?;
    let u8_element = match api.element_line() {
        ElementLine::Current => super::ov_sys::OV_TYPE_U8,
        ElementLine::Legacy2025_0 => OV_TYPE_U8_2025_0,
    };
    let k_mem_type = CString::new("SHARED_MEM_TYPE").unwrap();
    let v_va_surface = CString::new(super::ov_sys::va_keys::VA_SURFACE).unwrap();
    let k_dev_handle = CString::new(super::ov_sys::va_keys::DEV_OBJECT_HANDLE).unwrap();
    let k_va_plane = CString::new(super::ov_sys::va_keys::VA_PLANE).unwrap();
    let plane0 = CString::new("0").unwrap();
    let plane1 = CString::new("1").unwrap();
    let mut made: Vec<*mut ov_tensor_t> = Vec::new();
    unsafe {
        for (dims, plane) in [(&session.y_dims, &plane0), (&session.uv_dims, &plane1)] {
            let mut shape_dims = dims.clone();
            let shape = ov_shape_t {
                rank: shape_dims.len() as i64,
                dims: shape_dims.as_mut_ptr(),
            };
            let mut tensor: *mut ov_tensor_t = null_mut();
            let rc = create(
                session.context,
                u8_element,
                shape,
                6,
                &mut tensor,
                k_mem_type.as_ptr(),
                v_va_surface.as_ptr(),
                k_dev_handle.as_ptr(),
                texture,
                k_va_plane.as_ptr(),
                plane.as_ptr(),
            );
            if rc != 0 || tensor.is_null() {
                for made_tensor in made {
                    (api.tensor_free)(made_tensor);
                }
                return Err(Error(format!(
                    "vision.inferenceFailed|ov remote 张量创建失败 plane={} rc={rc}",
                    plane.to_string_lossy()
                )));
            }
            made.push(tensor);
        }
    }
    Ok((made[0], made[1]))
}

/// 执行段：绑定双平面张量→infer→输出回读（输出规格=会话输出，逐输出预算
/// 先扣后复制，F06 口径）。
pub(crate) fn infer(
    engine: &OpenvinoEngine,
    frame: &OvRemoteFrame,
    output_budget: usize,
) -> Result<Vec<TensorData>> {
    let api = frame.remote.api.clone();
    let set_tensor = api.infer_request_set_tensor.ok_or_else(|| {
        Error("vision.inferenceFailed|ov remote 符号缺失: infer_request_set_tensor".into())
    })?;
    unsafe {
        let rc = (set_tensor)(frame.remote.request, frame.remote.y_name.as_ptr(), frame.y);
        if rc != 0 {
            return Err(Error(format!(
                "vision.inferenceFailed|ov remote y 绑定失败 rc={rc}"
            )));
        }
        let rc = (set_tensor)(
            frame.remote.request,
            frame.remote.uv_name.as_ptr(),
            frame.uv,
        );
        if rc != 0 {
            return Err(Error(format!(
                "vision.inferenceFailed|ov remote uv 绑定失败 rc={rc}"
            )));
        }
        let rc = (api.infer_request_infer)(frame.remote.request);
        if rc != 0 {
            return Err(Error(format!(
                "vision.inferenceFailed|ov remote 推理失败 rc={rc}（{}）",
                domain_note(engine)
            )));
        }
        let mut remaining = output_budget;
        let mut outputs = Vec::with_capacity(engine.outputs.len());
        for (index, spec) in engine.outputs.iter().enumerate() {
            let built = super::openvino::read_output_tensor(
                &api,
                frame.remote.request,
                index,
                spec,
                remaining,
            )?;
            let allocated = built
                .bytes()
                .len()
                .checked_next_multiple_of(8)
                .ok_or_else(|| Error("sdk.resourceExhausted".into()))?;
            remaining = remaining
                .checked_sub(allocated)
                .ok_or_else(|| Error("sdk.resourceExhausted".into()))?;
            outputs.push(built);
        }
        Ok(outputs)
    }
}

/// 测试辅助：Intel 适配器 D3D11 设备（宿主形态：一个设备供全部帧纹理；
/// 每帧各建设备会令 remote 会话按设备身份错建多份）。生产路径纹理来自
/// 解码器/宿主；仅供测试构造（interop::nvidia_nv12_texture 先例）。库句柄
/// forget 保活（纹理 vtable 指向 d3d11 代码，卸载即悬空——interop.rs 同注）。
pub struct IntelTestDevice(pub(crate) Com);

/// 枚举 Intel 适配器建 D3D11 设备（无则 None）。
pub fn intel_test_device() -> Option<IntelTestDevice> {
    unsafe { intel_test_device_unsafe() }
}

unsafe fn intel_test_device_unsafe() -> Option<IntelTestDevice> {
    let dxgi = unsafe { libloading::Library::new("dxgi.dll").ok()? };
    let d3d11 = libloading::Library::new("d3d11.dll").ok()?;
    // IID_IDXGIFactory {7B7166EC-21C7-44AE-B21A-C9AE321AE369}（interop.rs 同值）。
    let iid = Guid {
        a: 0x7B71_66EC,
        b: 0x21C7,
        c: 0x44AE,
        d: [0xB2, 0x1A, 0xC9, 0xAE, 0x32, 0x1A, 0xE3, 0x69],
    };
    let create_factory: unsafe extern "system" fn(*const Guid, *mut *mut c_void) -> Hresult =
        unsafe { *dxgi.get(b"CreateDXGIFactory\0").ok()? };
    let mut factory: *mut c_void = null_mut();
    if unsafe { create_factory(&iid, &mut factory) } < 0 {
        return None;
    }
    let factory = Com(factory);
    let enum_adapters: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> Hresult =
        unsafe { std::mem::transmute(*vtable(factory.0).add(7)) };
    for index in 0..8 {
        let mut adapter: *mut c_void = null_mut();
        if unsafe { enum_adapters(factory.0, index, &mut adapter) } < 0 {
            break;
        }
        let adapter = Com(adapter);
        let get_desc: unsafe extern "system" fn(*mut c_void, *mut u8) -> Hresult =
            unsafe { std::mem::transmute(*vtable(adapter.0).add(8)) };
        let mut desc = [0u8; 304];
        if unsafe { get_desc(adapter.0, desc.as_mut_ptr()) } < 0 {
            continue;
        }
        let vendor = u32::from_le_bytes(desc[256..260].try_into().unwrap());
        if vendor != 0x8086 {
            continue; // Intel 侧（OV GPU 插件与 CL 互操作同适配器）
        }
        let create_device: unsafe extern "system" fn(
            *mut c_void,
            u32,
            *mut c_void,
            u32,
            *const u32,
            u32,
            u32,
            *mut *mut c_void,
            *mut u32,
            *mut *mut c_void,
        ) -> Hresult = unsafe { *d3d11.get(b"D3D11CreateDevice\0").ok()? };
        let mut device: *mut c_void = null_mut();
        let rc = unsafe {
            create_device(
                adapter.0,
                0, // D3D_DRIVER_TYPE_UNKNOWN（显式适配器）
                null_mut(),
                0x800, // D3D11_CREATE_DEVICE_VIDEO_SUPPORT
                std::ptr::null(),
                0,
                7,
                &mut device,
                null_mut(),
                null_mut(),
            )
        };
        if rc < 0 || device.is_null() {
            return None;
        }
        std::mem::forget((dxgi, d3d11));
        return Some(IntelTestDevice(Com(device)));
    }
    None
}

/// 单设备上的 NV12 创建期上传（Y/UV 两子资源，原型同参数）。创建期上传的
/// D3D 写由消费端 capture 的 GPU 完成同步点统一收口（事件查询等待此前全部
/// 立即上下文命令）。
///
/// # Safety
/// 返回的 Com 在 Drop 前有效；device 须来自 intel_test_device。
pub unsafe fn intel_nv12_texture_upload(
    device: &IntelTestDevice,
    width: u32,
    height: u32,
    y_plane: &[u8],
    uv_plane: &[u8],
) -> Option<Com> {
    // 纹理（设备子对象）持有设备引用：返回的 Com 存活期内设备存活。
    unsafe { upload_nv12(device.0 .0, width, height, y_plane, uv_plane) }.map(Com)
}

/// 创建期上传 NV12（SubresourceData 两子资源；原型同参数）。
unsafe fn upload_nv12(
    device: *mut c_void,
    width: u32,
    height: u32,
    y_plane: &[u8],
    uv_plane: &[u8],
) -> Option<*mut c_void> {
    #[repr(C)]
    struct SubresourceData {
        sys_mem: *const c_void,
        pitch: u32,
        slice_pitch: u32,
    }
    let create_tex2d: unsafe extern "system" fn(
        *mut c_void,
        *const TexDesc,
        *const SubresourceData,
        *mut *mut c_void,
    ) -> Hresult = unsafe { std::mem::transmute(*vtable(device).add(5)) };
    let desc = TexDesc {
        width,
        height,
        mip_levels: 1,
        array_size: 1,
        format: 103,
        sample_count: 1,
        sample_quality: 0,
        usage: 0,
        bind_flags: 0x200 | 0x8,
        cpu_access: 0,
        misc_flags: 0x2,
    };
    let initial = [
        SubresourceData {
            sys_mem: y_plane.as_ptr().cast(),
            pitch: width,
            slice_pitch: 0,
        },
        SubresourceData {
            sys_mem: uv_plane.as_ptr().cast(),
            pitch: width,
            slice_pitch: 0,
        },
    ];
    let mut texture: *mut c_void = null_mut();
    let rc = unsafe { create_tex2d(device, &desc, initial.as_ptr(), &mut texture) };
    (rc >= 0 && !texture.is_null()).then_some(texture)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ov_sys::OV_MEM_TYPE_GPU_SURFACE;
    use crate::sdk::device::vendor_token_of;

    /// letterbox pad 几何：与像素路径同比例同中心；偶数维恒可表达，奇数维/
    /// 非方形目标拒绝（NV12/图形态合同）。
    #[test]
    fn letterbox_pads_center_and_scale_match_pixel_path() {
        // 方形帧：零 pad，恒等。
        let identity = letterbox_pads(640, 640, 640, 640).unwrap();
        assert_eq!(
            (identity.top, identity.bottom, identity.left, identity.right),
            (0, 0, 0, 0)
        );
        assert!((identity.scale - 1.0).abs() < f32::EPSILON);
        // 横向帧 1920x1080 → 640²：scale=1/3，上下带 420/420，帧区 top=140。
        let wide = letterbox_pads(1920, 1080, 640, 640).unwrap();
        assert_eq!((wide.left, wide.right), (0, 0));
        assert_eq!((wide.top, wide.bottom), (420, 420));
        assert!((wide.scale - 640.0 / 1920.0).abs() < 1e-6);
        assert_eq!(wide.model_top, 140);
        assert_eq!(wide.model_left, 0);
        // 纵向帧 810x1080（bus.jpg 形态）→ 640²：左右带 135/135，帧区 left=80。
        let tall = letterbox_pads(810, 1080, 640, 640).unwrap();
        assert_eq!((tall.top, tall.bottom), (0, 0));
        assert_eq!((tall.left, tall.right), (135, 135));
        assert!((tall.scale - 640.0 / 1080.0).abs() < 1e-6);
        assert_eq!(tall.model_left, 80);
        // 拒绝面：奇数维、非方形目标、零维。
        assert!(letterbox_pads(641, 640, 640, 640).is_none());
        assert!(letterbox_pads(640, 640, 640, 640 + 2).is_none());
        assert!(letterbox_pads(0, 640, 640, 640).is_none());
    }

    /// 厂商核对词：命中/不命中/大小写/未知厂商镜像装载期语义。
    #[test]
    fn vendor_consistency_covers_match_mismatch_and_unknown() {
        let full = "Intel(R) Arc(TM) Graphics (iGPU)";
        assert!(vendor_consistent(Some("intel"), full));
        assert!(vendor_consistent(Some("INTEL"), full));
        assert!(!vendor_consistent(Some("nvidia"), full));
        assert!(!vendor_consistent(Some("amd"), full));
        // 未知厂商（无核对词）：镜像装载期「不做厂商筛」语义。
        assert!(vendor_consistent(None, full));
        assert!(vendor_consistent(None, ""));
        // vendor_token_of 直连：映射表与核对词一致性。
        assert_eq!(vendor_token_of(Some(0x8086)), Some("intel"));
        assert_eq!(vendor_token_of(Some(0x10de)), Some("nvidia"));
        assert_eq!(vendor_token_of(Some(0x1414)), None);
    }

    /// 两坑常量锁定：scale 除法语义传 255.0、pad 归一化前 114.0；色彩/resize/
    /// pad 枚举序数（原型 2025.0.0 头文件核对线）。
    #[test]
    fn preprocess_trap_constants_are_pinned() {
        assert_eq!(SCALE_DIVISOR, 255.0);
        assert_eq!(PAD_VALUE, 114.0);
        assert_eq!(OV_COLOR_NV12_TWO_PLANES, 2);
        assert_eq!(OV_COLOR_RGB, 5);
        assert_eq!(OV_RESIZE_BILINEAR, 0);
        assert_eq!(OV_PAD_CONSTANT, 0);
        assert_eq!(OV_MEM_TYPE_GPU_SURFACE, "GPU_SURFACE");
    }
}
