//! GPU 帧池单测（M2a 2.5）：池满拒绝、租约归还、审计计数。
use super::gpu_pool::*;
use super::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

fn inner(max_frames: usize, max_bytes: u64) -> Arc<GpuFramePoolInner> {
    Arc::new(GpuFramePoolInner {
        frames_in_use: AtomicU64::new(0),
        bytes_in_use: AtomicU64::new(0),
        max_frames,
        max_bytes,
        stats_delivered: AtomicU64::new(0),
        stats_cpu_fallbacks: AtomicU64::new(0),
        stats_bytes_est: AtomicU64::new(0),
        stopping: AtomicBool::new(false),
        software_only: false,
    })
}

fn info(rgba_bytes: usize) -> am_camera_core::media::codec::GpuFrameInfo {
    am_camera_core::media::codec::GpuFrameInfo {
        info: am_camera_core::media::codec::FrameInfo {
            width: 640,
            height: 360,
            pts: 100,
            rgba_bytes,
        },
        d3d_format: 87, // DXGI_FORMAT_NV12
        subresource: 0,
        shared_nt_handle: 0xF00D,
    }
}

#[test]
fn pool_full_rejects_and_does_not_overwrite() {
    let pool = inner(2, 1_000_000);
    let a = pool.publish(&info(100), std::ptr::null_mut()).unwrap();
    let _b = pool.publish(&info(100), std::ptr::null_mut()).unwrap();
    let result = pool.publish(&info(100), std::ptr::null_mut());
    assert_eq!(result.err(), Some(AM_ERR_SDK_RESOURCE_EXHAUSTED));
    // 池满时不覆写活帧：前两帧 desc 仍有效。
    assert_eq!(a.desc.width, 640);
    drop(a);
    // 归还一帧后新发布成功。
    let _c = pool.publish(&info(100), std::ptr::null_mut()).unwrap();
}

#[test]
fn byte_budget_rejects_oversized() {
    let pool = inner(10, 200);
    // 首帧须绑定变量持有租约：临时值会在语句尾 Drop 归还预算。
    let _held = pool.publish(&info(150), std::ptr::null_mut()).unwrap();
    let result = pool.publish(&info(100), std::ptr::null_mut());
    assert_eq!(result.err(), Some(AM_ERR_SDK_RESOURCE_EXHAUSTED));
}

#[test]
fn drop_returns_lease_and_updates_counters() {
    let pool = inner(4, 1_000_000);
    {
        let _frame = pool.publish(&info(200), std::ptr::null_mut()).unwrap();
        assert_eq!(pool.frames_in_use.load(Ordering::Acquire), 1);
        assert_eq!(pool.bytes_in_use.load(Ordering::Acquire), 200);
        assert_eq!(pool.stats_delivered.load(Ordering::Acquire), 1);
        assert_eq!(pool.stats_bytes_est.load(Ordering::Acquire), 200);
    }
    // Drop 后计数归零。
    assert_eq!(pool.frames_in_use.load(Ordering::Acquire), 0);
    assert_eq!(pool.bytes_in_use.load(Ordering::Acquire), 0);
}

#[test]
fn non_shared_texture_marks_fallback() {
    let pool = inner(4, 1_000_000);
    let mut fallback_info = info(100);
    fallback_info.shared_nt_handle = 0; // 不可共享
    let frame = pool.publish(&fallback_info, std::ptr::null_mut()).unwrap();
    assert_eq!(frame.desc.flags, 1, "缺共享标志帧应标 fallback=1");
    // 可共享帧 flags=0。
    let shared = pool.publish(&info(100), std::ptr::null_mut()).unwrap();
    assert_eq!(shared.desc.flags, 0);
}

#[test]
fn software_only_rejects_with_not_supported() {
    let mut pool = inner(4, 1_000_000);
    Arc::get_mut(&mut pool).unwrap().software_only = true;
    let result = pool.publish(&info(100), std::ptr::null_mut());
    // §2.6a：SOFTWARE_ONLY 下 GPU 订阅返回 notSupported，不静默降级。
    assert_eq!(result.err(), Some(AM_ERR_SDK_NOT_SUPPORTED));
    assert_eq!(pool.frames_in_use.load(Ordering::Acquire), 0);
}

#[test]
fn stopping_rejects_new_publish_but_leases_stay_valid() {
    let pool = inner(4, 1_000_000);
    let held = pool.publish(&info(120), std::ptr::null_mut()).unwrap();
    pool.stopping.store(true, Ordering::Release);
    // 关闭排空：拒绝新发布，语义与共享帧池关闭一致。
    let result = pool.publish(&info(100), std::ptr::null_mut());
    assert_eq!(result.err(), Some(AM_ERR_SDK_HANDLE_CLOSED));
    // 在途租约仍有效：desc 元数据可读，Drop 正常归还。
    assert_eq!(held.desc.width, 640);
    drop(held);
    assert_eq!(pool.frames_in_use.load(Ordering::Acquire), 0);
}

#[test]
fn fallback_counter_tracks_cpu_fallbacks() {
    let pool = inner(4, 1_000_000);
    let mut fallback_info = info(100);
    fallback_info.shared_nt_handle = 0;
    let _a = pool.publish(&fallback_info, std::ptr::null_mut()).unwrap();
    let _b = pool.publish(&fallback_info, std::ptr::null_mut()).unwrap();
    let _c = pool.publish(&info(100), std::ptr::null_mut()).unwrap();
    // 2 帧 flags=1（降级回读交付），1 帧正常共享。
    assert_eq!(pool.stats_cpu_fallbacks.load(Ordering::Acquire), 2);
    assert_eq!(pool.stats_delivered.load(Ordering::Acquire), 3);
}

#[test]
fn desc_metadata_complete() {
    let pool = inner(4, 1_000_000);
    let frame = pool.publish(&info(300), 0x1234_5678 as *mut _).unwrap();
    assert_eq!(frame.desc.width, 640);
    assert_eq!(frame.desc.height, 360);
    assert_eq!(frame.desc.d3d_format, 87);
    assert_eq!(frame.desc.subresource, 0);
    assert_eq!(frame.desc.shared_nt_handle, 0xF00D);
    assert_eq!(
        frame.desc.struct_size,
        std::mem::size_of::<am_gpu_frame_desc_t>() as u32
    );
}
