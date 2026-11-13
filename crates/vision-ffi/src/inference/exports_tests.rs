//! 审计盲区回归：amv_result_fetch 非终态TRY_AGAIN与终态错误码透传；
//! amv_set_realtime_policy 参数边界校验与生效。
use super::{
    exports::{amv_result_fetch, amv_set_realtime_policy},
    models::{Info, ModelHandle},
    state::{Record, State},
};
use crate::domain;
use am_sdk_support::{abi::*, context};
use std::{
    mem::size_of,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

fn open_context() -> usize {
    let params = amv_init_params_t {
        struct_size: size_of::<amv_init_params_t>() as u32,
        abi_version: AM_ABI_VERSION,
        infer_threads: 2,
        ..Default::default()
    };
    // 并行测试共享进程域的上下文准入上限（且telemetry测试按设计保留上下文至进程
    // 结束）；名额被占满时等待其它测试释放
    for _ in 0..600 {
        match unsafe { context::initialize_vision(domain(), &params) } {
            Ok(key) => return key,
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED) => {
                std::thread::sleep(std::time::Duration::from_millis(5))
            }
            Err(other) => panic!("vision ctx init: {other}"),
        }
    }
    panic!("vision ctx admission never freed")
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

#[test]
fn realtime_policy_validates_bounds_and_takes_effect_on_model() {
    let ctx = open_context();
    let state = State::get(ctx).unwrap();
    let handle = Arc::new(ModelHandle {
        session: Mutex::new(None),
        selection: Mutex::new(None),
        info: Info::stub(),
        state: state.clone(),
        inflight: AtomicUsize::new(0),
        stopping: AtomicBool::new(false),
        busy: AtomicBool::new(false),
        policy: Mutex::new(am_realtime_policy_t::default()),
        permit: Mutex::new(None),
        notify: Arc::default(),
        vocab: std::sync::RwLock::new(Arc::new(None)),
        pool: AtomicUsize::new(0),
    });
    let key = domain().insert(ctx, handle.clone()).unwrap();
    state.models.fetch_add(1, Ordering::AcqRel);
    let set = |p: *const am_realtime_policy_t| unsafe {
        amv_set_realtime_policy(key as *mut amv_model_t, p)
    };

    // 参数闸：空指针与短struct_size
    assert_eq!(set(std::ptr::null()), AM_ERR_SDK_NULL_ARGUMENT);
    let short = am_realtime_policy_t {
        struct_size: 4,
        ..Default::default()
    };
    assert_eq!(set(&short), AM_ERR_SDK_STRUCT_SIZE_INVALID);
    // 越界值逐字段拒绝
    for violated in [
        am_realtime_policy_t {
            max_inflight: 17,
            ..Default::default()
        },
        am_realtime_policy_t {
            slot_count: 17,
            ..Default::default()
        },
        am_realtime_policy_t {
            replace_oldest: 2,
            ..Default::default()
        },
        am_realtime_policy_t {
            fair_round_robin: 2,
            ..Default::default()
        },
    ] {
        assert_eq!(
            set(&violated),
            AM_ERR_SDK_LIMIT_OUT_OF_RANGE,
            "越界字段必须拒绝：{:?}",
            (
                violated.max_inflight,
                violated.slot_count,
                violated.replace_oldest,
                violated.fair_round_robin
            )
        );
    }
    // 边界值合法且必须生效（写入模型策略）
    let boundary = am_realtime_policy_t {
        max_inflight: 16,
        slot_count: 16,
        replace_oldest: 1,
        fair_round_robin: 1,
        ..Default::default()
    };
    assert_eq!(set(&boundary), AM_ERR_SDK_OK);
    let stored = *handle.policy.lock().unwrap();
    assert_eq!(
        (
            stored.max_inflight,
            stored.slot_count,
            stored.replace_oldest,
            stored.fair_round_robin
        ),
        (16, 16, 1, 1),
        "合法策略必须写入模型生效"
    );
    // 已关闭模型：拒绝再改
    assert_eq!(
        domain().close(key, 5000).unwrap(),
        AM_LIFECYCLE_CLOSED as u32
    );
    assert_eq!(set(&boundary), AM_ERR_SDK_HANDLE_CLOSED);
    domain().destroy(key).unwrap();
    drop((handle, state));
    assert_eq!(
        domain().close(ctx, 5000).unwrap(),
        AM_LIFECYCLE_CLOSED as u32
    );
    domain().destroy(ctx).unwrap();
}
