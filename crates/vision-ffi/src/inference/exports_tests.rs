//! 审计盲区回归：amv_result_fetch 非终态TRY_AGAIN与终态错误码透传。
use super::{
    exports::amv_result_fetch,
    state::{Record, State},
};
use crate::domain;
use am_sdk_support::{abi::*, context};
use std::{mem::size_of, sync::atomic::Ordering};

fn open_context() -> usize {
    let params = amv_init_params_t {
        struct_size: size_of::<amv_init_params_t>() as u32,
        abi_version: AM_ABI_VERSION,
        infer_threads: 2,
        ..Default::default()
    };
    unsafe { context::initialize_vision(domain(), &params) }.expect("vision ctx")
}

fn seed(state: &State, request_state: u32, code: i32) -> u64 {
    let mut records = state.records.lock().unwrap();
    records.reserve(state, 8, 64).unwrap();
    let id = state.prefix | state.next.fetch_add(1, Ordering::AcqRel);
    records.insert(
        id,
        Record {
            model: 1,
            slot: 0,
            state: request_state,
            code,
            created: 0,
            started: 0,
            finished: 0,
            expires: i64::MAX,
            result: None,
            canceled: false,
            offline: false,
        },
    );
    id
}

#[test]
fn result_fetch_gates_non_terminal_and_passes_through_terminal_codes() {
    let ctx = open_context();
    let state = State::get(ctx).unwrap();
    let mut out: *mut amv_result_t = std::ptr::null_mut();
    for pending in [AM_REQ_PENDING as u32, AM_REQ_RUNNING as u32] {
        let id = seed(&state, pending, 0);
        assert_eq!(
            unsafe { amv_result_fetch(ctx as *mut amv_context_t, id, &mut out) },
            AM_ERR_SDK_TRY_AGAIN,
            "非终态必须TRY_AGAIN而不是假成功"
        );
        assert!(out.is_null());
    }
    let failed = seed(&state, AM_REQ_FAILED as u32, AM_ERR_VISION_INFERENCE_FAILED);
    assert_eq!(
        unsafe { amv_result_fetch(ctx as *mut amv_context_t, failed, &mut out) },
        AM_ERR_VISION_INFERENCE_FAILED,
        "终态错误码必须原样透传"
    );
    assert!(out.is_null());
    // 取消路径由finish()落CANCELED且code恒0，fetch须以状态错误而非假成功收口。
    let canceled = seed(&state, AM_REQ_CANCELED as u32, 0);
    assert_eq!(
        unsafe { amv_result_fetch(ctx as *mut amv_context_t, canceled, &mut out) },
        AM_ERR_VISION_REQUEST_STATE_INVALID
    );
    assert!(out.is_null());
    let expired = seed(&state, AM_REQ_DONE as u32, 0);
    assert_eq!(
        unsafe { amv_result_fetch(ctx as *mut amv_context_t, expired, &mut out) },
        AM_ERR_SDK_RESULT_EXPIRED,
        "DONE但结果已不在不得凭空交付"
    );
    assert!(out.is_null());
    drop(state);
    assert_eq!(
        domain().close(ctx, 5000).unwrap(),
        AM_LIFECYCLE_CLOSED as u32
    );
    domain().destroy(ctx).unwrap();
}
