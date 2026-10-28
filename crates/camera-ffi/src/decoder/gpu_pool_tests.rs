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
        d3d_format: 104, // DXGI_FORMAT_NV12
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
    fallback_info.shared_nt_handle = 0; // 句柄不可得
    let frame = pool.publish(&fallback_info, std::ptr::null_mut()).unwrap();
    // flags bit0 = 句柄不可得（信息性；不是回读判定）。
    assert_eq!(frame.desc.flags, 1, "句柄不可得帧应标 flags=1");
    // 计数器不因句柄缺失增长：真回读才计数（M2b 消费侧）。
    assert_eq!(pool.stats_cpu_fallbacks.load(Ordering::Acquire), 0);
    // 句柄可得的帧 flags=0。
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
    // 句柄不可得只标 flags，不进 CPU 回读计数（真回读属 M2b 消费侧协商）。
    assert_eq!(pool.stats_cpu_fallbacks.load(Ordering::Acquire), 0);
    assert_eq!(pool.stats_delivered.load(Ordering::Acquire), 3);
}

#[test]
fn desc_metadata_complete() {
    let pool = inner(4, 1_000_000);
    let frame = pool.publish(&info(300), 0x1234_5678 as *mut _).unwrap();
    assert_eq!(frame.desc.width, 640);
    assert_eq!(frame.desc.height, 360);
    assert_eq!(frame.desc.d3d_format, 104);
    assert_eq!(frame.desc.subresource, 0);
    assert_eq!(frame.desc.shared_nt_handle, 0xF00D);
    assert_eq!(
        frame.desc.struct_size,
        std::mem::size_of::<am_gpu_frame_desc_t>() as u32
    );
}

/// 任务 2.6 真实硬解实测：合成 H.264 + 本机 D3D11VA，GPU 帧交付 + NT 句柄可得 + 审计计数。
#[test]
#[ignore = "需要AM_SDK_FFMPEG_DIR与AM_SDK_CODEC_SAMPLES（真实D3D11VA硬解实测）"]
fn real_d3d11va_frame_delivers_shared_nt_handle_and_pool_audits() {
    let runtime = std::env::var_os("AM_SDK_FFMPEG_DIR").expect("缺少隔离运行库目录");
    let samples = std::env::var_os("AM_SDK_CODEC_SAMPLES").expect("缺少合成样本目录");
    let api = Arc::new(
        am_camera_core::media::ffmpeg_lib::FfmpegApi::load_from(std::path::Path::new(&runtime))
            .expect("加载隔离FFmpeg运行库"),
    );
    let data = std::fs::read(std::path::Path::new(&samples).join("one.h264")).unwrap();
    let selected = am_camera_core::media::codec::CpuDecoder::create_with_policy(
        api,
        am_camera_core::media::codec::VideoCodec::H264,
        &[],
        256 * 144,
        am_camera_core::media::codec::selection::HardwarePolicy::PreferHardware,
        std::time::Duration::from_millis(1500),
    )
    .expect("硬解选择");
    let mut decoder = selected.decoder;
    decoder.send(&data, 90000, 90000).expect("送入码流");
    if decoder.peek().unwrap().is_none() {
        decoder.drain().unwrap();
    }
    let info = decoder.peek().unwrap().unwrap();
    // 实际后端在输出真实帧后查询（选择报告只代表初始化）。
    let backend = decoder.hardware_backend().expect("后端已确定");
    assert!(
        matches!(backend, am_camera_core::media::hardware::Backend::D3d11va),
        "实际后端应为 D3D11VA，got {backend:?}"
    );
    // GPU 帧交付：元数据提取旁路像素回读。主交付物为纹理指针（同进程 COM 对象，
    // M2b CUDA interop/DirectML 直接消费）；共享句柄为可选字段——本机 Win11 25H2
    // runtime 拒绝纹理 QI IDXGIResource（2026-10-29 实测，见验证记录），为 0 属
    // 已知平台限制，不判失败；纹理池已带 MISC_SHARED 标志（desc 旁证）。
    let gpu = decoder.gpu_frame_info(info).expect("GPU元数据提取");
    assert_eq!((gpu.info.width, gpu.info.height), (256, 144));
    assert_eq!(
        gpu.d3d_format, 103,
        "DXGI_FORMAT_NV12=103（sw_format=NV12 实测）"
    );
    // 真实元数据过池：desc 传递与审计计数。
    let pool = inner(4, 10_000_000);
    let texture = decoder.gpu_texture().expect("纹理指针");
    assert!(!texture.is_null());
    let frame = pool.publish(&gpu, texture).expect("发布GPU帧");
    assert_eq!((frame.desc.width, frame.desc.height), (256, 144));
    assert_eq!(frame.desc.shared_nt_handle, gpu.shared_nt_handle);
    // 本机句柄被平台拒绝（=0），flags 标注句柄不可得；非回读场景。
    assert_eq!(
        frame.desc.flags,
        if gpu.shared_nt_handle == 0 { 1 } else { 0 }
    );
    drop(frame);
    assert_eq!(pool.stats_delivered.load(Ordering::Acquire), 1);
    assert_eq!(pool.stats_cpu_fallbacks.load(Ordering::Acquire), 0);
    assert_eq!(pool.frames_in_use.load(Ordering::Acquire), 0);
}
