use super::selection::*;
use am_sdk_support::abi::*;
use std::{mem::size_of, ptr};

#[test]
#[cfg(feature = "shared-frames")]
fn newest_start_wins_and_model_cache_is_independent_and_bounded() {
    use am_vision_core::sdk::selection::{BackendSelectionPolicy, BackendTarget};
    unsafe {
        let p = amv_init_params_t {
            struct_size: size_of::<amv_init_params_t>() as u32,
            abi_version: AM_ABI_VERSION,
            infer_threads: 2,
            feature_flags: AM_INIT_SHARED_FRAMES as u64,
            ..Default::default()
        };
        let mut ctx = ptr::null_mut();
        assert_eq!(crate::amv_init(&p, &mut ctx), 0);
        let policy = BackendSelectionPolicy {
            priority: vec![BackendTarget {
                backend: 0,
                device_id: 0,
            }],
            allow_fallback: false,
            budget_ms: 1500,
        };
        let older = Attempt::begin(ctx as usize, &policy).unwrap();
        let newer = Attempt::begin(ctx as usize, &policy).unwrap();
        assert!(newer.id > older.id);
        older.finish(0).unwrap();
        let mut required = 0;
        assert_eq!(
            amv_get_selection_attempt_report(ctx, older.id, ptr::null_mut(), 0, &mut required),
            AM_ERR_SDK_RESULT_EXPIRED
        );
        assert_eq!(
            amv_get_selection_attempt_report(ctx, newer.id, ptr::null_mut(), 0, &mut required),
            0
        );
        let mut bytes = vec![0i8; required as usize];
        assert_eq!(
            amv_get_selection_attempt_report(
                ctx,
                newer.id,
                bytes.as_mut_ptr(),
                required,
                &mut required
            ),
            0
        );
        let json = std::ffi::CStr::from_ptr(bytes.as_ptr()).to_str().unwrap();
        assert!(json.contains("\"pending\""));
        let state = super::state::State::get(ctx as usize).unwrap();
        let cache = older.model_cache(&state).unwrap();
        assert_eq!(
            state
                .usage
                .control_bytes
                .load(std::sync::atomic::Ordering::Acquire),
            3 * 8192
        );
        drop(older);
        drop(cache);
        assert_eq!(
            state
                .usage
                .control_bytes
                .load(std::sync::atomic::Ordering::Acquire),
            8192
        );
        drop(newer);
        let mut lifecycle = 0;
        assert_eq!(crate::amv_shutdown(ctx, 1000, &mut lifecycle), 0);
        assert_eq!(lifecycle, AM_LIFECYCLE_CLOSED as u32);
        assert_eq!(crate::amv_object_destroy(ctx.cast()), 0);
        assert_eq!(
            state
                .usage
                .control_bytes
                .load(std::sync::atomic::Ordering::Acquire),
            0
        );
    }
}

#[test]
fn candidate_validation_rejects_ambiguous_or_unbounded_policies() {
    let candidates = [am_backend_candidate_t {
        backend: 0,
        device_id: 1,
    }];
    let mut p = am_backend_selection_t {
        struct_size: size_of::<am_backend_selection_t>() as u32,
        candidate_count: 1,
        candidates: candidates.as_ptr(),
        allow_fallback: 0,
        budget_ms: 1500,
    };
    assert!(unsafe { policy(&p) }.is_err());
    p.candidate_count = 4;
    assert!(unsafe { policy(&p) }.is_err());
    p.candidate_count = 0;
    p.candidates = ptr::null();
    assert!(unsafe { policy(&p) }.is_err());
}
