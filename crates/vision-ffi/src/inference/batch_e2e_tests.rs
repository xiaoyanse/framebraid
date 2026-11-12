//! 批 C 图像攒批端到端真机门（ignore 显式跑）：合成动态批轴单输入模型
//! （image-dynamic.onnx，按 batch 位置返回可辨识通道均值），经真实调度器
//! 窗口（AUTO+攒批启用）成物理批执行——组装/拆分/逐项身份/窗口统计/
//! 取消/尾项不足 2 逐项执行全断言。RAW 模型无档案解码，逐项 params 生效
//! 属 PROFILE 解码语义（当前档案无可批模型，见 RELEASE_STATUS 边界）。
use super::batching_exports::{amv_model_configure_batching, amv_model_get_batching};
use super::exports::*;
use crate::domain;
use am_sdk_support::{abi::*, context};
use std::ffi::{c_char, CString};
use std::time::{Duration, Instant};

const RUNTIME_DIR: &str = "E:\\projects\\framebraid\\.local\\runtimes\\ort-gpu-131-final";
const FIXTURES: &str = "E:\\projects\\framebraid\\.local\\tests\\batch-image";

fn init_ctx() -> usize {
    let names: Vec<CString> = vec![CString::new(RUNTIME_DIR).unwrap()];
    let pointers: Vec<*const c_char> = names.iter().map(|d| d.as_ptr()).collect();
    let params = amv_init_params_t {
        abi_version: AM_ABI_VERSION,
        runtime_dirs_utf8: pointers.as_ptr(),
        runtime_dir_count: 1,
        infer_threads: 2,
        ..Default::default()
    };
    unsafe { context::initialize_vision(domain(), &params) }.expect("vision ctx")
}

fn load_raw(ctx: usize) -> usize {
    let cpath = CString::new(format!("{FIXTURES}\\image-dynamic.onnx")).unwrap();
    let desc = am_model_desc_t {
        struct_size: std::mem::size_of::<am_model_desc_t>() as u32,
        path_utf8: cpath.as_ptr(),
        load_mode: AM_LOAD_RAW,
        ..Default::default()
    };
    let mut key = std::ptr::null_mut();
    assert_eq!(unsafe { amv_model_load(ctx as _, &desc, &mut key) }, 0);
    key as usize
}

fn configure(model: usize, items: u32, wait_ms: u32) {
    let options = am_batching_options_t {
        struct_size: std::mem::size_of::<am_batching_options_t>() as u32,
        max_batch_items: items,
        max_wait_ms: wait_ms,
        flags: 1, // AM_BATCHING_ENABLE
        ..Default::default()
    };
    assert_eq!(
        unsafe { amv_model_configure_batching(model as _, &options) },
        0
    );
}

fn stats(model: usize) -> am_batching_stats_t {
    let mut options = am_batching_options_t::default();
    let mut out = am_batching_stats_t::default();
    assert_eq!(
        unsafe { amv_model_get_batching(model as _, &mut options, &mut out) },
        0
    );
    out
}

fn png_input(
    name: &str,
) -> (
    Vec<u8>,
    am_image_input_t,
    am_raw_preprocess_t,
    am_infer_params_t,
) {
    let png = std::fs::read(format!("{FIXTURES}\\{name}.png")).expect("fixture png");
    let input = am_image_input_t {
        kind: AM_IMG_PNG,
        encoded: png.as_ptr(),
        encoded_size: png.len() as u64,
        ..Default::default()
    };
    let raw = am_raw_preprocess_t {
        struct_size: std::mem::size_of::<am_raw_preprocess_t>() as u32,
        resize: AM_RESIZE_LETTERBOX,
        target_w: 224,
        target_h: 224,
        fill_value: 114.0,
        color_order: AM_COLOR_RGB,
        normalize: AM_NORM_DIV255,
        ..Default::default()
    };
    let params = am_infer_params_t {
        raw_preprocess: &raw,
        ..Default::default()
    };
    (png, input, raw, params)
}

fn submit(_ctx: usize, model: usize, name: &str) -> u64 {
    let (_png, input, raw, mut params) = png_input(name);
    // raw_preprocess 借用语义：png_input 返回的是副本，此处重绑到存活局部。
    params.raw_preprocess = &raw;
    let mut id = 0u64;
    let rc = unsafe { amv_infer_image_async(model as _, &input, &params, &mut id) };
    if rc != 0 {
        if let Ok((_, Some(detail))) = am_sdk_support::diagnostics::snapshot(domain(), model) {
            eprintln!(
                "[batch-e2e] rc={rc} detail={}",
                detail.message.to_string_lossy()
            );
        }
        panic!("async 提交应被接受 rc={rc}");
    }
    id
}

fn wait_terminal(ctx: usize, id: u64) -> i32 {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let mut snap: *mut am_request_snapshot_t = std::ptr::null_mut();
        assert_eq!(unsafe { amv_request_query(ctx as _, id, &mut snap) }, 0);
        let state = unsafe { (*snap).state };
        let terminal = unsafe { (*snap).terminal };
        unsafe { amv_request_snapshot_release(snap) };
        if terminal != 0 {
            return state as i32;
        }
        assert!(Instant::now() < deadline, "等待终态超时 id={id}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// 取回 DONE 结果的 out0 九元组（[r,g,b,2r,2g,2b,4r,4g,4b]/255）。
fn fetch_values(ctx: usize, id: u64) -> [f32; 9] {
    let mut out: *mut amv_result_t = std::ptr::null_mut();
    assert_eq!(unsafe { amv_result_fetch(ctx as _, id, &mut out) }, 0);
    let result = unsafe { &*out };
    assert_eq!(result.raw_output_count, 1, "单输出模型");
    let tensor = unsafe { &*result.raw_outputs };
    assert_eq!(tensor.shape_len, 2);
    let shape = unsafe { std::slice::from_raw_parts(tensor.shape, 2) };
    assert_eq!(shape, &[1, 9], "拆分后逐项形状 [1,9]");
    let bytes =
        unsafe { std::slice::from_raw_parts(tensor.data.cast::<u8>(), tensor.data_size as usize) };
    let values: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    unsafe { amv_result_release(out) };
    values.try_into().expect("out0 应为 9 个 f32")
}

fn expect_of(rgb: [f32; 3]) -> [f32; 9] {
    let mut expected = [0f32; 9];
    for c in 0..3 {
        expected[c] = rgb[c];
        expected[3 + c] = rgb[c] * 2.0;
        expected[6 + c] = rgb[c] * 4.0;
    }
    expected
}

fn assert_identity(label: &str, values: [f32; 9], rgb: [f32; 3]) {
    let expected = expect_of(rgb);
    for (i, (a, b)) in values.iter().zip(&expected).enumerate() {
        assert!(
            (a - b).abs() < 1e-4,
            "{label}[{i}] 身份漂移: got={a} want={b}"
        );
    }
}

#[test]
#[ignore = "需真实 ORT 与合成批模型夹具（.local/tests/batch-image）"]
fn batch_e2e_window_assembles_splits_and_keeps_identity() {
    let ctx = init_ctx();
    let model = load_raw(ctx);
    configure(model, 4, 100);
    let before = stats(model);
    assert_eq!(before.batched_items, 0, "起始零计数");
    // 四路异色 PNG：进窗→凑满→单一物理批→拆分逐项交付。
    let ids = [
        submit(ctx, model, "red"),
        submit(ctx, model, "green"),
        submit(ctx, model, "blue"),
        submit(ctx, model, "yellow"),
    ];
    let colors = [
        [1.0f32, 0.0, 0.0],
        [0.0f32, 1.0, 0.0],
        [0.0f32, 0.0, 1.0],
        [1.0f32, 1.0, 0.0],
    ];
    for (id, color) in ids.iter().zip(&colors) {
        assert_eq!(wait_terminal(ctx, *id), AM_REQ_DONE);
        let label = format!("item{id}");
        assert_identity(&label, fetch_values(ctx, *id), *color);
        assert_eq!(unsafe { amv_result_ack(ctx as _, *id) }, 0);
    }
    let after = stats(model);
    assert!(after.formed_batches >= 1, "窗口应成物理批");
    assert!(after.batched_items >= 4, "逐项计入成批项");
}

/// 批 E 分期计费门：同一收紧预算（6MiB）下，旧口径（全周期估算峰值逐项
/// 入账，四项 ≈8.83MB）第三项起必拒、窗口凑不满；新口径（挂起期实驻留
/// ≈730B/项 + 执行期补账 ≈1.2MB/项，即 224²×24 不含 prepare 期瞬态解码
/// 源面，峰值 ≈4.8MB）整窗凑满并正确执行——攒批可用性由账目口径解锁的
/// 直接证据。完成/释放后账目归还（后续提交仍被接受），全程零准入背压。
#[test]
#[ignore = "需真实 ORT 与合成批模型夹具（.local/tests/batch-image）"]
fn batch_e2e_staged_accrual_fills_window_under_tightened_budget() {
    let ctx = init_ctx();
    // 收紧两层上限（ctx preprocess 预算 + 实时推理类队列字节上限，两者
    // 独立存储）；执行器 64MiB 硬顶不动。类队列收紧须经 amv_set_queue_policy。
    unsafe {
        let mut limits = std::mem::zeroed::<am_limits_t>();
        limits.struct_size = std::mem::size_of::<am_limits_t>() as u32;
        assert_eq!(crate::amv_get_limits(ctx as _, &mut limits), 0);
        limits.preprocess_budget_bytes = 6 << 20;
        assert_eq!(crate::amv_set_limits(ctx as _, &limits), 0);
        let mut policy = std::mem::zeroed::<am_queue_policy_t>();
        policy.struct_size = std::mem::size_of::<am_queue_policy_t>() as u32;
        assert_eq!(
            crate::amv_get_queue_policy(ctx as _, AM_Q_REALTIME_INFER as u32, &mut policy),
            0
        );
        policy.max_bytes = 6 << 20;
        assert_eq!(
            crate::amv_set_queue_policy(ctx as _, AM_Q_REALTIME_INFER as u32, &policy),
            0
        );
    }
    let model = load_raw(ctx);
    configure(model, 4, 100);
    let ids = [
        submit(ctx, model, "red"),
        submit(ctx, model, "green"),
        submit(ctx, model, "blue"),
        submit(ctx, model, "yellow"),
    ];
    let colors = [
        [1.0f32, 0.0, 0.0],
        [0.0f32, 1.0, 0.0],
        [0.0f32, 0.0, 1.0],
        [1.0f32, 1.0, 0.0],
    ];
    for (id, color) in ids.iter().zip(&colors) {
        assert_eq!(wait_terminal(ctx, *id), AM_REQ_DONE);
        let label = format!("item{id}");
        assert_identity(&label, fetch_values(ctx, *id), *color);
        assert_eq!(unsafe { amv_result_ack(ctx as _, *id) }, 0);
    }
    let after = stats(model);
    assert!(after.formed_batches >= 1, "收紧预算下窗口仍应凑满成批");
    assert!(after.batched_items >= 4, "逐项计入成批项");
    assert_eq!(after.rejected_items, 0, "收紧预算下不应触发准入背压");
    // 全周期账目守恒：补账若未随执行释放，6MiB 预算已被占满、后续提交必拒。
    let extra = submit(ctx, model, "blue");
    assert_eq!(wait_terminal(ctx, extra), AM_REQ_DONE);
    assert_identity("post-cycle", fetch_values(ctx, extra), [0.0, 0.0, 1.0]);
    assert_eq!(unsafe { amv_result_ack(ctx as _, extra) }, 0);
    assert_eq!(stats(model).rejected_items, 0);
}

#[test]
#[ignore = "需真实 ORT 与合成批模型夹具（.local/tests/batch-image）"]
fn batch_e2e_tail_single_and_cancel_semantics_hold() {
    let ctx = init_ctx();
    let model = load_raw(ctx);
    // 尾项不足 2：单项入窗→截止→不组批、逐项执行（结果正确、成批计数不增）。
    configure(model, 4, 150);
    let before = stats(model);
    let id = submit(ctx, model, "blue");
    assert_eq!(wait_terminal(ctx, id), AM_REQ_DONE);
    assert_identity("tail", fetch_values(ctx, id), [0.0, 0.0, 1.0]);
    assert_eq!(unsafe { amv_result_ack(ctx as _, id) }, 0);
    let after = stats(model);
    assert_eq!(
        after.formed_batches, before.formed_batches,
        "单项不足 2 不组批"
    );
    // 取消：两项在窗（wait 150ms），立即取消其一——取消项 CANCELED 终态、
    // 不进后续批；剩余项窗口截止成批执行并交付。
    let keep = submit(ctx, model, "red");
    let cancel = submit(ctx, model, "green");
    assert_eq!(unsafe { amv_request_cancel(ctx as _, cancel) }, 0);
    assert_eq!(wait_terminal(ctx, cancel), AM_REQ_CANCELED);
    assert_eq!(wait_terminal(ctx, keep), AM_REQ_DONE);
    assert_identity("keep", fetch_values(ctx, keep), [1.0, 0.0, 0.0]);
    assert_eq!(unsafe { amv_result_ack(ctx as _, keep) }, 0);
}

/// 双 Model 错峰真装配门：两 Model 实例各自开窗攒批（批 4），并发提交使两
/// 窗口同相凑满——执行期份额在 ctx 级执行槽上错峰入账，两窗都成批执行、
/// 逐项身份正确、全程零拒绝；一 Model 在另一 Model 在途期间关闭排空（其
/// 在途请求按关闭语义取消），另一 Model 不受干扰继续成批执行，共享执行器
/// 账目全周期守恒（关闭排空后提交仍被接受）。
#[test]
#[ignore = "需真实 ORT 与合成批模型夹具（.local/tests/batch-image）"]
fn batch_e2e_dual_model_staggered_execution() {
    let ctx = init_ctx();
    let model_a = load_raw(ctx);
    let model_b = load_raw(ctx);
    configure(model_a, 4, 100);
    configure(model_b, 4, 100);
    let names = ["red", "green", "blue", "yellow"];
    let colors = [
        [1.0f32, 0.0, 0.0],
        [0.0f32, 1.0, 0.0],
        [0.0f32, 0.0, 1.0],
        [1.0f32, 1.0, 0.0],
    ];
    // 屏障同步并发提交：两窗口同相凑满，执行期在共享执行槽上错峰。
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let lanes: Vec<_> = [model_a, model_b]
        .into_iter()
        .map(|model| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                names.map(|name| submit(ctx, model, name))
            })
        })
        .collect();
    let ids: Vec<[u64; 4]> = lanes
        .into_iter()
        .map(|lane| lane.join().expect("提交线程应正常结束"))
        .collect();
    for (lane_ids, model) in ids.iter().zip([model_a, model_b]) {
        for (id, color) in lane_ids.iter().zip(&colors) {
            assert_eq!(wait_terminal(ctx, *id), AM_REQ_DONE);
            assert_identity(&format!("lane{id}"), fetch_values(ctx, *id), *color);
            assert_eq!(unsafe { amv_result_ack(ctx as _, *id) }, 0);
        }
        let after = stats(model);
        assert!(after.formed_batches >= 1, "双 Model 两窗都应成批");
        assert!(after.batched_items >= 4, "逐项计入成批项");
        assert_eq!(after.rejected_items, 0, "错峰执行不应触发准入背压");
    }
    // A 在 B 在途期间关闭排空：A 在途按关闭语义取消收口，B 不受干扰继续
    // 成批执行；关闭等待排空返回，B 项终态在关闭返回后仍可查询。
    let ids_b2 = [submit(ctx, model_b, "blue"), submit(ctx, model_b, "red")];
    let ids_a2 = [
        submit(ctx, model_a, "green"),
        submit(ctx, model_a, "yellow"),
    ];
    let mut lifecycle: am_lifecycle_state_t = 0;
    assert_eq!(
        unsafe { amv_model_close(model_a as _, 5000, &mut lifecycle) },
        0
    );
    assert_eq!(lifecycle, AM_LIFECYCLE_CLOSED as u32, "关闭应排空至 CLOSED");
    for id in ids_a2 {
        assert_eq!(wait_terminal(ctx, id), AM_REQ_CANCELED, "关闭在途应取消");
    }
    let late_colors = [[0.0f32, 0.0, 1.0], [1.0f32, 0.0, 0.0]];
    for (id, color) in ids_b2.iter().zip(&late_colors) {
        assert_eq!(wait_terminal(ctx, *id), AM_REQ_DONE, "对侧不受关闭干扰");
        assert_identity(&format!("late{id}"), fetch_values(ctx, *id), *color);
        assert_eq!(unsafe { amv_result_ack(ctx as _, *id) }, 0);
    }
    let after = stats(model_b);
    assert!(after.formed_batches >= 2, "B 关闭 A 后应继续成批");
    assert!(after.batched_items >= 6, "两窗成批项累计");
    assert_eq!(after.rejected_items, 0, "全程零准入背压");
    // 账目守恒：关闭排空后共享执行器账目已归还，B 仍可接受并正确执行。
    let post = submit(ctx, model_b, "yellow");
    assert_eq!(wait_terminal(ctx, post), AM_REQ_DONE);
    assert_identity("post-close", fetch_values(ctx, post), [1.0, 1.0, 0.0]);
    assert_eq!(unsafe { amv_result_ack(ctx as _, post) }, 0);
    assert_eq!(stats(model_b).rejected_items, 0);
}
