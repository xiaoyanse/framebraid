//! D3D11↔CUDA 互操作（M2b 3.3）：纹理注册/映射生命周期。
//! 注册与 unmap 全程限制在 GPU 帧租约窗口内（合同 §3.3a），不跨租约持有纹理。
use super::cuda_sys::{CudaApi, CudaArray, GraphicsResource, REGISTER_DEFAULT};
use std::ffi::c_void;
use std::sync::Arc;

/// 注册的 D3D11 纹理：Drop 注销（unregister），借用期内有效。
pub struct RegisteredTexture {
    api: Arc<CudaApi>,
    resource: GraphicsResource,
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
        Ok(Self { api, resource })
    }

    /// 映射并取子资源数组；返回 RAII 映射守卫（Drop unmap）。
    pub fn map_subresource(&self, subresource: u32) -> Result<MappedArray<'_>, crate::Error> {
        let code = unsafe { (self.api.map_resources)(1, &self.resource, std::ptr::null_mut()) };
        self.api.check(code, "映射资源")?;
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

/// 映射中的 CUDA 数组：Drop 自动 unmap；持有期间纹理不可被 GPU 写入方复用。
pub struct MappedArray<'a> {
    api: Arc<CudaApi>,
    resource: GraphicsResource,
    /// 子资源映射数组（借用到 unmap 为止；kernel 采样输入）。
    pub array: CudaArray,
    _lease: std::marker::PhantomData<&'a RegisteredTexture>,
}

impl Drop for MappedArray<'_> {
    fn drop(&mut self) {
        unsafe { (self.api.unmap_resources)(1, &self.resource, std::ptr::null_mut()) };
    }
}
