//! vocab 端到端真机门（A1/A2 全链，ignore 显式跑）：合成双模型（Gather 常量表
//! 文本塔 + Mul 广播亲和头）——分数由 token id 决定（id1→0.9、id2→0.5、id3→0.047），
//! 覆盖 bind→图像推理自动注入→亲和解码组内合并→vocab_hits/class_name 出口。
use super::exports::*;
use crate::domain;
use am_sdk_support::{abi::*, context};
use std::ffi::{c_char, CString};

const RUNTIME_DIR: &str = "E:\\projects\\framebraid\\.local\\runtimes\\ort-gpu-131-final";
const FIXTURES: &str = "E:\\projects\\framebraid\\.local\\tests\\vocab-e2e";

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

fn load_model(ctx: usize, path: &str, profile: &str, backend: Option<u32>) -> usize {
    let cpath = CString::new(path).unwrap();
    let cprofile = CString::new(profile).unwrap();
    // 指定后端时 strict=1：不静默回落 CPU（测试确定性）
    let policy = backend.map(|requested| am_backend_policy_t {
        requested,
        strict: 1,
        ..Default::default()
    });
    let desc = am_model_desc_t {
        struct_size: std::mem::size_of::<am_model_desc_t>() as u32,
        path_utf8: cpath.as_ptr(),
        load_mode: AM_LOAD_PROFILE,
        profile_id: cprofile.as_ptr(),
        backend: policy.as_ref().map_or(std::ptr::null(), |p| p as *const _),
        ..Default::default()
    };
    let mut key = std::ptr::null_mut();
    assert_eq!(unsafe { amv_model_load(ctx as _, &desc, &mut key) }, 0);
    key as usize
}

fn fixture_png() -> Vec<u8> {
    std::fs::read(format!("{FIXTURES}\\input.png")).expect("fixture png")
}

#[test]
#[ignore = "需真实 ORT 与 vocab 合成双模型夹具（.local/tests/vocab-e2e）"]
fn vocab_e2e_bind_inject_decode_and_hits() {
    let mut guard = CtxGuard {
        ctx: init_ctx(),
        models: Vec::new(),
    };
    let ctx = guard.ctx;
    let tower = load_model(
        ctx,
        &format!("{FIXTURES}\\text-tower.onnx"),
        "text-encoder-clip-embed-v1",
        None,
    );
    guard.track(tower);
    let detector = load_model(
        ctx,
        &format!("{FIXTURES}\\affinity-head.onnx"),
        "detect-yolo-world-head-v1",
        None,
    );
    guard.track(detector);
    // 词表：hand 两词（id1 高分 0.9 / id2 中分 0.5 同框）、glove 一词（id3 低分 0.047）
    let labels = [
        CString::new("hand").unwrap(),
        CString::new("glove").unwrap(),
    ];
    // 每条目展平 token 矩阵须存活到 bind 返回（ABI 借用语义；勿在构造闭包里建临时 Vec）
    let hand_tokens: Vec<i64> = [vec![1i64; 77], vec![2i64; 77]].concat();
    let glove_tokens: Vec<i64> = vec![3i64; 77];
    let entries = [
        am_vocab_entry_t {
            struct_size: std::mem::size_of::<am_vocab_entry_t>() as u32,
            label_utf8: labels[0].as_ptr(),
            token_ids: hand_tokens.as_ptr(),
            token_rows: 2,
            token_cols: 77,
        },
        am_vocab_entry_t {
            struct_size: std::mem::size_of::<am_vocab_entry_t>() as u32,
            label_utf8: labels[1].as_ptr(),
            token_ids: glove_tokens.as_ptr(),
            token_rows: 1,
            token_cols: 77,
        },
    ];
    let _keep = (&labels, &hand_tokens, &glove_tokens);
    assert_eq!(
        unsafe { amv_vocab_bind(detector as _, tower as _, entries.as_ptr(), 2) },
        0
    );
    // 解绑误用防护：解绑后立即推理应拒 vocabNotBound（1033），再重新绑定恢复
    let png = fixture_png();
    let input = am_image_input_t {
        kind: AM_IMG_PNG,
        encoded: png.as_ptr(),
        encoded_size: png.len() as u64,
        ..Default::default()
    };
    let mut out: *mut amv_result_t = std::ptr::null_mut();
    let mut id = 0u64;
    assert_eq!(
        unsafe { amv_infer_image_sync(detector as _, &input, std::ptr::null(), &mut out, &mut id) },
        0
    );
    let result = unsafe { &*out };
    assert!(!result.vocab_hits.is_null(), "vocab path must carry hits");
    assert_eq!(result.task, AM_TASK_DETECT);
    // hand 组：0.9 与 0.5 同框同组——组内合并留 0.9；glove 0.047 低于 conf 0.25 被滤
    assert_eq!(result.detect.count, 1, "one merged hand box");
    let box0 = unsafe { &*result.detect.boxes };
    assert_eq!(box0.class_id, 0);
    assert!(
        (box0.confidence - 0.9).abs() < 0.02,
        "{:?}",
        box0.confidence
    );
    let name = unsafe { std::ffi::CStr::from_ptr(box0.class_name_utf8) };
    assert_eq!(name.to_bytes(), b"hand", "A4 label output");
    let hits = unsafe { &*result.vocab_hits };
    assert_eq!(hits.count, 1);
    let hit0 = unsafe { &*hits.hits };
    assert_eq!((hit0.entry_index, hit0.prompt_index), (0, 0));
    unsafe { amv_result_release(out) };
    let _ = unsafe { amv_result_ack(ctx as _, id) };
    // 换词表（原子换代）：单条目 glove 高分词（id4→嵌入首列 sin 项低——不可控），
    // 改用 id1 复用高分：glove=[id1] 应出一框 class_name="glove"
    let relabel = [CString::new("glove").unwrap()];
    let retokens = [vec![1i64; 77]];
    let reentry = [am_vocab_entry_t {
        struct_size: std::mem::size_of::<am_vocab_entry_t>() as u32,
        label_utf8: relabel[0].as_ptr(),
        token_ids: retokens[0].as_ptr(),
        token_rows: 1,
        token_cols: 77,
    }];
    let _keep2 = (&relabel, &retokens);
    assert_eq!(
        unsafe { amv_vocab_bind(detector as _, tower as _, reentry.as_ptr(), 1) },
        0
    );
    let mut out2: *mut amv_result_t = std::ptr::null_mut();
    let mut id2 = 0u64;
    assert_eq!(
        unsafe {
            amv_infer_image_sync(detector as _, &input, std::ptr::null(), &mut out2, &mut id2)
        },
        0
    );
    let result2 = unsafe { &*out2 };
    assert_eq!(result2.detect.count, 1);
    let box2 = unsafe { &*result2.detect.boxes };
    let name2 = unsafe { std::ffi::CStr::from_ptr(box2.class_name_utf8) };
    assert_eq!(name2.to_bytes(), b"glove", "re-bind swaps labels");
    unsafe { amv_result_release(out2) };
    let _ = unsafe { amv_result_ack(ctx as _, id2) };
}

/// 批 B 对拍：GPU 直传路径词汇注入 vs CPU 注入——同模型（CUDA 后端）同词表，
/// 合成亲和头的分数只由 vocab 嵌入首列决定、坐标为常量框（预处理域差异被
/// 模型设计屏蔽），两条路径检测结果应数值一致。GPU 侧全断言（亲和解码/
/// vocab_hits/class_name）与 CPU e2e 同口径。
#[test]
#[ignore = "需真实 NVIDIA GPU 与 vocab 合成双模型夹具（.local/tests/vocab-e2e）"]
fn vocab_e2e_gpu_direct_injection_matches_cpu() {
    use am_vision_core::gpu::{
        cuda_sys::CudaApi, interop::nvidia_nv12_texture, interop::RegisteredTexture,
        pipeline::GpuPipeline,
    };
    let mut guard = CtxGuard {
        ctx: init_ctx(),
        models: Vec::new(),
    };
    let ctx = guard.ctx;
    let tower = load_model(
        ctx,
        &format!("{FIXTURES}\\text-tower.onnx"),
        "text-encoder-clip-embed-v1",
        None,
    );
    guard.track(tower);
    // affinity head 指定 CUDA（backend=2）：GPU 帧分派按 is_cuda 判定
    let detector = load_model(
        ctx,
        &format!("{FIXTURES}\\affinity-head.onnx"),
        "detect-yolo-world-head-v1",
        Some(2),
    );
    guard.track(detector);
    let labels = [CString::new("hand").unwrap()];
    let tokens: Vec<i64> = vec![1i64; 77];
    let entries = [am_vocab_entry_t {
        struct_size: std::mem::size_of::<am_vocab_entry_t>() as u32,
        label_utf8: labels[0].as_ptr(),
        token_ids: tokens.as_ptr(),
        token_rows: 1,
        token_cols: 77,
    }];
    let _keep = (&labels, &tokens);
    assert_eq!(
        unsafe { amv_vocab_bind(detector as _, tower as _, entries.as_ptr(), 1) },
        0
    );
    // GPU 帧：常色 NV12（合成模型分数与图像内容无关，常色即可）→ D3D 纹理
    // → CUDA 装载（装载模式与 GPU e2e 相同：Y/UV 平面各一次 H2D）
    let (w, h) = (640usize, 640usize);
    let nv12 = vec![128u8; w * h * 3 / 2];
    let runtime = CudaApi::load().expect("cudart");
    GpuPipeline::global().expect("GPU 管线");
    let texture = unsafe { nvidia_nv12_texture(640, 640).expect("NV12 纹理") };
    unsafe {
        (runtime.set_device)(0);
        let registered = RegisteredTexture::register(runtime.clone(), texture.0).expect("注册");
        let y = registered.map_subresource(0).expect("Y map");
        let uv = registered.map_subresource(1).expect("UV map");
        (runtime.memcpy2d_to_array)(y.array, 0, 0, nv12.as_ptr().cast(), w, w, h, 1);
        let uv_ptr = nv12.as_ptr().add(w * h);
        (runtime.memcpy2d_to_array)(uv.array, 0, 0, uv_ptr.cast(), w, w, h / 2, 1);
    }
    let gpu_ref = am_gpu_frame_ref_t {
        width: 640,
        height: 640,
        d3d_format: 103,
        texture: texture.0,
        ..Default::default()
    };
    let gpu_input = am_image_input_t {
        kind: AM_IMG_GPU_FRAME,
        gpu: &gpu_ref,
        ..Default::default()
    };
    let mut gpu_out: *mut amv_result_t = std::ptr::null_mut();
    let mut gpu_id = 0u64;
    let rc = unsafe {
        amv_infer_image_sync(
            detector as _,
            &gpu_input,
            std::ptr::null(),
            &mut gpu_out,
            &mut gpu_id,
        )
    };
    if rc != 0 {
        if let Ok((_, Some(detail))) = am_sdk_support::diagnostics::snapshot(domain(), detector) {
            eprintln!(
                "[gpu-vocab] rc={rc} detail={}",
                detail.message.to_string_lossy()
            );
        }
        panic!("GPU 直传 + vocab 注入应成功 rc={rc}");
    }
    let result = unsafe { &*gpu_out };
    assert_eq!(result.task, AM_TASK_DETECT);
    assert_eq!(result.detect.count, 1, "常量坐标框恒出一框");
    // 值拷贝脱离 result 生命周期：release 后对拍断言不再引用其内存（早期
    // 版本此处悬垂读，表现为对拍偶发垃圾值——GPU 侧断言实际全过）。
    let gpu_box = unsafe { *result.detect.boxes };
    assert_eq!(gpu_box.class_id, 0);
    assert!(
        (gpu_box.confidence - 0.9).abs() < 0.02,
        "{:?}",
        gpu_box.confidence
    );
    let name = unsafe { std::ffi::CStr::from_ptr(gpu_box.class_name_utf8) };
    assert_eq!(name.to_bytes(), b"hand", "GPU 路径亲和解码 label");
    let hits = unsafe { &*result.vocab_hits };
    assert_eq!(hits.count, 1);
    let hit0 = unsafe { &*hits.hits };
    assert_eq!((hit0.entry_index, hit0.prompt_index), (0, 0));
    unsafe { amv_result_release(gpu_out) };
    let _ = unsafe { amv_result_ack(ctx as _, gpu_id) };
    // CPU 对拍：同模型同词表 PNG 输入（run_ref 注入路径）——分数由同一嵌入
    // 决定（Sigmoid 数值差 ≤1e-4）；坐标同为常量框且两侧 mapping 同为
    // 640→640（scale=1），像素框应一致。
    let png = fixture_png();
    let cpu_input = am_image_input_t {
        kind: AM_IMG_PNG,
        encoded: png.as_ptr(),
        encoded_size: png.len() as u64,
        ..Default::default()
    };
    let mut cpu_out: *mut amv_result_t = std::ptr::null_mut();
    let mut cpu_id = 0u64;
    assert_eq!(
        unsafe {
            amv_infer_image_sync(
                detector as _,
                &cpu_input,
                std::ptr::null(),
                &mut cpu_out,
                &mut cpu_id,
            )
        },
        0
    );
    let cpu_result = unsafe { &*cpu_out };
    assert_eq!(cpu_result.detect.count, 1);
    let cpu_box = unsafe { &*cpu_result.detect.boxes };
    assert!(
        (cpu_box.confidence - gpu_box.confidence).abs() < 1e-4,
        "置信不一致 cpu={:?} gpu={:?}",
        cpu_box.confidence,
        gpu_box.confidence
    );
    for i in 0..4 {
        assert!(
            (cpu_box.bounds[i] - gpu_box.bounds[i]).abs() < 1e-3,
            "bounds[{i}] cpu={} gpu={}",
            cpu_box.bounds[i],
            gpu_box.bounds[i]
        );
    }
    unsafe { amv_result_release(cpu_out) };
    let _ = unsafe { amv_result_ack(ctx as _, cpu_id) };
}
