//! 显式驱动探测与独立有界快照；查询不接触原生驱动或会话。
use super::{
    error, lock,
    state::{ControlCharge, State},
};
use am_sdk_support::{abi::*, args, context::Context, status, Result};
use am_vision_core::sdk::runtime::{self, devices};
use std::{
    any::TypeId,
    ffi::c_char,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

const CACHE_RESERVATION: usize = 16 * 1024;
const QUERY_RESERVATION: usize = 48 * 1024;
const MAX_REPORT: usize = 64 * 1024;
struct Cached {
    report: devices::DeviceProbeReport,
    completed_at_ms: i64,
    _charge: ControlCharge,
}
#[derive(Default)]
struct Slot {
    busy: AtomicBool,
    value: Mutex<Option<Arc<Cached>>>,
}
struct ProbeGuard<'a>(&'a AtomicBool);
impl Drop for ProbeGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
fn slot(context: &Context) -> Result<Arc<Slot>> {
    let mut services = lock(&context.services)?;
    let value = services
        .entry(TypeId::of::<Slot>())
        .or_insert_with(|| Arc::new(Slot::default()));
    value
        .clone()
        .downcast()
        .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)
}
#[no_mangle]
pub unsafe extern "C" fn amv_probe_inference_hardware(
    ctx: *mut amv_context_t,
    budget_ms: u32,
) -> i32 {
    status(crate::domain(), ctx as usize, || {
        am_sdk_support::callbacks::reject_reentry()?;
        if budget_ms == 0 || budget_ms > 5000 {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        let active = crate::domain().active(ctx as usize)?;
        let context = active.data::<Context>()?;
        let slot = slot(context)?;
        if slot.busy.swap(true, Ordering::AcqRel) {
            return Err(AM_ERR_SDK_TRY_AGAIN);
        }
        let _guard = ProbeGuard(&slot.busy);
        lock(&slot.value)?.take();
        let state = State::get(ctx as usize)?;
        // SDK内部缓存计入控制预算，但不能伪装成调用方未归还的外部租约。
        let charge = state.cached_control(CACHE_RESERVATION)?;
        let report =
            devices::probe(Duration::from_millis(budget_ms as u64)).map_err(|e| match e {
                devices::DeviceProbeFailure::Busy => AM_ERR_SDK_TRY_AGAIN,
                _ => AM_ERR_SDK_INVALID_ARGUMENT,
            })?;
        *lock(&slot.value)? = Some(Arc::new(Cached {
            report,
            completed_at_ms: am_sdk_support::clock::monotonic_ms(),
            _charge: charge,
        }));
        Ok(())
    })
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
        let start = buffer as usize;
        let end = start
            .checked_add(usize::try_from(capacity).map_err(|_| AM_ERR_SDK_INVALID_ARGUMENT)?)
            .ok_or(AM_ERR_SDK_INVALID_ARGUMENT)?;
        let count = required as usize;
        let count_end = count.checked_add(8).ok_or(AM_ERR_SDK_INVALID_ARGUMENT)?;
        if start < count_end && count < end {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
    }
    if bytes.len() >= MAX_REPORT {
        return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
    }
    unsafe {
        required.write(bytes.len() as u64 + 1);
    }
    if capacity == 0 {
        return Ok(());
    }
    unsafe {
        buffer.write(0);
    }
    if capacity <= bytes.len() as u64 {
        return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.cast(), bytes.len());
        buffer.add(bytes.len()).write(0);
    }
    Ok(())
}
#[no_mangle]
pub unsafe extern "C" fn amv_get_inference_hardware_report(
    ctx: *mut amv_context_t,
    buffer: *mut c_char,
    capacity: u64,
    required: *mut u64,
) -> i32 {
    status(crate::domain(), ctx as usize, || {
        let entry = crate::domain().get(ctx as usize)?;
        entry.data::<Context>()?;
        let read = match entry.readable() {
            Ok(value) => Some(value),
            Err(AM_ERR_SDK_HANDLE_CLOSED) => None,
            Err(code) => return Err(code),
        };
        let lifecycle = entry.state.load(Ordering::Acquire);
        if lifecycle != AM_LIFECYCLE_RUNNING as u32 {
            let bytes = if lifecycle == AM_LIFECYCLE_CLOSED as u32 {
                br#"{"schema":"framebraid.inference-hardware/1","state":"closed","completed_at_ms":null,"device_probe":null,"observations":null,"runtime_initialized":null}"#.as_slice()
            } else {
                br#"{"schema":"framebraid.inference-hardware/1","state":"closing","completed_at_ms":null,"device_probe":null,"observations":null,"runtime_initialized":null}"#.as_slice()
            };
            return unsafe { copy_report(bytes, buffer, capacity, required) };
        }
        let _read = read.ok_or(AM_ERR_SDK_HANDLE_CLOSED)?;
        let context = entry.data::<Context>()?;
        let state = State::get(ctx as usize)?;
        let _temporary = state.cached_control(QUERY_RESERVATION)?;
        let cache = lock(&slot(context)?.value)?.clone();
        let observations = runtime::observations().map_err(error)?;
        #[derive(serde::Serialize)]
        struct Report<'a> {
            schema: &'static str,
            state: &'static str,
            completed_at_ms: Option<i64>,
            device_probe: Option<&'a devices::DeviceProbeReport>,
            observations: runtime::BackendObservations,
            runtime_initialized: bool,
        }
        let value = Report {
            schema: "framebraid.inference-hardware/1",
            state: if cache.is_some() {
                "probe_complete"
            } else {
                "not_probed"
            },
            completed_at_ms: cache.as_ref().map(|c| c.completed_at_ms),
            device_probe: cache.as_ref().map(|c| &c.report),
            observations,
            runtime_initialized: runtime::initialized(),
        };
        let mut bytes = BoundedJson(Vec::with_capacity(32 * 1024));
        serde_json::to_writer(&mut bytes, &value).map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        let bytes = bytes.0;
        unsafe { copy_report(&bytes, buffer, capacity, required) }
    })
}

// 输出有界且不构建JSON Value副本，避免探测缓存与查询暂存突破默认64KiB控制预算。
struct BoundedJson(Vec<u8>);
impl std::io::Write for BoundedJson {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.0.capacity() - self.0.len() {
            return Err(std::io::Error::other("bounded inference report"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn serializer_capacity_is_fixed_and_worst_driver_names_fit() {
        let mut bounded = BoundedJson(Vec::with_capacity(32 * 1024));
        let devices: Vec<_> = (0..16)
            .map(|ordinal| devices::CudaDevice {
                ordinal,
                name: "\u{1}".repeat(256),
                uuid: Some("0".repeat(32)),
            })
            .collect();
        serde_json::to_writer(&mut bounded, &devices).unwrap();
        assert!(bounded.0.len() > 24 * 1024 && bounded.0.len() < 32 * 1024 - 2048);
        let rest = bounded.0.capacity() - bounded.0.len();
        bounded.write_all(&vec![0; rest]).unwrap();
        assert!(bounded.write_all(&[0]).is_err());
        assert_eq!(bounded.0.capacity(), 32 * 1024);
    }
}
