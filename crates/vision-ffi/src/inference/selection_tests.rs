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
            device: None,
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
        device: ptr::null(),
    };
    assert!(unsafe { policy(&p) }.is_err());
    p.candidate_count = 4;
    assert!(unsafe { policy(&p) }.is_err());
    p.candidate_count = 0;
    p.candidates = ptr::null();
    assert!(unsafe { policy(&p) }.is_err());
}

#[test]
fn device_selector_parsing_covers_the_full_contract_domain() {
    let candidates = [am_backend_candidate_t {
        backend: 0,
        device_id: 0,
    }];
    let needle = b"RTX 5060\0";
    let make = |match_bytes: u32, reserved: u32, struct_size: u32| am_device_selector_t {
        struct_size,
        match_utf8: needle.as_ptr().cast(),
        match_bytes,
        instance: 2,
        reserved,
    };
    let parse = |selector: *const am_device_selector_t| {
        let p = am_backend_selection_t {
            struct_size: size_of::<am_backend_selection_t>() as u32,
            candidate_count: 1,
            candidates: candidates.as_ptr(),
            allow_fallback: 0,
            budget_ms: 1500,
            device: selector,
        };
        unsafe { policy(&p) }
    };
    let full = size_of::<am_device_selector_t>() as u32;
    // selector NULL 与空 match_bytes 均为不定向：默认策略零变化（§3.5b）。
    let empty = make(0, 0, full);
    assert!(parse(ptr::null()).unwrap().device.is_none());
    assert!(parse(&empty).unwrap().device.is_none());
    // 合法 selector：needle 小写化（大小写不敏感匹配域）+instance 透传。
    let valid = make(needle.len() as u32, 0, full);
    let parsed = parse(&valid).unwrap().device.unwrap();
    assert_eq!(parsed.needle(), "rtx 5060");
    assert_eq!(parsed.instance(), 2);
    // reserved≠0 = sdk.invalidArgument。
    let reserved = make(needle.len() as u32, 1, full);
    assert_eq!(
        parse(&reserved).map(|_| ()).unwrap_err(),
        AM_ERR_SDK_INVALID_ARGUMENT
    );
    // match_bytes 与内容不一致（无 NUL 终结）= sdk.utf8Invalid。
    let unterminated = make(needle.len() as u32 - 1, 0, full);
    assert_eq!(
        parse(&unterminated).map(|_| ()).unwrap_err(),
        AM_ERR_SDK_UTF8_INVALID
    );
    // 前缀内嵌 NUL = 字节数与内容不一致 = sdk.utf8Invalid。
    let embedded = b"RT\0 5060\0";
    let nested = am_device_selector_t {
        struct_size: full,
        match_utf8: embedded.as_ptr().cast(),
        match_bytes: embedded.len() as u32,
        instance: 2,
        reserved: 0,
    };
    assert_eq!(
        parse(&nested).map(|_| ()).unwrap_err(),
        AM_ERR_SDK_UTF8_INVALID
    );
    // struct_size 非法 = sdk.structSizeInvalid。
    let short = make(needle.len() as u32, 0, 8);
    assert_eq!(
        parse(&short).map(|_| ()).unwrap_err(),
        AM_ERR_SDK_STRUCT_SIZE_INVALID
    );
}
