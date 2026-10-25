//! 映射与进程对象均RAII管理；命名对象只授权当前登录SID且禁止继承。
use crate::{abi::*, Result};
use std::{
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::Cryptography::*,
    Security::{Authorization::*, *},
    System::{Memory::*, RemoteDesktop::*, Threading::*},
};

pub struct Handle(pub HANDLE);
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
impl Handle {
    pub fn checked(raw: HANDLE) -> Result<Self> {
        if raw.is_null() || raw == INVALID_HANDLE_VALUE {
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
        } else {
            Ok(Self(raw))
        }
    }
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn sid(process: HANDLE) -> Result<String> {
    unsafe {
        let mut token = ptr::null_mut();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            return Err(AM_ERR_SDK_NOT_SUPPORTED);
        }
        let token = Handle::checked(token)?;
        let mut length = 0;
        GetTokenInformation(token.0, TokenUser, ptr::null_mut(), 0, &mut length);
        if length == 0 || length > 65536 {
            return Err(AM_ERR_SDK_NOT_SUPPORTED);
        }
        let mut buffer = vec![0u64; (length as usize).div_ceil(8)];
        if GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            length,
            &mut length,
        ) == 0
        {
            return Err(AM_ERR_SDK_NOT_SUPPORTED);
        }
        let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
        let mut text = ptr::null_mut();
        if ConvertSidToStringSidW(user.User.Sid, &mut text) == 0 {
            return Err(AM_ERR_SDK_NOT_SUPPORTED);
        }
        let mut n = 0;
        while *text.add(n) != 0 && n < 1024 {
            n += 1;
        }
        let result = String::from_utf16(std::slice::from_raw_parts(text, n))
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR);
        LocalFree(text.cast());
        result
    }
}
pub struct Security {
    descriptor: PSECURITY_DESCRIPTOR,
    pub attrs: SECURITY_ATTRIBUTES,
}
impl Security {
    pub fn current() -> Result<Self> {
        let sddl = wide(&format!(
            "D:P(A;;GA;;;{})",
            sid(unsafe { GetCurrentProcess() })?
        ));
        let mut descriptor = ptr::null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut descriptor,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(AM_ERR_SDK_NOT_SUPPORTED);
        }
        Ok(Self {
            descriptor,
            attrs: SECURITY_ATTRIBUTES {
                nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor,
                bInheritHandle: 0,
            },
        })
    }
}
impl Drop for Security {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.descriptor);
        }
    }
}

pub struct Mapping {
    pub ptr: *mut u8,
    pub len: usize,
    _handle: Handle,
    pub writable: bool,
}
unsafe impl Send for Mapping {}
unsafe impl Sync for Mapping {}
impl Mapping {
    fn view(handle: Handle, len: usize, writable: bool) -> Result<Self> {
        let access = if writable {
            FILE_MAP_WRITE | FILE_MAP_READ
        } else {
            FILE_MAP_READ
        };
        let view = unsafe { MapViewOfFile(handle.0, access, 0, 0, len) };
        if view.Value.is_null() {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        Ok(Self {
            ptr: view.Value.cast(),
            len,
            _handle: handle,
            writable,
        })
    }
    pub fn create(name: &str, len: usize, security: &Security) -> Result<Self> {
        let name = wide(name);
        let raw = unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                &security.attrs,
                PAGE_READWRITE,
                ((len as u64) >> 32) as u32,
                len as u32,
                name.as_ptr(),
            )
        };
        let existed = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        let handle = Handle::checked(raw)?;
        if existed {
            return Err(AM_ERR_SDK_TRY_AGAIN);
        }
        Self::view(handle, len, true)
    }
    pub fn open(name: &str, len: usize, writable: bool) -> Result<Self> {
        let access = if writable {
            FILE_MAP_WRITE | FILE_MAP_READ
        } else {
            FILE_MAP_READ
        };
        let raw = unsafe { OpenFileMappingW(access, 0, wide(name).as_ptr()) };
        Self::view(
            Handle::checked(raw).map_err(|_| AM_ERR_SDK_GENERATION_MISMATCH)?,
            len,
            writable,
        )
    }
    pub fn release_signal(&self, offset: usize, value: u64) {
        // 唯一无锁共享字为归还意向；实际租约状态仍由命名互斥量保护。
        debug_assert!(offset.is_multiple_of(8) && offset + 8 <= self.len && self.writable);
        unsafe {
            AtomicU64::from_ptr(self.ptr.add(offset).cast()).store(value, Ordering::Release);
        }
    }
    pub fn release_read(&self, offset: usize) -> u64 {
        debug_assert!(offset.is_multiple_of(8) && offset + 8 <= self.len);
        unsafe { AtomicU64::from_ptr(self.ptr.add(offset).cast()).load(Ordering::Acquire) }
    }
}
impl Drop for Mapping {
    fn drop(&mut self) {
        unsafe {
            UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                Value: self.ptr.cast(),
            });
        }
    }
}
pub struct NamedMutex(Handle);
pub struct Guard<'a>(&'a NamedMutex);
impl NamedMutex {
    pub fn create(name: &str, security: &Security) -> Result<Self> {
        let raw = unsafe { CreateMutexW(&security.attrs, 0, wide(name).as_ptr()) };
        let existed = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        let handle = Handle::checked(raw)?;
        if existed {
            return Err(AM_ERR_SDK_TRY_AGAIN);
        }
        Ok(Self(handle))
    }
    pub fn open(name: &str) -> Result<Self> {
        Ok(Self(
            Handle::checked(unsafe {
                OpenMutexW(
                    SYNCHRONIZATION_SYNCHRONIZE | MUTEX_MODIFY_STATE,
                    0,
                    wide(name).as_ptr(),
                )
            })
            .map_err(|_| AM_ERR_SDK_GENERATION_MISMATCH)?,
        ))
    }
    pub fn lock(&self, wait: u32) -> Result<(Guard<'_>, bool)> {
        match unsafe { WaitForSingleObject(self.0 .0, wait) } {
            WAIT_OBJECT_0 => Ok((Guard(self), false)),
            WAIT_ABANDONED => Ok((Guard(self), true)),
            WAIT_TIMEOUT => Err(AM_ERR_SDK_TRY_AGAIN),
            _ => Err(AM_ERR_SDK_INTERNAL_ERROR),
        }
    }
}
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.0 .0 .0);
        }
    }
}

pub struct Process {
    handle: Handle,
    pub pid: u64,
    pub started: u64,
}
impl Process {
    pub fn current() -> Result<Self> {
        Self::open(unsafe { GetCurrentProcessId() } as u64, None)
    }
    pub fn open(pid: u64, expected: Option<u64>) -> Result<Self> {
        let id = u32::try_from(pid).map_err(|_| AM_ERR_SDK_GENERATION_MISMATCH)?;
        let handle = Handle::checked(unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZATION_SYNCHRONIZE,
                0,
                id,
            )
        })
        .map_err(|_| AM_ERR_SDK_GENERATION_MISMATCH)?;
        let mut created = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        if unsafe { GetProcessTimes(handle.0, &mut created, &mut exit, &mut kernel, &mut user) }
            == 0
        {
            return Err(AM_ERR_SDK_NOT_SUPPORTED);
        }
        let started = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
        if expected.is_some_and(|e| e != started) {
            return Err(AM_ERR_SDK_GENERATION_MISMATCH);
        }
        let mut session = 0;
        let mut own_session = 0;
        if unsafe { ProcessIdToSessionId(id, &mut session) } == 0
            || unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut own_session) } == 0
            || session != own_session
            || sid(handle.0)? != sid(unsafe { GetCurrentProcess() })?
        {
            return Err(AM_ERR_SDK_NOT_SUPPORTED);
        }
        Ok(Self {
            handle,
            pid,
            started,
        })
    }
    pub fn exited(&self) -> bool {
        unsafe { WaitForSingleObject(self.handle.0, 0) == WAIT_OBJECT_0 }
    }
}
pub fn identity() -> Result<[u8; 32]> {
    let mut result = [0; 32];
    if unsafe {
        BCryptGenRandom(
            ptr::null_mut(),
            result.as_mut_ptr(),
            32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    } != 0
    {
        return Err(AM_ERR_SDK_INTERNAL_ERROR);
    }
    Ok(result)
}
pub fn name(tag: &[u8; 32], kind: &str) -> String {
    let hex: String = tag.iter().map(|b| format!("{b:02x}")).collect();
    format!("Local\\amrt-sfp-v1-{hex}-{kind}")
}
