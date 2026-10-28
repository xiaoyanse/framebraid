//! CUDA 互操作实测（M2b 3.3）：NVIDIA D3D11 纹理注册→映射→子资源数组。
//! 真实硬件链路，需 AM_SDK_ORT_DIR（含 cudart64_*.dll 的运行库目录）。
use super::cuda_sys::CudaApi;
use super::interop::RegisteredTexture;
use std::ffi::c_void;

#[repr(C)]
struct Guid {
    a: u32,
    b: u16,
    c: u16,
    d: [u8; 8],
}

#[repr(C)]
#[allow(dead_code)]
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

struct Com(*mut c_void);
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

/// NVIDIA D3D11 设备 + NV12 解码数组纹理（与 M2a 实测纹理同参数）。
/// dxgi/d3d11 库句柄保活到进程结束：纹理 vtable 指向 d3d11 代码，库卸载即悬空
/// （CUDA register 内部会对纹理 QI，实测 Drop 库导致访问冲突）。
unsafe fn nvidia_nv12_texture(width: u32, height: u32) -> Option<Com> {
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

#[test]
#[ignore = "需要 AM_SDK_ORT_DIR（真实 NVIDIA GPU 互操作实测）"]
fn register_map_unmap_nvidia_nv12_texture() {
    let api = CudaApi::load().expect("加载 cudart");
    let mut count = 0i32;
    api.check(unsafe { (api.get_device_count)(&mut count) }, "设备计数")
        .expect("CUDA 设备查询");
    assert!(count > 0, "应有可用 CUDA 设备");
    api.check(unsafe { (api.set_device)(0) }, "选择设备")
        .unwrap();

    let texture = unsafe { nvidia_nv12_texture(320, 240).expect("创建 NVIDIA NV12 纹理") };
    let registered =
        unsafe { RegisteredTexture::register(api.clone(), texture.0) }.expect("注册纹理");
    {
        let mapped = registered.map_subresource(0).expect("映射子资源");
        assert!(!mapped.array.is_null(), "映射数组非空");
    }
    // Drop 完成注销；再次注册/映射验证生命周期可重复。
    drop(registered);
    let again = unsafe { RegisteredTexture::register(api, texture.0) }.expect("重复注册");
    drop(again);
}
