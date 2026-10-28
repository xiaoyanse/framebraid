use super::telemetry::{amv_get_stats, amv_stats_release};
use am_sdk_support::{abi::*, context::initialize_vision};
use std::{
    mem::size_of,
    ptr,
    sync::{atomic::Ordering, Mutex},
};

// GPU审计计数为进程级全局，测试间串行避免相互观测。
static AUDIT_LOCK: Mutex<()> = Mutex::new(());

fn open_context() -> usize {
    let params = amv_init_params_t {
        struct_size: size_of::<amv_init_params_t>() as u32,
        abi_version: AM_ABI_VERSION,
        infer_threads: 2,
        ..Default::default()
    };
    unsafe { initialize_vision(crate::domain(), &params) }.expect("vision context init")
}

// 无后端初始化即无关闭钩子，destroy 需 CLOSED 生命周期；条目随测试进程结束释放。

#[test]
fn stats_expose_gpu_audit_counters_after_increment() {
    let _guard = AUDIT_LOCK.lock();
    let key = open_context();
    let before = am_vision_core::gpu::pipeline::audit::snapshot();
    am_vision_core::gpu::pipeline::audit::GPU_INPUTS.fetch_add(3, Ordering::Relaxed);
    let mut out: *mut am_stats_t = ptr::null_mut();
    assert_eq!(
        unsafe { amv_get_stats(key as *mut amv_context_t, ptr::null(), &mut out) },
        0
    );
    assert!(!out.is_null());
    let stats = unsafe { *out };
    assert_eq!(
        am_vision_core::gpu::pipeline::audit::snapshot().0,
        before.0 + 3
    );
    assert_eq!(stats.gpu_inputs, before.0 + 3);
    assert_eq!(stats.gpu_copies_per_frame, before.1);
    assert_eq!(unsafe { amv_stats_release(out) }, 0);
}

#[test]
fn stats_gpu_audit_fields_match_context_snapshot() {
    let _guard = AUDIT_LOCK.lock();
    let key = open_context();
    let mut out: *mut am_stats_t = ptr::null_mut();
    assert_eq!(
        unsafe { amv_get_stats(key as *mut amv_context_t, ptr::null(), &mut out) },
        0
    );
    let stats = unsafe { *out };
    let (inputs, copies) = am_vision_core::gpu::pipeline::audit::snapshot();
    assert_eq!(stats.gpu_inputs, inputs);
    assert_eq!(stats.gpu_copies_per_frame, copies);
    assert_eq!(stats.struct_size as usize, size_of::<am_stats_t>());
    assert_eq!(unsafe { amv_stats_release(out) }, 0);
}
