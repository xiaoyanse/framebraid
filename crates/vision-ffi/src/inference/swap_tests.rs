//! M5 热切换测试：参数与槽位面用合成句柄覆盖，真实链路标 ignore 显式跑。
use super::{
    models::{Info, ModelHandle},
    state::State,
    swap_exports::amv_model_swap,
};
use crate::domain;
use am_sdk_support::abi::AM_SWAP_DETACH;
use am_sdk_support::{abi::*, context};
use std::{
    ffi::{c_char, CString},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

const RUNTIME_DIR: &str = "E:\\projects\\framebraid\\.local\\runtimes\\ort-gpu-131-final";
const MODEL_A: &str = "E:\\projects\\framebraid\\.local\\tests\\met2-s3\\yolo26n-pose.onnx";
const INPUT_PNG: &str = "E:\\projects\\framebraid\\.local\\tests\\met2-s3\\input.png";
const PROFILE_ID: &str = "pose-yolo26-e2e-coco17-v1";

static FIRST_BOUNCE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

fn init_ctx(dirs: &[&str]) -> usize {
    let names: Vec<CString> = dirs.iter().map(|d| CString::new(*d).unwrap()).collect();
    let pointers: Vec<*const c_char> = names.iter().map(|d| d.as_ptr()).collect();
    let params = amv_init_params_t {
        abi_version: AM_ABI_VERSION,
        runtime_dirs_utf8: pointers.as_ptr(),
        runtime_dir_count: dirs.len() as u32,
        infer_threads: 2,
        ..Default::default()
    };
    unsafe { context::initialize_vision(domain(), &params) }.expect("vision ctx")
}

fn stub_model(ctx: usize) -> usize {
    let state = State::get(ctx).unwrap();
    let handle = Arc::new(ModelHandle {
        session: Mutex::new(None),
        selection: Mutex::new(None),
        info: Info::stub(),
        state: state.clone(),
        inflight: AtomicUsize::new(0),
        stopping: AtomicBool::new(false),
        busy: AtomicBool::new(false),
        policy: Mutex::new(am_realtime_policy_t::default()),
        permit: Mutex::new(None),
        notify: Arc::default(),
    });
    let key = domain().insert(ctx, handle).unwrap();
    state.models.fetch_add(1, Ordering::AcqRel);
    state.swap_slots.lock().unwrap().insert(0, key);
    key
}

fn dispose(keys: &[usize], ctx: usize) {
    for &k in keys {
        if k != 0 {
            assert_eq!(domain().close(k, 5000).unwrap(), AM_LIFECYCLE_CLOSED as u32);
            let _ = domain().destroy(k);
        }
    }
    let _ = domain().close(ctx, 0);
    let _ = domain().destroy(ctx);
}

fn do_swap(ctx: usize, new_path: &str, detach: bool) -> (i32, usize, u64) {
    let swap = am_model_swap_desc_t {
        flags: u32::from(detach),
        ..Default::default()
    };
    let path_c = CString::new(new_path).unwrap();
    let profile_c = CString::new(PROFILE_ID).unwrap();
    let desc = am_model_desc_t {
        path_utf8: path_c.as_ptr(),
        profile_id: profile_c.as_ptr(),
        load_mode: AM_LOAD_PROFILE,
        ..Default::default()
    };
    let mut out: *mut amv_model_t = std::ptr::null_mut();
    let mut old_id: u64 = 0;
    let code = unsafe { amv_model_swap(ctx as _, &swap, &desc, &mut out, &mut old_id) };
    (code, out as usize, old_id)
}

fn load_model(ctx: usize, path: &str, profile: &str) -> usize {
    let path_c = CString::new(path).unwrap();
    let profile_c = CString::new(profile).unwrap();
    let desc = am_model_desc_t {
        path_utf8: path_c.as_ptr(),
        profile_id: profile_c.as_ptr(),
        load_mode: AM_LOAD_PROFILE,
        ..Default::default()
    };
    let mut out: *mut amv_model_t = std::ptr::null_mut();
    let code = unsafe { super::exports::amv_model_load(ctx as _, &desc, &mut out) };
    assert_eq!(code, 0, "model load {path}");
    out as usize
}

fn infer_tracked(
    ctx: usize,
    key: usize,
    png: &[u8],
    failures: &AtomicUsize,
    bounces: &AtomicUsize,
) -> Option<u64> {
    loop {
        match infer_once(ctx, key, png) {
            Ok(uid) => return Some(uid),
            Err(
                code @ (AM_ERR_SDK_QUEUE_FULL
                | AM_ERR_SDK_RESOURCE_EXHAUSTED
                | AM_ERR_SDK_TRY_AGAIN),
            ) => {
                let _ = FIRST_BOUNCE.compare_exchange(0, code, Ordering::AcqRel, Ordering::Acquire);
                bounces.fetch_add(1, Ordering::AcqRel);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            Err(_) => {
                failures.fetch_add(1, Ordering::AcqRel);
                return None;
            }
        }
    }
}

fn uid_of(key: usize) -> u64 {
    let mut info = am_model_info_t::default();
    let code = unsafe { super::exports::amv_model_get_info(key as _, &mut info) };
    assert_eq!(code, 0);
    info.identity.model_uid
}

fn infer_once(ctx: usize, key: usize, png: &[u8]) -> Result<u64, i32> {
    let input = am_image_input_t {
        kind: AM_IMG_PNG,
        encoded: png.as_ptr(),
        encoded_size: png.len() as u64,
        ..Default::default()
    };
    let mut out: *mut amv_result_t = std::ptr::null_mut();
    let mut id = 0u64;
    let code = unsafe {
        super::exports::amv_infer_image_sync(key as _, &input, std::ptr::null(), &mut out, &mut id)
    };
    if code != 0 {
        return Err(code);
    }
    let uid = unsafe { (*out).model.model_uid };
    unsafe { super::exports::amv_result_release(out) };
    let _ = unsafe { super::exports::amv_result_ack(ctx as _, id) };
    Ok(uid)
}

#[test]
fn swap_rejects_bad_arguments() {
    let ctx = init_ctx(&[]);
    let desc = am_model_desc_t {
        path_utf8: c"x.onnx".as_ptr(),
        load_mode: AM_LOAD_RAW,
        ..Default::default()
    };
    let mut out: *mut amv_model_t = std::ptr::null_mut();
    let mut old_id: u64 = 0;
    let base = am_model_swap_desc_t::default();
    assert_eq!(
        unsafe { amv_model_swap(ctx as _, std::ptr::null(), &desc, &mut out, &mut old_id) },
        AM_ERR_SDK_NULL_ARGUMENT
    );
    assert_eq!(
        unsafe { amv_model_swap(ctx as _, &base, &desc, std::ptr::null_mut(), &mut old_id,) },
        AM_ERR_SDK_NULL_ARGUMENT
    );
    assert_eq!(
        unsafe { amv_model_swap(ctx as _, &base, &desc, &mut out, std::ptr::null_mut()) },
        AM_ERR_SDK_NULL_ARGUMENT
    );
    let bad_flags = am_model_swap_desc_t {
        flags: AM_SWAP_DETACH + 1,
        ..Default::default()
    };
    assert_eq!(
        unsafe { amv_model_swap(ctx as _, &bad_flags, &desc, &mut out, &mut old_id) },
        AM_ERR_SDK_INVALID_ARGUMENT
    );
    let bad_reserved = am_model_swap_desc_t {
        reserved: 1,
        ..Default::default()
    };
    assert_eq!(
        unsafe { amv_model_swap(ctx as _, &bad_reserved, &desc, &mut out, &mut old_id) },
        AM_ERR_SDK_INVALID_ARGUMENT
    );
    let bad_slot = am_model_swap_desc_t {
        slot_key: 7,
        ..Default::default()
    };
    assert_eq!(
        unsafe { amv_model_swap(ctx as _, &bad_slot, &desc, &mut out, &mut old_id) },
        AM_ERR_SDK_HANDLE_INVALID
    );
    let _ = domain().close(ctx, 0);
    let _ = domain().destroy(ctx);
}

#[test]
fn swap_requires_bound_slot() {
    let ctx = init_ctx(&[]);
    let (code, out, old) = do_swap(ctx, "missing.onnx", false);
    assert_eq!(code, AM_ERR_SDK_HANDLE_INVALID);
    assert_eq!(out, 0);
    assert_eq!(old, 0);
    let _ = domain().close(ctx, 0);
    let _ = domain().destroy(ctx);
}

#[test]
fn swap_failure_keeps_slot() {
    let ctx = init_ctx(&[]);
    let old_key = stub_model(ctx);
    let state = State::get(ctx).unwrap();
    let swap = am_model_swap_desc_t::default();
    let desc = am_model_desc_t {
        path_utf8: c"missing.onnx".as_ptr(),
        load_mode: AM_LOAD_PROFILE,
        profile_id: std::ptr::null(),
        ..Default::default()
    };
    let mut out: *mut amv_model_t = std::ptr::null_mut();
    let mut old_id: u64 = 0;
    let code = unsafe { amv_model_swap(ctx as _, &swap, &desc, &mut out, &mut old_id) };
    assert_eq!(code, AM_ERR_VISION_PROFILE_REQUIRED);
    assert_eq!(out, std::ptr::null_mut());
    assert_eq!(old_id, 0);
    assert_eq!(
        state.swap_slots.lock().unwrap().get(&0),
        Some(&old_key),
        "swap 失败后槽位必须保持原模型"
    );
    dispose(&[old_key], ctx);
}

#[test]
#[ignore = "需要 ORT 运行库与真实模型；验证槽位寻址/身份/两条释放路径（M5.4）"]
fn swap_slot_addressing_and_release_paths() {
    let ctx = init_ctx(&[RUNTIME_DIR]);
    let k1 = load_model(ctx, MODEL_A, PROFILE_ID);
    let uid1 = uid_of(k1);
    let state = State::get(ctx).unwrap();
    let (code, k2, old) = do_swap(ctx, MODEL_A, false);
    assert_eq!(code, 0);
    assert_ne!(k2, k1);
    assert_eq!(old, k1 as u64);
    let uid2 = uid_of(k2);
    assert_ne!(uid1, uid2);
    assert_eq!(
        state.swap_slots.lock().unwrap().get(&0),
        Some(&k2),
        "swap 后槽位必须指向新模型"
    );
    assert_eq!(
        domain().state(k1).unwrap(),
        AM_LIFECYCLE_RUNNING as u32,
        "非 DETACH：旧模型保持运行态等调用方处置"
    );
    let png = std::fs::read(INPUT_PNG).unwrap();
    assert_eq!(infer_once(ctx, k1, &png), Ok(uid1));
    assert_eq!(infer_once(ctx, k2, &png), Ok(uid2));
    let (code, k3, old2) = do_swap(ctx, MODEL_A, false);
    assert_eq!(code, 0);
    assert_eq!(old2, k2 as u64);
    assert_ne!(k3, k2);
    let (code, k4, old3) = do_swap(ctx, MODEL_A, true);
    assert_eq!(code, 0);
    assert_eq!(old3, k3 as u64);
    assert_eq!(
        domain().close(k3, 5000).unwrap(),
        AM_LIFECYCLE_CLOSED as u32,
        "DETACH：旧模型走既有 close 排空直至 CLOSED"
    );
    assert_eq!(
        domain().state(k1).unwrap(),
        AM_LIFECYCLE_RUNNING as u32,
        "非 DETACH 换出模型不随 DETACH 关闭"
    );
    dispose(&[k1, k2, k3, k4], ctx);
}

#[test]
#[ignore = "需要 ORT 运行库与真实模型；swap 新模型加载失败槽位保持（M5.4）"]
fn swap_failure_keeps_serving_model() {
    let ctx = init_ctx(&[RUNTIME_DIR]);
    let k1 = load_model(ctx, MODEL_A, PROFILE_ID);
    let uid1 = uid_of(k1);
    let state = State::get(ctx).unwrap();
    let (code, out, old) = do_swap(ctx, "E:\\nonexistent\\missing.onnx", false);
    assert_ne!(code, 0);
    assert_eq!(out, 0);
    assert_eq!(old, 0);
    assert_eq!(state.swap_slots.lock().unwrap().get(&0), Some(&k1));
    let png = std::fs::read(INPUT_PNG).unwrap();
    assert_eq!(infer_once(ctx, k1, &png), Ok(uid1));
    dispose(&[k1], ctx);
}

#[test]
#[ignore = "需要 ORT 运行库与真实模型；并发请求跨 swap 零失败且身份分流（M5.4 原子性）"]
fn swap_concurrent_requests_split_identity() {
    let ctx = init_ctx(&[RUNTIME_DIR]);
    let k1 = load_model(ctx, MODEL_A, PROFILE_ID);
    let uid1 = uid_of(k1);
    let png = Arc::new(std::fs::read(INPUT_PNG).unwrap());
    let swapped = Arc::new(AtomicBool::new(false));
    let failures = Arc::new(AtomicUsize::new(0));
    let bounces = Arc::new(AtomicUsize::new(0));
    let pre_ok = Arc::new(AtomicUsize::new(0));
    let post_ok = Arc::new(AtomicUsize::new(0));
    let slot = Arc::new(Mutex::new((0usize, 0u64)));
    let mut workers = Vec::new();
    for _ in 0..4 {
        let (png, swapped, failures, bounces, pre, post, slot) = (
            png.clone(),
            swapped.clone(),
            failures.clone(),
            bounces.clone(),
            pre_ok.clone(),
            post_ok.clone(),
            slot.clone(),
        );
        workers.push(std::thread::spawn(move || {
            while !swapped.load(Ordering::Acquire) {
                if infer_tracked(ctx, k1, &png, &failures, &bounces) == Some(uid1) {
                    pre.fetch_add(1, Ordering::AcqRel);
                } else if !swapped.load(Ordering::Acquire) {
                    failures.fetch_add(1, Ordering::AcqRel);
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            let (k2, uid2) = *slot.lock().unwrap();
            let mut remaining = 2;
            while remaining > 0 {
                match infer_tracked(ctx, k2, &png, &failures, &bounces) {
                    Some(uid) if uid == uid2 => {
                        post.fetch_add(1, Ordering::AcqRel);
                        remaining -= 1;
                    }
                    Some(_) => {
                        failures.fetch_add(1, Ordering::AcqRel);
                        remaining -= 1;
                    }
                    None => remaining -= 1,
                }
            }
        }));
    }
    while pre_ok.load(Ordering::Acquire) == 0 {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let bp_pre = bounces.load(Ordering::Acquire);
    let (k2, uid2) = do_swap_publish(ctx, MODEL_A, k1);
    *slot.lock().unwrap() = (k2, uid2);
    swapped.store(true, Ordering::Release);
    for w in workers {
        w.join().unwrap();
    }
    assert_eq!(
        failures.load(Ordering::Acquire),
        0,
        "swap 期间不允许任何请求失败"
    );
    assert!(pre_ok.load(Ordering::Acquire) > 0);
    assert_eq!(
        post_ok.load(Ordering::Acquire),
        8,
        "swap 后新请求必须全部路由新模型"
    );
    println!(
        "swap_concurrent_identity {{\"workers\":4,\"pre_ok\":{},\"post_ok\":{},\"backpressure\":{},\"backpressure_pre_swap\":{},\"first_bounce_code\":{},\"failures\":{}}}",
        pre_ok.load(Ordering::Acquire),
        post_ok.load(Ordering::Acquire),
        bounces.load(Ordering::Acquire),
        bp_pre,
        FIRST_BOUNCE.load(Ordering::Acquire),
        failures.load(Ordering::Acquire)
    );
    dispose(&[k1, k2], ctx);
}

fn do_swap_publish(ctx: usize, path: &str, expected_old: usize) -> (usize, u64) {
    let (code, key, old) = do_swap(ctx, path, false);
    assert_eq!(code, 0);
    assert_eq!(old, expected_old as u64);
    assert_ne!(key, 0);
    (key, uid_of(key))
}

#[test]
#[ignore = "真实热切换实测（M5.5）：ORT 运行库 + 真实模型；8 线程持续推理跨 swap 零失败"]
fn real_hot_swap_sustained_inference() {
    let ctx = init_ctx(&[RUNTIME_DIR]);
    let k1 = load_model(ctx, MODEL_A, PROFILE_ID);
    let uid1 = uid_of(k1);
    let state = State::get(ctx).unwrap();
    let b_path = std::path::Path::new(MODEL_A).with_file_name("yolo26n-pose.meToo.onnx");
    std::fs::copy(MODEL_A, &b_path).unwrap();
    let png = Arc::new(std::fs::read(INPUT_PNG).unwrap());
    let swapped = Arc::new(AtomicBool::new(false));
    let failures = Arc::new(AtomicUsize::new(0));
    let bounces = Arc::new(AtomicUsize::new(0));
    let pre_ok = Arc::new(AtomicUsize::new(0));
    let post_ok = Arc::new(AtomicUsize::new(0));
    let slot = Arc::new(Mutex::new((0usize, 0u64)));
    let mut workers = Vec::new();
    for _ in 0..8 {
        let (png, swapped, failures, bounces, pre, post, slot) = (
            png.clone(),
            swapped.clone(),
            failures.clone(),
            bounces.clone(),
            pre_ok.clone(),
            post_ok.clone(),
            slot.clone(),
        );
        workers.push(std::thread::spawn(move || {
            while !swapped.load(Ordering::Acquire) {
                if infer_tracked(ctx, k1, &png, &failures, &bounces) == Some(uid1) {
                    pre.fetch_add(1, Ordering::AcqRel);
                } else if !swapped.load(Ordering::Acquire) {
                    failures.fetch_add(1, Ordering::AcqRel);
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            let (k2, uid2) = *slot.lock().unwrap();
            let mut remaining = 4;
            while remaining > 0 {
                match infer_tracked(ctx, k2, &png, &failures, &bounces) {
                    Some(uid) if uid == uid2 => {
                        post.fetch_add(1, Ordering::AcqRel);
                        remaining -= 1;
                    }
                    Some(_) => {
                        failures.fetch_add(1, Ordering::AcqRel);
                        remaining -= 1;
                    }
                    None => remaining -= 1,
                }
            }
        }));
    }
    while pre_ok.load(Ordering::Acquire) < 8 {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let b_str = b_path.to_str().unwrap();
    let bp_pre = bounces.load(Ordering::Acquire);
    let (k2, uid2) = do_swap_publish(ctx, b_str, k1);
    *slot.lock().unwrap() = (k2, uid2);
    swapped.store(true, Ordering::Release);
    for w in workers {
        w.join().unwrap();
    }
    assert_eq!(failures.load(Ordering::Acquire), 0, "全程零失败");
    assert!(pre_ok.load(Ordering::Acquire) >= 8);
    assert_eq!(post_ok.load(Ordering::Acquire), 32);
    assert_ne!(uid1, uid2);
    dispose(&[k1, k2], ctx);
    assert_eq!(
        state.models.load(Ordering::Acquire),
        0,
        "模型计数归位（无泄漏迹象）"
    );
    println!(
        "real_hot_swap {{\"workers\":8,\"pre_ok\":{},\"post_ok\":{},\"backpressure\":{},\"backpressure_pre_swap\":{},\"first_bounce_code\":{},\"failures\":{}}}",
        pre_ok.load(Ordering::Acquire),
        post_ok.load(Ordering::Acquire),
        bounces.load(Ordering::Acquire),
        bp_pre,
        FIRST_BOUNCE.load(Ordering::Acquire),
        failures.load(Ordering::Acquire)
    );
    let _ = std::fs::remove_file(&b_path);
}
