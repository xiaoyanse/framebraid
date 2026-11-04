//! 目标跟踪四入口 FFI 回归：建档/确认门/同 ID/透传/空帧/TTL 重现/
//! 容量闸与参数校验/负类拒绝/关闭与重复关闭语义。
use super::tracker::{
    amv_tracker_close, amv_tracker_create, amv_tracker_reset, amv_tracker_update,
};
use crate::domain;
use am_sdk_support::{abi::*, context};
use std::{mem::size_of, ptr};

fn open_context() -> usize {
    let params = amv_init_params_t {
        struct_size: size_of::<amv_init_params_t>() as u32,
        abi_version: AM_ABI_VERSION,
        infer_threads: 2,
        ..Default::default()
    };
    // 并行测试共享进程域的上下文准入上限；名额被占满时等待其它测试释放
    for _ in 0..600 {
        match unsafe { context::initialize_vision(domain(), &params) } {
            Ok(key) => return key,
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED) => {
                std::thread::sleep(std::time::Duration::from_millis(5))
            }
            Err(other) => panic!("vision ctx init: {other}"),
        }
    }
    panic!("vision ctx admission never freed")
}

fn close_context(ctx: usize) {
    assert_eq!(
        domain().close(ctx, 5000).unwrap(),
        AM_LIFECYCLE_CLOSED as u32
    );
    domain().destroy(ctx).unwrap();
}

fn detect(x0: f32, y0: f32, x1: f32, y1: f32, conf: f32, class: i32) -> am_detect_box_t {
    am_detect_box_t {
        bounds: [x0, y0, x1, y1],
        confidence: conf,
        class_id: class,
        class_name_utf8: ptr::null(),
    }
}

fn track_params(iou: f32, ttl: u32, confirm: u32, max: u32) -> am_track_params_t {
    am_track_params_t {
        struct_size: size_of::<am_track_params_t>() as u32,
        iou_threshold: iou,
        ttl_frames: ttl,
        confirm_hits: confirm,
        max_tracks: max,
    }
}

fn create(ctx: usize, params: *const am_track_params_t) -> usize {
    let mut out: *mut amv_tracker_t = ptr::null_mut();
    assert_eq!(
        unsafe { amv_tracker_create(ctx as *mut amv_context_t, params, &mut out) },
        AM_ERR_SDK_OK,
        "合法建档参数必须成功"
    );
    out as usize
}

fn reset(tracker: usize) -> i32 {
    unsafe { amv_tracker_reset(tracker as *mut amv_tracker_t) }
}

fn close(tracker: usize) -> i32 {
    unsafe { amv_tracker_close(tracker as *mut amv_tracker_t) }
}

fn update(
    tracker: usize,
    boxes: &[am_detect_box_t],
    capacity: u32,
) -> (i32, u32, Vec<am_tracked_box_t>) {
    let mut out = vec![am_tracked_box_t::default(); capacity as usize];
    let mut written: u32 = u32::MAX;
    let code = unsafe {
        amv_tracker_update(
            tracker as *mut amv_tracker_t,
            if boxes.is_empty() {
                ptr::null()
            } else {
                boxes.as_ptr()
            },
            boxes.len() as u32,
            if capacity == 0 {
                ptr::null_mut()
            } else {
                out.as_mut_ptr()
            },
            capacity,
            &mut written,
        )
    };
    out.truncate(written.min(capacity) as usize);
    (code, written, out)
}

#[test]
fn frame_sequence_confirm_identity_and_passthrough() {
    let ctx = open_context();
    let tracker = create(ctx, &track_params(0.3, 30, 1, 0));
    // 帧1：两框建档（confirm=1 即输出），ID 按建档序 1/2，值透传
    let (code, written, out) = update(
        tracker,
        &[
            detect(0.10, 0.10, 0.30, 0.30, 0.90, 5),
            detect(0.60, 0.60, 0.80, 0.80, 0.75, 7),
        ],
        4,
    );
    assert_eq!((code, written), (AM_ERR_SDK_OK, 2));
    assert_eq!((out[0].track_id, out[1].track_id), (1, 2));
    assert_eq!(
        (out[0].bounds, out[0].confidence, out[0].class_id),
        ([0.10, 0.10, 0.30, 0.30], 0.90, 5),
        "输出=喂入值透传"
    );
    assert_eq!((out[0].age_frames, out[0].hits), (0, 1));
    // 帧2：同框同 ID（age/hits 递增）
    let (code, written, out) = update(
        tracker,
        &[
            detect(0.10, 0.10, 0.30, 0.30, 0.90, 5),
            detect(0.60, 0.60, 0.80, 0.80, 0.75, 7),
        ],
        4,
    );
    assert_eq!((code, written), (AM_ERR_SDK_OK, 2));
    assert_eq!((out[0].track_id, out[1].track_id), (1, 2));
    assert_eq!((out[0].age_frames, out[0].hits), (1, 2));
    // 帧3：移动框（小幅位移 IoU 仍匹配）保持同 ID
    let (_, written, out) = update(
        tracker,
        &[
            detect(0.12, 0.10, 0.32, 0.30, 0.88, 5),
            detect(0.60, 0.60, 0.80, 0.80, 0.75, 7),
        ],
        4,
    );
    assert_eq!(written, 2);
    assert_eq!(out[0].track_id, 1, "移动框恒速跟住不换 ID");
    assert_eq!(out[0].bounds, [0.12, 0.10, 0.32, 0.30]);
    assert_eq!(close(tracker), AM_ERR_SDK_OK);
    close_context(ctx);
}

#[test]
fn default_params_null_pointer_and_confirm_gate() {
    let ctx = open_context();
    // NULL params = 全默认（confirm=3）
    let tracker = create(ctx, ptr::null());
    let b = [detect(0.2, 0.2, 0.5, 0.5, 0.9, 1)];
    let (code, written, _) = update(tracker, &b, 1);
    assert_eq!(
        (code, written),
        (AM_ERR_SDK_OK, 0),
        "默认 confirm=3：第1帧不输出"
    );
    let (_, written, _) = update(tracker, &b, 1);
    assert_eq!(written, 0, "第2帧 hits=2 仍不输出");
    let (code, written, out) = update(tracker, &b, 1);
    assert_eq!((code, written), (AM_ERR_SDK_OK, 1), "第3帧达确认输出");
    assert_eq!((out[0].track_id, out[0].hits, out[0].age_frames), (1, 3, 2));
    assert_eq!(close(tracker), AM_ERR_SDK_OK);
    close_context(ctx);
}

#[test]
fn disappearance_ttl_reappearance_semantics() {
    let ctx = open_context();
    let tracker = create(ctx, &track_params(0.3, 2, 1, 0));
    let b = [detect(0.3, 0.3, 0.6, 0.6, 0.9, 2)];
    let (_, written, out) = update(tracker, &b, 1);
    assert_eq!((written, out[0].track_id), (1, 1));
    // 消失帧（count=0 合法=空帧）：不输出
    let (code, written, _) = update(tracker, &[], 0);
    assert_eq!((code, written), (AM_ERR_SDK_OK, 0));
    // TTL 内（misses=1 < 2）重现：同 ID
    let (_, written, out) = update(tracker, &b, 1);
    assert_eq!((written, out[0].track_id), (1, 1), "TTL 内重现同 ID");
    // 再连续丢 2 帧（misses 达 ttl=2 回收）后重现：新 ID
    update(tracker, &[], 0);
    update(tracker, &[], 0);
    let (_, written, out) = update(tracker, &b, 1);
    assert_eq!((written, out[0].track_id), (1, 2), "超 TTL 重现新 ID");
    assert_eq!(close(tracker), AM_ERR_SDK_OK);
    close_context(ctx);
}

#[test]
fn update_argument_gates_keep_state_intact() {
    let ctx = open_context();
    let tracker = create(ctx, &track_params(0.3, 30, 1, 0));
    let b = [detect(0.2, 0.2, 0.4, 0.4, 0.9, 0)];
    assert_eq!(update(tracker, &b, 1).1, 1);
    // 容量不足拒绝且 out_written 归零
    let (code, written, _) = update(tracker, &b, 0);
    assert_eq!((code, written), (AM_ERR_SDK_INVALID_ARGUMENT, 0));
    // 负 class_id 拒绝
    let bad = [detect(0.2, 0.2, 0.4, 0.4, 0.9, -3)];
    let (code, written, _) = update(tracker, &bad, 1);
    assert_eq!((code, written), (AM_ERR_SDK_INVALID_ARGUMENT, 0));
    // out_written 指针为 NULL 拒绝
    let code = unsafe {
        amv_tracker_update(
            tracker as *mut amv_tracker_t,
            b.as_ptr(),
            1,
            &mut am_tracked_box_t::default(),
            1,
            ptr::null_mut(),
        )
    };
    assert_eq!(code, AM_ERR_SDK_NULL_ARGUMENT);
    // 上述失败不改状态：原轨迹仍按同 ID 继续
    let (_, written, out) = update(tracker, &b, 1);
    assert_eq!((written, out[0].track_id, out[0].hits), (1, 1, 2));
    // 非法句柄
    let code = unsafe {
        amv_tracker_update(
            0xdead as *mut amv_tracker_t,
            b.as_ptr(),
            1,
            &mut am_tracked_box_t::default(),
            1,
            &mut 0,
        )
    };
    assert_eq!(code, AM_ERR_SDK_HANDLE_INVALID);
    assert_eq!(close(tracker), AM_ERR_SDK_OK);
    close_context(ctx);
}

#[test]
fn create_validates_params_and_output_pointer() {
    let ctx = open_context();
    let mut out: *mut amv_tracker_t = ptr::null_mut();
    // out 指针 NULL
    assert_eq!(
        unsafe { amv_tracker_create(ctx as *mut amv_context_t, ptr::null(), ptr::null_mut()) },
        AM_ERR_SDK_NULL_ARGUMENT
    );
    // struct_size 过小
    let mut short = track_params(0.3, 30, 3, 0);
    short.struct_size = 4;
    assert_eq!(
        unsafe { amv_tracker_create(ctx as *mut amv_context_t, &short, &mut out) },
        AM_ERR_SDK_STRUCT_SIZE_INVALID
    );
    // 越界字段逐一拒绝（iou/ttl/confirm/max）
    for violated in [
        track_params(1.5, 30, 3, 0),
        track_params(0.3, 10_001, 3, 0),
        track_params(0.3, 30, 1_001, 0),
        track_params(0.3, 30, 3, 5_000),
    ] {
        assert_eq!(
            unsafe { amv_tracker_create(ctx as *mut amv_context_t, &violated, &mut out) },
            AM_ERR_SDK_INVALID_ARGUMENT,
            "越界参数必须拒绝"
        );
    }
    // NaN iou 同样拒绝
    let mut nan = track_params(0.3, 30, 3, 0);
    nan.iou_threshold = f32::NAN;
    assert_eq!(
        unsafe { amv_tracker_create(ctx as *mut amv_context_t, &nan, &mut out) },
        AM_ERR_SDK_INVALID_ARGUMENT
    );
    // 上下文句柄非法（域内不存在的 key）
    assert_eq!(
        unsafe { amv_tracker_create(0xdead as *mut amv_context_t, ptr::null(), &mut out) },
        AM_ERR_SDK_HANDLE_INVALID
    );
    close_context(ctx);
}

#[test]
fn close_is_immediate_and_repeat_close_invalid() {
    let ctx = open_context();
    let tracker = create(ctx, &track_params(0.3, 30, 1, 0));
    assert_eq!(close(tracker), AM_ERR_SDK_OK);
    // 关闭后：update/reset 均 handleInvalid（句柄已注销）
    let (code, _, _) = update(tracker, &[detect(0.2, 0.2, 0.4, 0.4, 0.9, 0)], 1);
    assert_eq!(code, AM_ERR_SDK_HANDLE_INVALID);
    assert_eq!(reset(tracker), AM_ERR_SDK_HANDLE_INVALID);
    // 重复 close = handleInvalid（对齐模型句柄语义）
    assert_eq!(close(tracker), AM_ERR_SDK_HANDLE_INVALID);
    close_context(ctx);
}

#[test]
fn reset_clears_and_renumbers() {
    let ctx = open_context();
    let tracker = create(ctx, &track_params(0.3, 30, 1, 0));
    let b = [detect(0.2, 0.2, 0.4, 0.4, 0.9, 0)];
    let (_, written, out) = update(tracker, &b, 1);
    assert_eq!((written, out[0].track_id), (1, 1));
    assert_eq!(reset(tracker), AM_ERR_SDK_OK);
    // reset 后：表已清、ID 重新从 1 编号，确认计数重新累计（confirm=1 直接输出）
    let (_, written, out) = update(tracker, &b, 1);
    assert_eq!((written, out[0].track_id, out[0].hits), (1, 1, 1));
    assert_eq!(close(tracker), AM_ERR_SDK_OK);
    close_context(ctx);
}
