//! 每解码器仅保留一个转换上下文，配置变化时重建，不持有输入或输出像素地址。
use super::*;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Key {
    pub width: i32,
    pub height: i32,
    pub source: i32,
    pub target: i32,
}
pub(super) struct Converter {
    api: Arc<FfmpegApi>,
    pub key: Key,
    pub pointer: *mut SwsContext,
}
impl Converter {
    pub(super) fn create(api: Arc<FfmpegApi>, key: Key) -> Result<Self> {
        let pointer = unsafe {
            (api.sws_get_context)(
                key.width,
                key.height,
                key.source,
                key.width,
                key.height,
                key.target,
                2,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        if pointer.is_null() {
            return Err(DecodeError::AllocationFailed);
        }
        Ok(Self { api, key, pointer })
    }
}
impl Drop for Converter {
    fn drop(&mut self) {
        unsafe { (self.api.sws_free_context)(self.pointer) };
    }
}
