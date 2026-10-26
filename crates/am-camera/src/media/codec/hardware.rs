use super::*;
use crate::media::hardware::{Adapter, Backend, Device, ProbeError};
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) struct Binding {
    backend: Backend,
    format: i32,
    verified: AtomicBool,
}
impl Binding {
    pub(super) unsafe fn format(
        api: &FfmpegApi,
        codec: *const core::ffi::c_void,
        backend: Backend,
    ) -> Option<i32> {
        if codec.is_null() {
            return None;
        }
        for index in 0..64 {
            let config = unsafe { (api.avcodec_get_hw_config)(codec, index) };
            if config.is_null() {
                break;
            }
            let config = unsafe { &*config };
            if config.device_type == backend.device_type()
                && config.methods & AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX != 0
            {
                return Some(config.pix_fmt);
            }
        }
        None
    }
    pub(super) unsafe fn attach(
        api: Arc<FfmpegApi>,
        ctx: *mut AVCodecContext,
        codec: *const core::ffi::c_void,
        backend: Backend,
        adapter: &Adapter,
    ) -> Result<Box<Self>> {
        // 不支持目标解码配置时，不触碰驱动或初始化设备。
        unsafe { Self::format(&api, codec, backend) }.ok_or(DecodeError::InvalidInput)?;
        let device = Device::create(api.clone(), backend, adapter).map_err(|e| match e {
            ProbeError::DeviceInit(code) => DecodeError::Runtime(code),
            _ => DecodeError::InvalidInput,
        })?;
        unsafe { Self::attach_reference(&api, ctx, codec, backend, device.pointer) }
    }
    pub(super) unsafe fn attach_reference(
        api: &FfmpegApi,
        ctx: *mut AVCodecContext,
        codec: *const core::ffi::c_void,
        backend: Backend,
        device: *mut AVBufferRef,
    ) -> Result<Box<Self>> {
        if device.is_null() {
            return Err(DecodeError::InvalidInput);
        }
        let format =
            unsafe { Self::format(api, codec, backend) }.ok_or(DecodeError::InvalidInput)?;
        let binding = Box::new(Self {
            backend,
            format,
            verified: AtomicBool::new(false),
        });
        unsafe {
            // CodecContext拥有独立设备引用，guard归还原引用，避免free_context双重释放。
            (*ctx).hw_device_ctx = (api.av_buffer_ref)(device);
            if (*ctx).hw_device_ctx.is_null() {
                return Err(DecodeError::AllocationFailed);
            }
            // Box地址稳定到CodecContext释放；不把栈上像素格式留给迟到的get_format。
            (*ctx).opaque = std::ptr::from_ref(binding.as_ref()).cast_mut().cast();
            (*ctx).get_format = Some(select_format);
        }
        Ok(binding)
    }
    pub(super) fn backend(&self) -> Backend {
        self.backend
    }
    pub(super) fn observe(&self, format: i32) {
        self.verified.store(self.matches(format), Ordering::Relaxed);
    }
    pub(super) fn matches(&self, format: i32) -> bool {
        self.format == format
    }
    pub(super) fn verified_backend(&self) -> Option<Backend> {
        self.verified
            .load(Ordering::Relaxed)
            .then_some(self.backend)
    }
}

unsafe extern "C" fn select_format(ctx: *mut AVCodecContext, formats: *const i32) -> i32 {
    if ctx.is_null() || formats.is_null() {
        return -1;
    }
    let binding = unsafe { (*ctx).opaque.cast::<Binding>().as_ref() };
    let Some(binding) = binding else { return -1 };
    for index in 0..64 {
        let format = unsafe { *formats.add(index) };
        if format == -1 {
            break;
        }
        if binding.matches(format) {
            return format;
        }
    }
    // 显式硬件构造失败应报错，由上层策略重建CPU上下文，不悄悄选另一硬件格式。
    -1
}
