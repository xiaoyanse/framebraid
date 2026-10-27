//! 编码探测单测（批 1.6）：分支分类（认证/超时/不可达）、H.264 通过（本地真源）、
//! 非 H.264 指引（ffmpeg 自产 MJPEG 源）、缓存有效期。
use super::codec_probe::*;
use crate::media::testkit::which_ffmpeg;
use std::time::Duration;

/// 失败分类纯函数：认证→超时→不可达→其他，先命中先归类。
#[test]
fn classify_open_failure_branches() {
    assert!(matches!(
        classify_open_failure("打开输入失败: Server returned 401 Unauthorized"),
        ProbeOutcome::AuthFailed { .. }
    ));
    assert!(matches!(
        classify_open_failure("method DESCRIBE failed: 403 Forbidden"),
        ProbeOutcome::AuthFailed { .. }
    ));
    assert!(matches!(
        classify_open_failure("打开输入失败: Immediate exit requested"),
        ProbeOutcome::Timeout { .. }
    ));
    assert!(matches!(
        classify_open_failure("TCP 预连接失败: os error 10060"),
        ProbeOutcome::Timeout { .. }
    ));
    assert!(matches!(
        classify_open_failure("TCP 预连接失败: Connection refused"),
        ProbeOutcome::Unreachable { .. }
    ));
    assert!(matches!(
        classify_open_failure("主机解析失败: bad host"),
        ProbeOutcome::Unreachable { .. }
    ));
    assert!(matches!(
        classify_open_failure("打开输入失败: Invalid data found"),
        ProbeOutcome::Other { .. }
    ));
    assert_eq!(codec_display(27), "h264");
    assert_eq!(codec_display(173), "hevc");
    assert_eq!(codec_display(7), "mjpeg");
    assert_eq!(codec_display(99), "99");
}

/// 本地 H.264 真源：头直读命中（ffprobe 语义不拉全流），返回流参数。
#[test]
#[ignore = "需要显式FFmpeg运行库（AM_RT_FFMPEG_DIR）；CI无运行库环境跳过"]
fn probe_h264_local_file_passes() {
    let sample = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/media/testdata/probe_source.mp4");
    if !sample.is_file() {
        eprintln!("[探测] 本地样本缺失，跳过");
        return;
    }
    let outcome = probe_once(sample.to_str().unwrap_or_default());
    match outcome {
        ProbeOutcome::H264 { width, height, .. } => {
            assert_eq!((width, height), (640, 360));
        }
        other => panic!("H.264 样本应通过探测: {other:?}"),
    }
}

/// 非 H.264（MJPEG）分支：返回 NotSupported+编码名（指引文案由端点组装）。
#[test]
#[ignore = "需要显式FFmpeg运行库（AM_RT_FFMPEG_DIR）；CI无运行库环境跳过"]
fn probe_non_h264_reports_codec_name() {
    let Some(ffmpeg) = which_ffmpeg() else {
        eprintln!("[探测] 未找到 ffmpeg 命令，MJPEG 分支用例跳过");
        return;
    };
    let dir = tempfile::tempdir().expect("临时目录");
    let target = dir.path().join("mjpeg.avi");
    let status = std::process::Command::new(&ffmpeg)
        .args([
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=160x120:rate=5",
            "-t",
            "1",
            "-c:v",
            "mjpeg",
        ])
        .arg(&target)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("ffmpeg 子进程应可启动");
    if !status.success() || !target.is_file() {
        eprintln!("[探测] MJPEG 样本生成失败，跳过");
        return;
    }
    let outcome = probe_once(target.to_str().unwrap_or_default());
    assert_eq!(
        outcome,
        ProbeOutcome::NotSupported {
            codec: "mjpeg".into()
        },
        "非 H.264 应返回编码名供指引"
    );
}

/// 不可达源分类：本机拒绝端口即拒绝连接（不产生长等待）。
#[test]
#[ignore = "需要显式FFmpeg运行库（AM_RT_FFMPEG_DIR）；CI无运行库环境跳过"]
fn probe_refused_port_classified_unreachable() {
    let outcome = probe_once("rtsp://127.0.0.1:1/none");
    assert!(
        matches!(
            outcome,
            ProbeOutcome::Unreachable { .. } | ProbeOutcome::Timeout { .. }
        ),
        "拒绝/超时均可分诊，不应是其他类型: {outcome:?}"
    );
}

/// 缓存有效期：新鲜命中、过期重探；成功与失败 TTL 独立。
#[test]
fn probe_cache_ttl_and_reprobe() {
    let cache = CodecProbeCache::with_ttl(Duration::from_millis(300), Duration::from_millis(80));
    cache.put(
        "ok",
        ProbeOutcome::H264 {
            width: 1,
            height: 1,
            fps: None,
        },
    );
    cache.put(
        "bad",
        ProbeOutcome::NotSupported {
            codec: "hevc".into(),
        },
    );
    assert!(matches!(
        cache.get("ok").as_deref(),
        Some(ProbeOutcome::H264 { .. })
    ));
    assert!(matches!(
        cache.get("bad").as_deref(),
        Some(ProbeOutcome::NotSupported { .. })
    ));
    std::thread::sleep(Duration::from_millis(120));
    assert!(cache.get("bad").is_none(), "失败 TTL 80ms 已过应失效重探");
    assert!(cache.get("ok").is_some(), "成功 TTL 300ms 未过应命中");
    // probe 入口：未命中执行 url_provider 并回填
    let cache = CodecProbeCache::with_ttl(Duration::from_secs(60), Duration::from_secs(60));
    let outcome = cache.probe("live", || "E:/definitely/not/a/stream".to_string());
    assert!(matches!(
        *outcome,
        ProbeOutcome::Unreachable { .. } | ProbeOutcome::Other { .. }
    ));
    assert!(
        matches!(
            cache.get("live").as_deref(),
            Some(ProbeOutcome::Unreachable { .. }) | Some(ProbeOutcome::Other { .. })
        ),
        "探测结果应回填缓存"
    );
}
