use super::*;
use am_sdk_support::handles::Managed;

#[test]
#[ignore = "显式FFmpeg与合成H264单AU，真实时间突发输入及实际采样选择间隔"]
fn real_native_pixel_worker_keeps_strict_sampling_intervals_and_frame_identity_under_jitter() {
    run(AM_CODEC_H264, "AM_SDK_H264_SAMPLE", "AM_SDK_H264_ALTERNATE");
}

#[test]
#[ignore = "显式HEVC无延迟合成AU，实际worker采样与尺寸变化"]
fn real_hevc_worker_preserves_cadence_and_spatial_configuration() {
    run(AM_CODEC_H265, "AM_SDK_HEVC_SAMPLE", "AM_SDK_HEVC_ALTERNATE");
}

#[test]
#[ignore = "显式JPEG合成画面，实际worker采样与尺寸变化"]
fn real_mjpeg_worker_preserves_cadence_and_spatial_configuration() {
    run(
        AM_CODEC_MJPEG,
        "AM_SDK_JPEG_SAMPLE",
        "AM_SDK_JPEG_ALTERNATE",
    );
}

fn run(codec: u32, first: &str, alternate: &str) {
    let sample = std::fs::read(std::env::var(first).unwrap()).unwrap();
    let alternate = std::env::var(alternate)
        .ok()
        .map(|p| std::fs::read(p).unwrap());
    let (domain, source, mut hub, _) = super::super::tests::fixture();
    Arc::get_mut(&mut hub).unwrap().clock = None;
    source.decode.lock().unwrap().policy = AM_DHW_SOFTWARE_ONLY as u32;
    hub.codec.store(codec, Ordering::Release);
    let mut config = configuration(&source, codec, 1, (256, 144));
    let rates = [5, 10, 30, 30];
    let subscribers: Snapshot = rates
        .iter()
        .map(|fps| super::super::tests::sub(&source, &hub, *fps, AM_SUB_PULL))
        .collect();
    {
        let mut registry = hub.subscribers.lock().unwrap();
        for sub in subscribers.iter() {
            registry.push(Arc::downgrade(sub)).unwrap();
        }
    }
    let stamp = SessionStamp {
        revision: 0,
        generation: 1,
        credentials: source.credential_version(),
    };
    let mut counts = [0; 4];
    let mut dimensions_seen = [0u8; 4];
    let mut last_pts = [-1; 4];
    std::thread::scope(|scope| {
        let mut execution = Execution::new(scope, &hub, &source, stamp);
        let start = Instant::now();
        for n in 0..90 {
            if n == 45 && alternate.is_some() {
                for (sub, rate) in subscribers.iter().zip(rates) {
                    verify_intervals(sub, rate);
                }
                // 复现Gate先变配置再递交新随机访问点；旧在途结果不得贴新代次。
                let _guard = hub.configuration.lock().unwrap();
                source.config_generation.store(2, Ordering::Release);
                config = configuration(&source, codec, 2, (128, 96));
            }
            let current = if n >= 45 {
                alternate.as_ref().unwrap_or(&sample)
            } else {
                &sample
            };
            let payload = Arc::new(queue::Payload {
                abi: am_encoded_packet_t {
                    codec,
                    flags: AM_PKT_KEYFRAME | AM_PKT_AU_COMPLETE | AM_PKT_PARAMSETS,
                    pts: n,
                    dts: n,
                    seq: n as u64 + 1,
                    source_generation: 1,
                    config_generation: config.generation,
                    size: current.len() as u64,
                    ..Default::default()
                },
                _charge: source.state.charge(current.len() as u64, false).unwrap(),
                bytes: current.clone(),
            });
            execution
                .submit(Job {
                    packet: payload,
                    config: config.clone(),
                    received: am_sdk_support::clock::monotonic_ms(),
                    epoch: 0,
                })
                .unwrap();
            // 25/42ms交替，输入仍约30FPS；不借用虚拟时钟放宽deadline。
            let jitter = if n % 2 == 0 { 8_000_000 } else { 0 };
            let until = start + Duration::from_nanos((n as u64 + 1) * 33_333_334 - jitter);
            while Instant::now() < until {
                for (i, sub) in subscribers.iter().enumerate() {
                    while let Ok(Item::Frame(frame)) = sub.pull(0) {
                        assert!(frame.meta.pts > last_pts[i]);
                        assert_eq!(frame.meta.source_generation, 1);
                        let changed = alternate.is_some() && frame.meta.pts >= 45;
                        assert_eq!(
                            (frame.view.width, frame.view.height),
                            if changed { (128, 96) } else { (256, 144) }
                        );
                        assert_eq!(frame.meta.config_generation, if changed { 2 } else { 1 });
                        dimensions_seen[i] |= if changed { 2 } else { 1 };
                        last_pts[i] = frame.meta.pts;
                        counts[i] += 1;
                    }
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    });
    for (i, sub) in subscribers.iter().enumerate() {
        let times = sub.selection_times();
        let minimum = Duration::from_nanos(1_000_000_000u64.div_ceil(rates[i] as u64));
        assert!(counts[i] > 0);
        assert!(times
            .windows(2)
            .all(|pair| pair[1].duration_since(pair[0]) >= minimum));
        println!("native-cadence {{\"fps\":{},\"frames_pulled\":{},\"selections\":{},\"min_gap_ns\":{},\"observed_min_ns\":{},\"scope\":\"real worker FFmpeg/clock; exact selection times; not DLL performance\"}}", rates[i], counts[i], times.len(), minimum.as_nanos(), times.windows(2).map(|p| p[1].duration_since(p[0]).as_nanos()).min().unwrap());
        sub.request_close();
    }
    println!(
        "codec-cadence codec={codec} dimensions_changed={} frames={counts:?}",
        alternate.is_some()
    );
    assert!(counts.iter().all(|count| *count >= 6));
    if alternate.is_some() {
        assert_eq!(source.config_generation.load(Ordering::Acquire), 2);
        assert_eq!(dimensions_seen, [3; 4]);
    }
    drop((subscribers, config, hub));
    assert_eq!(source.state.usage_snapshot().unwrap(), (0, 0, 0));
    let key = source.state.ctx_key;
    drop(source);
    domain.close(key, 100).unwrap();
    domain.destroy(key).unwrap();
}

fn verify_intervals(sub: &Subscription, fps: u32) {
    let times = sub.selection_times();
    let minimum = Duration::from_nanos(1_000_000_000u64.div_ceil(fps as u64));
    assert!(times.len() >= 2);
    assert!(times
        .windows(2)
        .all(|p| p[1].duration_since(p[0]) >= minimum));
}

fn configuration(
    source: &SharedSource,
    codec: u32,
    generation: u32,
    dimensions: (u32, u32),
) -> Arc<Configuration> {
    let mut config = super::tests::config(source);
    let c = Arc::get_mut(&mut config).unwrap();
    c.generation = generation;
    let info = &mut Arc::get_mut(&mut c.stream).unwrap().info;
    info.codec = codec;
    info.width = dimensions.0;
    info.height = dimensions.1;
    config
}
