use super::*;
use am_camera_core::media::{ffmpeg_lib::FfmpegApi, ffmpeg_sys::SwsContext};
use std::{ffi::c_int, hash::Hasher, sync::OnceLock};
static API: OnceLock<Arc<FfmpegApi>> = OnceLock::new();
static CONVERSIONS: AtomicU64 = AtomicU64::new(0);
unsafe extern "C" fn scale(
    ctx: *mut SwsContext,
    src: *const *const u8,
    stride: *const c_int,
    y: c_int,
    h: c_int,
    dst: *const *mut u8,
    out_stride: *const c_int,
) -> c_int {
    CONVERSIONS.fetch_add(1, Ordering::Relaxed);
    unsafe { (API.get().unwrap().sws_scale)(ctx, src, stride, y, h, dst, out_stride) }
}
fn fixture() -> (
    am_sdk_support::handles::Domain,
    Arc<SharedSource>,
    Arc<Hub>,
    Arc<AtomicU64>,
) {
    let (domain, state) = crate::frames::isolated_test_state();
    let source = crate::sources::isolated_test_source(state);
    let base = Instant::now();
    let nanos = Arc::new(AtomicU64::new(0));
    let tick = nanos.clone();
    let hub = Arc::new(Hub {
        source: Arc::downgrade(&source),
        subscribers: Mutex::default(),
        codec: AtomicU32::new(AM_CODEC_H264),
        changed: Condvar::new(),
        revision: AtomicU64::new(0),
        packet_sequence: AtomicU64::new(0),
        frame_sequence: AtomicU64::new(0),
        dimensions: Mutex::default(),
        interrupt: Mutex::default(),
        wait: Mutex::new(()),
        alive: AtomicBool::new(false),
        handle: Mutex::default(),
        clock: Some(Arc::new(move || {
            base + Duration::from_nanos(tick.load(Ordering::Relaxed))
        })),
    });
    (domain, source, hub, nanos)
}
fn sub(source: &SharedSource, hub: &Arc<Hub>, fps: u32, mode: u32) -> Arc<Subscription> {
    Subscription::new(
        source.state.clone(),
        hub,
        Kind::Frames {
            format: AM_PF_RGBA8 as u32,
            fps,
            presets: vec![],
        },
        mode,
        0,
        0,
    )
    .unwrap()
}
#[test]
fn sampling_peek_is_non_consuming_and_handles_reset_close_and_callback_absence() {
    let (domain, source, hub, nanos) = fixture();
    let subscription = sub(&source, &hub, 5, AM_SUB_PULL);
    let now = hub.now();
    assert!(subscription.needs_pixels(now));
    assert!(subscription.needs_pixels(now));
    assert!(subscription.due(now));
    assert!(!subscription.needs_pixels(now));
    nanos.store(200_000_000, Ordering::Relaxed);
    assert!(subscription.needs_pixels(hub.now()));
    subscription.reset();
    assert!(subscription.needs_pixels(hub.now()));
    am_sdk_support::handles::Managed::request_close(subscription.as_ref());
    assert!(!subscription.needs_pixels(hub.now()));
    let push = sub(&source, &hub, 0, AM_SUB_PUSH);
    assert!(!push.needs_pixels(hub.now()));
    drop((push, subscription, hub));
    let key = source.state.ctx_key;
    drop(source);
    assert_eq!(domain.close(key, 100).unwrap(), AM_LIFECYCLE_CLOSED as u32);
    domain.destroy(key).unwrap();
}
#[test]
#[ignore = "显式设置AM_RT_FFMPEG_DIR、AM_SDK_H264_SAMPLE，真实1080p原语+生产fanout，虚拟30FPS时钟"]
fn native_sampling_preserves_outputs_and_quantifies_conversion_savings() {
    let directory = std::env::var("AM_RT_FFMPEG_DIR").unwrap();
    let sample = std::fs::read(std::env::var("AM_SDK_H264_SAMPLE").unwrap()).unwrap();
    API.set(Arc::new(
        FfmpegApi::load_from(std::path::Path::new(&directory)).unwrap(),
    ))
    .ok()
    .unwrap();
    let mut api = FfmpegApi::load_from(std::path::Path::new(&directory)).unwrap();
    api.sws_scale = scale;
    let api = Arc::new(api);
    for fps in [
        vec![5],
        vec![10],
        vec![30],
        vec![5, 10],
        vec![5, 10, 30],
        vec![],
    ] {
        if std::env::var_os("AM_SDK_SAMPLE_MIXED_ONLY").is_some() && fps != [5, 10, 30] {
            continue;
        }
        let mut reference_hashes = None;
        let modes = if std::env::var_os("AM_SDK_SAMPLE_REVERSE").is_some() {
            [true, false]
        } else {
            [false, true]
        };
        for early in modes {
            let (domain, source, hub, nanos) = fixture();
            let subscribers: Vec<_> = fps
                .iter()
                .map(|fps| sub(&source, &hub, *fps, AM_SUB_PULL))
                .collect();
            let mut decoder: crate::decoder::budget::BudgetedDecoder =
                CpuDecoder::create(api.clone(), VideoCodec::H264, &[], 1920 * 1080)
                    .unwrap()
                    .into();
            let info = StreamInfo {
                codec: AM_CODEC_H264,
                width: 1920,
                height: 1080,
                fps: 30.,
                time_base_num: 1,
                time_base_den: 30,
                extradata: vec![],
            };
            let stamp = SessionStamp {
                revision: 0,
                generation: 1,
                credentials: source.credential_version(),
            };
            for n in -20..0 {
                assert!(decoder.send(&sample, n, n).unwrap());
                assert!(decoder.copy_rgba(1920 * 1080 * 4).unwrap().is_some());
            }
            let mut elapsed = 0;
            let mut frame_us = Vec::new();
            let count_before = CONVERSIONS.load(Ordering::Relaxed);
            let mut delivered = vec![0; fps.len()];
            let mut hashes = Vec::new();
            for n in 0..90 {
                nanos.store(n as u64 * 33_333_334, Ordering::Relaxed);
                let start = Instant::now();
                assert!(decoder.send(&sample, n, n).unwrap());
                let times = VecDeque::from([(n, 1_000 + n)]);
                if early {
                    hub.emit_decoded::<true>(
                        &source,
                        &mut decoder,
                        &info,
                        &subscribers,
                        &times,
                        stamp,
                    )
                    .unwrap();
                } else {
                    hub.emit_decoded::<false>(
                        &source,
                        &mut decoder,
                        &info,
                        &subscribers,
                        &times,
                        stamp,
                    )
                    .unwrap();
                }
                let duration = start.elapsed().as_micros();
                elapsed += duration;
                frame_us.push(duration);
                for (index, subscription) in subscribers.iter().enumerate() {
                    if let Ok(Item::Frame(frame)) = subscription.pull(0) {
                        assert_eq!(frame.meta.pts, n);
                        assert_eq!(frame.meta.received_at_ms, 1_000 + n);
                        assert_eq!(frame.meta.seq, n as u64 + 1);
                        // 全平面指纹在计时外计算，不以稀疏采样掩盖内容错误。
                        let mut hasher = std::collections::hash_map::DefaultHasher::new();
                        hasher.write(&frame.data);
                        let checksum = hasher.finish();
                        hashes.push((index, n, frame.meta.seq, checksum));
                        delivered[index] += 1;
                    }
                }
            }
            let converted = CONVERSIONS.load(Ordering::Relaxed) - count_before;
            assert_eq!(source.state.metrics.decoded.load(Ordering::Acquire), 90);
            assert_eq!(delivered, fps.iter().map(|f| 3 * f).collect::<Vec<_>>());
            assert_eq!(
                converted,
                if early {
                    fps.iter().max().map_or(0, |f| 3 * u64::from(*f))
                } else {
                    90
                }
            );
            if let Some(reference) = &reference_hashes {
                assert_eq!(*reference, hashes);
            } else {
                reference_hashes = Some(hashes);
            }
            println!("sampling-quant {{\"fps\":{fps:?},\"early\":{early},\"decoded\":90,\"converted\":{converted},\"delivered\":{delivered:?},\"elapsed_us\":{elapsed},\"frame_us\":{frame_us:?},\"warmup\":20,\"clock\":\"virtual 30FPS; not live throughput\"}}");
            drop((subscribers, decoder, hub));
            assert_eq!(source.state.usage_snapshot().unwrap(), (0, 0, 0));
            let key = source.state.ctx_key;
            drop(source);
            assert_eq!(domain.close(key, 100).unwrap(), AM_LIFECYCLE_CLOSED as u32);
            domain.destroy(key).unwrap();
        }
    }
}
