#[cfg(windows)]
use crate::Error;
use crate::Result;
use std::path::Path;

pub struct CheckedLibrary {
    #[cfg(windows)]
    module: *mut std::ffi::c_void,
}
impl Drop for CheckedLibrary {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            FreeLibrary(self.module);
        }
    }
}
pub fn preflight(path: &Path) -> Result<CheckedLibrary> {
    #[cfg(windows)]
    {
        let path = path
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let module = unsafe { LoadLibraryExW(path.as_ptr(), std::ptr::null_mut(), 0x1100) };
        if module.is_null() {
            return Err(Error("vision.runtimeLoadFailed".into()));
        }
        let library = CheckedLibrary { module };
        let symbol = unsafe { GetProcAddress(module, c"OrtGetApiBase".as_ptr()) };
        if symbol.is_null() {
            return Err(Error("vision.runtimeLoadFailed".into()));
        }
        let getter: unsafe extern "system" fn() -> *const ort::sys::OrtApiBase =
            unsafe { std::mem::transmute(symbol) };
        let base = unsafe { getter() };
        if base.is_null() {
            return Err(Error("vision.runtimeLoadFailed".into()));
        }
        let version = unsafe { ((*base).GetVersionString)() };
        if version.is_null() {
            return Err(Error("vision.runtimeLoadFailed".into()));
        }
        let version = unsafe { std::ffi::CStr::from_ptr(version) }.to_string_lossy();
        let mut parts = version.split('.');
        if parts.next() != Some("1")
            || parts
                .next()
                .and_then(|v| v.parse::<u32>().ok())
                .is_none_or(|v| v < ort::sys::ORT_API_VERSION)
            || unsafe { ((*base).GetApi)(ort::sys::ORT_API_VERSION) }.is_null()
        {
            return Err(Error("vision.runtimeLoadFailed".into()));
        }
        // 在ort初始化前拒绝损坏/缺符号/旧API；持有句柄至其加载完成，防止失败污染一次槽。
        Ok(library)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Ok(CheckedLibrary {})
    }
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut std::ffi::c_void;
    fn GetModuleFileNameW(module: *mut std::ffi::c_void, path: *mut u16, size: u32) -> u32;
    fn LoadLibraryExW(
        path: *const u16,
        file: *mut std::ffi::c_void,
        flags: u32,
    ) -> *mut std::ffi::c_void;
    fn GetProcAddress(
        module: *mut std::ffi::c_void,
        name: *const std::ffi::c_char,
    ) -> *mut std::ffi::c_void;
    fn FreeLibrary(module: *mut std::ffi::c_void) -> i32;
}
pub fn check_loaded(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        let name = "onnxruntime.dll\0".encode_utf16().collect::<Vec<_>>();
        let module = unsafe { GetModuleHandleW(name.as_ptr()) };
        if !module.is_null() {
            let mut buffer = [0u16; 32768];
            let count =
                unsafe { GetModuleFileNameW(module, buffer.as_mut_ptr(), buffer.len() as u32) }
                    as usize;
            if count == 0 || count == buffer.len() {
                return Err(Error("vision.backendConflict".into()));
            }
            let loaded = std::path::PathBuf::from(String::from_utf16_lossy(&buffer[..count]))
                .canonicalize()
                .map_err(|_| Error("vision.backendConflict".into()))?;
            if loaded != path {
                return Err(Error("vision.backendConflict".into()));
            }
        }
    }
    let _ = path;
    Ok(())
}
