use super::*;
#[cfg(all(windows, target_arch = "x86_64", feature = "shared-frames"))]
use am_sdk_support::context::Context;
use am_sdk_support::{args, buffers::Buffer, clock};
use std::{
    any::Any,
    ffi::CString,
    mem::size_of,
    sync::{atomic::Ordering, Arc},
};
struct Snapshot {
    value: Box<am_stats_t>,
    _scope: CString,
    _charge: frames::ControlCharge,
}
// 输出指针仅指向同一只读拥有者的CString，填充经args::write清除。
unsafe impl Send for Snapshot {}
unsafe impl Sync for Snapshot {}
impl Buffer for Snapshot {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
#[no_mangle]
pub unsafe extern "C" fn amc_get_stats(
    ctx: *mut amc_context_t,
    scope: *const std::ffi::c_char,
    out: *mut *mut am_stats_t,
) -> i32 {
    status(domain(), ctx as usize, || unsafe {
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
        let entry = domain().get(ctx as usize)?;
        let _read = entry.readable()?;
        let state = frames::camera_state(ctx as usize)?;
        let charge = state.control(
            size_of::<Snapshot>() + size_of::<am_stats_t>() + scope.len() + 1,
            true,
        )?;
        let name = CString::new(scope).map_err(|_| AM_ERR_SDK_INVALID_ARGUMENT)?;
        let (_bytes, external_bytes, external_frames) = state.usage_snapshot()?;
        let source_queue = sources::queue_usage(ctx as usize);
        let captures = captures::queue_usage(ctx as usize)?;
        let queue_items = source_queue
            .0
            .saturating_add(captures.0)
            .saturating_add(state.metrics.decoder_queue_items.load(Ordering::Acquire));
        let queue_bytes = source_queue
            .1
            .saturating_add(captures.1)
            .saturating_add(state.metrics.decoder_queue_bytes.load(Ordering::Acquire));
        #[cfg(all(windows, target_arch = "x86_64", feature = "shared-frames"))]
        let shared = {
            let context = entry.data::<Context>()?;
            let value = context
                .shared
                .lock()
                .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?
                .summary()?;
            value
        };
        #[cfg(not(all(windows, target_arch = "x86_64", feature = "shared-frames")))]
        let shared = (0, 0, 0, 0);
        let m = &state.metrics;
        let received = m.newest_received.load(Ordering::Acquire);
        let (threads_active, threads_peak) = m.threads.snapshot()?;
        let candidate = am_stats_t {
            scope_utf8: name.as_ptr(),
            queue_items,
            queue_bytes,
            external_held_items: external_frames as u64,
            external_held_bytes: external_bytes,
            accepted: m.accepted.load(Ordering::Acquire),
            rejected_backpressure: m.rejected.load(Ordering::Acquire),
            dropped: m.dropped.load(Ordering::Acquire),
            frames_decoded: m.decoded.load(Ordering::Acquire),
            frames_dropped: m.frames_dropped.load(Ordering::Acquire),
            threads_active,
            threads_peak,
            newest_frame_age_ms: if received > 0 {
                clock::monotonic_ms().saturating_sub(received)
            } else {
                -1
            },
            reconnect_count: m.reconnects.load(Ordering::Acquire),
            resync_count: m.resyncs.load(Ordering::Acquire),
            actual_capture_fps: f64::from_bits(m.capture_fps_bits.load(Ordering::Acquire)),
            shared_mapped_bytes: shared.0,
            shared_metadata_bytes: shared.1,
            shared_pool_count: shared.2,
            shared_outstanding_borrows: shared.3,
            ..Default::default()
        };
        let mut value = Box::new(am_stats_t::default());
        args::write(&mut *value, candidate)?;
        let owner = Arc::new(Snapshot {
            value,
            _scope: name,
            _charge: charge,
        });
        let key = (&*owner.value) as *const am_stats_t as usize;
        domain().buffers.insert(key, owner)?;
        out.write(key as _);
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_stats_release(pointer: *mut am_stats_t) -> i32 {
    status(domain(), 0, || {
        let owner = domain().buffers.get(pointer as usize)?;
        if owner.as_any().downcast_ref::<Snapshot>().is_none() {
            return Err(AM_ERR_SDK_HANDLE_INVALID);
        }
        domain().buffers.release(pointer as usize)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_scope_gate_and_release_type_checks() {
        let params = amc_init_params_t {
            abi_version: AM_ABI_VERSION,
            ..Default::default()
        };
        // 并行测试共享进程域的上下文准入上限；名额被占满时等待其它测试释放
        let mut admitted = None;
        for _ in 0..600 {
            match unsafe { am_sdk_support::context::initialize_camera(domain(), &params) } {
                Ok(key) => {
                    admitted = Some(key);
                    break;
                }
                Err(AM_ERR_SDK_RESOURCE_EXHAUSTED) => {
                    std::thread::sleep(std::time::Duration::from_millis(5))
                }
                Err(other) => panic!("camera ctx init: {other}"),
            }
        }
        let ctx = admitted.expect("camera ctx admission never freed");
        let mut out: *mut am_stats_t = std::ptr::null_mut();
        // 未知scope拒绝；空scope默认context
        assert_eq!(
            unsafe { amc_get_stats(ctx as *mut amc_context_t, c"models".as_ptr(), &mut out) },
            AM_ERR_SDK_INVALID_ARGUMENT
        );
        assert!(out.is_null());
        assert_eq!(
            unsafe { amc_get_stats(ctx as *mut amc_context_t, std::ptr::null(), &mut out) },
            AM_OK as i32
        );
        assert!(!out.is_null());
        let stats = unsafe { *out };
        assert_eq!(stats.struct_size as usize, size_of::<am_stats_t>());
        assert_eq!(
            unsafe { std::ffi::CStr::from_ptr(stats.scope_utf8).to_bytes() },
            b"context",
            "scope字符串必须指向拥有者的context副本"
        );
        // 正确视图可释放；双重释放与外来指针按类型/登记拒绝
        assert_eq!(unsafe { amc_stats_release(out) }, AM_OK as i32);
        assert_eq!(
            unsafe { amc_stats_release(out) },
            AM_ERR_SDK_HANDLE_INVALID,
            "双重释放必须拒绝"
        );
        assert_eq!(
            unsafe { amc_stats_release(0x777_0000 as *mut am_stats_t) },
            AM_ERR_SDK_HANDLE_INVALID
        );
        domain().close(ctx, 1000).unwrap();
        domain().destroy(ctx).unwrap();
    }
}
