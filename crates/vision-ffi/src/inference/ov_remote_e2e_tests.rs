//! OV remote 直传真机门（ABI 1.8 §3.3a OV 侧；tasks 4.2）：Arc 140T 上
//! OV GPU 会话的 GPU 帧全链——自组装 am_gpu_frame_ref_t（Intel D3D11 NV12
//! 纹理创建期上传，接受段 CopySubresourceRegion+事件查询同步）→ preprocess
//! 图（NV12→RGB→pad(114)→resize→/255）→remote 推理。三件证据：
//! （1）身份断言：bus=5/zidane=3 零漂移（恒等几何+非恒等 letterbox 臂双验）；
//! （2）top conf 对拍：同图 OV CPU 像素路径臂 ≤5e-2（GPU 域统计档）；
//! （3）D3 参考输出对照：编码输入臂（同权重静态导出）vs 冻结参考 JSON
//! （count/类别/框 IoU/conf GPU 域档；detect 档案冻结参考仅 bus），另像素
//! NV12 臂经 letterbox 逆映射对照参考帧。
//! 搬运审计：gpu_inputs 计数增量=remote 帧数、copies=0（纯 OV 流量）。
//! 证据 JSON 落 .local/evidence/sdk-intel-direct-ib-20261119/。
//! 真机依赖项 #[ignore] 同现行惯例；本批实跑记录留档（运行命令见证据目录）。
use super::exports::*;
use super::selected_exports::amv_model_load_selected;
use crate::amv_get_last_error;
use am_sdk_support::{abi::*, context};
use std::{
    ffi::CString,
    path::{Path, PathBuf},
    ptr,
};

/// 夹具根（同仓测试惯例：CARGO_MANIFEST_DIR 相对）。
const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
const PHOTOS: [(&str, usize); 2] = [("bus", 5), ("zidane", 3)];
/// GPU 域统计档（合同 §3.3a ABI 1.8 容差分级：CUDA/OV 直传对 CPU 参考位级
/// 口径 1e-3 的跨域统计放宽，sdk-ovgpu 先例）。
const CONF_GATE: f32 = 5.0e-2;
const PAIR_IOU_MIN: f32 = 0.5;

#[derive(Clone, serde::Serialize)]
struct Hit {
    class_id: i32,
    confidence: f32,
    bounds: [f32; 4],
}

fn hits_of(result: *mut amv_result_t) -> Vec<Hit> {
    unsafe {
        let view = &*result;
        assert_eq!(view.task, AM_TASK_DETECT);
        let detect = &view.detect;
        let hits: Vec<Hit> = (0..detect.count as usize)
            .map(|i| {
                let row = &*detect.boxes.add(i);
                Hit {
                    class_id: row.class_id,
                    confidence: row.confidence,
                    bounds: row.bounds,
                }
            })
            .collect();
        assert_eq!(amv_result_release(result), 0);
        hits
    }
}

/// 同步推理（图像输入三形态共用；label 标注失败臂）。
fn infer_sync(
    label: &str,
    model: usize,
    input: &am_image_input_t,
    params: *const am_infer_params_t,
) -> (Vec<Hit>, f64) {
    let begin = std::time::Instant::now();
    let mut out = ptr::null_mut();
    let mut id = 0u64;
    let rc = unsafe { amv_infer_image_sync(model as _, input, params, &mut out, &mut id) };
    let ms = begin.elapsed().as_secs_f64() * 1000.0;
    if rc != 0 {
        let mut info = am_error_info_t::default();
        let _ = unsafe { amv_get_last_error(model as _, &mut info) };
        let detail = if info.message_utf8.is_null() {
            String::new()
        } else {
            unsafe { std::ffi::CStr::from_ptr(info.message_utf8) }
                .to_string_lossy()
                .into_owned()
        };
        panic!("sync 推理失败 arm={label} rc={rc}（id={id}）detail={detail}");
    }
    (hits_of(out), ms)
}

/// GPU 帧引用输入（同两段式：link 后 input.gpu 指向自有 gpu_ref）。
struct GpuFrameInput {
    gpu_ref: am_gpu_frame_ref_t,
    input: am_image_input_t,
}

fn gpu_frame_input(texture: *mut std::ffi::c_void, w: u32, h: u32) -> GpuFrameInput {
    GpuFrameInput {
        gpu_ref: am_gpu_frame_ref_t {
            width: w,
            height: h,
            d3d_format: 103,
            texture,
            ..Default::default()
        },
        input: am_image_input_t {
            kind: AM_IMG_GPU_FRAME,
            ..Default::default()
        },
    }
}

impl GpuFrameInput {
    fn link(&mut self) {
        self.input.gpu = &self.gpu_ref;
    }
}

/// NV12 像素视图输入（ABI 结构含裸指针链；两段式——构造仅落值，调用方
/// `link()` 后才可传指针，此后结构不可再移动）。
struct Nv12ViewInput {
    frame: am_frame_view_t,
    view: am_pixel_view_t,
    input: am_image_input_t,
}

fn nv12_view_input(bytes: &[u8], w: u32, h: u32) -> Nv12ViewInput {
    let y_len = w as usize * h as usize;
    let mut frame = am_frame_view_t {
        width: w,
        height: h,
        format: AM_PF_NV12 as u32,
        plane_count: 2,
        strides: [w, w, 0, 0],
        plane_size_bytes: [y_len as u64, (y_len / 2) as u64, 0, 0],
        ..Default::default()
    };
    frame.planes = [
        bytes.as_ptr(),
        bytes[y_len..].as_ptr(),
        ptr::null(),
        ptr::null(),
    ];
    Nv12ViewInput {
        frame,
        view: am_pixel_view_t {
            magic: AM_PIXEL_VIEW_MAGIC,
            ..Default::default()
        },
        input: am_image_input_t {
            kind: AM_IMG_PIXEL_VIEW,
            ..Default::default()
        },
    }
}

impl Nv12ViewInput {
    fn link(&mut self) {
        self.view.view = &self.frame;
        self.input.view = &self.view;
    }
}

fn encoded_input(jpeg: &[u8]) -> am_image_input_t {
    am_image_input_t {
        kind: AM_IMG_JPEG,
        encoded: jpeg.as_ptr(),
        encoded_size: jpeg.len() as u64,
        ..Default::default()
    }
}

/// 类别一致 + 框 IoU 贪心配对（参考对照/对拍共端口径）。
fn compare_hits(reference: &[Hit], actual: &[Hit]) -> (usize, f32, f32) {
    let iou = |a: &[f32; 4], b: &[f32; 4]| -> f32 {
        let (ix, iy) = (
            (a[2].min(b[2]) - a[0].max(b[0])).max(0.0),
            (a[3].min(b[3]) - a[1].max(b[1])).max(0.0),
        );
        let inter = ix * iy;
        let union = (a[2] - a[0]) * (a[3] - a[1]) + (b[2] - b[0]) * (b[3] - b[1]) - inter;
        if union <= 1e-12 {
            0.0
        } else {
            inter / union
        }
    };
    let mut remaining = actual.to_vec();
    let (mut matched, mut worst_iou, mut max_conf) = (0usize, 1.0f32, 0.0f32);
    for expected in reference {
        let candidate = remaining
            .iter()
            .enumerate()
            .filter(|(_, h)| h.class_id == expected.class_id)
            .map(|(i, h)| (i, iou(&expected.bounds, &h.bounds)))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, pair_iou)) = candidate {
            let hit = remaining.swap_remove(i);
            matched += 1;
            worst_iou = worst_iou.min(pair_iou);
            max_conf = max_conf.max((expected.confidence - hit.confidence).abs());
        }
    }
    (matched, worst_iou, max_conf)
}

fn top_conf(hits: &[Hit]) -> f32 {
    hits.iter().map(|h| h.confidence).fold(0.0f32, f32::max)
}

/// nv12 帧坐标 → 原图坐标（letterbox 逆映射；nv12=原图 letterbox 快照，
/// 几何参数由 SDK prepare 同式从原图尺寸推导——e2e 内以参考图实际尺寸计算）。
fn map_to_original(bounds: [f32; 4], w_src: u32, h_src: u32, side: u32) -> [f32; 4] {
    let scale = side as f32 / w_src.max(h_src) as f32;
    let left = (side as f32 - w_src as f32 * scale) / 2.0;
    let top = (side as f32 - h_src as f32 * scale) / 2.0;
    let unpad =
        |v: f32, off: f32, dim: u32| ((v * side as f32 - off) / scale / dim as f32).clamp(0.0, 1.0);
    [
        unpad(bounds[0], left, w_src),
        unpad(bounds[1], top, h_src),
        unpad(bounds[2], left, w_src),
        unpad(bounds[3], top, h_src),
    ]
}

/// 2x 整数放大（NV12 平面各自最近邻）：非恒等 letterbox 臂的素材构造。
fn upscale_nv12_2x(nv12: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 6];
    let (y, uv) = nv12.split_at(w * h);
    let (oy, ouv) = out.split_at_mut(w * h * 4);
    for row in 0..h * 2 {
        for col in 0..w * 2 {
            oy[row * w * 2 + col] = y[(row / 2) * w + col / 2];
        }
    }
    for row in 0..h / 2 {
        for col in 0..w / 2 {
            let src = row * w + col * 2;
            let (u, v) = (uv[src], uv[src + 1]);
            for (dr, dc) in [(0usize, 0usize), (0, 1), (1, 0), (1, 1)] {
                let dst = (row * 2 + dr) * (w * 2) + (col * 2 + dc) * 2;
                ouv[dst] = u;
                ouv[dst + 1] = v;
            }
        }
    }
    out
}

struct Guard {
    ctx: usize,
    models: Vec<usize>,
}
impl Drop for Guard {
    fn drop(&mut self) {
        for key in self.models.drain(..).rev() {
            let _ = crate::domain().close(key, 5000);
            let _ = crate::domain().destroy(key);
        }
        let _ = crate::domain().close(self.ctx, 5000);
        let _ = crate::domain().destroy(self.ctx);
    }
}

fn load_ov_gpu(ctx: usize, path: &Path, profile: &CString) -> usize {
    let path_c = CString::new(path.to_str().unwrap()).unwrap();
    let needle = CString::new("Arc").unwrap();
    let selector = am_device_selector_t {
        struct_size: std::mem::size_of::<am_device_selector_t>() as u32,
        match_utf8: needle.as_ptr(),
        match_bytes: needle.as_bytes_with_nul().len() as u32,
        instance: 0,
        reserved: 0,
    };
    let candidates = [am_backend_candidate_t {
        backend: AM_BACKEND_OPENVINO_NATIVE as u32,
        device_id: 0,
    }];
    let selection = am_backend_selection_t {
        struct_size: std::mem::size_of::<am_backend_selection_t>() as u32,
        candidate_count: 1,
        candidates: candidates.as_ptr(),
        allow_fallback: 0,
        budget_ms: 1500,
        device: &selector,
    };
    let desc = am_model_desc_t {
        struct_size: std::mem::size_of::<am_model_desc_t>() as u32,
        path_utf8: path_c.as_ptr(),
        load_mode: AM_LOAD_PROFILE,
        profile_id: profile.as_ptr(),
        backend: ptr::null(),
        intra_threads: 2,
    };
    let mut out = ptr::null_mut();
    let mut id = 0u64;
    assert_eq!(
        unsafe { amv_model_load_selected(ctx as _, &desc, &selection, &mut out, &mut id) },
        0,
        "OV GPU 定向加载应成功（needle=Arc strict）"
    );
    let mut info = am_model_info_t::default();
    assert_eq!(unsafe { amv_model_get_info(out as _, &mut info) }, 0);
    let label = unsafe { std::ffi::CStr::from_ptr(info.execution.device_utf8) }
        .to_string_lossy()
        .into_owned();
    assert_eq!(label, "OpenVINO:GPU", "定向命中应为 OV GPU（实测 {label}）");
    out as usize
}

fn load_plain(ctx: usize, path: &Path, profile: &CString, backend: u32) -> usize {
    let path_c = CString::new(path.to_str().unwrap()).unwrap();
    let policy = am_backend_policy_t {
        requested: backend,
        strict: 1,
        ..Default::default()
    };
    let desc = am_model_desc_t {
        struct_size: std::mem::size_of::<am_model_desc_t>() as u32,
        path_utf8: path_c.as_ptr(),
        load_mode: AM_LOAD_PROFILE,
        profile_id: profile.as_ptr(),
        backend: &policy,
        intra_threads: 2,
    };
    let mut out = ptr::null_mut();
    assert_eq!(unsafe { amv_model_load(ctx as _, &desc, &mut out) }, 0);
    out as usize
}

#[test]
#[ignore = "OV remote 真机门：本机 Arc 140T + ort-ov-124-final 运行库（openvino_c 含 intel_gpu 插件）"]
fn ov_remote_gpu_frame_identity_cpu_parity_and_reference() {
    let runtime_dir = std::env::var_os("AM_SDK_OPENVINO_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(REPO).join(".local/runtimes/ort-ov-124-final"));
    let photos_dir = Path::new(REPO).join(".local/tests/roi-photos");
    let model_path = photos_dir.join("yolo11n-static-640.onnx");
    let reference_path =
        Path::new(REPO).join("crates/vision/resources/reference/detect-yolo11n-nms-v1.json");
    assert!(
        model_path.is_file(),
        "静态夹具缺失：{}",
        model_path.display()
    );
    assert!(reference_path.is_file());
    let dir = CString::new(runtime_dir.to_str().unwrap()).unwrap();
    let dirs = [dir.as_ptr()];
    let options = amv_init_params_t {
        abi_version: AM_ABI_VERSION,
        runtime_dirs_utf8: dirs.as_ptr(),
        runtime_dir_count: 1,
        infer_threads: 2,
        ..Default::default()
    };
    let ctx = unsafe { context::initialize_vision(crate::domain(), &options) }.unwrap();
    let mut guard = Guard {
        ctx,
        models: Vec::new(),
    };
    let profile = CString::new("detect-yolo11n-nms-v1").unwrap();
    let ov_gpu = load_ov_gpu(ctx, &model_path, &profile);
    guard.models.push(ov_gpu);
    let ov_cpu = load_plain(
        ctx,
        &model_path,
        &profile,
        AM_BACKEND_OPENVINO_NATIVE as u32,
    );
    guard.models.push(ov_cpu);

    // 能力位 bit10（ABI 1.8：无 CUDA 机器经 OV remote 面置位——本机双 GPU，
    // CUDA 运行库在位与否不遮蔽 OV 侧贡献，断言只验证置位非唯一来源）。
    let mut capability = amv_capability_t::default();
    assert_eq!(
        unsafe { crate::amv_query_capability(ctx as _, &mut capability) },
        0
    );
    assert_ne!(
        u64::from(capability.feature_bits) & AMV_CAP_GPU_FRAME_INPUT,
        0,
        "bit10 应经 OV remote 面置位"
    );

    // 参考口径（D3 harness 同款）：NULL params=档案默认（conf 0.25/iou 0.7/
    // maxDet 300），显式默认结构体的 conf=0.0 会放行全部锚点（300 满检出）。
    let params = ptr::null();
    // 单 D3D11 设备供全部帧纹理（宿主形态；remote 会话按设备身份缓存复用）。
    let intel_device = am_vision_core::sdk::intel_test_device().expect("Intel 适配器 D3D11 设备");
    let audit_before = am_vision_core::gpu::pipeline::audit::snapshot();
    let mut doc = serde_json::json!({
        "schema": "ov-remote-e2e/1",
        "question": "OV GPU 会话 GPU 帧直传（remote context+preprocess 图）在 Arc 140T 上身份/对拍/参考对照",
        "model": model_path.to_string_lossy(),
        "runtime_dir": runtime_dir.to_string_lossy(),
        "conf_gate": CONF_GATE,
        "arms": {},
    });
    let mut remote_frames = 0usize;

    for (name, expect) in PHOTOS {
        let nv12 = std::fs::read(photos_dir.join(format!("{name}.nv12"))).unwrap();
        assert_eq!(nv12.len(), 640 * 640 * 3 / 2);
        let jpeg = std::fs::read(
            Path::new(REPO)
                .join("crates/vision/resources/reference/images")
                .join(format!("{name}.jpg")),
        )
        .unwrap();

        // remote 臂：Intel D3D11 NV12 纹理创建期上传 → GPU 帧直传。
        let texture = unsafe {
            am_vision_core::sdk::intel_nv12_texture_upload(
                &intel_device,
                640,
                640,
                &nv12[..640 * 640],
                &nv12[640 * 640..],
            )
            .expect("Intel NV12 纹理创建上传")
        };
        let mut gpu_input = gpu_frame_input(texture.0, 640, 640);
        gpu_input.link();
        let (remote, remote_ms) = infer_sync("remote", ov_gpu, &gpu_input.input, params);
        remote_frames += 1;
        // CPU 像素路径臂（同图 NV12 view）与编码输入臂（参考帧原图）。
        let mut nv12_input = nv12_view_input(&nv12, 640, 640);
        nv12_input.link();
        let (cpu_nv12, cpu_ms) = infer_sync("cpu-nv12", ov_cpu, &nv12_input.input, params);
        let encoded = encoded_input(&jpeg);
        let (encoded_hits, encoded_ms) = infer_sync("encoded", ov_cpu, &encoded, params);

        // 证据 1：身份断言（remote 臂检测数逐次=期望）。
        if remote.len() != expect {
            for hit in remote.iter().take(3) {
                eprintln!(
                    "[ov-remote-diag] {name} remote hit class={} conf={:.4} bounds={:?}",
                    hit.class_id, hit.confidence, hit.bounds
                );
            }
        }
        assert_eq!(remote.len(), expect, "{name}: remote 身份漂移");
        // 证据 2：对拍（remote vs CPU 像素臂：计数/类别/框配对/top conf 门）。
        assert_eq!(cpu_nv12.len(), expect, "{name}: CPU 像素臂身份");
        let (matched, worst_iou, _max_conf) = compare_hits(&cpu_nv12, &remote);
        assert_eq!(matched, expect, "{name}: remote/_cpu 配对数");
        assert!(worst_iou >= PAIR_IOU_MIN, "{name}: 配对 IoU {worst_iou}");
        let conf_delta = (top_conf(&cpu_nv12) - top_conf(&remote)).abs();
        assert!(conf_delta <= CONF_GATE, "{name}: top conf Δ={conf_delta}");
        // 证据 3：D3 参考对照——detect 档案冻结参考仅 bus.jpg（D3 单图），
        // zidane 保留身份+对拍两证（原型协议口径 bus=5/zidane=3）。
        let mut enc_iou = f32::NAN;
        let mut enc_conf = f32::NAN;
        let mut map_iou = f32::NAN;
        let mut map_conf = f32::NAN;
        if name == "bus" {
            let reference: Vec<Hit> = {
                let doc: serde_json::Value =
                    serde_json::from_str(&std::fs::read_to_string(&reference_path).unwrap())
                        .unwrap();
                doc["instances"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| Hit {
                        class_id: v["classId"].as_i64().unwrap() as i32,
                        confidence: v["confidence"].as_f64().unwrap() as f32,
                        bounds: [
                            v["bounds"][0].as_f64().unwrap() as f32,
                            v["bounds"][1].as_f64().unwrap() as f32,
                            v["bounds"][2].as_f64().unwrap() as f32,
                            v["bounds"][3].as_f64().unwrap() as f32,
                        ],
                    })
                    .collect()
            };
            assert_eq!(
                encoded_hits.len(),
                reference.len(),
                "{name}: 编码臂 vs 参考计数"
            );
            let (enc_matched, iou, conf) = compare_hits(&reference, &encoded_hits);
            assert_eq!(enc_matched, reference.len(), "{name}: 编码臂 vs 参考配对");
            assert!(iou >= PAIR_IOU_MIN, "{name}: 编码臂配对 IoU {iou}");
            assert!(conf <= CONF_GATE, "{name}: 编码臂 conf Δ={conf}");
            (enc_iou, enc_conf) = (iou, conf);
            // 像素 NV12 臂 → 参考帧逆映射对照（nv12=原图 letterbox 快照）。
            let mapped: Vec<Hit> = cpu_nv12
                .iter()
                .map(|h| Hit {
                    class_id: h.class_id,
                    confidence: h.confidence,
                    bounds: map_to_original(h.bounds, 810, 1080, 640),
                })
                .collect();
            let (map_matched, iou, conf) = compare_hits(&reference, &mapped);
            assert_eq!(
                map_matched,
                reference.len(),
                "{name}: nv12 逆映射 vs 参考配对"
            );
            assert!(iou >= PAIR_IOU_MIN, "{name}: 逆映射 IoU {iou}");
            assert!(conf <= CONF_GATE, "{name}: 逆映射 conf Δ={conf}");
            (map_iou, map_conf) = (iou, conf);
        }

        eprintln!(
            "[ov-remote-e2e] {name}: remote={remote_ms:.1}ms cpu={cpu_ms:.1}ms encoded={encoded_ms:.1}ms | parity iou={worst_iou:.3} Δconf={conf_delta:.4} | vs-ref(encoded) iou={enc_iou:.3} Δconf={enc_conf:.4} | vs-ref(mapped) iou={map_iou:.3} Δconf={map_conf:.4}"
        );
        doc["arms"][name] = serde_json::json!({
            "remote": {"hits": remote, "ms": remote_ms},
            "cpu_nv12": {"hits": cpu_nv12, "ms": cpu_ms},
            "encoded": {"hits": encoded_hits, "ms": encoded_ms},
            "parity": {"pair_iou": worst_iou, "conf_delta": conf_delta},
            "vs_reference_encoded": {"pair_iou": enc_iou, "conf_delta": enc_conf},
            "vs_reference_mapped": {"pair_iou": map_iou, "conf_delta": map_conf},
        });
        drop(texture);
    }

    // 非恒等 letterbox 臂：bus 2x 放大（1280x960→640²：scale=0.5、上下带
    // 160/160）——pad/resize 图路径实数据验证（恒等臂 pad 恒零不触发）。
    // 同引擎多帧尺寸按首版边界显式 notSupported（OV 2025.0.0 同模型二次
    // preprocess 构建实测失败）——先用已建 640² 会话的模型验证拒绝链，再以
    // 独立模型句柄（自有一份 read model）验证非恒等几何本体。
    let bus = std::fs::read(photos_dir.join("bus.nv12")).unwrap();
    let upscaled = upscale_nv12_2x(&bus, 640, 640);
    let texture = unsafe {
        am_vision_core::sdk::intel_nv12_texture_upload(
            &intel_device,
            1280,
            960,
            &upscaled[..1280 * 960],
            &upscaled[1280 * 960..],
        )
        .expect("放大纹理创建")
    };
    let mut gpu_input = gpu_frame_input(texture.0, 1280, 960);
    gpu_input.link();
    {
        let mut out = ptr::null_mut();
        let mut id = 0u64;
        let rc = unsafe {
            amv_infer_image_sync(ov_gpu as _, &gpu_input.input, params, &mut out, &mut id)
        };
        assert_eq!(
            rc, AM_ERR_SDK_NOT_SUPPORTED,
            "同模型第二帧尺寸应显式 notSupported（实测 rc={rc}）"
        );
        eprintln!("[ov-remote-e2e] multi-size rejection: rc={rc}（显式拒绝，像素路径回退可用）");
    }
    let ov_gpu_2x = load_ov_gpu(ctx, &model_path, &profile);
    guard.models.push(ov_gpu_2x);
    let (remote_up, up_ms) = infer_sync("remote-2x", ov_gpu_2x, &gpu_input.input, params);
    remote_frames += 1;
    let mut nv12_input = nv12_view_input(&upscaled, 1280, 960);
    nv12_input.link();
    let (cpu_up, _) = infer_sync("cpu-nv12-2x", ov_cpu, &nv12_input.input, params);
    assert_eq!(remote_up.len(), 5, "放大 letterbox 臂身份漂移");
    assert_eq!(cpu_up.len(), 5, "放大 letterbox CPU 臂身份");
    let (up_matched, up_iou, up_conf) = compare_hits(&cpu_up, &remote_up);
    assert_eq!(up_matched, 5, "放大臂配对数");
    assert!(up_iou >= PAIR_IOU_MIN, "放大臂 IoU {up_iou}");
    assert!(up_conf <= CONF_GATE, "放大臂 conf Δ={up_conf}");
    eprintln!("[ov-remote-e2e] bus-2x letterbox: iou={up_iou:.3} Δconf={up_conf:.4} ms={up_ms:.1}");
    doc["arms"]["bus_2x_letterbox"] = serde_json::json!({
        "remote": remote_up,
        "cpu_nv12": cpu_up,
        "parity": {"pair_iou": up_iou, "conf_delta": up_conf},
        "note": "独立模型句柄（多尺寸单模型为首版 notSupported 边界）",
    });

    // 搬运审计（合同 L790-792）：gpu_inputs 增量=remote 帧数；纯 OV 流量
    // copies=0。
    let audit_after = am_vision_core::gpu::pipeline::audit::snapshot();
    assert_eq!(
        audit_after.0 - audit_before.0,
        remote_frames as u64,
        "gpu_inputs 增量=remote 帧数"
    );
    assert_eq!(audit_after.1, 0, "纯 OV remote 流量 copies 按零计");
    doc["audit"] = serde_json::json!({
        "before": {"gpu_inputs": audit_before.0, "copies": audit_before.1},
        "after": {"gpu_inputs": audit_after.0, "copies": audit_after.1},
        "remote_frames": remote_frames,
    });

    // 证据落档（测试工件，非报告）。
    let evidence = Path::new(REPO).join(".local/evidence/sdk-intel-direct-ib-20261119");
    std::fs::create_dir_all(&evidence).unwrap();
    let out_path = evidence.join("ov-remote-e2e.json");
    std::fs::write(&out_path, serde_json::to_vec_pretty(&doc).unwrap()).unwrap();
    eprintln!("[ov-remote-e2e] evidence: {}", out_path.display());
}
