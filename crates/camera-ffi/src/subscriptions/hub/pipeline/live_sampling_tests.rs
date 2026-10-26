use super::tests::config;
use super::*;
use am_sdk_support::handles::Managed;

#[test]
#[ignore = "显式FFmpeg与合成H264单AU，真实时间突发输入及实际采样选择间隔"]
fn real_native_pixel_worker_keeps_strict_sampling_intervals_and_frame_identity_under_jitter() {
    let sample = std::fs::read(std::env::var("AM_SDK_H264_SAMPLE").unwrap()).unwrap();
    let (domain, source, mut hub, _) = super::super::tests::fixture();
    Arc::get_mut(&mut hub).unwrap().clock = None;
    source.decode.lock().unwrap().policy = AM_DHW_SOFTWARE_ONLY as u32;
    let config = config(&source);
    let rates = [5, 10, 30, 30];
    let subscribers: Vec<_> = rates
        .iter()
        .map(|fps| super::super::tests::sub(&source, &hub, *fps, AM_SUB_PULL))
        .collect();
    hub.subscribers
        .lock()
        .unwrap()
        .extend(subscribers.iter().map(Arc::downgrade));
    let stamp = SessionStamp {
        revision: 0,
        generation: 1,
        credentials: source.credential_version(),
    };
    let mut counts = [0; 4];
    let mut last_pts = [-1; 4];
    std::thread::scope(|scope| {
        let mut execution = Execution::new(scope, &hub, &source, stamp);
        let start = Instant::now();
        for n in 0..90 {
            let payload = Arc::new(queue::Payload {
                abi: am_encoded_packet_t {
                    codec: AM_CODEC_H264,
                    flags: AM_PKT_KEYFRAME | AM_PKT_AU_COMPLETE | AM_PKT_PARAMSETS,
                    pts: n,
                    dts: n,
                    seq: n as u64 + 1,
                    source_generation: 1,
                    config_generation: 1,
                    size: sample.len() as u64,
                    ..Default::default()
                },
                _charge: source.state.charge(sample.len() as u64, false).unwrap(),
                bytes: sample.clone(),
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
                        assert_eq!((frame.view.width, frame.view.height), (256, 144));
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
    drop((subscribers, config, hub));
    assert_eq!(source.state.usage_snapshot().unwrap(), (0, 0, 0));
    let key = source.state.ctx_key;
    drop(source);
    domain.close(key, 100).unwrap();
    domain.destroy(key).unwrap();
}
