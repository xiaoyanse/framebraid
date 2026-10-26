use super::*;
use am_camera_core::media::{ffmpeg_lib::FfmpegApi, ffmpeg_sys::AVFrame};
use std::sync::{atomic::AtomicBool, OnceLock};
static API: OnceLock<Arc<FfmpegApi>> = OnceLock::new();
static FAIL: AtomicBool = AtomicBool::new(false);
unsafe extern "C" fn transfer(out: *mut AVFrame, src: *const AVFrame, flags: i32) -> i32 {
    if FAIL.swap(false, Ordering::AcqRel) {
        return -5;
    }
    unsafe { (API.get().unwrap().av_hwframe_transfer_data)(out, src, flags) }
}
unsafe fn submit(dec: *mut amc_decoder_t, bytes: &[u8], pts: i64) {
    let packet = am_decoder_input_t {
        data: bytes.as_ptr(),
        size: bytes.len() as u64,
        pts,
        dts: pts,
        flags: AM_PKT_AU_COMPLETE,
        ..Default::default()
    };
    assert_eq!(unsafe { amc_decoder_submit(dec, &packet) }, 0);
}
unsafe fn frame(dec: *mut amc_decoder_t) -> (*mut amc_frame_t, am_frame_meta_t, Vec<u8>) {
    let mut frame = std::ptr::null_mut();
    assert_eq!(unsafe { amc_decoder_receive(dec, 0, &mut frame) }, 0);
    let mut meta = am_frame_meta_t::default();
    assert_eq!(
        unsafe { crate::frames::amc_frame_get_meta(frame, &mut meta) },
        0
    );
    let mut view = am_frame_view_t::default();
    assert_eq!(
        unsafe { crate::frames::amc_frame_get_view(frame, &mut view) },
        0
    );
    let bytes =
        unsafe { std::slice::from_raw_parts(view.planes[0], view.plane_size_bytes[0] as usize) }
            .to_vec();
    (frame, meta, bytes)
}
#[test]
#[ignore = "显式设置AM_RT_FFMPEG_DIR、AM_SDK_H264_SAMPLE、AM_SDK_H264_P_SAMPLE，真实Windows硬解故障注入"]
fn native_auto_fallback_reserves_budget_replays_only_unpublished_startup_and_keeps_old_frames() {
    let directory = std::env::var("AM_RT_FFMPEG_DIR").unwrap();
    let sample = std::fs::read(std::env::var("AM_SDK_H264_SAMPLE").unwrap()).unwrap();
    let non_random = std::fs::read(std::env::var("AM_SDK_H264_P_SAMPLE").unwrap()).unwrap();
    API.set(Arc::new(
        FfmpegApi::load_from(std::path::Path::new(&directory)).unwrap(),
    ))
    .ok()
    .unwrap();
    let path = std::ffi::CString::new(directory.clone()).unwrap();
    let dirs = [path.as_ptr()];
    let params = amc_init_params_t {
        abi_version: AM_ABI_VERSION,
        runtime_dirs_utf8: dirs.as_ptr(),
        runtime_dir_count: 1,
        ..Default::default()
    };
    let mut ctx = std::ptr::null_mut();
    assert_eq!(unsafe { crate::amc_init(&params, &mut ctx) }, 0);
    let state = camera_state(ctx as usize).unwrap();
    // 额度转交不得重复计费；外部限额拒绝时原预留保持不变。
    let mut reserved = state.charge(4096, false).unwrap();
    let before = state.usage_snapshot().unwrap();
    assert!(matches!(
        reserved.take_external(4097),
        Err(AM_ERR_SDK_INVALID_ARGUMENT)
    ));
    assert_eq!(state.usage_snapshot().unwrap(), before);
    let external = reserved.take_external(1024).unwrap();
    assert_eq!(state.usage_snapshot().unwrap(), (4096, 1024, 1));
    drop(reserved);
    assert_eq!(state.usage_snapshot().unwrap(), (1024, 1024, 1));
    drop(external);
    assert_eq!(state.usage_snapshot().unwrap(), (0, 0, 0));
    {
        let mut reservation = state.charge(4096, false).unwrap();
        let context = state.ctx.data::<Context>().unwrap();
        let old = *context.limits.lock().unwrap();
        context.limits.lock().unwrap().external_hold_max_bytes = 512;
        assert!(matches!(
            reservation.take_external(1024),
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
        ));
        assert_eq!(reservation.bytes(), 4096);
        assert_eq!(state.usage_snapshot().unwrap(), (4096, 0, 0));
        *context.limits.lock().unwrap() = old;
    }
    for format in [AM_PF_RGBA8, AM_PF_RGB8, AM_PF_BGR8, AM_PF_NV12] {
        for fail_first in [true, false] {
            let cfg = am_decoder_config_t {
                codec: AM_CODEC_H264,
                output_pixel_format: format as u32,
                hw_policy: AM_DHW_PREFER_HW as u32,
                time_base_num: 1,
                time_base_den: 1000,
                ..Default::default()
            };
            let mut dec = std::ptr::null_mut();
            assert_eq!(unsafe { amc_decoder_create(ctx, &cfg, &mut dec) }, 0);
            let entry = domain().get(dec as usize).unwrap();
            let decoder = entry.data::<Decoder>().unwrap();
            assert!(decoder
                .codec_info
                .lock()
                .unwrap()
                .backend_name_utf8
                .is_null());
            {
                let mut api = FfmpegApi::load_from(std::path::Path::new(&directory)).unwrap();
                api.av_hwframe_transfer_data = transfer;
                let selected = CpuDecoder::create_with_policy(
                    Arc::new(api),
                    VideoCodec::H264,
                    &[],
                    1920 * 1080,
                    HardwarePolicy::PreferHardware,
                    Duration::from_secs(5),
                )
                .unwrap();
                assert!(selected.report.selected.is_some());
                let mut lock = decoder.inner.lock().unwrap();
                let inner = lock.as_mut().unwrap();
                inner.cpu = selected.decoder.into();
                inner.selection = Arc::new(selected.report);
            }
            if fail_first {
                let required = crate::frames::scale::bytes(1920, 1080, format as u32).unwrap()
                    as u64
                    + if format == AM_PF_NV12 {
                        1920 * 1080 * 4
                    } else {
                        0
                    };
                let blocker = state
                    .charge(
                        state.limits().unwrap().frame_budget_bytes - required + 1,
                        false,
                    )
                    .unwrap();
                {
                    let mut lock = decoder.inner.lock().unwrap();
                    let inner = lock.as_mut().unwrap();
                    assert_eq!(
                        inner.recover(decoder, Some((1920, 1080)), DecodeError::Runtime(-5)),
                        Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
                    );
                    assert!(inner.hardware_active());
                    assert_eq!(hardware_report(dec as usize).unwrap()["state"], "failed");
                    assert_eq!(decoder.generation.load(Ordering::Acquire), 1);
                }
                drop(blocker);
            }
            let held = if fail_first {
                None
            } else {
                unsafe { submit(dec, &sample, 100) };
                let output = unsafe { frame(dec) };
                assert_eq!(output.1.source_generation, 1);
                assert_eq!(decoder.codec_info.lock().unwrap().hw_accelerated, 1);
                Some(output)
            };
            FAIL.store(true, Ordering::Release);
            unsafe { submit(dec, &sample, 200) };
            if fail_first {
                // 已提交drain也必须先完成启动包的单次CPU重试，而非提前EOF。
                assert_eq!(unsafe { amc_decoder_drain(dec) }, 0);
            }
            let recovered = if fail_first {
                unsafe { frame(dec) }
            } else {
                let mut output = std::ptr::null_mut();
                assert_eq!(
                    unsafe { amc_decoder_receive(dec, 0, &mut output) },
                    AM_ERR_SDK_TRY_AGAIN
                );
                assert!(output.is_null());
                let failure =
                    am_sdk_support::diagnostics::snapshot(domain(), dec as usize).unwrap();
                assert_eq!(failure.0, AM_ERR_SDK_TRY_AGAIN);
                assert_eq!(
                    failure.1.unwrap().source,
                    c"camera.decoder.hardwareFallback"
                );
                unsafe { submit(dec, &non_random, 250) };
                assert_eq!(
                    unsafe { amc_decoder_receive(dec, 0, &mut output) },
                    AM_ERR_SDK_TRY_AGAIN
                );
                assert!(output.is_null());
                assert_eq!(
                    decoder
                        .inner
                        .lock()
                        .unwrap()
                        .as_ref()
                        .unwrap()
                        .buffers
                        .queue
                        .len(),
                    0
                );
                // 已发布后不能历史重放，即使刚失败的是IDR；新RAP才能恢复。
                unsafe { submit(dec, &sample, 300) };
                unsafe { frame(dec) }
            };
            assert_eq!(recovered.1.source_generation, 2);
            assert_eq!(recovered.1.pts, if fail_first { 200 } else { 300 });
            assert!(!FAIL.load(Ordering::Acquire));
            assert_eq!(decoder.codec_info.lock().unwrap().hw_accelerated, 0);
            let report = hardware_report(dec as usize).unwrap();
            assert_eq!(report["state"], "recovered_cpu");
            assert_eq!(report["last_failure"]["Runtime"], -5);
            assert_eq!(report["stream_verified"], true);
            assert_eq!(
                unsafe {
                    std::ffi::CStr::from_ptr(decoder.codec_info.lock().unwrap().backend_name_utf8)
                },
                c"ffmpeg-cpu"
            );
            if let Some((old, meta, bytes)) = held {
                let mut view = am_frame_view_t::default();
                assert_eq!(
                    unsafe { crate::frames::amc_frame_get_view(old, &mut view) },
                    0
                );
                assert_eq!(
                    unsafe { std::slice::from_raw_parts(view.planes[0], bytes.len()) },
                    bytes
                );
                assert_eq!(meta.source_generation, 1);
                assert_eq!(unsafe { crate::frames::amc_frame_release(old) }, 0);
            }
            assert_eq!(unsafe { amc_decoder_reset(dec, std::ptr::null()) }, 0);
            assert!(decoder
                .codec_info
                .lock()
                .unwrap()
                .backend_name_utf8
                .is_null());
            unsafe { submit(dec, &sample, 400) };
            let fresh = unsafe { frame(dec) };
            assert_eq!(fresh.1.source_generation, 3);
            assert!(hardware_report(dec as usize).unwrap()["last_failure"].is_null());
            assert_eq!(decoder.codec_info.lock().unwrap().hw_accelerated, 1);
            let mut view = am_frame_view_t::default();
            assert_eq!(
                unsafe { crate::frames::amc_frame_get_view(recovered.0, &mut view) },
                0
            );
            assert_eq!(
                unsafe { std::slice::from_raw_parts(view.planes[0], recovered.2.len()) },
                recovered.2
            );
            assert_eq!(unsafe { crate::frames::amc_frame_release(fresh.0) }, 0);
            assert_eq!(unsafe { crate::frames::amc_frame_release(recovered.0) }, 0);
            drop(entry);
            let mut lifecycle = 0;
            assert_eq!(unsafe { amc_decoder_close(dec, 100, &mut lifecycle) }, 0);
            assert_eq!(crate::amc_object_destroy(dec.cast()), 0);
            assert_eq!(state.usage_snapshot().unwrap(), (0, 0, 0));
        }
    }
    drop(state);
    let mut lifecycle = 0;
    assert_eq!(unsafe { crate::amc_shutdown(ctx, 100, &mut lifecycle) }, 0);
    assert_eq!(crate::amc_object_destroy(ctx.cast()), 0);
}
