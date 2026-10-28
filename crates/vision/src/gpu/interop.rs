//! D3D11↔CUDA 互操作（M2b 3.3）：纹理注册/映射生命周期。
//! 注册与 unmap 全程限制在 GPU 帧租约窗口内（合同 §3.3a），不跨租约持有纹理。
use super::cuda_sys::{CudaApi, CudaArray, GraphicsResource, REGISTER_DEFAULT};
use std::ffi::c_void;
use std::sync::Arc;

/// 注册的 D3D11 纹理：Drop 注销（unregister），借用期内有效。
/// map 为 resource 级：多子资源共享一次 map，引用计数归零才 unmap。
pub struct RegisteredTexture {
    api: Arc<CudaApi>,
    resource: GraphicsResource,
    mapped: std::sync::atomic::AtomicUsize,
}

impl RegisteredTexture {
    /// 注册 ID3D11Texture2D；调用线程隐式 CUDA context（cudaSetDevice 先行）。
    /// 纹理须与 CUDA 设备同族（NVIDIA D3D11 设备 ↔ NVIDIA CUDA 设备）。
    ///
    /// # Safety
    /// `texture` 必须是有效的 ID3D11Texture2D 指针（GPU 帧租约借用期内）。
    pub unsafe fn register(api: Arc<CudaApi>, texture: *mut c_void) -> Result<Self, crate::Error> {
        if texture.is_null() {
            return Err(crate::Error("纹理指针为空".into()));
        }
        let mut resource: GraphicsResource = std::ptr::null_mut();
        let code = unsafe { (api.register_d3d11)(&mut resource, texture, REGISTER_DEFAULT) };
        api.check(code, "纹理注册")?;
        if resource.is_null() {
            return Err(crate::Error("注册成功但资源为空".into()));
        }
        Ok(Self {
            api,
            resource,
            mapped: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    /// 映射并取子资源数组；返回 RAII 映射守卫（计数归零 unmap）。
    /// 首次调用执行 resource 级 map，后续子资源仅取 array（CUDA 语义：map
    /// per-resource，mapped_array 可重复调用）。
    pub fn map_subresource(&self, subresource: u32) -> Result<MappedArray<'_>, crate::Error> {
        use std::sync::atomic::Ordering;
        let was = self.mapped.fetch_add(1, Ordering::AcqRel);
        if was == 0 {
            let code = unsafe { (self.api.map_resources)(1, &self.resource, std::ptr::null_mut()) };
            if code != 0 {
                self.mapped.store(0, Ordering::Release);
                self.api.check(code, "映射资源")?;
            }
        }
        let mut array: CudaArray = std::ptr::null_mut();
        let code = unsafe { (self.api.mapped_array)(&mut array, self.resource, subresource, 0) };
        if code != 0 {
            unsafe {
                (self.api.unmap_resources)(1, &self.resource, std::ptr::null_mut());
            }
            return Err(crate::Error(format!("CUDA 子资源映射失败({code})")));
        }
        if array.is_null() {
            unsafe {
                (self.api.unmap_resources)(1, &self.resource, std::ptr::null_mut());
            }
            return Err(crate::Error("映射数组为空".into()));
        }
        Ok(MappedArray {
            api: self.api.clone(),
            resource: self.resource,
            mapped: &self.mapped,
            array,
            _lease: std::marker::PhantomData,
        })
    }
}

impl Drop for RegisteredTexture {
    fn drop(&mut self) {
        unsafe { (self.api.unregister)(self.resource) };
    }
}

/// 映射中的 CUDA 数组：Drop 计数递减，归零才 unmap；持有期间纹理不可被 GPU
/// 写入方复用。
pub struct MappedArray<'a> {
    api: Arc<CudaApi>,
    resource: GraphicsResource,
    mapped: &'a std::sync::atomic::AtomicUsize,
    /// 子资源映射数组（借用到 unmap 为止；kernel 采样输入）。
    pub array: CudaArray,
    _lease: std::marker::PhantomData<&'a RegisteredTexture>,
}

impl Drop for MappedArray<'_> {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;
        if self.mapped.fetch_sub(1, Ordering::AcqRel) == 1 {
            unsafe { (self.api.unmap_resources)(1, &self.resource, std::ptr::null_mut()) };
        }
    }
}

#[repr(C)]
pub struct Guid {
    a: u32,
    b: u16,
    c: u16,
    d: [u8; 8],
}

#[repr(C)]
#[allow(dead_code)]
pub struct TexDesc {
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

pub struct Com(pub *mut c_void);
impl Drop for Com {
    fn drop(&mut self) {
        if !self.0.is_null() {
            let table = unsafe { *self.0.cast::<*const *const c_void>() };
            let release: unsafe extern "system" fn(*mut c_void) -> u32 =
                unsafe { std::mem::transmute(*table.add(2)) };
            unsafe { release(self.0) };
        }
    }
}
/// 测试辅助：NVIDIA D3D11 设备 + NV12 解码数组纹理（M2a 实测同参数；
/// 生产路径纹理来自解码器，本函数仅供测试构造。
/// dxgi/d3d11 库句柄保活到进程结束：纹理 vtable 指向 d3d11 代码，库卸载即悬空
/// （CUDA register 内部会对纹理 QI，实测 Drop 库导致访问冲突）。
/// 测试辅助构造 NVIDIA NV12 纹理；生产路径纹理来自解码器。
///
/// # Safety
/// 返回的 Com 指针在 Drop 前有效；内部按需保活 dxgi/d3d11 库。
pub unsafe fn nvidia_nv12_texture(width: u32, height: u32) -> Option<Com> {
    let dxgi = unsafe { libloading::Library::new("dxgi.dll").ok()? };
    let d3d11 = unsafe { libloading::Library::new("d3d11.dll").ok()? };
    // IID_IDXGIFactory {7B7166EC-21C7-44AE-B21A-C9AE321AE369}
    let iid = Guid {
        a: 0x7B71_66EC,
        b: 0x21C7,
        c: 0x44AE,
        d: [0xB2, 0x1A, 0xC9, 0xAE, 0x32, 0x1A, 0xE3, 0x69],
    };
    let create_factory: unsafe extern "system" fn(*const Guid, *mut *mut c_void) -> i32 =
        unsafe { *dxgi.get(b"CreateDXGIFactory\0").ok()? };
    let mut factory: *mut c_void = std::ptr::null_mut();
    if unsafe { create_factory(&iid, &mut factory) } < 0 {
        return None;
    }
    let factory = Com(factory);
    let vt = unsafe { *factory.0.cast::<*const *const c_void>() };
    // IDXGIFactory::EnumAdapters = slot 7
    let enum_adapters: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> i32 =
        unsafe { std::mem::transmute(*vt.add(7)) };
    for index in 0..4 {
        let mut adapter: *mut c_void = std::ptr::null_mut();
        if unsafe { enum_adapters(factory.0, index, &mut adapter) } < 0 {
            break;
        }
        let adapter = Com(adapter);
        let ad_vt = unsafe { *adapter.0.cast::<*const *const c_void>() };
        let get_desc: unsafe extern "system" fn(*mut c_void, *mut u8) -> i32 =
            unsafe { std::mem::transmute(*ad_vt.add(8)) };
        let mut desc = [0u8; 304]; // DXGI_ADAPTER_DESC 前缀：Description[128]wchar 后 vendor_id
        if unsafe { get_desc(adapter.0, desc.as_mut_ptr()) } < 0 {
            continue;
        }
        let vendor = u32::from_le_bytes(desc[256..260].try_into().unwrap());
        if vendor != 0x10DE {
            continue; // 仅 NVIDIA（CUDA 同族）
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
        ) -> i32 = unsafe { *d3d11.get(b"D3D11CreateDevice\0").ok()? };
        let mut device: *mut c_void = std::ptr::null_mut();
        let video_support = 0x800u32;
        let rc = unsafe {
            create_device(
                adapter.0,
                0, // D3D_DRIVER_TYPE_UNKNOWN（指定 adapter）
                std::ptr::null_mut(),
                video_support,
                std::ptr::null(),
                0,
                7,
                &mut device,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if rc < 0 || device.is_null() {
            continue;
        }
        let device = Com(device);
        let dev_vt = unsafe { *device.0.cast::<*const *const c_void>() };
        let create_tex2d: unsafe extern "system" fn(
            *mut c_void,
            *const TexDesc,
            *const c_void,
            *mut *mut c_void,
        ) -> i32 = unsafe { std::mem::transmute(*dev_vt.add(5)) };
        let desc = TexDesc {
            width,
            height,
            mip_levels: 1,
            array_size: 4,
            format: 103, // DXGI_FORMAT_NV12
            sample_count: 1,
            sample_quality: 0,
            usage: 0, // DEFAULT
            bind_flags: 0x200 | 0x8,
            cpu_access: 0,
            misc_flags: 0x2,
        };
        let mut texture: *mut c_void = std::ptr::null_mut();
        let rc = unsafe { create_tex2d(device.0, &desc, std::ptr::null(), &mut texture) };
        if rc < 0 || texture.is_null() {
            return None;
        }
        // 库句柄保活到函数出口：纹理 vtable 指向 d3d11 代码，FreeLibrary 后悬空。
        std::mem::forget((dxgi, d3d11));
        return Some(Com(texture));
    }
    None
}
