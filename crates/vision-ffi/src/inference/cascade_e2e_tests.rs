//! cascade C1 端到端真机门（全链，ignore 显式跑）：合成双 detect 模型（Identity
//! 常量表，坐标/分数固定可手算）——覆盖静态 ROI 坐标回映、动态规则类过滤+外扩+
//! primary_index、无匹配类空 cascade、secondary_confidence 继承与独立阈值。
use super::exports::*;
use crate::domain;
use am_sdk_support::{abi::*, context};
use std::ffi::CString;

const RUNTIME_DIR: &str = "E:\\projects\\framebraid\\.local\\runtimes\\ort-gpu-131-final";
const FIXTURES: &str = "E:\\projects\\framebraid\\.local\\tests\\cascade-e2e";

fn init_ctx() -> usize {
    let names: Vec<CString> = vec![CString::new(RUNTIME_DIR).unwrap()];
    let pointers: Vec<*const std::ffi::c_char> = names.iter().map(|d| d.as_ptr()).collect();
    let params = amv_init_params_t {
        abi_version: AM_ABI_VERSION,
        runtime_dirs_utf8: pointers.as_ptr(),
        runtime_dir_count: 1,
        infer_threads: 2,
        ..Default::default()
    };
    unsafe { context::initialize_vision(domain(), &params) }.expect("vision ctx")
}

/// 各门共用进程级 Domain（ctx 上限 SDK_MAX_CONTEXTS=8），串行跑时前门不释放
/// 会挤占后门初始化：退出（含断言失败提前退出）统一关闭并注销。
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

fn load_model(ctx: usize, path: &str) -> usize {
    let cpath = CString::new(path).unwrap();
    let cprofile = CString::new("detect-yolo11n-nms-v1").unwrap();
    let desc = am_model_desc_t {
        struct_size: std::mem::size_of::<am_model_desc_t>() as u32,
        path_utf8: cpath.as_ptr(),
        load_mode: AM_LOAD_PROFILE,
        profile_id: cprofile.as_ptr(),
        ..Default::default()
    };
    let mut key = std::ptr::null_mut();
    assert_eq!(unsafe { amv_model_load(ctx as _, &desc, &mut key) }, 0);
    key as usize
}

fn rule(class: i32, expand: f32) -> am_cascade_rule_t {
    am_cascade_rule_t {
        struct_size: std::mem::size_of::<am_cascade_rule_t>() as u32,
        source_class_id: class,
        expand,
        min_roi_size: 0.0,
    }
}

fn near(got: &[f32], want: [f32; 4], tol: f32, what: &str) {
    for (g, w) in got.iter().zip(want) {
        assert!((g - w).abs() < tol, "{what}: {got:?} !~ {want:?}");
    }
}

#[test]
#[ignore = "需真实 ORT 与 cascade 合成双模型夹具（.local/tests/cascade-e2e）"]
fn cascade_e2e_static_dynamic_confidence_full_flow() {
    let mut guard = CtxGuard {
        ctx: init_ctx(),
        models: Vec::new(),
    };
    let ctx = guard.ctx;
    let primary = load_model(ctx, &format!("{FIXTURES}\\primary.onnx"));
    guard.track(primary);
    let secondary = load_model(ctx, &format!("{FIXTURES}\\secondary.onnx"));
    guard.track(secondary);
    let png = std::fs::read(format!("{FIXTURES}\\input.png")).expect("fixture png");
    let input = am_image_input_t {
        kind: AM_IMG_PNG,
        encoded: png.as_ptr(),
        encoded_size: png.len() as u64,
        ..Default::default()
    };
    let mut out: *mut amv_result_t = std::ptr::null_mut();
    let mut id = 0u64;

    // --- 1. 静态 ROI：坐标回映手算 + secondary_confidence=0 继承默认 0.25 ---
    let static_rois: [f32; 4] = [0.25, 0.25, 0.75, 0.75];
    let request = am_cascade_request_t {
        static_rois: static_rois.as_ptr(),
        static_roi_count: 1,
        ..Default::default()
    };
    assert_eq!(
        unsafe {
            amv_infer_cascade_sync(
                primary as _,
                secondary as _,
                &input,
                &request,
                std::ptr::null(),
                &mut out,
                &mut id,
            )
        },
        0
    );
    let result = unsafe { &*out };
    // 一级主通道不变：anchor0 class3@0.9 + anchor1 class7@0.8（置信度降序）
    assert_eq!(result.task, AM_TASK_DETECT);
    assert_eq!(result.detect.count, 2);
    let detect0 = unsafe { &*result.detect.boxes };
    assert_eq!(detect0.class_id, 3);
    assert!((detect0.confidence - 0.9).abs() < 0.01);
    near(&detect0.bounds, [0.4, 0.4, 0.6, 0.6], 1e-3, "primary box0");
    // 级联：crop 320² letterbox 640（scale 恰 2）→ ROI 内归一化 [0.4,0.45,0.6,0.55]
    // → roi[0.25..0.75]：0.25+n*0.5 → 原图 [0.45,0.475,0.55,0.525]
    assert!(!result.cascade.is_null(), "cascade entry constructs result");
    let cascade = unsafe { &*result.cascade };
    assert_eq!(
        cascade.secondary_count, 1,
        "0.1 box filtered by inherited 0.25"
    );
    let box0 = unsafe { &*cascade.secondary };
    assert_eq!(box0.class_id, 5);
    assert!((box0.confidence - 0.7).abs() < 0.01);
    near(
        &box0.bounds,
        [0.45, 0.475, 0.55, 0.525],
        1e-3,
        "static ROI mapped box",
    );
    assert!(
        box0.class_name_utf8.is_null(),
        "non-vocab secondary has no label"
    );
    let hit0 = unsafe { &*cascade.hits };
    assert_eq!(hit0.primary_index, -1, "static ROI has no primary source");
    near(&hit0.roi, static_rois, 1e-6, "static hit roi");
    assert_eq!(cascade.skipped_count, 0, "healthy secondary skips nothing");
    unsafe { amv_result_release(out) };
    let _ = unsafe { amv_result_ack(ctx as _, id) };

    // --- 2. 动态规则：class3 过滤 + expand 0.5 外扩 + primary_index 指向 ---
    let rules = [rule(3, 0.5)];
    let request = am_cascade_request_t {
        rules: rules.as_ptr(),
        rule_count: 1,
        ..Default::default()
    };
    let mut out2: *mut amv_result_t = std::ptr::null_mut();
    let mut id2 = 0u64;
    assert_eq!(
        unsafe {
            amv_infer_cascade_sync(
                primary as _,
                secondary as _,
                &input,
                &request,
                std::ptr::null(),
                &mut out2,
                &mut id2,
            )
        },
        0
    );
    let result2 = unsafe { &*out2 };
    assert_eq!(result2.detect.count, 2, "primary unaffected");
    let cascade2 = unsafe { &*result2.cascade };
    // 一级框 [0.4,0.4,0.6,0.6] 外扩 0.5：每侧 0.2*0.25=0.05 → ROI [0.35,0.35,0.65,0.65]
    assert_eq!(cascade2.secondary_count, 1);
    let hit = unsafe { &*cascade2.hits };
    assert_eq!(
        hit.primary_index, 0,
        "source is decode[0] (confidence desc)"
    );
    near(&hit.roi, [0.35, 0.35, 0.65, 0.65], 1e-3, "expanded roi");
    // 框回映：ROI 内 [0.4,0.45,0.6,0.55] → 0.35+n*0.3 → [0.47,0.485,0.53,0.515]
    let dbox = unsafe { &*cascade2.secondary };
    near(
        &dbox.bounds,
        [0.47, 0.485, 0.53, 0.515],
        2e-3,
        "dynamic ROI mapped box",
    );
    unsafe { amv_result_release(out2) };
    let _ = unsafe { amv_result_ack(ctx as _, id2) };

    // --- 3. 无匹配类：cascade 空且请求成功 ---
    let rules = [rule(42, 0.0)];
    let request = am_cascade_request_t {
        rules: rules.as_ptr(),
        rule_count: 1,
        ..Default::default()
    };
    let mut out3: *mut amv_result_t = std::ptr::null_mut();
    let mut id3 = 0u64;
    assert_eq!(
        unsafe {
            amv_infer_cascade_sync(
                primary as _,
                secondary as _,
                &input,
                &request,
                std::ptr::null(),
                &mut out3,
                &mut id3,
            )
        },
        0,
        "no matching class keeps request successful"
    );
    let result3 = unsafe { &*out3 };
    assert_eq!(result3.detect.count, 2);
    let cascade3 = unsafe { &*result3.cascade };
    assert_eq!(cascade3.secondary_count, 0);
    assert!(cascade3.secondary.is_null(), "empty cascade nulls arrays");
    assert!(cascade3.hits.is_null());
    // 三态可观测：无 ROI 生成（无匹配类）≠ 有 ROI 被跳过
    assert_eq!(cascade3.skipped_count, 0, "no-ROI path is not skipped path");
    unsafe { amv_result_release(out3) };
    let _ = unsafe { amv_result_ack(ctx as _, id3) };

    // --- 4. secondary_confidence 独立阈值：0.05 放行 0.1 低分框 ---
    let static_rois: [f32; 4] = [0.25, 0.25, 0.75, 0.75];
    let request = am_cascade_request_t {
        static_rois: static_rois.as_ptr(),
        static_roi_count: 1,
        secondary_confidence: 0.05,
        ..Default::default()
    };
    let mut out4: *mut amv_result_t = std::ptr::null_mut();
    let mut id4 = 0u64;
    assert_eq!(
        unsafe {
            amv_infer_cascade_sync(
                primary as _,
                secondary as _,
                &input,
                &request,
                std::ptr::null(),
                &mut out4,
                &mut id4,
            )
        },
        0
    );
    let cascade4 = unsafe { &*(*out4).cascade };
    assert_eq!(cascade4.secondary_count, 2, "0.05 threshold admits 0.1 box");
    let boxes4 = unsafe { std::slice::from_raw_parts(cascade4.secondary, 2) };
    assert!((boxes4[0].confidence - 0.7).abs() < 0.01, "confidence desc");
    assert!((boxes4[1].confidence - 0.1).abs() < 0.01);
    let hits4 = unsafe { std::slice::from_raw_parts(cascade4.hits, 2) };
    assert_eq!(hits4[0].primary_index, -1);
    // 第二框 ROI 内归一化 [0.2,0.2,0.3,0.3] → 0.25+n*0.5 → [0.35,0.35,0.4,0.4]
    near(
        &boxes4[1].bounds,
        [0.35, 0.35, 0.4, 0.4],
        2e-3,
        "low-score box mapped",
    );
    unsafe { amv_result_release(out4) };
    let _ = unsafe { amv_result_ack(ctx as _, id4) };
}
