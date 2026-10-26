//! ABI 1.1显式探测及只读JSON快照；查询不创建原生会话。
use super::*;
use am_camera_core::media::codec::{selection::HardwarePolicy, CpuDecoder, VideoCodec};
use am_sdk_support::{args, context::Context, Result};
use std::{
    ffi::{c_char, c_void},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
pub(crate) const REPORT_BYTES: usize = 16 * 1024;
const QUERY_RESERVATION: usize = 32 * 1024;
struct Cached {
    bytes: Vec<u8>,
    _charge: frames::ControlCharge,
}
#[derive(Default)]
struct Slot {
    value: Mutex<Option<Arc<Cached>>>,
    busy: AtomicBool,
}
struct ProbeGuard<'a>(&'a AtomicBool);
impl Drop for ProbeGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
fn probe_slot(state: &frames::CameraState) -> Result<Arc<Slot>> {
    let mut services = state
        .ctx
        .data::<Context>()?
        .services
        .lock()
        .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
    let id = std::any::TypeId::of::<Slot>();
    services
        .get_or_insert_with(id, || Arc::new(Slot::default()))?
        .downcast::<Slot>()
        .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)
}
#[no_mangle]
pub unsafe extern "C" fn amc_probe_decode_hardware(
    ctx: *mut amc_context_t,
    codec: u32,
    budget_ms: u32,
) -> i32 {
    status(domain(), ctx as usize, || {
        am_sdk_support::callbacks::reject_reentry()?;
        let _active = domain().active(ctx as usize)?;
        let codec = match codec {
            AM_CODEC_H264 => VideoCodec::H264,
            AM_CODEC_H265 => VideoCodec::H265,
            _ => return Err(AM_ERR_CAMERA_CODEC_UNSUPPORTED),
        };
        if budget_ms == 0 || budget_ms > 5000 {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        let state = frames::camera_state(ctx as usize)?;
        let slot = probe_slot(&state)?;
        if slot.busy.swap(true, Ordering::AcqRel) {
            return Err(AM_ERR_SDK_TRY_AGAIN);
        }
        let _probe = ProbeGuard(&slot.busy);
        let _permit = state.decoder()?;
        let previous = slot
            .value
            .lock()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?
            .take();
        drop(previous);
        let mut charge = state.control(QUERY_RESERVATION, false)?;
        let pixels = (state.limits()?.frame_budget_bytes / 4).min(8192 * 8192);
        if pixels == 0 {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        let selected = CpuDecoder::create_with_policy(
            crate::decoder::config::runtime(&state)?,
            codec,
            &[],
            pixels,
            HardwarePolicy::PreferHardware,
            Duration::from_millis(budget_ms as u64),
        )
        .map_err(crate::decoder::config::convert)?;
        // 本入口测试并释放设备；绝不将初始化测试报告成正在执行码流。
        drop(selected.decoder);
        #[derive(serde::Serialize)]
        struct Report<'a> {
            schema: &'static str,
            kind: &'static str,
            state: &'static str,
            selection: &'a am_camera_core::media::codec::selection::SelectionReport,
            actual_backend: Option<&'static str>,
            last_failure: Option<&'static str>,
            stream_verified: bool,
            completed_at_ms: i64,
        }
        let bytes = serialize(&Report {
            schema: "framebraid.decode-hardware/1",
            kind: "probe",
            state: "probe_complete",
            selection: &selected.report,
            actual_backend: None,
            last_failure: None,
            stream_verified: false,
            completed_at_ms: am_sdk_support::clock::monotonic_ms(),
        })?;
        drop(selected.report);
        charge.shrink_to(bytes.capacity() + std::mem::size_of::<Cached>() + 16)?;
        *slot.value.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)? = Some(Arc::new(Cached {
            bytes,
            _charge: charge,
        }));
        Ok(())
    })
}
// 借用报告直接写入固定输出，超限不增长，不先构造JSON树副本。
pub(crate) fn serialize(value: &impl serde::Serialize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(REPORT_BYTES)
        .map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
    if bytes.capacity() != REPORT_BYTES {
        return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
    }
    let mut output = BoundedJson(bytes);
    serde_json::to_writer(&mut output, value).map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
    Ok(output.0)
}
struct BoundedJson(Vec<u8>);
impl std::io::Write for BoundedJson {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > REPORT_BYTES - 1 - self.0.len() {
            return Err(std::io::ErrorKind::WriteZero.into());
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
unsafe fn copy_report(
    bytes: &[u8],
    buffer: *mut c_char,
    capacity: u64,
    required: *mut u64,
) -> Result<()> {
    args::pointer(required)?;
    if capacity > 0 {
        args::pointer(buffer)?;
        let end = (buffer as usize)
            .checked_add(usize::try_from(capacity).map_err(|_| AM_ERR_SDK_INVALID_ARGUMENT)?)
            .ok_or(AM_ERR_SDK_INVALID_ARGUMENT)?;
        let required_end = (required as usize)
            .checked_add(8)
            .ok_or(AM_ERR_SDK_INVALID_ARGUMENT)?;
        if (buffer as usize) < required_end && (required as usize) < end {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
    }
    let needed = bytes.len() as u64 + 1;
    unsafe {
        required.write(needed);
    }
    if capacity == 0 {
        return Ok(());
    }
    unsafe {
        buffer.write(0);
    }
    if capacity < needed {
        return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.cast(), bytes.len());
        buffer.add(bytes.len()).write(0);
    }
    Ok(())
}
#[no_mangle]
pub unsafe extern "C" fn amc_get_decode_hardware_report(
    object: *mut c_void,
    buffer: *mut c_char,
    capacity: u64,
    required: *mut u64,
) -> i32 {
    status(domain(), object as usize, || {
        args::pointer(required)?;
        unsafe {
            required.write(0);
        }
        let entry = domain().get(object as usize)?;
        if entry.data::<Context>().is_ok() {
            let state = frames::camera_state(object as usize)?;
            let cache = probe_slot(&state)?
                .value
                .lock()
                .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?
                .clone();
            let empty = br#"{"schema":"framebraid.decode-hardware/1","kind":"probe","state":"not_probed","selection":null,"actual_backend":null,"last_failure":null,"stream_verified":false}"#;
            return unsafe {
                copy_report(
                    cache
                        .as_ref()
                        .map_or(empty.as_slice(), |cached| &cached.bytes),
                    buffer,
                    capacity,
                    required,
                )
            };
        }
        // 独立decoder保留拥有的初始化报告；序列化暂存也先过控制字节额度。
        let source = entry.data::<crate::sources::Source>().is_ok();
        let state = if source {
            crate::sources::hardware::owner(object as usize)?
        } else {
            crate::decoder::hardware_report_owner(object as usize)?
        };
        let _temporary = state.control(QUERY_RESERVATION, false)?;
        let bytes = if source {
            crate::sources::hardware::report(object as usize)?
        } else {
            crate::decoder::hardware_report(object as usize)?
        };
        unsafe { copy_report(&bytes, buffer, capacity, required) }
    })
}

#[cfg(test)]
mod tests;
