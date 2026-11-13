//! 池 P1.5 编排真装配端到端门（ignore 显式跑）：创建→8 路 lane 路由→并发
//! 提交→逐片攒批→关闭排空守恒。GPU 帧直传臂走真实 world 双模型（CUDA
//! strict，批4/w40），CPU 像素臂走合成 image-dynamic（批2/w40，颜色身份）。
use super::batching_exports::amv_model_get_batching;
use super::exports::*;
use super::pool;
use super::state::State;
use crate::{amv_object_query_state, domain};
use am_sdk_support::{abi::*, context};
use std::{
    ffi::{c_char, c_void, CString},
    mem::size_of,
    ptr,
    sync::{
        atomic::Ordering,
        {Arc, Barrier},
    },
    time::{Duration, Instant},
};

const RUNTIME_DIR: &str = "E:\\projects\\framebraid\\.local\\runtimes\\ort-gpu-131-final";
const FIXTURES: &str = "E:\\projects\\framebraid\\.local\\tests\\batch-image";
const WORLD: &str = "E:\\projects\\framebraid\\.local\\tests\\yolo-world";
const BATCHD: &str = "E:\\projects\\framebraid\\.local\\tests\\yolo-world-batchd";
const CLIP_BOS: i64 = 49406;
const CLIP_EOS: i64 = 49407;
const TOKEN_COLS: usize = 77;

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

/// 串行跑时前门不释放会挤占 SDK_MAX_CONTEXTS：退出（含断言失败提前退出）
/// 统一关闭并注销；分片先于池（子先父后），按 track 顺序析构。
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

fn models_of(ctx: usize) -> usize {
    State::get(ctx).unwrap().models.load(Ordering::Acquire)
}

fn create_pool(ctx: usize, desc: &am_pool_desc_t) -> usize {
    let mut out: *mut amv_pool_t = ptr::null_mut();
    let rc = unsafe { pool::amv_pool_create(ctx as *mut amv_context_t, desc, &mut out) };
    assert_eq!(rc, 0, "池创建应成功 rc={rc}");
    out as usize
}

fn lane(pool_key: usize, i: u32) -> usize {
    let mut out: *mut amv_model_t = ptr::null_mut();
    let rc = unsafe { pool::amv_pool_lane_model(pool_key as *mut amv_pool_t, i, &mut out) };
    assert_eq!(rc, 0, "lane{i} 取句柄应成功 rc={rc}");
    out as usize
}

/// 8 路句柄路由断言（shards=2/lanes=8 在 lane4 分组切换），返回 8 路句柄。
fn assert_lane_routing(pool_key: usize) -> Vec<usize> {
    let mut sink: *mut amv_model_t = ptr::null_mut();
    assert_eq!(
        unsafe { pool::amv_pool_lane_model(pool_key as _, 8, &mut sink) },
        AM_ERR_SDK_INVALID_ARGUMENT,
        "lane=lanes 越界"
    );
    let handles: Vec<usize> = (0..8u32).map(|i| lane(pool_key, i)).collect();
    for (i, handle) in handles.iter().enumerate() {
        let expected = if i < 4 { handles[0] } else { handles[4] };
        assert_eq!(handle, &expected, "lane{i} 分组路由");
    }
    assert_ne!(handles[0], handles[4], "两组分片句柄互异");
    handles
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

fn wait_terminal(ctx: usize, id: u64) -> i32 {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let mut snap: *mut am_request_snapshot_t = ptr::null_mut();
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

fn close_and_assert_drain(pool_key: usize, shards: &[usize], ctx: usize, kept: usize) {
    let mut state = 0u32;
    assert_eq!(
        unsafe { pool::amv_pool_close(pool_key as _, 5000, &mut state) },
        0
    );
    assert_eq!(state, AM_LIFECYCLE_CLOSED as u32, "关闭应排空至 CLOSED");
    let mut sink: *mut amv_model_t = ptr::null_mut();
    assert_eq!(
        unsafe { pool::amv_pool_lane_model(pool_key as _, 0, &mut sink) },
        AM_ERR_SDK_HANDLE_CLOSED,
        "池关闭后 lane_model=handleClosed"
    );
    for key in shards {
        let mut shard_state = 0u32;
        assert_eq!(
            unsafe { amv_object_query_state(*key as _, &mut shard_state) },
            0
        );
        assert_eq!(shard_state, AM_LIFECYCLE_CLOSED as u32, "逐片随池排空");
    }
    for key in shards {
        domain().destroy(*key).unwrap();
    }
    domain().destroy(pool_key).unwrap();
    assert_eq!(models_of(ctx), kept, "槽位归还");
}

fn load_profile_model(
    ctx: usize,
    path: &str,
    profile: &str,
    backend: Option<am_backend_policy_t>,
) -> usize {
    let cpath = CString::new(path).unwrap();
    let cprofile = CString::new(profile).unwrap();
    let desc = am_model_desc_t {
        struct_size: size_of::<am_model_desc_t>() as u32,
        path_utf8: cpath.as_ptr(),
        load_mode: AM_LOAD_PROFILE,
        profile_id: cprofile.as_ptr(),
        backend: backend.as_ref().map_or(ptr::null(), |p| p as *const _),
        ..Default::default()
    };
    let mut key: *mut amv_model_t = ptr::null_mut();
    assert_eq!(
        unsafe { amv_model_load(ctx as _, &desc, &mut key) },
        0,
        "{path}"
    );
    key as usize
}

fn submit_png(model: usize, name: &str) -> u64 {
    let png = std::fs::read(format!("{FIXTURES}\\{name}.png")).expect("fixture png");
    let input = am_image_input_t {
        kind: AM_IMG_PNG,
        encoded: png.as_ptr(),
        encoded_size: png.len() as u64,
        ..Default::default()
    };
    let raw = am_raw_preprocess_t {
        struct_size: size_of::<am_raw_preprocess_t>() as u32,
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
    let mut id = 0u64;
    let rc = unsafe { amv_infer_image_async(model as _, &input, &params, &mut id) };
    assert_eq!(rc, 0, "PNG 异步提交应被接受 rc={rc}");
    id
}

fn fetch_values(ctx: usize, id: u64) -> [f32; 9] {
    let mut out: *mut amv_result_t = ptr::null_mut();
    assert_eq!(unsafe { amv_result_fetch(ctx as _, id, &mut out) }, 0);
    let result = unsafe { &*out };
    assert_eq!(result.raw_output_count, 1, "单输出模型");
    let tensor = unsafe { &*result.raw_outputs };
    let shape = unsafe { std::slice::from_raw_parts(tensor.shape, tensor.shape_len as usize) };
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

fn assert_color_identity(label: &str, values: [f32; 9], rgb: [f32; 3]) {
    let mut expected = [0f32; 9];
    for c in 0..3 {
        expected[c] = rgb[c];
        expected[3 + c] = rgb[c] * 2.0;
        expected[6 + c] = rgb[c] * 4.0;
    }
    for (i, (a, b)) in values.iter().zip(&expected).enumerate() {
        assert!(
            (a - b).abs() < 1e-4,
            "{label}[{i}] 身份漂移: got={a} want={b}"
        );
    }
}

/// CPU 像素通道批 2 臂：shards=2/lanes=8，8 路并发提交逐项颜色身份，
/// 两片各自成 2+2 满批、零拒绝，关闭排空后槽位归零可再建池。
#[test]
#[ignore = "需真实 ORT 与合成批模型夹具（.local/tests/batch-image）"]
fn pool_e2e_cpu_pixel_batch2_sharded_flow() {
    let mut guard = CtxGuard {
        ctx: init_ctx(),
        models: Vec::new(),
    };
    let ctx = guard.ctx;
    let cpath = CString::new(format!("{FIXTURES}\\image-dynamic.onnx")).unwrap();
    let model_desc = am_model_desc_t {
        struct_size: size_of::<am_model_desc_t>() as u32,
        path_utf8: cpath.as_ptr(),
        load_mode: AM_LOAD_RAW,
        ..Default::default()
    };
    let batching = am_batching_options_t {
        abi_minor: AM_ABI_VERSION & 0xffff,
        max_batch_items: 2,
        max_wait_ms: 40,
        flags: AM_BATCHING_ENABLE,
        ..Default::default()
    };
    let desc = am_pool_desc_t {
        struct_size: size_of::<am_pool_desc_t>() as u32,
        model: &model_desc,
        shards: 2,
        lanes: 8,
        batching: &batching,
        ..Default::default()
    };
    let pool_key = create_pool(ctx, &desc);
    let handles = assert_lane_routing(pool_key);
    for handle in &handles {
        guard.track(*handle);
    }
    guard.track(pool_key);
    let (shard_a, shard_b) = (handles[0], handles[4]);
    let before_a = stats(shard_a);
    let before_b = stats(shard_b);
    let names = ["red", "green", "blue", "yellow"];
    let colors = [
        [1.0f32, 0.0, 0.0],
        [0.0f32, 1.0, 0.0],
        [0.0f32, 0.0, 1.0],
        [1.0f32, 1.0, 0.0],
    ];
    let barrier = Arc::new(Barrier::new(8));
    let ids: Vec<u64> = std::thread::scope(|s| {
        let threads: Vec<_> = handles
            .iter()
            .enumerate()
            .map(|(i, handle)| {
                let barrier = barrier.clone();
                let name = names[i % 4];
                s.spawn(move || {
                    barrier.wait();
                    submit_png(*handle, name)
                })
            })
            .collect();
        threads
            .into_iter()
            .map(|t| t.join().expect("提交线程应正常结束"))
            .collect()
    });
    for (i, id) in ids.iter().enumerate() {
        assert_eq!(wait_terminal(ctx, *id), AM_REQ_DONE, "lane{i} 终态");
        let label = format!("lane{i}");
        assert_color_identity(&label, fetch_values(ctx, *id), colors[i % 4]);
        assert_eq!(unsafe { amv_result_ack(ctx as _, *id) }, 0);
    }
    for (shard, before, tag) in [(shard_a, before_a, "A"), (shard_b, before_b, "B")] {
        let after = stats(shard);
        eprintln!(
            "[pool-e2e-cpu] 分片{tag} formed={} batched={} rejected={}",
            after.formed_batches, after.batched_items, after.rejected_items
        );
        assert!(
            after.formed_batches >= before.formed_batches + 2,
            "分片{tag} 4 项应成 2+2 两批"
        );
        assert!(
            after.batched_items >= before.batched_items + 4,
            "分片{tag} 全部 4 项计入成批"
        );
        assert_eq!(after.rejected_items, 0, "分片{tag} 零准入背压");
    }
    close_and_assert_drain(pool_key, &[shard_a, shard_b], ctx, 0);
    let rebuild_model = am_model_desc_t {
        struct_size: size_of::<am_model_desc_t>() as u32,
        path_utf8: cpath.as_ptr(),
        load_mode: AM_LOAD_RAW,
        ..Default::default()
    };
    let rebuild_desc = am_pool_desc_t {
        struct_size: size_of::<am_pool_desc_t>() as u32,
        model: &rebuild_model,
        shards: 2,
        lanes: 2,
        ..Default::default()
    };
    let rebuild = create_pool(ctx, &rebuild_desc);
    let rebuild_shards = [lane(rebuild, 0), lane(rebuild, 1)];
    for handle in rebuild_shards {
        guard.track(handle);
    }
    guard.track(rebuild);
    close_and_assert_drain(rebuild, &rebuild_shards, ctx, 0);
}

/// GPU 帧直传臂：真实 world 检测分片 ×2（CUDA strict，desc 逐片批4/w40）+
/// 文本塔逐片绑词表。纹理 setup 一次装载（POC multilane 同款），逐纹理预热
/// 令 SDK 注册缓存全命中后 8 路屏障并发直传；逐项 DONE+输出布局断言，两片
/// formed/batched 增长零拒绝；关闭排空守恒可再建池。检测语义质量归批 D 门。
#[test]
#[ignore = "需真实 NVIDIA GPU 与 yolo-world 双模型（.local/tests/yolo-world 及 -batchd 派生件）"]
fn pool_e2e_sharded_batching_gpu_direct_full_flow() {
    use am_vision_core::gpu::{
        cuda_sys::CudaApi,
        interop::{nvidia_nv12_texture, RegisteredTexture},
        pipeline::GpuPipeline,
    };
    let mut guard = CtxGuard {
        ctx: init_ctx(),
        models: Vec::new(),
    };
    let ctx = guard.ctx;
    let tower_a = load_profile_model(
        ctx,
        &format!("{WORLD}\\text.onnx"),
        "text-encoder-clip-dynseq-v1",
        None,
    );
    guard.track(tower_a);
    let tower_b = load_profile_model(
        ctx,
        &format!("{WORLD}\\text.onnx"),
        "text-encoder-clip-dynseq-v1",
        None,
    );
    guard.track(tower_b);

    let cpath = CString::new(format!("{BATCHD}\\world-batch.onnx")).unwrap();
    let cprofile = CString::new("detect-yolo-world-batch-v1").unwrap();
    let cuda = am_backend_policy_t {
        requested: AM_BACKEND_CUDA as u32,
        strict: 1,
        ..Default::default()
    };
    let shard_desc = am_model_desc_t {
        struct_size: size_of::<am_model_desc_t>() as u32,
        path_utf8: cpath.as_ptr(),
        load_mode: AM_LOAD_PROFILE,
        profile_id: cprofile.as_ptr(),
        backend: &cuda,
        ..Default::default()
    };
    let batching = am_batching_options_t {
        abi_minor: AM_ABI_VERSION & 0xffff,
        max_batch_items: 4,
        max_wait_ms: 40,
        flags: AM_BATCHING_ENABLE,
        ..Default::default()
    };
    let desc = am_pool_desc_t {
        struct_size: size_of::<am_pool_desc_t>() as u32,
        model: &shard_desc,
        shards: 2,
        lanes: 8,
        batching: &batching,
        ..Default::default()
    };
    let pool_key = create_pool(ctx, &desc);
    let handles = assert_lane_routing(pool_key);
    for handle in &handles {
        guard.track(*handle);
    }
    guard.track(pool_key);
    let (shard_a, shard_b) = (handles[0], handles[4]);

    let label = CString::new("helmet").unwrap();
    let tokens = {
        let mut row = vec![0i64; TOKEN_COLS];
        row[0] = CLIP_BOS;
        row[1] = 777;
        row[2] = CLIP_EOS;
        row
    };
    let entry = am_vocab_entry_t {
        struct_size: size_of::<am_vocab_entry_t>() as u32,
        label_utf8: label.as_ptr(),
        token_ids: tokens.as_ptr(),
        token_rows: 1,
        token_cols: TOKEN_COLS as u32,
    };
    for (shard, tower) in [(shard_a, tower_a), (shard_b, tower_b)] {
        assert_eq!(
            unsafe { amv_vocab_bind(shard as _, tower as _, &entry, 1) },
            0,
            "逐片绑词表"
        );
    }

    let runtime = CudaApi::load().expect("cudart");
    GpuPipeline::global().expect("GPU 管线");
    unsafe { (runtime.set_device)(0) };
    let frames = ["seek_f030", "seek_f031", "s1_f013", "s1_f014"];
    let textures: Vec<am_vision_core::gpu::interop::Com> = frames
        .iter()
        .cycle()
        .take(8)
        .map(|frame| {
            let nv12 = std::fs::read(format!("{BATCHD}\\{frame}.nv12")).expect("nv12");
            let (w, h) = (640usize, 640usize);
            let texture = unsafe { nvidia_nv12_texture(640, 640).expect("NV12 纹理") };
            unsafe {
                let registered =
                    RegisteredTexture::register(runtime.clone(), texture.0).expect("注册");
                let y = registered.map_subresource(0).expect("Y map");
                let uv = registered.map_subresource(1).expect("UV map");
                (runtime.memcpy2d_to_array)(y.array, 0, 0, nv12.as_ptr().cast(), w, w, h, 1);
                let uv_ptr = nv12.as_ptr().add(w * h);
                (runtime.memcpy2d_to_array)(uv.array, 0, 0, uv_ptr.cast(), w, w, h / 2, 1);
            }
            texture
        })
        .collect();
    let tex_ptrs: Vec<usize> = textures.iter().map(|t| t.0 as usize).collect();

    let submit_gpu = |model: usize, texture: *mut c_void| -> u64 {
        let gpu_ref = am_gpu_frame_ref_t {
            struct_size: size_of::<am_gpu_frame_ref_t>() as u32,
            width: 640,
            height: 640,
            d3d_format: 103,
            texture,
            ..Default::default()
        };
        let input = am_image_input_t {
            kind: AM_IMG_GPU_FRAME,
            gpu: &gpu_ref,
            ..Default::default()
        };
        let params = am_infer_params_t {
            confidence_threshold: 1.0,
            ..Default::default()
        };
        let mut id = 0u64;
        let rc = unsafe { amv_infer_image_async(model as _, &input, &params, &mut id) };
        assert_eq!(rc, 0, "GPU 直传异步提交应被接受 rc={rc}");
        id
    };
    // 逐纹理预热：PTX JIT+每纹理 SDK 注册在预热段完成并缓存，40ms 窗口内
    // 并发提交不再付首触成本（主线程预热即 POC multilane 同款）。
    for (i, texture) in tex_ptrs.iter().enumerate() {
        let id = submit_gpu(handles[i], *texture as *mut c_void);
        assert_eq!(wait_terminal(ctx, id), AM_REQ_DONE, "预热 lane{i}");
        let mut out: *mut amv_result_t = ptr::null_mut();
        assert_eq!(unsafe { amv_result_fetch(ctx as _, id, &mut out) }, 0);
        unsafe { amv_result_release(out) };
        assert_eq!(unsafe { amv_result_ack(ctx as _, id) }, 0);
    }
    let before_a = stats(shard_a);
    let before_b = stats(shard_b);

    let barrier = Arc::new(Barrier::new(8));
    let ids: Vec<u64> = std::thread::scope(|s| {
        let threads: Vec<_> = handles
            .iter()
            .zip(&tex_ptrs)
            .map(|(handle, texture)| {
                let barrier = barrier.clone();
                let handle = *handle;
                let texture = *texture;
                s.spawn(move || {
                    barrier.wait();
                    submit_gpu(handle, texture as *mut c_void)
                })
            })
            .collect();
        threads
            .into_iter()
            .map(|t| t.join().expect("提交线程应正常结束"))
            .collect()
    });
    for (i, id) in ids.iter().enumerate() {
        let id = *id;
        assert_eq!(wait_terminal(ctx, id), AM_REQ_DONE, "lane{i} 直传终态");
        let mut out: *mut amv_result_t = ptr::null_mut();
        assert_eq!(unsafe { amv_result_fetch(ctx as _, id, &mut out) }, 0);
        let result = unsafe { &*out };
        assert_eq!(result.task, AM_TASK_DETECT, "lane{i} 检测任务出口");
        assert_eq!(result.raw_output_count, 1, "单输出检测头");
        let tensor = unsafe { &*result.raw_outputs };
        let shape = unsafe { std::slice::from_raw_parts(tensor.shape, tensor.shape_len as usize) };
        assert_eq!(
            shape,
            &[1, 5, 8400],
            "lane{i} 拆分后逐项布局 [1,4+1行,8400]"
        );
        unsafe { amv_result_release(out) };
        assert_eq!(unsafe { amv_result_ack(ctx as _, id) }, 0);
    }
    for (shard, before, tag) in [(shard_a, before_a, "A"), (shard_b, before_b, "B")] {
        let after = stats(shard);
        eprintln!(
            "[pool-e2e-gpu] 分片{tag} formed={} batched={} rejected={}",
            after.formed_batches, after.batched_items, after.rejected_items
        );
        assert!(
            after.formed_batches > before.formed_batches,
            "分片{tag} 攒批窗口应成批"
        );
        assert!(
            after.batched_items >= before.batched_items + 2,
            "分片{tag} 应有多项物理批"
        );
        assert_eq!(after.rejected_items, 0, "分片{tag} 零准入背压");
    }

    close_and_assert_drain(pool_key, &[shard_a, shard_b], ctx, 2);
    let rebuild_cpath = CString::new(format!("{WORLD}\\text.onnx")).unwrap();
    let rebuild_cprofile = CString::new("text-encoder-clip-dynseq-v1").unwrap();
    let rebuild_model = am_model_desc_t {
        struct_size: size_of::<am_model_desc_t>() as u32,
        path_utf8: rebuild_cpath.as_ptr(),
        load_mode: AM_LOAD_PROFILE,
        profile_id: rebuild_cprofile.as_ptr(),
        ..Default::default()
    };
    let rebuild_source = am_pool_desc_t {
        struct_size: size_of::<am_pool_desc_t>() as u32,
        model: &rebuild_model,
        shards: 2,
        lanes: 2,
        ..Default::default()
    };
    let rebuild = create_pool(ctx, &rebuild_source);
    let rebuild_shards = [lane(rebuild, 0), lane(rebuild, 1)];
    for handle in rebuild_shards {
        guard.track(handle);
    }
    guard.track(rebuild);
    close_and_assert_drain(rebuild, &rebuild_shards, ctx, 2);
}
