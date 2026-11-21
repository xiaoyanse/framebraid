//! 驻留形态（ABI 1.9 §3.6a）单测：device 账本进出账/上限拒绝/峰值观测、
//! 非驻留 result 的 token 恒 0、readout 参数校验（非驻留拒绝/越界/失效
//! 指针/跨 ctx）、set_limits 的 device_resident_budget_bytes=0 忽略语义与
//! 只可调低。容量两段式与 readout 值对拍需真驻留张量——归真机 e2e 门
//!（resident_e2e_tests）。
use super::{
    exports::{amv_result_device_readout, amv_result_release},
    models::{Info, ModelHandle},
    results::ResultBuffer,
    state::{charge_device_resident, State, Usage},
};
use crate::domain;
use crate::{amv_get_limits, amv_set_limits};
use am_sdk_support::{abi::*, context};
use am_vision_core::sdk::{TensorData, TensorInfo};
use std::{
    mem::size_of,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
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

fn stub_handle(state: &Arc<State>) -> Arc<ModelHandle> {
    Arc::new(ModelHandle {
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
        cuda_ordinal: AtomicU32::new(0),
    })
}

fn tensor(name: &str, shape: &[i64]) -> TensorData {
    let bytes = vec![0u8; am_vision_core::sdk::byte_count(0, shape).unwrap()];
    TensorData::new(
        TensorInfo {
            name: name.into(),
            dtype: 0,
            shape: shape.to_vec(),
        },
        &bytes,
    )
    .unwrap()
}

#[test]
fn device_ledger_charges_rejects_and_releases() {
    let usage = Arc::new(Usage::default());
    let first = charge_device_resident(&usage, 60, 100).expect("预算内进账必须成功");
    assert_eq!(usage.device_resident_bytes.load(Ordering::Acquire), 60);
    assert_eq!(usage.device_resident_peak.load(Ordering::Acquire), 60);
    // 超限新提交拒绝（60+50>100），账面与峰值不动。
    let over = charge_device_resident(&usage, 50, 100);
    assert_eq!(over.err(), Some(AM_ERR_SDK_RESOURCE_EXHAUSTED));
    assert_eq!(usage.device_resident_bytes.load(Ordering::Acquire), 60);
    // 预算内第二笔：峰值抬升。
    let second = charge_device_resident(&usage, 40, 100).unwrap();
    assert_eq!(usage.device_resident_bytes.load(Ordering::Acquire), 100);
    assert_eq!(usage.device_resident_peak.load(Ordering::Acquire), 100);
    // Drop 出账（release/TTL 收敛路径）；峰值保持历史高水位。
    drop(first);
    assert_eq!(usage.device_resident_bytes.load(Ordering::Acquire), 40);
    assert_eq!(usage.device_resident_peak.load(Ordering::Acquire), 100);
    drop(second);
    assert_eq!(usage.device_resident_bytes.load(Ordering::Acquire), 0);
}

#[test]
fn device_ledger_rejects_overflowing_budget_zero() {
    let usage = Arc::new(Usage::default());
    assert_eq!(
        charge_device_resident(&usage, 1, 0).err(),
        Some(AM_ERR_SDK_RESOURCE_EXHAUSTED),
        "预算 0=驻留禁用，任何提交必须拒绝"
    );
}

#[test]
fn regular_result_keeps_token_zero_and_readout_rejects() {
    let ctx = open_context();
    let state = State::get(ctx).unwrap();
    let handle = stub_handle(&state);
    let charge = state.charge(64 * 1024).unwrap();
    let buffer = ResultBuffer::build(
        &handle,
        0x100,
        None,
        None,
        vec![tensor("output0", &[1, 2, 2])],
        None,
        None,
        0,
        0,
        charge,
    )
    .unwrap();
    // 非驻留结果：标志与令牌恒 0（合同 §3.6a）。
    assert_eq!(buffer.value.device_resident, 0);
    assert_eq!(buffer.value.device_outputs_token, 0);
    let key = buffer.transfer().unwrap() as *const amv_result_t;
    let mut needed: u64 = 0;
    // 参数闸：out_size 空指针。
    assert_eq!(
        unsafe {
            amv_result_device_readout(
                ctx as *mut amv_context_t,
                key,
                0,
                std::ptr::null_mut(),
                16,
                std::ptr::null_mut(),
            )
        },
        AM_ERR_SDK_NULL_ARGUMENT
    );
    // 非驻留 result 拒绝（invalidArgument，且在容量判定之前）。
    assert_eq!(
        unsafe {
            amv_result_device_readout(
                ctx as *mut amv_context_t,
                key,
                0,
                std::ptr::null_mut(),
                16,
                &mut needed,
            )
        },
        AM_ERR_SDK_INVALID_ARGUMENT
    );
    assert_eq!(needed, 0, "拒绝路径不得回填所需值");
    // 跨 ctx 的 result 指针拒绝（句柄归属校验）。
    let other = open_context();
    assert_eq!(
        unsafe {
            amv_result_device_readout(
                other as *mut amv_context_t,
                key,
                0,
                std::ptr::null_mut(),
                16,
                &mut needed,
            )
        },
        AM_ERR_SDK_HANDLE_INVALID
    );
    // release 后残留指针调用：registry 已出账 → handleInvalid（token 随
    // release 失效的机制载体，§3.6a）。
    assert_eq!(unsafe { amv_result_release(key as *mut amv_result_t) }, 0);
    assert_eq!(
        unsafe {
            amv_result_device_readout(
                ctx as *mut amv_context_t,
                key,
                0,
                std::ptr::null_mut(),
                16,
                &mut needed,
            )
        },
        AM_ERR_SDK_HANDLE_INVALID
    );
    // 本地 Arc 先于 close 释放：ResultCharge 随 ResultBuffer 析构归还
    // ctx.buffers 计数，否则 ctx 排干（drained=buffers==0）被测试自身卡住。
    drop(buffer);
    drop(handle);
    drop(state);
    for c in [ctx, other] {
        assert_eq!(domain().close(c, 5000).unwrap(), AM_LIFECYCLE_CLOSED as u32);
        domain().destroy(c).unwrap();
    }
}

#[test]
fn infer_params_only_reads_complete_abi_prefixes() {
    const OLD: usize = 48;
    const NEW: usize = 64;
    assert_eq!(
        std::mem::offset_of!(am_infer_params_t, device_resident_outputs),
        OLD
    );
    assert_eq!(size_of::<am_infer_params_t>(), NEW);
    let defaults = unsafe { super::input::params(std::ptr::null()) }.unwrap();
    assert_eq!(defaults.confidence_threshold, 0.25);
    assert_eq!(defaults.device_resident_outputs, 0);
    assert_eq!(defaults.device_filter.enabled, 0);

    let mut words = vec![0xa5a5_a5a5_a5a5_a5a5_u64; 8195];
    let pointer = unsafe { words.as_mut_ptr().add(1) }.cast::<am_infer_params_t>();
    unsafe {
        pointer.write(am_infer_params_t {
            confidence_threshold: 0.5,
            slot_key: 37,
            device_resident_outputs: 1,
            device_filter: am_device_filter_t {
                enabled: 1,
                class_id: -1,
                min_confidence: 0.75,
            },
            ..Default::default()
        });
    }
    for size in [4, OLD - 1, 65537] {
        unsafe { pointer.cast::<u32>().write(size as u32) };
        assert_eq!(
            unsafe { super::input::params(pointer) }.err(),
            Some(AM_ERR_SDK_STRUCT_SIZE_INVALID)
        );
    }
    for size in OLD..=NEW + 1 {
        unsafe { pointer.cast::<u32>().write(size as u32) };
        let parsed = unsafe { super::input::params(pointer) }.unwrap();
        assert_eq!(parsed.struct_size, size as u32);
        assert_eq!(parsed.confidence_threshold, 0.5);
        assert_eq!(parsed.slot_key, 37);
        let tail = size >= NEW;
        assert_eq!(parsed.device_resident_outputs, u32::from(tail));
        assert_eq!(parsed.device_filter.enabled, u32::from(tail));
        assert_eq!(parsed.device_filter.class_id, if tail { -1 } else { 0 });
        assert_eq!(
            parsed.device_filter.min_confidence,
            if tail { 0.75 } else { 0.0 }
        );
    }
    assert_eq!(words[0], 0xa5a5_a5a5_a5a5_a5a5);
    assert_eq!(words[NEW / 8 + 1], words[0]);
}

#[test]
fn set_limits_zero_is_unset_and_budget_only_tightens() {
    let ctx = open_context();
    let mut current = unsafe {
        let mut out = am_limits_t::default();
        amv_get_limits(ctx as *mut amv_context_t, &mut out);
        out
    };
    let default_budget = am_sdk_support::abi::default_limits(true).device_resident_budget_bytes;
    assert_eq!(
        current.device_resident_budget_bytes, default_budget,
        "默认 64MiB 独立账"
    );
    // =0 未设置：保持当前值（=0 忽略惯例）。
    let mut candidate = current;
    candidate.device_resident_budget_bytes = 0;
    assert_eq!(
        unsafe { amv_set_limits(ctx as *mut amv_context_t, &candidate) },
        0,
        "=0 忽略语义不得整单拒绝"
    );
    unsafe { amv_get_limits(ctx as *mut amv_context_t, &mut current) };
    assert_eq!(current.device_resident_budget_bytes, default_budget);
    // 只可调低：回调默认值被拒（LIMIT_OUT_OF_RANGE）。
    let mut expand = current;
    expand.device_resident_budget_bytes = default_budget + 1;
    assert_eq!(
        unsafe { amv_set_limits(ctx as *mut amv_context_t, &expand) },
        AM_ERR_SDK_LIMIT_OUT_OF_RANGE
    );
    // 调低生效。
    let mut tighten = current;
    tighten.device_resident_budget_bytes = 1024;
    assert_eq!(
        unsafe { amv_set_limits(ctx as *mut amv_context_t, &tighten) },
        0
    );
    unsafe { amv_get_limits(ctx as *mut amv_context_t, &mut current) };
    assert_eq!(current.device_resident_budget_bytes, 1024);
    assert_eq!(
        domain().close(ctx, 5000).unwrap(),
        AM_LIFECYCLE_CLOSED as u32
    );
    domain().destroy(ctx).unwrap();
}
