//! AI 扩面定性硬门（设计 .local/tests/ai-photos/gate-design.md，#[ignore] 显式跑）：
//! 16 张 AI 生成 NV12 逐图 CPU 推理（9 正样本形态 + 7 负样本），分层断言
//! L1 不崩 / L2 正样本关注族非零 / L3 负样本白名单零 / L4 九形态独立 conf 带。批身份门（ai_photos_batch_identity_gate）
//! 同夹具经自适应攒批窗口成物理批执行，逐图断言批/单帧检出身份一致。
//! env：AM_SDK_ORT_DIR（CPU ORT 运行库目录，探针同款 ort-ov-124-final）、
//! FB_ROI_PHOTOS（yolo11n.onnx 目录）、FB_AI_PHOTOS（face*.nv12 夹具目录）。
use super::batching_exports::{amv_model_configure_batching, amv_model_get_batching};
use super::exports::*;
use crate::domain;
use am_sdk_support::{abi::*, context};
use std::{
    ffi::{c_char, CString},
    path::{Path, PathBuf},
    ptr,
    time::{Duration, Instant},
};

/// 关注类别族 COCO id：person/car/bus/truck/motorbike/bicycle/train。
const FOCUS_FAMILY: [i32; 7] = [0, 1, 2, 3, 5, 7, 8];
/// 前置过滤：归一化框面积 <0.1% 画幅剔除（低置信段零宽退化框）。
const MIN_AREA_FRAC: f32 = 0.001;
const CONF_THRESHOLD: f32 = 0.25;
/// 九形态腿（设计 §3）：（夹具名，top1 下限，关注族检出数下限——均按交付
/// 检出计；面积过滤仅用于 L3 负样本零断言，§2 仅该层标注"过滤后"且九腿
/// 下限/交付实测比均匀 22%~36% 可证校准参照）。首轮校准 2026-11-22 实测−
/// 裕量，重生成必重校。
const LEGS: [(&str, f32, u32); 9] = [
    ("face1-1", 0.85, 1),
    ("face1-2", 0.85, 4),
    ("face1-3", 0.60, 5),
    ("face2-1", 0.85, 3),
    ("face2-2", 0.80, 3),
    ("face2-3", 0.80, 3),
    ("face3-1", 0.85, 4),
    ("face3-2", 0.80, 1),
    ("face3-3", 0.55, 2),
];
/// 负样本腿（L3 白名单零）：空街/纯景物/空步道/空房间/雨夜空街/茂密植被/
/// 水面倒影。face4-4~4-7 二批扩集（2026-11-22，探针 family=0 见
/// probe-results-7neg-20261122.json）。
const NEGATIVES: [&str; 7] = [
    "face4-1", "face4-2", "face4-3", "face4-4", "face4-5", "face4-6", "face4-7",
];
/// 批/单帧身份容差：同进程同权重同后端，物理批合并仅可能引入核内 ULP 级差。
const IDENTITY_TOL: f32 = 1e-6;
/// 单物理批项数：执行期补账 24B/px（640²≈9.375MiB/项）叠挂起实驻留，
/// 默认 64MiB 上下文预算下 6 项起在全 12 项在窗时越界，4 项留 ~19MiB 余量。
const BATCH_ITEMS: u32 = 4;
const BATCH_WAIT_MS: u32 = 2000;

fn init_ctx() -> usize {
    let ort = std::env::var("AM_SDK_ORT_DIR").expect("AM_SDK_ORT_DIR");
    let names: Vec<CString> = vec![CString::new(ort).unwrap()];
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

struct CtxGuard {
    ctx: usize,
    models: Vec<usize>,
}
impl CtxGuard {
    fn track(&mut self, key: usize) {
        self.models.push(key);
    }
}
impl Drop for CtxGuard {
    fn drop(&mut self) {
        for key in self.models.iter() {
            let _ = domain().close(*key, 5000);
            let _ = domain().destroy(*key);
        }
        let _ = domain().close(self.ctx, 5000);
        let _ = domain().destroy(self.ctx);
    }
}

fn load_detect_cpu(ctx: usize, model_dir: &str, model_file: &str) -> usize {
    let cpath = CString::new(format!("{model_dir}\\{model_file}")).unwrap();
    let cprofile = CString::new("detect-yolo11n-nms-v1").unwrap();
    let policy = am_backend_policy_t {
        requested: AM_BACKEND_CPU as u32,
        strict: 1,
        ..Default::default()
    };
    let desc = am_model_desc_t {
        struct_size: std::mem::size_of::<am_model_desc_t>() as u32,
        path_utf8: cpath.as_ptr(),
        load_mode: AM_LOAD_PROFILE,
        profile_id: cprofile.as_ptr(),
        backend: &policy,
        ..Default::default()
    };
    let mut key = std::ptr::null_mut();
    assert_eq!(unsafe { amv_model_load(ctx as _, &desc, &mut key) }, 0);
    key as usize
}

#[derive(Clone, Copy)]
struct Hit {
    class_id: i32,
    confidence: f32,
    bounds: [f32; 4],
    area: f32,
}

fn family_kept(hits: &[Hit]) -> usize {
    hits.iter()
        .filter(|h| FOCUS_FAMILY.contains(&h.class_id) && h.area >= MIN_AREA_FRAC)
        .count()
}

fn load_nv12(dir: &Path, name: &str) -> Vec<u8> {
    let bytes =
        std::fs::read(dir.join(format!("{name}.nv12"))).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert_eq!(bytes.len(), 614400, "{name}: 须 640x640 NV12");
    bytes
}

fn detect_hits(result: &amv_result_t, name: &str) -> Vec<Hit> {
    assert_eq!(result.task, AM_TASK_DETECT, "{name}: detect 任务身份");
    (0..result.detect.count as usize)
        .map(|i| {
            let b = unsafe { &*result.detect.boxes.add(i) };
            Hit {
                class_id: b.class_id,
                confidence: b.confidence,
                bounds: b.bounds,
                area: ((b.bounds[2] - b.bounds[0]) * (b.bounds[3] - b.bounds[1])).max(0.0),
            }
        })
        .collect()
}

fn nv12_frame(bytes: &[u8]) -> am_frame_view_t {
    am_frame_view_t {
        width: 640,
        height: 640,
        format: AM_PF_NV12 as u32,
        plane_count: 2,
        planes: [
            bytes.as_ptr(),
            bytes[409600..].as_ptr(),
            ptr::null(),
            ptr::null(),
        ],
        strides: [640, 640, 0, 0],
        plane_size_bytes: [409600, 204800, 0, 0],
        ..Default::default()
    }
}

fn sync_hits(model: usize, bytes: &[u8], name: &str) -> (u64, Vec<Hit>, f64) {
    let frame = nv12_frame(bytes);
    let view = am_pixel_view_t {
        magic: AM_PIXEL_VIEW_MAGIC,
        view: &frame,
        ..Default::default()
    };
    let input = am_image_input_t {
        kind: AM_IMG_PIXEL_VIEW,
        view: &view,
        ..Default::default()
    };
    let params = am_infer_params_t {
        confidence_threshold: CONF_THRESHOLD,
        ..Default::default()
    };
    let begin = Instant::now();
    let mut out = ptr::null_mut();
    let mut id = 0u64;
    let rc = unsafe { amv_infer_image_sync(model as _, &input, &params, &mut out, &mut id) };
    let ms = begin.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(rc, 0, "L1 {name}: 推理 rc={rc}");
    let hits = unsafe {
        let hits = detect_hits(&*out, name);
        assert_eq!(amv_result_release(out), 0);
        hits
    };
    (id, hits, ms)
}

/// 异步提交（攒批窗口路径）；接受段已拷贝平面，返回后 bytes 可释放。
fn submit_async(model: usize, bytes: &[u8]) -> u64 {
    let frame = nv12_frame(bytes);
    let view = am_pixel_view_t {
        magic: AM_PIXEL_VIEW_MAGIC,
        view: &frame,
        ..Default::default()
    };
    let input = am_image_input_t {
        kind: AM_IMG_PIXEL_VIEW,
        view: &view,
        ..Default::default()
    };
    let params = am_infer_params_t {
        confidence_threshold: CONF_THRESHOLD,
        ..Default::default()
    };
    let mut id = 0u64;
    let rc = unsafe { amv_infer_image_async(model as _, &input, &params, &mut id) };
    assert_eq!(rc, 0, "async 提交应被接受 rc={rc}");
    id
}

/// 非 DONE 终态诊断：fetch 取回请求记录内的失败码（fetch 契约对非 DONE
/// 返回 r.code）。
fn fetch_code(ctx: usize, id: u64) -> i32 {
    let mut out: *mut amv_result_t = ptr::null_mut();
    unsafe { amv_result_fetch(ctx as _, id, &mut out) }
}

fn wait_terminal(ctx: usize, id: u64, name: &str) {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let mut snap: *mut am_request_snapshot_t = ptr::null_mut();
        assert_eq!(unsafe { amv_request_query(ctx as _, id, &mut snap) }, 0);
        let (state, terminal) = unsafe { ((*snap).state, (*snap).terminal) };
        unsafe { amv_request_snapshot_release(snap) };
        if terminal != 0 {
            assert_eq!(
                state as i32,
                AM_REQ_DONE,
                "{name}: 终态非 DONE state={state} code={}",
                fetch_code(ctx, id),
            );
            return;
        }
        assert!(Instant::now() < deadline, "{name}: 等待终态超时 id={id}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn fetch_hits(ctx: usize, id: u64, name: &str) -> Vec<Hit> {
    let mut out: *mut amv_result_t = ptr::null_mut();
    assert_eq!(
        unsafe { amv_result_fetch(ctx as _, id, &mut out) },
        0,
        "{name}: fetch"
    );
    let hits = unsafe {
        let hits = detect_hits(&*out, name);
        assert_eq!(amv_result_release(out), 0);
        hits
    };
    assert_eq!(unsafe { amv_result_ack(ctx as _, id) }, 0, "{name}: ack");
    hits
}

fn sorted_hits(hits: &[Hit]) -> Vec<Hit> {
    let mut v = hits.to_vec();
    v.sort_by(|a, b| {
        a.class_id
            .cmp(&b.class_id)
            .then(b.confidence.total_cmp(&a.confidence))
            .then(a.bounds[0].total_cmp(&b.bounds[0]))
            .then(a.bounds[1].total_cmp(&b.bounds[1]))
    });
    v
}

fn batching_stats(model: usize) -> am_batching_stats_t {
    let mut options = am_batching_options_t::default();
    let mut out = am_batching_stats_t::default();
    assert_eq!(
        unsafe { amv_model_get_batching(model as _, &mut options, &mut out) },
        0
    );
    out
}

/// F13 同款（batch_e2e）：公开累计计数区分不了批拆分，经内部攒批注册表补读
/// 单物理批最大项数。
fn max_batch_observed(model: usize) -> u64 {
    let entry = domain().active(model).expect("model entry");
    let handle = entry
        .data::<super::models::ModelHandle>()
        .expect("ModelHandle");
    let key = handle.info.key.load(std::sync::atomic::Ordering::Acquire);
    super::scheduler::batching_snapshot(key).1.max_batch_size
}

#[test]
#[ignore = "需真实 CPU ORT 运行库与 AI 扩面夹具（AM_SDK_ORT_DIR + FB_ROI_PHOTOS 模型 + FB_AI_PHOTOS）"]
fn ai_photos_qualitative_gate() {
    let mut guard = CtxGuard {
        ctx: init_ctx(),
        models: Vec::new(),
    };
    let ctx = guard.ctx;
    let model = load_detect_cpu(
        ctx,
        &std::env::var("FB_ROI_PHOTOS").expect("FB_ROI_PHOTOS"),
        "yolo11n.onnx",
    );
    guard.track(model);
    let mut info = am_model_info_t::default();
    assert_eq!(unsafe { amv_model_get_info(model as _, &mut info) }, 0);
    eprintln!("[ai-photos] backend_actual={}", info.execution.actual);
    let dir = PathBuf::from(std::env::var("FB_AI_PHOTOS").expect("FB_AI_PHOTOS"));

    let mut runs: Vec<(&str, Vec<Hit>)> = Vec::new();
    let mut wall_total = 0.0f64;
    for name in LEGS.iter().map(|leg| leg.0).chain(NEGATIVES) {
        let bytes = load_nv12(&dir, name);
        let (id, hits, ms) = sync_hits(model, &bytes, name);
        wall_total += ms;
        assert_eq!(unsafe { amv_result_ack(ctx as _, id) }, 0);
        eprintln!(
            "[ai-photos] {name} detections={} family_kept={} ms={ms:.1}",
            hits.len(),
            family_kept(&hits)
        );
        runs.push((name, hits));
    }
    eprintln!(
        "[ai-photos] {} 图 wall 总耗时 {wall_total:.0}ms（记录不断言）",
        LEGS.len() + NEGATIVES.len()
    );

    let hits_of = |name: &str| -> &Vec<Hit> {
        runs.iter()
            .find(|(n, _)| *n == name)
            .map(|(_, h)| h)
            .expect("L1 已采集该夹具")
    };
    for (name, top1_min, count_min) in LEGS {
        let hits = hits_of(name);
        let confs: Vec<f32> = hits
            .iter()
            .filter(|h| FOCUS_FAMILY.contains(&h.class_id))
            .map(|h| h.confidence)
            .collect();
        assert!(!confs.is_empty(), "L2 {name}: 关注族检出 0");
        let top1 = confs.iter().copied().fold(0.0f32, f32::max);
        assert!(
            top1 >= top1_min,
            "L4 {name}: top1 {top1:.3} < 带下限 {top1_min}（能力回归或夹具漂移，先重跑探针对表）"
        );
        assert!(
            confs.len() as u32 >= count_min,
            "L4 {name}: 关注族检出 {} < 下限 {count_min}（先重跑探针对表）",
            confs.len()
        );
    }
    for name in NEGATIVES {
        let kept = family_kept(hits_of(name));
        assert_eq!(
            kept, 0,
            "L3 {name}: 负样本关注族必须为 0（白名单外类别不断言）"
        );
    }
}

/// 批窗口身份一致门：同夹具单帧（攒批未启用，现行逐项语义）与攒批窗口物理
/// 批（AUTO 语义）两形态逐图对表——框数/类别/conf/bounds 全等（容差仅覆盖
/// 物理批合并的核内 ULP 级差）。单帧先行：sync 提交在攒批启用后同样入窗，
/// 先行保证单帧臂为纯净基线。
/// 模型用 FB_AI_PHOTOS 内 yolo11n-batch640.onnx（yolo11n.onnx 同权重派生，
/// 仅输出非批维按档案声明具体化为 [·,84,8400]）：符号输出维的档案物理批按
/// 合同显式拒绝（native_execution 行字节数 None→RESOURCE_EXHAUSTED，原生
/// yolo11n.onnx 输出全符号维无法成物理批——批次边界登记），非批维具体化
/// 后行字节可估、批轴保持动态。
#[test]
#[ignore = "需真实 CPU ORT 运行库与 AI 扩面夹具（AM_SDK_ORT_DIR + FB_ROI_PHOTOS 模型 + FB_AI_PHOTOS）"]
fn ai_photos_batch_identity_gate() {
    let mut guard = CtxGuard {
        ctx: init_ctx(),
        models: Vec::new(),
    };
    let ctx = guard.ctx;
    let ai_photos = std::env::var("FB_AI_PHOTOS").expect("FB_AI_PHOTOS");
    let model = load_detect_cpu(ctx, &ai_photos, "yolo11n-batch640.onnx");
    guard.track(model);
    let mut info = am_model_info_t::default();
    assert_eq!(unsafe { amv_model_get_info(model as _, &mut info) }, 0);
    assert_eq!(
        info.native_batch, 1,
        "自适应攒批仅接受动态批量轴模型（native_batch=1）"
    );
    let dir = PathBuf::from(ai_photos);
    let names: Vec<&str> = LEGS.iter().map(|leg| leg.0).chain(NEGATIVES).collect();

    let mut single: Vec<(&str, Vec<Hit>)> = Vec::new();
    for name in &names {
        let bytes = load_nv12(&dir, name);
        let (id, hits, ms) = sync_hits(model, &bytes, name);
        assert_eq!(unsafe { amv_result_ack(ctx as _, id) }, 0);
        eprintln!(
            "[ai-batch-identity] 单帧 {name} boxes={} ms={ms:.1}",
            hits.len()
        );
        single.push((name, hits));
    }

    let options = am_batching_options_t {
        struct_size: std::mem::size_of::<am_batching_options_t>() as u32,
        max_batch_items: BATCH_ITEMS,
        max_wait_ms: BATCH_WAIT_MS,
        flags: 1,
        ..Default::default()
    };
    assert_eq!(
        unsafe { amv_model_configure_batching(model as _, &options) },
        0
    );
    let mut ids = Vec::with_capacity(names.len());
    for name in &names {
        let bytes = load_nv12(&dir, name);
        ids.push((*name, submit_async(model, &bytes)));
    }
    let mut batched: Vec<(&str, Vec<Hit>)> = Vec::new();
    for (name, id) in &ids {
        wait_terminal(ctx, *id, name);
        batched.push((name, fetch_hits(ctx, *id, name)));
    }
    let batches = (names.len() as u32).div_ceil(BATCH_ITEMS);
    let after = batching_stats(model);
    assert_eq!(
        after.formed_batches, batches as u64,
        "全数凑满应为 {} 个物理批",
        batches
    );
    assert_eq!(after.batched_items, names.len() as u64, "逐项计入成批项");
    assert_eq!(after.timeout_batches, 0, "应凑满触发而非窗口截止");
    assert_eq!(after.rejected_items, 0, "零准入背压");
    assert_eq!(
        max_batch_observed(model),
        BATCH_ITEMS as u64,
        "单物理批项数应达窗口容量"
    );

    let mut max_conf_diff = 0.0f32;
    let mut max_bound_diff = 0.0f32;
    for (name, one) in &single {
        let many = batched
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, h)| h)
            .expect("批形态已采集该夹具");
        let one = sorted_hits(one);
        let many = sorted_hits(many);
        assert_eq!(
            many.len(),
            one.len(),
            "{name}: 框数批/单帧不一致（单帧 {} vs 批 {}）",
            one.len(),
            many.len()
        );
        let mut conf_diff = 0.0f32;
        let mut bound_diff = 0.0f32;
        for (i, (a, b)) in one.iter().zip(&many).enumerate() {
            assert_eq!(a.class_id, b.class_id, "{name}[{i}]: 类别不一致");
            conf_diff = conf_diff.max((a.confidence - b.confidence).abs());
            for axis in 0..4 {
                bound_diff = bound_diff.max((a.bounds[axis] - b.bounds[axis]).abs());
            }
        }
        assert!(
            conf_diff <= IDENTITY_TOL,
            "{name}: conf 漂移 {conf_diff:.3e} 超容差 {IDENTITY_TOL}"
        );
        assert!(
            bound_diff <= IDENTITY_TOL,
            "{name}: bounds 漂移 {bound_diff:.3e} 超容差 {IDENTITY_TOL}"
        );
        max_conf_diff = max_conf_diff.max(conf_diff);
        max_bound_diff = max_bound_diff.max(bound_diff);
        eprintln!(
            "[ai-batch-identity] 批 {name} boxes={} top1={:.6}/{:.6} conf_diff={conf_diff:.3e} bound_diff={bound_diff:.3e}",
            many.len(),
            one.first().map_or(0.0, |h| h.confidence),
            many.first().map_or(0.0, |h| h.confidence),
        );
    }
    eprintln!(
        "[ai-batch-identity] {} 图批/单帧全表最大漂移 conf={max_conf_diff:.3e} bounds={max_bound_diff:.3e}（容差 {IDENTITY_TOL}）",
        names.len()
    );
}
