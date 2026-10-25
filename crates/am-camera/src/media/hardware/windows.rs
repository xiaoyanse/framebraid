use super::{Adapter, ProbeError};

#[cfg(all(windows, target_arch = "x86_64"))]
mod native {
    use super::*;
    use std::{
        ffi::c_void,
        mem::{size_of, transmute},
        ptr,
    };

    #[repr(C)]
    struct Guid {
        a: u32,
        b: u16,
        c: u16,
        d: [u8; 8],
    }
    #[repr(C)]
    pub(super) struct Description {
        name: [u16; 128],
        vendor: u32,
        device: u32,
        subsystem: u32,
        revision: u32,
        dedicated_video: usize,
        dedicated_system: usize,
        shared: usize,
        luid_low: u32,
        luid_high: i32,
        flags: u32,
    }
    // 系统库按需加载，不能因可选硬件路径让CPU DLL增加隐式装载依赖。
    unsafe fn system_library(
        name: &'static str,
    ) -> Result<libloading::os::windows::Library, ProbeError> {
        unsafe { libloading::os::windows::Library::load_with_flags(name, 0x800) }
            .map_err(|_| ProbeError::DependencyMissing(name))
    }

    struct Com(*mut c_void);
    impl Com {
        unsafe fn method(&self, index: usize) -> *const c_void {
            unsafe { *(*self.0.cast::<*const *const c_void>()).add(index) }
        }
    }
    impl Drop for Com {
        fn drop(&mut self) {
            if !self.0.is_null() {
                let release: unsafe extern "system" fn(*mut c_void) -> u32 =
                    unsafe { transmute(self.method(2)) };
                unsafe { release(self.0) };
            }
        }
    }

    pub(super) unsafe fn d3d11_luid(
        reference: *mut crate::media::ffmpeg_sys::AVBufferRef,
    ) -> Result<u64, ProbeError> {
        // 对应锁定libavutil61的AVHWDeviceContext前缀与AVD3D11VADeviceContext首字段。
        #[repr(C)]
        struct DeviceContext {
            class: *const c_void,
            kind: i32,
            hwctx: *mut c_void,
        }
        #[repr(C)]
        struct BufferRef {
            buffer: *mut c_void,
            data: *mut u8,
            size: usize,
        }
        const { assert!(std::mem::offset_of!(DeviceContext, hwctx) == 16) };
        const { assert!(std::mem::offset_of!(BufferRef, data) == 8) };
        let reference = unsafe { reference.cast::<BufferRef>().as_ref() }
            .ok_or(ProbeError::AdapterIdentityMismatch)?;
        let context = unsafe { reference.data.cast::<DeviceContext>().as_ref() }
            .ok_or(ProbeError::AdapterIdentityMismatch)?;
        if context.kind != crate::media::ffmpeg_sys::AV_HWDEVICE_TYPE_D3D11VA
            || context.hwctx.is_null()
        {
            return Err(ProbeError::AdapterIdentityMismatch);
        }
        let device = unsafe { *context.hwctx.cast::<*mut c_void>() };
        if device.is_null() {
            return Err(ProbeError::AdapterIdentityMismatch);
        }
        // 借用device不Release；QI和GetAdapter返回的引用由独立guard负责。
        let table = unsafe { *device.cast::<*const *const c_void>() };
        let query: unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32 =
            unsafe { transmute(*table) };
        let iid = Guid {
            a: 0x54ec77fa,
            b: 0x1377,
            c: 0x44e6,
            d: [0x8c, 0x32, 0x88, 0xfd, 0x5f, 0x44, 0xc8, 0x4c],
        };
        let mut dxgi = Com(ptr::null_mut());
        let rc = unsafe { query(device, &iid, &mut dxgi.0) };
        if rc < 0 || dxgi.0.is_null() {
            return Err(ProbeError::AdapterQuery(rc));
        }
        let get_adapter: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32 =
            unsafe { transmute(dxgi.method(7)) };
        let mut adapter = Com(ptr::null_mut());
        let rc = unsafe { get_adapter(dxgi.0, &mut adapter.0) };
        if rc < 0 || adapter.0.is_null() {
            return Err(ProbeError::AdapterQuery(rc));
        }
        // IDXGIAdapter::GetDesc只写前304字节；Description额外的flags不参与读取。
        let desc: unsafe extern "system" fn(*mut c_void, *mut Description) -> i32 =
            unsafe { transmute(adapter.method(8)) };
        let mut value: Description = unsafe { std::mem::zeroed() };
        let rc = unsafe { desc(adapter.0, &mut value) };
        if rc < 0 {
            return Err(ProbeError::AdapterQuery(rc));
        }
        Ok(u64::from(value.luid_low) | ((value.luid_high as u32 as u64) << 32))
    }

    pub(super) fn adapters() -> Result<Vec<Adapter>, ProbeError> {
        const { assert!(size_of::<Description>() == 312) };
        let library = unsafe { system_library("dxgi.dll") }?;
        let create = unsafe {
            library.get::<unsafe extern "system" fn(*const Guid, *mut *mut c_void) -> i32>(
                b"CreateDXGIFactory1\0",
            )
        }
        .map_err(|_| ProbeError::DependencyMissing("CreateDXGIFactory1"))?;
        let iid = Guid {
            a: 0x770aae78,
            b: 0xf26f,
            c: 0x4dba,
            d: [0xa8, 0x29, 0x25, 0x3c, 0x83, 0xd1, 0xb3, 0x87],
        };
        let mut factory = Com(ptr::null_mut());
        let rc = unsafe { create(&iid, &mut factory.0) };
        if rc < 0 || factory.0.is_null() {
            return Err(ProbeError::AdapterQuery(rc));
        }
        // IDXGIFactory1::EnumAdapters1和IDXGIAdapter1::GetDesc1，布局核对dxgi.h。
        let enumerate: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> i32 =
            unsafe { transmute(factory.method(12)) };
        let mut output = Vec::new();
        for index in 0..super::super::MAX_ADAPTERS {
            let mut adapter = Com(ptr::null_mut());
            let rc = unsafe { enumerate(factory.0, index, &mut adapter.0) };
            if rc as u32 == 0x887a0002 {
                break;
            }
            if rc < 0 || adapter.0.is_null() {
                return Err(ProbeError::AdapterQuery(rc));
            }
            let get_desc: unsafe extern "system" fn(*mut c_void, *mut Description) -> i32 =
                unsafe { transmute(adapter.method(10)) };
            let mut desc: Description = unsafe { std::mem::zeroed() };
            let rc = unsafe { get_desc(adapter.0, &mut desc) };
            if rc < 0 {
                return Err(ProbeError::AdapterQuery(rc));
            }
            let length = desc.name.iter().position(|c| *c == 0).unwrap_or(128);
            output.push(Adapter {
                index,
                name: String::from_utf16_lossy(&desc.name[..length]),
                vendor_id: desc.vendor,
                device_id: desc.device,
                luid: u64::from(desc.luid_low) | ((desc.luid_high as u32 as u64) << 32),
                software: desc.flags & 2 != 0,
                dedicated_video_bytes: desc.dedicated_video as u64,
                dxva2_index: None,
                dxva2_mapping_error: None,
            });
        }
        let mapping = map_d3d9(&mut output);
        for adapter in &mut output {
            if adapter.dxva2_index.is_none() {
                adapter.dxva2_mapping_error = Some(
                    mapping
                        .clone()
                        .err()
                        .unwrap_or(ProbeError::AdapterMappingUnavailable),
                );
            }
        }
        Ok(output)
    }

    fn map_d3d9(adapters: &mut [Adapter]) -> Result<(), ProbeError> {
        let library = unsafe { system_library("d3d9.dll") }?;
        let create = unsafe {
            library.get::<unsafe extern "system" fn(u32, *mut *mut c_void) -> i32>(
                b"Direct3DCreate9Ex\0",
            )
        }
        .map_err(|_| ProbeError::DependencyMissing("Direct3DCreate9Ex"))?;
        let mut object = Com(ptr::null_mut());
        let rc = unsafe { create(32, &mut object.0) };
        if rc < 0 || object.0.is_null() {
            return Err(ProbeError::AdapterQuery(rc));
        }
        let count: unsafe extern "system" fn(*mut c_void) -> u32 =
            unsafe { transmute(object.method(4)) };
        #[repr(C)]
        struct Luid {
            low: u32,
            high: i32,
        }
        let luid: unsafe extern "system" fn(*mut c_void, u32, *mut Luid) -> i32 =
            unsafe { transmute(object.method(21)) };
        for index in 0..unsafe { count(object.0) }.min(super::super::MAX_ADAPTERS) {
            let mut id = Luid { low: 0, high: 0 };
            let rc = unsafe { luid(object.0, index, &mut id) };
            if rc < 0 {
                return Err(ProbeError::AdapterQuery(rc));
            }
            let id = u64::from(id.low) | ((id.high as u32 as u64) << 32);
            super::super::assign_dxva2_index(adapters, id, index);
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn verify_layout() {
        assert_eq!(size_of::<Description>(), 312);
        assert_eq!(std::mem::offset_of!(Description, dedicated_video), 272);
        assert_eq!(std::mem::offset_of!(Description, flags), 304);
    }
}

pub(super) fn adapters() -> Result<Vec<Adapter>, ProbeError> {
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        native::adapters()
    }
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        Err(ProbeError::UnsupportedPlatform)
    }
}
pub(super) unsafe fn d3d11_luid(
    reference: *mut crate::media::ffmpeg_sys::AVBufferRef,
) -> Result<u64, ProbeError> {
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        unsafe { native::d3d11_luid(reference) }
    }
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        let _ = reference;
        Err(ProbeError::UnsupportedPlatform)
    }
}
#[cfg(all(test, windows, target_arch = "x86_64"))]
pub(super) fn verify_layout() {
    native::verify_layout()
}
