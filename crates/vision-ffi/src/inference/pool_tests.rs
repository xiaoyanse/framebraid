//! 推理池 P1.4 关键路径单测：desc 校验域/槽位记账/路由映射/借用语义/原子回滚/
//! 关闭排空守恒。模型加载经 stub 注入（test_pool），真实链路归 P1.5 ignore 门。
use super::{
    batching_exports::amv_model_get_batching,
    exports::{amv_model_close, amv_model_get_info, amv_set_realtime_policy},
    pool,
    state::State,
};
use crate::{amv_object_query_state, domain};
use am_sdk_support::{abi::*, context};
use std::{mem::size_of, ptr, sync::atomic::Ordering};

fn open_ctx() -> usize {
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

fn create(desc: *const am_pool_desc_t, ctx: usize) -> (i32, usize) {
    let mut out: *mut amv_pool_t = ptr::null_mut();
    let code = unsafe { pool::amv_pool_create(ctx as *mut amv_context_t, desc, &mut out) };
    (code, out as usize)
}

fn lane(pool: usize, i: u32) -> Result<usize, i32> {
    let mut out: *mut amv_model_t = ptr::null_mut();
    let code = unsafe { pool::amv_pool_lane_model(pool as *mut amv_pool_t, i, &mut out) };
    if code == 0 {
        Ok(out as usize)
    } else {
        Err(code)
    }
}

fn models_of(ctx: usize) -> usize {
    State::get(ctx).unwrap().models.load(Ordering::Acquire)
}

fn dispose_pool(keys: &[usize], pool_key: usize, ctx: usize) {
    let mut st = 0u32;
    assert_eq!(
        unsafe { pool::amv_pool_close(pool_key as *mut amv_pool_t, 5000, &mut st) },
        0
    );
    for k in keys.iter().collect::<std::collections::BTreeSet<_>>() {
        domain().destroy(*k).unwrap();
    }
    domain().destroy(pool_key).unwrap();
    let _ = domain().close(ctx, 0);
    let _ = domain().destroy(ctx);
}

const ENABLE: u32 = AM_BATCHING_ENABLE;

fn batching(items: u32, minor: u32) -> am_batching_options_t {
    am_batching_options_t {
        abi_minor: minor,
        max_batch_items: items,
        max_wait_ms: 10,
        flags: ENABLE,
        ..Default::default()
    }
}

#[test]
fn create_validates_desc_domain_and_null_args() {
    let ctx = open_ctx();
    let ok_model = am_model_desc_t {
        path_utf8: c"stub.onnx".as_ptr(),
        load_mode: AM_LOAD_RAW,
        ..Default::default()
    };
    let ok_memory = am_model_memory_desc_t {
        bytes: [1u8; 4].as_ptr(),
        size: 4,
        load_mode: AM_LOAD_RAW,
        ..Default::default()
    };
    let no_path = am_model_desc_t {
        path_utf8: ptr::null(),
        load_mode: AM_LOAD_RAW,
        ..Default::default()
    };
    let base = am_pool_desc_t {
        model: &ok_model,
        shards: 2,
        lanes: 2,
        ..Default::default()
    };
    let mut sink: *mut amv_pool_t = ptr::null_mut();
    assert_eq!(
        unsafe { pool::amv_pool_create(ptr::null_mut(), &base, &mut sink) },
        AM_ERR_SDK_NULL_ARGUMENT,
        "ctx NULL"
    );
    let mut out: *mut amv_pool_t = ptr::null_mut();
    assert_eq!(
        unsafe { pool::amv_pool_create(ctx as _, ptr::null(), &mut out) },
        AM_ERR_SDK_NULL_ARGUMENT,
        "desc NULL"
    );
    assert!(out.is_null());
    assert_eq!(
        unsafe { pool::amv_pool_create(ctx as _, &base, ptr::null_mut()) },
        AM_ERR_SDK_NULL_ARGUMENT,
        "out NULL"
    );
    let short = am_pool_desc_t {
        struct_size: 4,
        ..base
    };
    let case = |desc: &am_pool_desc_t, expect: i32, why: &str| {
        let mut out: *mut amv_pool_t = ptr::null_mut();
        let code = unsafe { pool::amv_pool_create(ctx as _, desc, &mut out) };
        assert_eq!(code, expect, "{why}");
        assert!(out.is_null(), "{why} 失败不得产出池句柄");
    };
    case(&short, AM_ERR_SDK_STRUCT_SIZE_INVALID, "struct_size 过短");
    for shards in [0u32, 1, 9] {
        case(
            &am_pool_desc_t {
                shards,
                lanes: 8,
                model: &ok_model,
                ..base
            },
            AM_ERR_SDK_LIMIT_OUT_OF_RANGE,
            "shards 域外",
        );
    }
    for (shards, lanes) in [(2u32, 0u32), (4, 2), (3, 5)] {
        case(
            &am_pool_desc_t {
                shards,
                lanes,
                model: &ok_model,
                ..base
            },
            AM_ERR_SDK_INVALID_ARGUMENT,
            "lanes 域违规",
        );
    }
    case(
        &am_pool_desc_t {
            model: &ok_model,
            model_memory: &ok_memory,
            ..base
        },
        AM_ERR_SDK_INVALID_ARGUMENT,
        "双源拒绝",
    );
    case(
        &am_pool_desc_t {
            shards: 2,
            lanes: 2,
            model: ptr::null(),
            model_memory: ptr::null(),
            ..base
        },
        AM_ERR_SDK_INVALID_ARGUMENT,
        "缺源拒绝",
    );
    case(
        &am_pool_desc_t {
            model: &no_path,
            ..base
        },
        AM_ERR_SDK_INVALID_ARGUMENT,
        "路径源缺失",
    );
    let zero_size = am_model_memory_desc_t {
        size: 0,
        ..ok_memory
    };
    case(
        &am_pool_desc_t {
            model: ptr::null(),
            model_memory: &zero_size,
            shards: 2,
            lanes: 2,
            ..base
        },
        AM_ERR_VISION_MODEL_TOO_LARGE,
        "内存源 size=0",
    );
    let huge = am_model_memory_desc_t {
        size: SDK_MAX_MEMORY_MODEL_BYTES + 1,
        ..ok_memory
    };
    case(
        &am_pool_desc_t {
            model: ptr::null(),
            model_memory: &huge,
            shards: 2,
            lanes: 2,
            ..base
        },
        AM_ERR_VISION_MODEL_TOO_LARGE,
        "内存源超上限",
    );
    let future = batching(4, (AM_ABI_VERSION & 0xffff) + 1);
    case(
        &am_pool_desc_t {
            batching: &future,
            ..base
        },
        AM_ERR_SDK_ABI_UNSUPPORTED,
        "batching 未来次版本",
    );
    case(
        &am_pool_desc_t {
            reserved: 7,
            ..base
        },
        AM_ERR_SDK_INVALID_ARGUMENT,
        "reserved 非 0",
    );
    assert_eq!(models_of(ctx), 0, "纯校验失败不得占用模型槽位");
    let _ = domain().close(ctx, 0);
    let _ = domain().destroy(ctx);
}

#[test]
fn create_slot_accounting_gates_before_load() {
    let ctx = open_ctx();
    let ok_model = am_model_desc_t {
        path_utf8: c"stub.onnx".as_ptr(),
        load_mode: AM_LOAD_RAW,
        ..Default::default()
    };
    let limits = am_limits_t {
        max_models: 2,
        ..Default::default()
    };
    assert_eq!(
        unsafe { crate::amv_set_limits(ctx as _, &limits) },
        0,
        "调低 max_models=2"
    );
    let over = am_pool_desc_t {
        model: &ok_model,
        shards: 3,
        lanes: 3,
        ..Default::default()
    };
    let (code, pool_key) = create(&over, ctx);
    assert_eq!(
        code, AM_ERR_SDK_RESOURCE_EXHAUSTED,
        "k 超槽位包络先于逐片 load"
    );
    assert_eq!(pool_key, 0);
    assert_eq!(models_of(ctx), 0, "记账失败零槽位占用");
    let _ = domain().close(ctx, 0);
    let _ = domain().destroy(ctx);
}

#[test]
fn create_shard_load_failure_rolls_back_pool() {
    let ctx = open_ctx();
    let model = am_model_desc_t {
        path_utf8: c"stub.onnx".as_ptr(),
        load_mode: AM_LOAD_RAW,
        ..Default::default()
    };
    let desc = am_pool_desc_t {
        model: &model,
        shards: 2,
        lanes: 2,
        ..Default::default()
    };
    let (code, pool_key) = create(&desc, ctx);
    assert_eq!(code, AM_ERR_VISION_RUNTIME_MISSING, "第 1 片 load 失败透传");
    assert_eq!(pool_key, 0);
    assert_eq!(models_of(ctx), 0, "回滚后槽位归还");
    let _ = domain().close(ctx, 0);
    let _ = domain().destroy(ctx);
}

#[test]
fn configure_failure_on_second_shard_rolls_back_first() {
    let ctx = open_ctx();
    let state = State::get(ctx).unwrap();
    let opts = batching(4, AM_ABI_VERSION & 0xffff);
    let code = pool::test_pool(ctx, 2, 2, &|i| if i == 0 { 1 } else { 0 }, Some(opts))
        .expect_err("第 2 片 configure 固定批量轴必须失败");
    assert_eq!(code, AM_ERR_VISION_BATCH_UNSUPPORTED);
    assert_eq!(
        state.models.load(Ordering::Acquire),
        0,
        "第 1 片已随回滚释放（槽位归还）"
    );
    let ok = pool::test_pool(ctx, 2, 2, &|_| 1, None).expect("ctx 可继续建池");
    let keys: Vec<usize> = (0..2u32).map(|i| lane(ok, i).unwrap()).collect();
    dispose_pool(&keys, ok, ctx);
    assert_eq!(state.models.load(Ordering::Acquire), 0);
}

#[test]
fn lane_maps_contiguous_groups_and_rejects_out_of_range() {
    let ctx = open_ctx();
    let one = |_: u32| 1u32;
    let pool_key = pool::test_pool(ctx, 2, 8, &one, None).unwrap();
    let mut shards = Vec::new();
    for i in 0..8u32 {
        shards.push(lane(pool_key, i).unwrap());
    }
    assert_eq!(shards[0], shards[1]);
    assert_eq!(shards[0], shards[2]);
    assert_eq!(shards[0], shards[3]);
    assert_eq!(shards[4], shards[5]);
    assert_eq!(shards[4], shards[6]);
    assert_eq!(shards[4], shards[7]);
    assert_ne!(shards[0], shards[4], "shards=2/lanes=8 在 lane4 分组切换");
    assert_eq!(
        lane(pool_key, 8),
        Err(AM_ERR_SDK_INVALID_ARGUMENT),
        "lane=lanes 越界"
    );
    assert_eq!(
        lane(pool_key, u32::MAX),
        Err(AM_ERR_SDK_INVALID_ARGUMENT),
        "lane 上溢越界"
    );
    let mut out: *mut amv_model_t = ptr::null_mut();
    assert_eq!(
        unsafe { pool::amv_pool_lane_model(ptr::null_mut(), 0, &mut out) },
        AM_ERR_SDK_NULL_ARGUMENT
    );
    assert_eq!(
        unsafe { pool::amv_pool_lane_model(pool_key as _, 0, ptr::null_mut()) },
        AM_ERR_SDK_NULL_ARGUMENT
    );
    let identity_pool = pool::test_pool(ctx, 8, 8, &one, None).unwrap();
    let identity_keys: Vec<usize> = (0..8u32).map(|i| lane(identity_pool, i).unwrap()).collect();
    assert_eq!(
        identity_keys
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        8,
        "shards=8/lanes=8 身份独立（k 片身份不串版）"
    );
    assert_eq!(identity_keys[0], lane(identity_pool, 0).unwrap(), "首 lane");
    assert_eq!(identity_keys[7], lane(identity_pool, 7).unwrap(), "末 lane");
    dispose_pool(&shards, pool_key, ctx);
    dispose_pool(&identity_keys, identity_pool, ctx);
}

#[test]
fn lane_groups_thirds_for_shards3_lanes12() {
    let ctx = open_ctx();
    let one = |_: u32| 1u32;
    let pool_key = pool::test_pool(ctx, 3, 12, &one, None).unwrap();
    let keys: Vec<usize> = (0..12u32).map(|i| lane(pool_key, i).unwrap()).collect();
    for (i, k) in keys.iter().enumerate() {
        let expect = i / 4;
        assert_eq!(k, &keys[expect * 4], "lane{i} → shard{expect}");
    }
    assert_eq!(
        keys.iter().collect::<std::collections::BTreeSet<_>>().len(),
        3
    );
    dispose_pool(&keys, pool_key, ctx);
}

#[test]
fn borrowed_handle_close_rejected_and_invalidated_by_pool_close() {
    let ctx = open_ctx();
    let one = |_: u32| 1u32;
    let pool_key = pool::test_pool(ctx, 2, 2, &one, None).unwrap();
    let m0 = lane(pool_key, 0).unwrap();
    let m1 = lane(pool_key, 1).unwrap();
    assert_ne!(m0, m1, "分片句柄身份独立");
    let mut st = 0u32;
    assert_eq!(
        unsafe { amv_model_close(m0 as *mut amv_model_t, 100, &mut st) },
        AM_ERR_SDK_HANDLE_INVALID,
        "借出句柄直调 model close 必须拒绝"
    );
    assert_eq!(
        domain().state(m0).unwrap(),
        AM_LIFECYCLE_RUNNING as u32,
        "拒绝后分片不受影响"
    );
    assert_eq!(
        unsafe { pool::amv_pool_close(pool_key as _, 1000, &mut st) },
        0
    );
    assert_eq!(st, AM_LIFECYCLE_CLOSED as u32);
    assert_eq!(
        lane(pool_key, 0),
        Err(AM_ERR_SDK_HANDLE_CLOSED),
        "池 CLOSED 后 lane_model 返回 handleClosed"
    );
    let mut info = am_model_info_t::default();
    assert_eq!(
        unsafe { amv_model_get_info(m0 as _, &mut info) },
        0,
        "借出句柄 close 后仍在册可查询（model close 后语义一致）"
    );
    let policy = am_realtime_policy_t::default();
    assert_eq!(
        unsafe { amv_set_realtime_policy(m0 as _, &policy) },
        AM_ERR_SDK_HANDLE_CLOSED,
        "业务调用返回 handleClosed"
    );
    assert_eq!(
        domain().state(m0).unwrap(),
        AM_LIFECYCLE_CLOSED as u32,
        "分片已随池排空"
    );
    domain().destroy(m0).unwrap();
    domain().destroy(m1).unwrap();
    domain().destroy(pool_key).unwrap();
    assert_eq!(models_of(ctx), 0, "全生命周期槽位守恒");
    let _ = domain().close(ctx, 0);
    let _ = domain().destroy(ctx);
}

#[test]
fn close_is_idempotent_and_drains_all_shards() {
    let ctx = open_ctx();
    let one = |_: u32| 1u32;
    let pool_key = pool::test_pool(ctx, 4, 4, &one, None).unwrap();
    let shards: Vec<usize> = (0..4u32).map(|i| lane(pool_key, i).unwrap()).collect();
    let mut st = 0u32;
    assert_eq!(
        unsafe { pool::amv_pool_close(pool_key as _, 1000, &mut st) },
        0
    );
    assert_eq!(st, AM_LIFECYCLE_CLOSED as u32);
    st = 99;
    assert_eq!(
        unsafe { pool::amv_pool_close(pool_key as _, 1000, &mut st) },
        0,
        "重复 close 幂等"
    );
    assert_eq!(st, AM_LIFECYCLE_CLOSED as u32);
    for &k in &shards {
        let mut state = 0u32;
        assert_eq!(unsafe { amv_object_query_state(k as _, &mut state) }, 0);
        assert_eq!(state, AM_LIFECYCLE_CLOSED as u32, "逐片到达 CLOSED");
    }
    assert_eq!(
        domain().destroy(pool_key).unwrap_err(),
        AM_ERR_SDK_HANDLE_INVALID,
        "分片在册时池不可销毁（子先父后）"
    );
    for &k in &shards {
        domain().destroy(k).unwrap();
    }
    domain().destroy(pool_key).unwrap();
    assert_eq!(models_of(ctx), 0, "池级排空守恒");
    let _ = domain().close(ctx, 0);
    let _ = domain().destroy(ctx);
}

#[test]
fn shard_batching_config_applies_to_every_shard() {
    let ctx = open_ctx();
    let one = |_: u32| 1u32;
    let opts = batching(4, AM_ABI_VERSION & 0xffff);
    let pool_key = pool::test_pool(ctx, 4, 4, &one, Some(opts)).unwrap();
    let shards: Vec<usize> = (0..4u32).map(|i| lane(pool_key, i).unwrap()).collect();
    for &k in &shards {
        let mut out = am_batching_options_t::default();
        let mut stats = am_batching_stats_t::default();
        assert_eq!(
            unsafe { amv_model_get_batching(k as _, &mut out, &mut stats) },
            0
        );
        assert_eq!(out.flags, ENABLE, "逐片启用");
        assert_eq!(out.max_batch_items, 4, "批形状逐片一致");
        assert_eq!(out.max_wait_ms, 10);
    }
    dispose_pool(&shards, pool_key, ctx);
}

#[test]
fn ctx_shutdown_cascades_open_pool() {
    let ctx = open_ctx();
    let state = State::get(ctx).unwrap();
    let one = |_: u32| 1u32;
    let pool_key = pool::test_pool(ctx, 2, 2, &one, None).unwrap();
    let shards: Vec<usize> = (0..2u32).map(|i| lane(pool_key, i).unwrap()).collect();
    let mut running = 0u32;
    assert_eq!(
        unsafe { amv_object_query_state(pool_key as _, &mut running) },
        0
    );
    assert_eq!(running, AM_LIFECYCLE_RUNNING as u32);
    assert_eq!(
        domain().close(ctx, 5000).unwrap(),
        AM_LIFECYCLE_CLOSED as u32,
        "ctx shutdown 级联关闭未显式关闭的池"
    );
    let mut pool_state = 0u32;
    assert_eq!(
        unsafe { amv_object_query_state(pool_key as _, &mut pool_state) },
        0
    );
    assert_eq!(pool_state, AM_LIFECYCLE_CLOSED as u32);
    for &k in &shards {
        let mut shard_state = 0u32;
        assert_eq!(
            unsafe { amv_object_query_state(k as _, &mut shard_state) },
            0
        );
        assert_eq!(shard_state, AM_LIFECYCLE_CLOSED as u32);
    }
    for &k in &shards {
        domain().destroy(k).unwrap();
    }
    domain().destroy(pool_key).unwrap();
    assert_eq!(state.models.load(Ordering::Acquire), 0);
    domain().destroy(ctx).unwrap();
}

#[test]
fn pool_exports_reject_non_pool_handles() {
    let ctx = open_ctx();
    let model = pool::stub_shard(ctx, ctx, 1).expect("普通模型 stub");
    let mut st = 0u32;
    assert_eq!(
        unsafe { pool::amv_pool_close(model as *mut amv_pool_t, 100, &mut st) },
        AM_ERR_SDK_HANDLE_INVALID,
        "model 句柄不得当作池关闭"
    );
    assert_eq!(
        lane(model, 0),
        Err(AM_ERR_SDK_HANDLE_INVALID),
        "model 句柄不得当作池取分片"
    );
    let mut st2 = 0u32;
    assert_eq!(
        unsafe { amv_model_close(model as *mut amv_model_t, 100, &mut st2) },
        0,
        "非池借出模型 close 语义不变"
    );
    assert_eq!(st2, AM_LIFECYCLE_CLOSED as u32);
    domain().destroy(model).unwrap();
    let _ = domain().close(ctx, 0);
    let _ = domain().destroy(ctx);
}
