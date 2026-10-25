use super::IoError;
use crate::media::ffmpeg_lib::FfmpegApi;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

type GetLevel = unsafe extern "C" fn() -> i32;
type SetLevel = unsafe extern "C" fn(i32);
struct Lease {
    readers: usize,
    previous: i32,
}
fn leases() -> &'static Mutex<HashMap<usize, Lease>> {
    static LEASES: OnceLock<Mutex<HashMap<usize, Lease>>> = OnceLock::new();
    LEASES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// FFmpeg 错误文本可能包含带凭据 URL；原生网络会话存续期间静默同实例日志。
/// 不安装回调，避免 DLL 卸载后原生库保留失效函数指针。
pub(super) struct Guard {
    _api: Arc<FfmpegApi>,
    setter: SetLevel,
    key: usize,
}
impl Guard {
    pub fn new(api: Arc<FfmpegApi>) -> Result<Self, IoError> {
        let setter = api.av_log_set_level;
        let key = setter as usize;
        acquire(key, api.av_log_get_level, setter)?;
        Ok(Self {
            _api: api,
            setter,
            key,
        })
    }
}
fn acquire(key: usize, getter: GetLevel, setter: SetLevel) -> Result<(), IoError> {
    let mut leases = leases().lock().map_err(|_| IoError::Resource)?;
    if let Some(lease) = leases.get_mut(&key) {
        lease.readers = lease.readers.checked_add(1).ok_or(IoError::Resource)?;
    } else {
        // SAFETY：函数来自仍由 Guard 持有的原生库；在同一锁下成对保存/恢复。
        let previous = unsafe { getter() };
        unsafe {
            setter(super::super::ffmpeg_sys::AV_LOG_QUIET);
        }
        leases.insert(
            key,
            Lease {
                readers: 1,
                previous,
            },
        );
    }
    Ok(())
}
fn release(key: usize, setter: SetLevel) {
    let mut leases = leases().lock().unwrap_or_else(|error| error.into_inner());
    if let Some(lease) = leases.get_mut(&key) {
        lease.readers -= 1;
        if lease.readers == 0 {
            let previous = lease.previous;
            leases.remove(&key);
            // SAFETY：Guard 在此调用返回前保留 api 的强引用。
            unsafe {
                setter(previous);
            }
        }
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        release(self.key, self.setter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, Ordering};
    static LEVEL: AtomicI32 = AtomicI32::new(32);
    unsafe extern "C" fn get() -> i32 {
        LEVEL.load(Ordering::Acquire)
    }
    unsafe extern "C" fn set(value: i32) {
        LEVEL.store(value, Ordering::Release);
    }
    #[test]
    fn last_reader_restores_log_level() {
        let key = set as *const () as usize;
        acquire(key, get, set).unwrap();
        acquire(key, get, set).unwrap();
        assert_eq!(LEVEL.load(Ordering::Acquire), -8);
        release(key, set);
        assert_eq!(LEVEL.load(Ordering::Acquire), -8);
        release(key, set);
        assert_eq!(LEVEL.load(Ordering::Acquire), 32);
    }
}
