//! CUDA 互操作实测（M2b 3.3）：NVIDIA D3D11 纹理注册→映射→子资源数组。
//! 真实硬件链路，需 AM_SDK_ORT_DIR（含 cudart64_*.dll 的运行库目录）。
use super::cuda_sys::CudaApi;
use super::interop::{nvidia_nv12_texture, RegisteredTexture};

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
