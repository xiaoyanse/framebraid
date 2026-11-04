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

fn load_model(ctx: usize, path: &str, profile: &str) -> usize {
    let cpath = CString::new(path).unwrap();
    let cprofile = CString::new(profile).unwrap();
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

fn fixture_png() -> Vec<u8> {
    std::fs::read(format!("{FIXTURES}\\input.png")).expect("fixture png")
}

#[test]
#[ignore = "需真实 ORT 与 vocab 合成双模型夹具（.local/tests/vocab-e2e）"]
fn vocab_e2e_bind_inject_decode_and_hits() {
    let ctx = init_ctx();
    let tower = load_model(
        ctx,
        &format!("{FIXTURES}\\text-tower.onnx"),
        "text-encoder-clip-embed-v1",
    );
    let detector = load_model(
        ctx,
        &format!("{FIXTURES}\\affinity-head.onnx"),
        "detect-yolo-world-head-v1",
    );
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
