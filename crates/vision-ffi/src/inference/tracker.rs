#![allow(clippy::missing_safety_doc)]
//! 目标跟踪（§4.1b）句柄接线与四入口：update 为同步短调用（不走调度器），
//! 调用线程域锁内串行；close 无在途（无 timeout 语义）立即注销。
use super::lock;
use am_sdk_support::{abi::*, args, handles::Managed, status, Result};
use am_vision_core::sdk::tracker::{
    Tracker, TrackerParams, CONFIRM_HITS_DEFAULT, IOU_THRESHOLD_DEFAULT, MAX_TRACKS_DEFAULT,
    TTL_FRAMES_DEFAULT,
};
use std::{
    any::Any,
    mem::size_of,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub struct TrackerHandle {
    pub tracker: Mutex<Tracker>,
}
// 无调度器参与、无在途请求：默认 request_close/drained 语义即"立即可关"。
impl Managed for TrackerHandle {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// 解析建档参数：逐字段 0=默认，再做有限性/范围校验；违规记参数详情并拒绝。
unsafe fn resolve(raw: am_track_params_t) -> Result<TrackerParams> {
    let params = TrackerParams {
        iou_threshold: if raw.iou_threshold == 0.0 {
            IOU_THRESHOLD_DEFAULT
        } else {
            raw.iou_threshold
        },
        ttl_frames: if raw.ttl_frames == 0 {
            TTL_FRAMES_DEFAULT
        } else {
            raw.ttl_frames
        },
        confirm_hits: if raw.confirm_hits == 0 {
            CONFIRM_HITS_DEFAULT
        } else {
            raw.confirm_hits
        },
        max_tracks: if raw.max_tracks == 0 {
            MAX_TRACKS_DEFAULT
        } else {
            raw.max_tracks as usize
        },
    };
    if !params.valid() {
        am_sdk_support::diagnostics::record(
            crate::domain(),
            0,
            AM_ERR_SDK_INVALID_ARGUMENT,
            am_sdk_support::diagnostics::Detail {
                message: c"track params out of range: iou_threshold 0..=1 (0=0.3), ttl_frames 1..=10000 (0=30), confirm_hits 1..=1000 (0=3), max_tracks 1..=4096 (0=64)",
                source: c"vision.tracker.create",
                fault_class: 0,
            },
        );
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    Ok(params)
}

#[no_mangle]
pub unsafe extern "C" fn amv_tracker_create(
    ctx: *mut amv_context_t,
    params: *const am_track_params_t,
    out_tracker: *mut *mut amv_tracker_t,
) -> i32 {
    status(crate::domain(), ctx as usize, || unsafe {
        am_sdk_support::callbacks::reject_reentry()?;
        args::pointer(out_tracker)?;
        out_tracker.write(std::ptr::null_mut());
        // 传入 ctx 必须是运行中的 Context（防以其它对象句柄冒充）
        let active = crate::domain().active(ctx as usize)?;
        active.data::<am_sdk_support::context::Context>()?;
        let params = if params.is_null() {
            TrackerParams::default()
        } else {
            resolve(args::read(params, size_of::<am_track_params_t>())?)?
        };
        let handle = Arc::new(TrackerHandle {
            tracker: Mutex::new(Tracker::new(params)),
        });
        let key = crate::domain().insert(ctx as usize, handle)?;
        out_tracker.write(key as *mut amv_tracker_t);
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn amv_tracker_update(
    tracker: *mut amv_tracker_t,
    boxes: *const am_detect_box_t,
    count: u32,
    out: *mut am_tracked_box_t,
    out_capacity: u32,
    out_written: *mut u32,
) -> i32 {
    status(crate::domain(), tracker as usize, || unsafe {
        am_sdk_support::callbacks::reject_reentry()?;
        args::pointer(out_written)?;
        out_written.write(0);
        let entry = crate::domain().active(tracker as usize)?;
        let handle = entry.data::<TrackerHandle>()?;
        if out_capacity < count {
            am_sdk_support::diagnostics::record(
                crate::domain(),
                tracker as usize,
                AM_ERR_SDK_INVALID_ARGUMENT,
                am_sdk_support::diagnostics::Detail {
                    message: c"out_capacity < boxes count: output upper bound = input count",
                    source: c"vision.tracker.update",
                    fault_class: 0,
                },
            );
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        let mut inputs: Vec<([f32; 4], f32, u32)> = Vec::new();
        inputs
            .try_reserve_exact(count as usize)
            .map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        if count > 0 {
            args::pointer(boxes)?;
            for raw in std::slice::from_raw_parts(boxes, count as usize) {
                if raw.class_id < 0 {
                    am_sdk_support::diagnostics::record(
                        crate::domain(),
                        tracker as usize,
                        AM_ERR_SDK_INVALID_ARGUMENT,
                        am_sdk_support::diagnostics::Detail {
                            message: c"negative class_id rejected (class_name_utf8 not read)",
                            source: c"vision.tracker.update",
                            fault_class: 0,
                        },
                    );
                    return Err(AM_ERR_SDK_INVALID_ARGUMENT);
                }
                inputs.push((raw.bounds, raw.confidence, raw.class_id as u32));
            }
        }
        let outputs = lock(&handle.tracker)?.update(&inputs);
        let written = outputs.len() as u32;
        debug_assert!(written <= count, "输出上界=输入数");
        if !outputs.is_empty() {
            args::pointer(out)?;
            for (slot, o) in std::slice::from_raw_parts_mut(out, out_capacity as usize)
                .iter_mut()
                .zip(outputs)
            {
                *slot = am_tracked_box_t {
                    bounds: o.bounds,
                    confidence: o.confidence,
                    class_id: o.class_id,
                    track_id: o.track_id,
                    age_frames: o.age_frames,
                    hits: o.hits,
                };
            }
        }
        out_written.write(written);
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn amv_tracker_reset(tracker: *mut amv_tracker_t) -> i32 {
    status(crate::domain(), tracker as usize, || {
        am_sdk_support::callbacks::reject_reentry()?;
        let entry = crate::domain().active(tracker as usize)?;
        lock(&entry.data::<TrackerHandle>()?.tracker)?.reset();
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn amv_tracker_close(tracker: *mut amv_tracker_t) -> i32 {
    status(crate::domain(), tracker as usize, || {
        am_sdk_support::callbacks::reject_reentry()?;
        let entry = crate::domain().get(tracker as usize)?;
        entry.data::<TrackerHandle>()?;
        // 校验后立即放引用：destroy 的 try_unwrap 要求注册槽是最后一个持有者，
        // 持引用注销会恒 tryAgain，句柄滞留注册表 CLOSED 态
        drop(entry);
        // update 为同步短调用：常态 close(0) 一次即 CLOSED；并发 update 在途时
        // 有界等待其退出（域锁内串行，微秒级），仍超时返回 tryAgain 由调用方重试。
        let deadline = Instant::now() + Duration::from_millis(100);
        let state = loop {
            let state = crate::domain().close(tracker as usize, 0)?;
            if state == AM_LIFECYCLE_CLOSED as u32 || Instant::now() >= deadline {
                break state;
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        if state != AM_LIFECYCLE_CLOSED as u32 {
            return Err(AM_ERR_SDK_TRY_AGAIN);
        }
        match crate::domain().destroy(tracker as usize) {
            // destroy 撞上并发只读引用（tryAgain）：状态已 CLOSED、资源已释放，
            // 注册槽由后续 close/destroy 收尾；此时重复 close 返回 OK 属合法中间态
            Ok(()) | Err(AM_ERR_SDK_TRY_AGAIN) => Ok(()),
            Err(code) => Err(code),
        }
    })
}
