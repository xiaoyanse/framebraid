//! AI 扩面定性硬门（设计 .local/tests/ai-photos/gate-design.md，#[ignore] 显式跑）：
//! 12 张 AI 生成 NV12 逐图 CPU 推理，分层断言 L1 不崩 / L2 正样本关注族非零 /
//! L3 负样本白名单零 / L4 九形态独立 conf 带。env：AM_SDK_ORT_DIR（CPU ORT
//! 运行库目录，探针同款 ort-ov-124-final）、FB_ROI_PHOTOS（yolo11n.onnx 目录）、
//! FB_AI_PHOTOS（12 张 face*.nv12 夹具目录）。
use super::exports::*;
use crate::domain;
use am_sdk_support::{abi::*, context};
use std::{
    ffi::{c_char, CString},
    path::PathBuf,
    ptr,
    time::Instant,
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
const NEGATIVES: [&str; 3] = ["face4-1", "face4-2", "face4-3"];

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

fn load_detect_cpu(ctx: usize) -> usize {
    let photos = std::env::var("FB_ROI_PHOTOS").expect("FB_ROI_PHOTOS");
    let cpath = CString::new(format!("{photos}\\yolo11n.onnx")).unwrap();
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

struct Hit {
    class_id: i32,
    confidence: f32,
    area: f32,
}

fn family_kept(hits: &[Hit]) -> usize {
    hits.iter()
        .filter(|h| FOCUS_FAMILY.contains(&h.class_id) && h.area >= MIN_AREA_FRAC)
        .count()
}

#[test]
#[ignore = "需真实 CPU ORT 运行库与 AI 扩面夹具（AM_SDK_ORT_DIR + FB_ROI_PHOTOS 模型 + FB_AI_PHOTOS）"]
fn ai_photos_qualitative_gate() {
    let mut guard = CtxGuard {
        ctx: init_ctx(),
        models: Vec::new(),
    };
    let ctx = guard.ctx;
    let model = load_detect_cpu(ctx);
    guard.track(model);
    let mut info = am_model_info_t::default();
    assert_eq!(unsafe { amv_model_get_info(model as _, &mut info) }, 0);
    eprintln!("[ai-photos] backend_actual={}", info.execution.actual);
    let dir = PathBuf::from(std::env::var("FB_AI_PHOTOS").expect("FB_AI_PHOTOS"));

    let mut runs: Vec<(&str, Vec<Hit>)> = Vec::new();
    let mut wall_total = 0.0f64;
    for name in LEGS.iter().map(|leg| leg.0).chain(NEGATIVES) {
        let bytes = std::fs::read(dir.join(format!("{name}.nv12")))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(bytes.len(), 614400, "{name}: 须 640x640 NV12");
        let frame = am_frame_view_t {
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
        };
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
        wall_total += ms;
        assert_eq!(rc, 0, "L1 {name}: 推理 rc={rc}");
        let hits: Vec<Hit> = unsafe {
            let result = &*out;
            assert_eq!(result.task, AM_TASK_DETECT, "{name}: detect 任务身份");
            let hits = (0..result.detect.count as usize)
                .map(|i| {
                    let b = &*result.detect.boxes.add(i);
                    Hit {
                        class_id: b.class_id,
                        confidence: b.confidence,
                        area: ((b.bounds[2] - b.bounds[0]) * (b.bounds[3] - b.bounds[1])).max(0.0),
                    }
                })
                .collect();
            assert_eq!(amv_result_release(out), 0);
            hits
        };
        assert_eq!(unsafe { amv_result_ack(ctx as _, id) }, 0);
        eprintln!(
            "[ai-photos] {name} detections={} family_kept={} ms={ms:.1}",
            hits.len(),
            family_kept(&hits)
        );
        runs.push((name, hits));
    }
    eprintln!("[ai-photos] 12 图 wall 总耗时 {wall_total:.0}ms（记录不断言）");

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
