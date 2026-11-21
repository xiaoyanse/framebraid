//! 驻留形态（ABI 1.9 §3.6a）真机三门（ignore 显式跑）：detect-yolo11n 档案
//! GPU 帧直传全链——①驻留 result 元数据/data=NULL/token + readout 两段式 +
//! 同链 CPU 绑定值对拍 + 多次 readout 幂等 + release 后 token 失效；②GPU 侧
//! 筛选计数 vs readout 同张量宿主同谓词对拍（整数相等，无容差放宽）+ 暗帧
//! 零目标计数交付 + 非驻留交付过滤 + 非法参数拒绝；③独立 device 账本预算
//! 拒绝 + stats 进出账/峰值观测。env：AM_SDK_ORT_DIR（ORT/CUDA 运行库目录）、
//! FB_ROI_PHOTOS（yolo11n.onnx + bus.nv12/zidane.nv12 640² 夹具目录）。
use super::exports::*;
use crate::{amv_get_limits, amv_set_limits, domain};
use am_sdk_support::{abi::*, context};
use am_vision_core::gpu::{interop::nvidia_nv12_texture, vp_transit::upload_nv12_slice};
use std::ffi::{c_char, CString};

/// 单帧驻留字节数（[1,84,8400] f32 检测头）。
const HEAD_BYTES: u64 = 84 * 8400 * 4;
const ANCHORS: usize = 8400;
const CLASSES: usize = 80;

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

fn load_detect(ctx: usize) -> usize {
    let photos = std::env::var("FB_ROI_PHOTOS").expect("FB_ROI_PHOTOS");
    let cpath = CString::new(format!("{photos}\\yolo11n.onnx")).unwrap();
    let cprofile = CString::new("detect-yolo11n-nms-v1").unwrap();
    let policy = am_backend_policy_t {
        requested: 2, // CUDA
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

/// 合成暗帧（BT.601 limited 黑：Y=16/U=V=128）——零目标形态。
fn dark_nv12() -> Vec<u8> {
    let mut nv12 = vec![0u8; 614400];
    for byte in nv12.iter_mut().take(409600) {
        *byte = 16;
    }
    for byte in nv12.iter_mut().skip(409600) {
        *byte = 128;
    }
    nv12
}

struct GpuInput {
    frame: am_gpu_frame_ref_t,
    _texture: am_vision_core::gpu::interop::Com,
}

impl GpuInput {
    fn image(&self) -> am_image_input_t {
        am_image_input_t {
            kind: AM_IMG_GPU_FRAME,
            gpu: &self.frame,
            ..Default::default()
        }
    }
}

fn gpu_input(nv12: &[u8]) -> GpuInput {
    let texture = unsafe { nvidia_nv12_texture(640, 640).expect("NV12 纹理") };
    unsafe { upload_nv12_slice(texture.0, 0, nv12, 640, 640).expect("D3D staging 装载") };
    GpuInput {
        frame: am_gpu_frame_ref_t {
            width: 640,
            height: 640,
            d3d_format: 103,
            texture: texture.0,
            ..Default::default()
        },
        _texture: texture,
    }
}

fn infer_sync(
    model: usize,
    input: &am_image_input_t,
    params: am_infer_params_t,
) -> (i32, u64, *mut amv_result_t) {
    let mut out: *mut amv_result_t = std::ptr::null_mut();
    let mut id = 0u64;
    let rc = unsafe { amv_infer_image_sync(model as _, input, &params, &mut out, &mut id) };
    if rc != 0 {
        if let Ok((_, Some(detail))) = am_sdk_support::diagnostics::snapshot(domain(), model) {
            eprintln!(
                "[resident-e2e] rc={rc} detail={}",
                detail.message.to_string_lossy()
            );
        }
    }
    (rc, id, out)
}

fn readout_f32(ctx: usize, result: *const amv_result_t, index: u32) -> Vec<f32> {
    let mut needed: u64 = 0;
    assert_eq!(
        unsafe {
            amv_result_device_readout(
                ctx as *mut amv_context_t,
                result,
                index,
                std::ptr::null_mut(),
                0,
                &mut needed,
            )
        },
        AM_ERR_SDK_LIMIT_OUT_OF_RANGE,
        "两段式：容量 0 探询必须回填所需并拒绝"
    );
    assert_eq!(needed, HEAD_BYTES);
    let mut bytes = vec![0u8; needed as usize];
    let mut written: u64 = 0;
    assert_eq!(
        unsafe {
            amv_result_device_readout(
                ctx as *mut amv_context_t,
                result,
                index,
                bytes.as_mut_ptr(),
                needed,
                &mut written,
            )
        },
        0,
        "足量 readout 必须成功"
    );
    assert_eq!(written, needed);
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// 宿主同谓词计数（与 filter kernel/decode 首段逐字对齐：严格大于 argmax、
/// `<= conf` 跳过；detect-yolo11n 档案 layout=Transposed [4+C,anchors] 寻址）。
fn host_argmax(values: &[f32]) -> impl Iterator<Item = (usize, f32)> + '_ {
    (0..ANCHORS).map(|anchor| {
        let mut best = (0usize, 0.0f32);
        for class in 0..CLASSES {
            let score = values[(4 + class) * ANCHORS + anchor];
            if score > best.1 {
                best = (class, score);
            }
        }
        best
    })
}

fn count_host(values: &[f32], class_sel: i32, min_conf: f32) -> usize {
    host_argmax(values)
        .filter(|&(class, score)| {
            score > min_conf && (class_sel < 0 || class == class_sel as usize)
        })
        .count()
}

/// 门一：驻留全链（元数据/两段式 readout/同链对拍/幂等/token）。
#[test]
#[ignore = "需真实 NVIDIA GPU（AM_SDK_ORT_DIR + FB_ROI_PHOTOS 的 yolo11n 权重与 bus.nv12）"]
fn resident_full_chain_readout_matches_cpu_binding() {
    let mut guard = CtxGuard {
        ctx: init_ctx(),
        models: Vec::new(),
    };
    let ctx = guard.ctx;
    let model = load_detect(ctx);
    guard.track(model);
    let nv12 =
        std::fs::read(std::env::var("FB_ROI_PHOTOS").unwrap() + "\\bus.nv12").expect("bus.nv12");
    let input = gpu_input(&nv12);

    // 基线：同帧非驻留（输出绑 CPU 回读）。
    let (rc, _id, baseline) = infer_sync(model, &input.image(), am_infer_params_t::default());
    assert_eq!(rc, 0, "非驻留基线推理");
    let base = unsafe { &*baseline };
    assert_eq!(base.device_resident, 0);
    assert_eq!(base.device_outputs_token, 0, "非驻留 token 恒 0");
    let base_view = unsafe { &*base.raw_outputs };
    assert!(!base_view.data.is_null(), "非驻留 data 有效");
    let base_bytes =
        unsafe { std::slice::from_raw_parts(base_view.data as *const u8, HEAD_BYTES as usize) };
    eprintln!(
        "[resident] baseline detect.count={}（CPU 绑定回读解码基线）",
        base.detect.count
    );

    // 驻留推理。
    let resident_params = am_infer_params_t {
        device_resident_outputs: 1,
        ..Default::default()
    };
    let (rc, _id2, resident) = infer_sync(model, &input.image(), resident_params);
    assert_eq!(rc, 0, "驻留推理");
    let res = unsafe { &*resident };
    assert_eq!(res.device_resident, 1, "驻留标志");
    assert_ne!(res.device_outputs_token, 0, "驻留 token 非零");
    assert_eq!(res.raw_output_count, 1);
    let view = unsafe { &*res.raw_outputs };
    assert_eq!(view.shape_len, 3);
    let shape = unsafe { std::slice::from_raw_parts(view.shape, 3) };
    assert_eq!(shape, &[1, 84, 8400], "元数据 shape 有效");
    assert_eq!(view.dtype, 0);
    assert_eq!(view.data_size, HEAD_BYTES);
    assert!(view.data.is_null(), "驻留 data 必须为 NULL");
    assert_eq!(res.detect.count, 0, "未启用 filter 时 count 仅元数据=0");
    assert!(res.detect.boxes.is_null(), "驻留不解码：实例指针 NULL");

    // 两段式 readout + 同链对拍（同帧同会话两次推理，GPU 前处理同域；CUDA EP
    // 位确定预期，容差 1e-5 兜底记录实测）。
    let values = readout_f32(ctx, resident, 0);
    let base_values: Vec<f32> = base_bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    let mut max_delta = 0.0f32;
    for (a, b) in values.iter().zip(&base_values) {
        max_delta = max_delta.max((a - b).abs());
    }
    eprintln!("[resident] readout vs CPU 绑定回读 max|Δ|={max_delta:.3e}");
    assert!(max_delta < 1e-5, "readout 值与同链 CPU 绑定差异过大");

    // 多次 readout 幂等（不同/重复 index 均可，值不变）。
    let again = readout_f32(ctx, resident, 0);
    assert_eq!(values, again, "重复 readout 必须幂等");

    // 第二笔驻留：token 唯一 + 值逐位一致（同帧同会话同配置——arena 复用
    // 块跨流竞争的回归防护，2026-11-20 驻留批定位后固化断言）。
    let (rc, _id3, second) = infer_sync(model, &input.image(), resident_params);
    assert_eq!(rc, 0);
    assert_ne!(
        unsafe { (*second).device_outputs_token },
        res.device_outputs_token,
        "会话内 token 唯一"
    );
    let second_values = readout_f32(ctx, second, 0);
    assert_eq!(
        second_values, values,
        "第二笔驻留 readout 必须与第一笔逐位一致"
    );
    unsafe {
        amv_result_release(second);
        amv_result_release(resident);
        amv_result_release(baseline);
    }
    // release 后残留指针 readout：token 已随 release 失效。
    let mut needed: u64 = 0;
    assert_eq!(
        unsafe {
            amv_result_device_readout(
                ctx as *mut amv_context_t,
                resident,
                0,
                std::ptr::null_mut(),
                16,
                &mut needed,
            )
        },
        AM_ERR_SDK_HANDLE_INVALID
    );
}

/// 门二：GPU 侧筛选（kernel 计数 vs 宿主同谓词；暗帧零目标；非法参数）。
#[test]
#[ignore = "需真实 NVIDIA GPU（AM_SDK_ORT_DIR + FB_ROI_PHOTOS 的 yolo11n 权重与 bus.nv12）"]
fn resident_device_filter_counts_match_host_predicate() {
    let mut guard = CtxGuard {
        ctx: init_ctx(),
        models: Vec::new(),
    };
    let ctx = guard.ctx;
    let model = load_detect(ctx);
    guard.track(model);
    let nv12 =
        std::fs::read(std::env::var("FB_ROI_PHOTOS").unwrap() + "\\bus.nv12").expect("bus.nv12");
    let input = gpu_input(&nv12);

    // 非法参数：min_confidence 越界 / class_id < -1 ——提交即拒（静态校验）。
    for bad in [
        am_device_filter_t {
            enabled: 1,
            class_id: -1,
            min_confidence: 1.5,
        },
        am_device_filter_t {
            enabled: 1,
            class_id: -2,
            min_confidence: 0.25,
        },
    ] {
        let params = am_infer_params_t {
            device_resident_outputs: 1,
            device_filter: bad,
            ..Default::default()
        };
        let (rc, _, out) = infer_sync(model, &input.image(), params);
        assert_eq!(rc, AM_ERR_SDK_INVALID_ARGUMENT, "非法筛选参数必须提交即拒");
        assert!(out.is_null());
    }

    // 有目标帧：全类筛选——kernel 计数 vs readout 同张量宿主同谓词（整数
    // 相等，无容差；零目标帧以外 SDK 内部只回读 4B 计数张量）。
    let filter = am_device_filter_t {
        enabled: 1,
        class_id: -1,
        min_confidence: 0.25,
    };
    let (rc, _id, resident) = infer_sync(
        model,
        &input.image(),
        am_infer_params_t {
            device_resident_outputs: 1,
            device_filter: filter,
            ..Default::default()
        },
    );
    assert_eq!(rc, 0, "驻留+筛选推理");
    let res = unsafe { &*resident };
    assert_eq!(res.device_resident, 1);
    let detected = res.detect.count;
    eprintln!("[filter] bus 全类 kernel count={detected}");
    assert!(detected > 0, "bus 帧必须有检出（计数=NMS 前候选数）");
    let values = readout_f32(ctx, resident, 0);
    let host = count_host(&values, -1, 0.25);
    assert_eq!(detected as usize, host, "kernel 计数必须与宿主同谓词相等");

    // 首笔是带筛选的有效推理；之后才执行同纹理、同 CUDA 会话的 CPU 绑定对照。
    let (rc, baseline_id, baseline_ptr) = infer_sync(
        model,
        &input.image(),
        am_infer_params_t {
            confidence_threshold: 0.25,
            ..Default::default()
        },
    );
    assert_eq!(rc, 0, "同帧未筛选对照");
    assert!(!baseline_ptr.is_null());
    let baseline = unsafe { &*baseline_ptr };
    assert_eq!(baseline.raw_output_count, 1);
    assert!(!baseline.raw_outputs.is_null());
    assert!(baseline.detect.count > 0 && !baseline.detect.boxes.is_null());
    let output = unsafe { &*baseline.raw_outputs };
    assert_eq!(output.data_size, HEAD_BYTES);
    assert_eq!(output.dtype, AM_DTYPE_F32 as u32);
    assert!(!output.data.is_null());
    let baseline_values =
        unsafe { std::slice::from_raw_parts(output.data.cast::<f32>(), (HEAD_BYTES / 4) as usize) };
    let delta = values
        .iter()
        .zip(baseline_values)
        .map(|(a, b)| {
            assert!(a.is_finite() && b.is_finite());
            (a - b).abs()
        })
        .fold(0.0f32, f32::max);
    let unfiltered = count_host(baseline_values, -1, 0.25);
    eprintln!("[filter] 首笔 GPU={detected}，宿主同张量={host}，未筛选 CPU 绑定候选={unfiltered}，NMS 后={}，max|Δ|={delta:.3e}", baseline.detect.count);
    assert!(delta < 1e-5);
    assert_eq!(unfiltered, host);
    let expected_boxes = unsafe {
        std::slice::from_raw_parts(baseline.detect.boxes, baseline.detect.count as usize)
    }
    .iter()
    .filter(|b| b.class_id == 5 && b.confidence > 0.25)
    .count();
    assert_eq!(unsafe { amv_result_release(baseline_ptr) }, 0);
    assert_eq!(unsafe { amv_result_ack(ctx as _, baseline_id) }, 0);

    // 指定类（bus=5）：类命中谓词对拍。
    let filter_class = am_device_filter_t {
        enabled: 1,
        class_id: 5,
        min_confidence: 0.25,
    };
    let (rc, _id2, class_resident) = infer_sync(
        model,
        &input.image(),
        am_infer_params_t {
            device_resident_outputs: 1,
            device_filter: filter_class,
            ..Default::default()
        },
    );
    assert_eq!(rc, 0);
    let class_detected = unsafe { (*class_resident).detect.count };
    let class_values = readout_f32(ctx, class_resident, 0);
    let class_host = count_host(&class_values, 5, 0.25);
    eprintln!("[filter] bus class=5 kernel={class_detected} host={class_host}");
    assert_eq!(
        class_detected as usize, class_host,
        "指定类计数必须与宿主同谓词相等"
    );
    unsafe {
        amv_result_release(class_resident);
        amv_result_release(resident);
    }

    let max_class_score = host_argmax(&values)
        .filter(|&(class, _)| class == 5)
        .map(|(_, score)| score)
        .fold(0.0f32, f32::max);
    assert!(max_class_score > 0.25);
    let inherited = count_host(&values, 5, 0.25);
    for (threshold, expected) in [
        (max_class_score, 0),
        (
            max_class_score.next_down(),
            count_host(&values, 5, max_class_score.next_down()),
        ),
        (0.0, inherited),
    ] {
        let (rc, id, out) = infer_sync(
            model,
            &input.image(),
            am_infer_params_t {
                confidence_threshold: 0.25,
                device_resident_outputs: 1,
                device_filter: am_device_filter_t {
                    min_confidence: threshold,
                    ..filter_class
                },
                ..Default::default()
            },
        );
        assert_eq!(rc, 0);
        let count = unsafe { (*out).detect.count } as usize;
        eprintln!("[filter] class=5 threshold={threshold:.9}, GPU={count}, 宿主={expected}");
        assert_eq!(count, expected);
        assert_eq!(unsafe { amv_result_release(out) }, 0);
        assert_eq!(unsafe { amv_result_ack(ctx as _, id) }, 0);
    }

    // 暗帧零目标：计数=0 交付，零 readout 需求（消费方不调 readout 即可判断）。
    let dark = dark_nv12();
    let dark_input = gpu_input(&dark);
    let (rc, _id3, dark_result) = infer_sync(
        model,
        &dark_input.image(),
        am_infer_params_t {
            device_resident_outputs: 1,
            device_filter: filter,
            ..Default::default()
        },
    );
    assert_eq!(rc, 0, "暗帧驻留+筛选");
    assert_eq!(
        unsafe { (*dark_result).detect.count },
        0,
        "暗帧零目标：计数交付=0（未发生输出本体回读）"
    );
    assert!(unsafe { (*dark_result).detect.boxes }.is_null());
    eprintln!("[filter] 暗帧计数=0；调用方未读出张量");
    unsafe { amv_result_release(dark_result) };

    // 非驻留+filter：交付过滤生效（NMS 后实例口径——全部命中类+conf 谓词）。
    let (rc, _id4, filtered) = infer_sync(
        model,
        &input.image(),
        am_infer_params_t {
            confidence_threshold: 0.25,
            device_filter: filter_class,
            ..Default::default()
        },
    );
    assert_eq!(rc, 0, "非驻留+筛选推理");
    let f = unsafe { &*filtered };
    assert!(f.detect.count > 0, "class=5 过滤后仍有交付");
    assert_eq!(f.detect.count as usize, expected_boxes);
    eprintln!(
        "[filter] class=5 驻留候选={class_detected}，非驻留 NMS 后交付={}",
        f.detect.count
    );
    let boxes = unsafe { std::slice::from_raw_parts(f.detect.boxes, f.detect.count as usize) };
    for b in boxes {
        assert_eq!(b.class_id, 5, "交付实例必须全部命中筛选类");
        assert!(
            b.confidence > 0.25,
            "交付实例必须满足 conf 谓词（严格大于）"
        );
    }
    unsafe { amv_result_release(filtered) };
}

/// 门三：独立 device 账本（预算拒绝/进出账/峰值 stats）。
#[test]
#[ignore = "需真实 NVIDIA GPU（AM_SDK_ORT_DIR + FB_ROI_PHOTOS 的 yolo11n 权重与 bus.nv12）"]
fn resident_budget_ledger_rejects_and_stats_track() {
    let mut guard = CtxGuard {
        ctx: init_ctx(),
        models: Vec::new(),
    };
    let ctx = guard.ctx;
    let model = load_detect(ctx);
    guard.track(model);
    // 预算=单帧驻留字节+余量但 <2 帧（只可调低：默认 64MiB → 4MiB 合法）。
    let mut limits = am_limits_t::default();
    assert_eq!(
        unsafe { amv_get_limits(ctx as *mut amv_context_t, &mut limits) },
        0
    );
    limits.device_resident_budget_bytes = HEAD_BYTES + 1024 * 1024;
    assert_eq!(
        unsafe { amv_set_limits(ctx as *mut amv_context_t, &limits) },
        0
    );

    let stats = |ctx: usize| -> (u64, u64) {
        let mut out: *mut am_stats_t = std::ptr::null_mut();
        assert_eq!(
            unsafe {
                super::telemetry::amv_get_stats(
                    ctx as *mut amv_context_t,
                    std::ptr::null(),
                    &mut out,
                )
            },
            0
        );
        let value = unsafe { (*out).device_resident_bytes };
        let peak = unsafe { (*out).device_resident_peak_bytes };
        assert_eq!(unsafe { super::telemetry::amv_stats_release(out) }, 0);
        (value, peak)
    };

    let nv12 =
        std::fs::read(std::env::var("FB_ROI_PHOTOS").unwrap() + "\\bus.nv12").expect("bus.nv12");
    let resident_params = am_infer_params_t {
        device_resident_outputs: 1,
        ..Default::default()
    };
    // 第一帧驻留成功并持有（fetch 交付引用，计费至 release；record 缓存
    // 持有另一引用——TTL/ack 只出缓存账，合同 §5.1/§3.6a）。
    let input = gpu_input(&nv12);
    let (rc, id_a, held) = infer_sync(model, &input.image(), resident_params);
    assert_eq!(rc, 0, "预算内第一帧驻留");
    let (bytes, peak) = stats(ctx);
    eprintln!("[budget] 持有中 bytes={bytes} peak={peak}");
    assert_eq!(bytes, HEAD_BYTES, "驻留字节=单帧检测头");
    assert_eq!(peak, HEAD_BYTES, "峰值观测");

    // 第二帧：超预算新驻留提交 resourceExhausted。
    let (rc, id_b, out_b) = infer_sync(model, &input.image(), resident_params);
    assert_eq!(rc, AM_ERR_SDK_RESOURCE_EXHAUSTED, "超限新驻留提交必须拒绝");
    assert!(out_b.is_null());
    let _ = unsafe { amv_result_ack(ctx as *mut amv_context_t, id_b) };

    // release+ack 出账（release 出外部引用、ack 出缓存账——两条引用全部
    // 归零 ResultBuffer 才析构）：bytes 归零，峰值保持历史高水位。
    unsafe { amv_result_release(held) };
    let (bytes, _) = stats(ctx);
    assert_eq!(
        bytes, HEAD_BYTES,
        "缓存引用仍在（ack 前）——外部持有与缓存账分列"
    );
    assert_eq!(
        unsafe { amv_result_ack(ctx as *mut amv_context_t, id_a) },
        0
    );
    let (bytes, peak) = stats(ctx);
    eprintln!("[budget] release+ack 后 bytes={bytes} peak={peak}");
    assert_eq!(bytes, 0, "release+ack 出账归零");
    assert_eq!(peak, HEAD_BYTES, "峰值不回退");
}
