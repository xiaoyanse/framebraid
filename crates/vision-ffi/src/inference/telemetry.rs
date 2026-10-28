#![allow(clippy::missing_safety_doc)]
use super::{
    lock, scheduler,
    state::{ControlCharge, State},
};
use am_sdk_support::{abi::*, args, buffers::Buffer, status};
use std::{
    any::Any,
    ffi::CString,
    mem::size_of,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

#[derive(Default)]
pub struct Metrics {
    pub accepted: AtomicU64,
    pub rejected: AtomicU64,
    pub replaced: AtomicU64,
    pub expired: AtomicU64,
    pub dropped: AtomicU64,
    pub executed: AtomicU64,
    pub failed: AtomicU64,
    pub queue_wait: AtomicU64,
    pub exec_ms: AtomicU64,
}
struct Snapshot {
    value: Box<am_stats_t>,
    _scope: CString,
    _charge: ControlCharge,
}
// 指针只引用同一拥有者的CString，输出在交付后只读。
unsafe impl Send for Snapshot {}
unsafe impl Sync for Snapshot {}
impl Buffer for Snapshot {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[no_mangle]
pub unsafe extern "C" fn amv_get_stats(
    ctx: *mut amv_context_t,
    scope: *const std::ffi::c_char,
    out: *mut *mut am_stats_t,
) -> i32 {
    status(crate::domain(), ctx as usize, || unsafe {
        args::pointer(out)?;
        out.write(std::ptr::null_mut());
        let scope = if scope.is_null() {
            String::from("context")
        } else {
            args::string(scope)?
        };
        if scope != "context" {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        let state = State::get(ctx as usize)?;
        let charge =
            state.control(size_of::<Snapshot>() + size_of::<am_stats_t>() + scope.len() + 1)?;
        let name = CString::new(scope).map_err(|_| AM_ERR_SDK_INVALID_ARGUMENT)?;
        let (pending, pending_bytes, threads, peak) = scheduler::snapshot(&state)?;
        let mut records = lock(&state.records)?;
        state.purge(&mut records);
        let cached = records.values().filter(|r| r.result.is_some()).count();
        let m = &state.metrics;
        let count = m.executed.load(Ordering::Acquire);
        #[cfg(all(windows, target_arch = "x86_64", feature = "shared-frames"))]
        let shared = {
            let ctx = state.context()?;
            let context = ctx.data::<am_sdk_support::context::Context>()?;
            let value = lock(&context.shared)?.summary()?;
            value
        };
        #[cfg(not(all(windows, target_arch = "x86_64", feature = "shared-frames")))]
        let shared = (0, 0, 0, 0);
        let (gpu_inputs, gpu_copies_per_frame) = am_vision_core::gpu::pipeline::audit::snapshot();
        let candidate = am_stats_t {
            scope_utf8: name.as_ptr(),
            queue_items: pending as u64 + cached as u64,
            queue_bytes: pending_bytes as u64 + state.usage.bytes.load(Ordering::Acquire) as u64,
            external_held_items: state.usage.external_items.load(Ordering::Acquire) as u64,
            external_held_bytes: state.usage.external_bytes.load(Ordering::Acquire) as u64,
            accepted: m.accepted.load(Ordering::Acquire),
            rejected_backpressure: m.rejected.load(Ordering::Acquire),
            replaced: m.replaced.load(Ordering::Acquire),
            expired: m.expired.load(Ordering::Acquire),
            dropped: m.dropped.load(Ordering::Acquire),
            infer_executed: count,
            infer_failed: m.failed.load(Ordering::Acquire),
            infer_queue_wait_avg_ms: m
                .queue_wait
                .load(Ordering::Acquire)
                .checked_div(count)
                .unwrap_or(0) as i64,
            infer_exec_avg_ms: m
                .exec_ms
                .load(Ordering::Acquire)
                .checked_div(count)
                .unwrap_or(0) as i64,
            newest_frame_age_ms: -1,
            threads_active: threads as u32,
            threads_peak: peak as u32,
            shared_mapped_bytes: shared.0,
            shared_metadata_bytes: shared.1,
            shared_pool_count: shared.2,
            shared_outstanding_borrows: shared.3,
            gpu_inputs,
            gpu_copies_per_frame,
            ..Default::default()
        };
        let mut value = Box::new(am_stats_t::default());
        args::write(&mut *value, candidate)?;
        drop(records);
        let owner = Arc::new(Snapshot {
            value,
            _scope: name,
            _charge: charge,
        });
        let key = (&*owner.value) as *const am_stats_t as usize;
        crate::domain().buffers.insert(key, owner)?;
        out.write(key as _);
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amv_stats_release(pointer: *mut am_stats_t) -> i32 {
    status(crate::domain(), 0, || {
        let owner = crate::domain().buffers.get(pointer as usize)?;
        if owner.as_any().downcast_ref::<Snapshot>().is_none() {
            return Err(AM_ERR_SDK_HANDLE_INVALID);
        }
        crate::domain().buffers.release(pointer as usize)
    })
}
