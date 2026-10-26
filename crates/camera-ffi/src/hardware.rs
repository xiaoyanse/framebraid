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
const MAX_REPORT: usize = 64 * 1024;
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
    if let Some(slot) = services.get(&id) {
        return slot
            .clone()
            .downcast::<Slot>()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR);
    }
    let slot = Arc::new(Slot::default());
    services.insert(id, slot.clone());
    Ok(slot)
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
        let mut charge = state.control(MAX_REPORT, false)?;
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
        let value = serde_json::json!({"schema":"framebraid.decode-hardware/1", "kind":"probe",
            "state":"probe_complete", "selection":selected.report, "actual_backend":null,
            "last_failure":null, "stream_verified":false,
            "completed_at_ms":am_sdk_support::clock::monotonic_ms()});
        let bytes = serialize(value)?;
        charge.shrink_to(bytes.capacity() + std::mem::size_of::<Cached>() + 16)?;
        *slot.value.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)? = Some(Arc::new(Cached {
            bytes,
            _charge: charge,
        }));
        Ok(())
    })
}
fn serialize(value: serde_json::Value) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(&value).map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
    if bytes.len() >= MAX_REPORT {
        return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
    }
    Ok(bytes)
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
        let state = crate::decoder::hardware_report_owner(object as usize)?;
        let _temporary = state.control(QUERY_RESERVATION, false)?;
        let bytes = serialize(crate::decoder::hardware_report(object as usize)?)?;
        unsafe { copy_report(&bytes, buffer, capacity, required) }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blocking_probe_rejects_callback_before_touching_handles_or_drivers() {
        let _callback = am_sdk_support::callbacks::Scope::enter();
        assert_eq!(
            unsafe { amc_probe_decode_hardware(std::ptr::null_mut(), AM_CODEC_H264, 1) },
            AM_ERR_SDK_CALLBACK_CONTRACT_VIOLATION
        );
    }
    #[test]
    fn json_copy_is_owned_bounded_nul_terminated_and_rejects_overlap() {
        let mut needed = 0;
        assert_eq!(
            unsafe { copy_report(b"{}", std::ptr::null_mut(), 0, &mut needed) },
            Ok(())
        );
        assert_eq!(needed, 3);
        let mut out = [42i8; 4];
        assert_eq!(
            unsafe { copy_report(b"{}", out.as_mut_ptr(), 2, &mut needed) },
            Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE)
        );
        assert_eq!(out, [0, 42, 42, 42]);
        assert_eq!(
            unsafe { copy_report(b"{}", out.as_mut_ptr(), 4, &mut needed) },
            Ok(())
        );
        assert_eq!(out, [123, 125, 0, 42]);
        let mut overlap = 0u64;
        let ptr = &mut overlap as *mut u64;
        assert_eq!(
            unsafe { copy_report(b"{}", ptr.cast(), 8, ptr) },
            Err(AM_ERR_SDK_INVALID_ARGUMENT)
        );
    }
}
