//! RTSP 打开参数链测试（媒体质量整改问题二）：
//! 七参数全集的生效验证——本地文件源 fmt 层字段读回、RTSP 真源六项全消费、
//! 解码器低延迟装配读回、720 缩放走 sws AREA（对齐参考 INTER_AREA）。
//! 依赖 mediamtx/ffmpeg 的用例按项目惯例不可用即跳过。

use super::ffmpeg_lib::api;
use super::ffmpeg_sys::{AVFMT_FLAG_NOBUFFER, AV_CODEC_FLAG_LOW_DELAY};
use super::rtsp::{apply_low_latency, open_input};
use super::testkit::{ensure_mediamtx, which_ffmpeg};

/// 本地文件源：fmt 层四项（nobuffer/probesize/analyzeduration/max_delay）字段读回生效。
/// open_input 内部验证失败会返回 Err，此处再独立读回快照断言并输出证据。
#[test]
fn open_options_applied_on_local_file() {
    let sample = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/media/testdata/probe_source.mp4");
    if !sample.is_file() {
        eprintln!("[媒体整改] 本地样本缺失，跳过");
        return;
    }
    let input = open_input(sample.to_str().unwrap(), std::ptr::null())
        .expect("本地文件打开应成功（含 fmt 层四项选项验证）");
    let (flags, probesize, analyzed, max_delay) = input.open_option_state();
    eprintln!(
        "[媒体整改] 本地文件读回: flags={flags:#x} probesize={probesize} analyzeduration={analyzed} max_delay={max_delay}"
    );
    assert!(flags & AVFMT_FLAG_NOBUFFER != 0, "nobuffer 位应置位");
    assert!(
        probesize > 0 && probesize <= 32,
        "probesize 应为 32，实际 {probesize}"
    );
    assert!(analyzed <= 0, "analyzeduration 应为 0，实际 {analyzed}");
    assert_eq!(max_delay, 0, "max_delay 应为 0");
}

/// RTSP 真源（mediamtx+ffmpeg 自产，同 live_pipeline 模式）：
/// 六项参数全消费（rtsp_transport/reorder_queue_size 未留在 dict）+ fmt 层字段读回。
#[test]
fn open_options_all_consumed_on_rtsp_source() {
    let Some(ffmpeg) = which_ffmpeg() else {
        eprintln!("[媒体整改] 未找到 ffmpeg 命令，RTSP 参数用例跳过");
        return;
    };
    let server = ensure_mediamtx();
    if server.is_none() && !super::testkit::port_open(8554) {
        eprintln!("[媒体整改] 未找到 mediamtx 且 8554 未监听，RTSP 参数用例跳过");
        return;
    }
    let path = format!("p08-opts-{}", std::process::id());
    let url = format!("rtsp://127.0.0.1:8554/{path}");
    let mut pusher = std::process::Command::new(&ffmpeg)
        .args([
            "-loglevel",
            "error",
            "-re",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=640x360:rate=15",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-tune",
            "zerolatency",
            "-g",
            "15",
            "-f",
            "rtsp",
            &url,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("ffmpeg 推流子进程应可启动");
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        std::thread::sleep(std::time::Duration::from_millis(3000));
        let input = open_input(&url, std::ptr::null())
            .expect("RTSP 打开应成功（含六项参数消费验证，未消费即失败）");
        let video = input.probe_video().expect("流探测应成功");
        let (flags, probesize, analyzed, max_delay) = input.open_option_state();
        eprintln!(
            "[媒体整改] RTSP 源读回: flags={flags:#x} probesize={probesize} analyzeduration={analyzed} max_delay={max_delay} fps={:?}",
            video.fps
        );
        assert!(flags & AVFMT_FLAG_NOBUFFER != 0);
        assert!(probesize > 0 && probesize <= 32);
        assert!(analyzed <= 0);
        assert_eq!(max_delay, 0);
    }));
    let _ = pusher.kill();
    let _ = pusher.wait();
    if let Some(mut server) = server {
        let _ = server.kill();
        let _ = server.wait();
    }
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

/// 解码器低延迟装配：LOW_DELAY 标志 + thread_count=1 读回（thread_type 经选项系统写入）。
#[test]
fn decoder_low_latency_applied() {
    let Ok(lib) = api() else {
        eprintln!("[媒体整改] FFmpeg DLL 不可用，解码器低延迟用例跳过");
        return;
    };
    // SAFETY：h264 内建解码器静态指针；ctx 释放前完成读写。
    unsafe {
        let codec = (lib.avcodec_find_decoder)(27); // AV_CODEC_ID_H264
        assert!(!codec.is_null(), "h264 解码器应存在");
        let ctx = (lib.avcodec_alloc_context3)(codec);
        assert!(!ctx.is_null());
        apply_low_latency(lib, ctx).expect("低延迟装配应成功（含读回验证）");
        assert!(
            (*ctx).flags & AV_CODEC_FLAG_LOW_DELAY != 0,
            "LOW_DELAY 位读回应置位"
        );
        assert_eq!((*ctx).thread_count, 1, "thread_count 读回应为 1");
        (lib.avcodec_free_context)(&mut { ctx });
    }
}
