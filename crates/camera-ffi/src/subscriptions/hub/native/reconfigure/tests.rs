use super::*;
use am_camera_core::media::{ffmpeg_lib::FfmpegApi, ffmpeg_sys::*};
use am_sdk_support::{context::Context, handles::Managed};
use std::{ffi::c_void, path::PathBuf, sync::OnceLock};

static API: OnceLock<Arc<FfmpegApi>> = OnceLock::new();
static CREATES: AtomicU64 = AtomicU64::new(0);
static FAIL_ALLOC: AtomicBool = AtomicBool::new(false);

unsafe extern "C" fn create_device(
    out: *mut *mut AVBufferRef,
    kind: i32,
    name: *const std::ffi::c_char,
    options: *mut c_void,
    flags: i32,
) -> i32 {
    CREATES.fetch_add(1, Ordering::AcqRel);
    unsafe { (API.get().unwrap().av_hwdevice_ctx_create)(out, kind, name, options, flags) }
}
unsafe extern "C" fn alloc_context(codec: *const c_void) -> *mut AVCodecContext {
    if FAIL_ALLOC.load(Ordering::Acquire) {
        std::ptr::null_mut()
    } else {
        unsafe { (API.get().unwrap().avcodec_alloc_context3)(codec) }
    }
}

#[test]
#[ignore = "本机FFmpeg和合成H264/H265参数变化，必须真实硬件；不连接现场设备"]
fn rtsp_parameter_rebuild_reuses_device_and_permit_and_preserves_owned_pixels() {
    let runtime = PathBuf::from(std::env::var("AM_RT_FFMPEG_DIR").unwrap());
    let inputs = PathBuf::from(std::env::var("AM_SDK_RECONFIG_INPUTS").unwrap());
    API.set(Arc::new(FfmpegApi::load_from(&runtime).unwrap()))
        .ok();
    for (codec, name, next) in [
        (VideoCodec::H264, "one.h264", "alternate.h264"),
        (
            VideoCodec::H265,
            "first-lowdelay.hevc",
            "alternate-lowdelay.hevc",
        ),
    ] {
        for policy in [HardwarePolicy::SoftwareOnly, HardwarePolicy::PreferHardware] {
            let before = CREATES.load(Ordering::Acquire);
            let mut observed = FfmpegApi::load_from(&runtime).unwrap();
            observed.av_hwdevice_ctx_create = create_device;
            observed.avcodec_alloc_context3 = alloc_context;
            let mut selection = CpuDecoder::create_with_policy(
                Arc::new(observed),
                codec,
                &[],
                8192 * 8192,
                policy,
                Duration::from_secs(5),
            )
            .unwrap();
            let hardware = policy == HardwarePolicy::PreferHardware;
            assert_eq!(
                selection.report.selected.is_some(),
                hardware,
                "真实硬件不可用不能冒充通过"
            );
            let created = CREATES.load(Ordering::Acquire);
            let (domain, source, hub, _) = super::super::super::tests::fixture();
            let context = source.state.ctx.data::<Context>().unwrap();
            context.limits.lock().unwrap().max_decoders = 1;
            let first = std::fs::read(inputs.join(name)).unwrap();
            let second = std::fs::read(inputs.join(next)).unwrap();
            let info = StreamInfo {
                codec: if codec == VideoCodec::H264 {
                    AM_CODEC_H264
                } else {
                    AM_CODEC_H265
                },
                width: 256,
                height: 144,
                fps: 30.,
                time_base_num: 1,
                time_base_den: 30,
                extradata: vec![],
            };
            let mut gate = Gate::new(&configuration(codec, &info)).unwrap();
            assert!(gate.observe(codec, &first).unwrap());
            gate.dirty = false;
            gate.started = true;
            assert!(selection.decoder.send(&first, 1, 1).unwrap());
            let permit = Arc::new(source.state.decoder().unwrap());
            let cpu_output =
                (!hardware).then(|| source.state.charge(256 * 144 * 4, false).unwrap());
            {
                let mut state = lock(&source.decode).unwrap();
                state.selection = Some(Arc::new(selection.report));
                state.report_charge = Some(source.state.control(8192, false).unwrap());
                state.policy = if hardware {
                    AM_DHW_PREFER_HW
                } else {
                    AM_DHW_SOFTWARE_ONLY
                } as u32;
            }
            let mut decoder = Some(NativeDecoder {
                raw: selection.decoder.into(),
                _permit: permit,
                _cpu_output: cpu_output,
                hardware,
                source: Arc::downgrade(&source),
            });
            let pixel = super::super::super::tests::sub(&source, &hub, 0, AM_SUB_PULL);
            let stamp = SessionStamp {
                revision: 0,
                generation: 1,
                credentials: source.credential_version(),
            };
            let emit = |decoder: &mut NativeDecoder, pts| {
                hub.emit_decoded::<true>(
                    &source,
                    &mut decoder.raw,
                    &info,
                    std::slice::from_ref(&pixel),
                    &VecDeque::from([(pts, pts)]),
                    stamp,
                )
                .unwrap();
                let Item::Frame(frame) = pixel.pull(0).unwrap() else {
                    panic!("缺少像素帧")
                };
                frame
            };
            let retained = emit(decoder.as_mut().unwrap(), 1);
            let retained_bytes = retained.data.clone();
            assert_eq!(
                lock(&source.decode).unwrap().actual,
                Some(if hardware {
                    "ffmpeg-d3d11va"
                } else {
                    "ffmpeg-cpu"
                })
            );
            assert!(gate.observe(codec, &second).unwrap() && gate.dirty);
            let extra = gate.extra();
            assert!(!hub
                .reconfigure_pixels(&source, &mut decoder, codec, &info, &extra, false)
                .unwrap());
            assert_eq!(lock(&source.decode).unwrap().generation, 1);
            // 暂存预算拒绝不能损坏旧decoder；旧PTS仍可读取。
            if !hardware {
                assert!(decoder.as_mut().unwrap().raw.send(&first, 2, 2).unwrap());
                let limit = context.limits.lock().unwrap().frame_budget_bytes;
                context.limits.lock().unwrap().frame_budget_bytes =
                    source.state.usage_snapshot().unwrap().0;
                assert!(!hub
                    .reconfigure_pixels(&source, &mut decoder, codec, &info, &extra, true)
                    .unwrap());
                assert!(decoder.is_some());
                assert_eq!(
                    decoder.as_mut().unwrap().raw.peek().unwrap().unwrap().pts,
                    2
                );
                context.limits.lock().unwrap().frame_budget_bytes = limit;
            }
            let started = Instant::now();
            assert!(hub
                .reconfigure_pixels(&source, &mut decoder, codec, &info, &extra, true)
                .unwrap());
            let rebuild_us = started.elapsed().as_micros();
            assert_eq!(CREATES.load(Ordering::Acquire), created);
            assert!(matches!(
                source.state.decoder(),
                Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
            ));
            assert!(decoder.as_mut().unwrap().raw.hardware_backend().is_none());
            let state = lock(&source.decode).unwrap();
            assert_eq!(state.actual, None);
            assert_eq!(state.generation, 2);
            drop(state);
            assert!(decoder.as_mut().unwrap().raw.send(&second, 3, 3).unwrap());
            let output = emit(decoder.as_mut().unwrap(), 3);
            assert_eq!((output.meta.pts, output.meta.source_generation), (3, 1));
            assert_eq!((output.view.width, output.view.height), (128, 96));
            assert_eq!(retained.data, retained_bytes);
            assert_eq!((retained.view.width, retained.view.height), (256, 144));
            assert_eq!(CREATES.load(Ordering::Acquire), created);
            // 驱动/原生构造失败沿像素恢复，未更改压缩源代次。
            let config_generation = source.config_generation.load(Ordering::Acquire);
            FAIL_ALLOC.store(true, Ordering::Release);
            assert!(!hub
                .reconfigure_pixels(&source, &mut decoder, codec, &info, &extra, true)
                .unwrap());
            FAIL_ALLOC.store(false, Ordering::Release);
            assert!(decoder.is_none());
            assert_eq!(source.generation.load(Ordering::Acquire), 1);
            assert_eq!(
                source.config_generation.load(Ordering::Acquire),
                config_generation
            );
            let state = lock(&source.decode).unwrap();
            assert_eq!(state.failure, Some(DecodeError::AllocationFailed));
            assert_eq!(state.recovered, hardware);
            drop(state);
            println!("rtsp-reconfigure {{\"codec\":\"{codec:?}\",\"hardware\":{hardware},\"initial_creates\":{},\"rebuild_creates\":0,\"rebuild_us\":{rebuild_us},\"held_frame_unchanged\":true}}", created - before);
            pixel.request_close();
            drop((retained, output, pixel, hub));
            assert_eq!(source.state.usage_snapshot().unwrap(), (0, 0, 0));
            let permit = source.state.decoder().unwrap();
            drop(permit);
            let key = source.state.ctx_key;
            drop(source);
            domain.close(key, 100).unwrap();
            domain.destroy(key).unwrap();
        }
    }
}
