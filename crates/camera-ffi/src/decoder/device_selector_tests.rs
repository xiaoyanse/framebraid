//! camera 侧设备定向真机门（V8.5，§3.5b）：decoder config device 子串定向命中
//! Intel 核显（D3D11VA 硬解真实出帧+选择报告适配器身份）与零命中同步拒绝；
//! FB_V85_DEC_SOAK_MS 控制多路硬解浸泡窗口（默认 60s 冒烟/验收 600000）。
use super::*;
use std::ffi::CString;

fn selector_of(needle: &str) -> (CString, am_device_selector_t) {
    let text = CString::new(needle).unwrap();
    let selector = am_device_selector_t {
        struct_size: std::mem::size_of::<am_device_selector_t>() as u32,
        match_utf8: text.as_ptr(),
        // match_bytes 含 NUL 终结（§3.5b）。
        match_bytes: text.as_bytes_with_nul().len() as u32,
        instance: 0,
        reserved: 0,
    };
    (text, selector)
}

fn hardware_report(key: usize) -> serde_json::Value {
    let bytes = super::hardware_report(key).expect("decode-hardware 报告");
    serde_json::from_slice(&bytes).unwrap()
}

/// 定向创建+单帧硬解：返回解码器句柄；断言选择报告命中 Intel 核显。
fn targeted_decoder(ctx: *mut amc_context_t, needle: &str) -> *mut amc_decoder_t {
    let (_text, selector) = selector_of(needle);
    let cfg = am_decoder_config_t {
        codec: AM_CODEC_H264,
        bitstream_format: AM_BSF_ANNEXB,
        output_pixel_format: AM_PF_RGBA8 as u32,
        hw_policy: AM_DHW_PREFER_HW as u32,
        time_base_num: 1,
        time_base_den: 1000,
        device: &selector,
        ..Default::default()
    };
    let mut dec = std::ptr::null_mut();
    assert_eq!(unsafe { amc_decoder_create(ctx, &cfg, &mut dec) }, 0);
    dec
}

unsafe fn decode_one(dec: *mut amc_decoder_t, sample: &[u8], pts: i64) -> f64 {
    let begin = std::time::Instant::now();
    let packet = am_decoder_input_t {
        data: sample.as_ptr(),
        size: sample.len() as u64,
        pts,
        dts: pts,
        flags: AM_PKT_AU_COMPLETE,
        ..Default::default()
    };
    assert_eq!(unsafe { amc_decoder_submit(dec, &packet) }, 0);
    let mut frame = std::ptr::null_mut();
    // wait 上限 SDK_RECEIVE_TIMEOUT_MS=1000。
    assert_eq!(unsafe { amc_decoder_receive(dec, 1000, &mut frame) }, 0);
    let ms = begin.elapsed().as_secs_f64() * 1000.0;
    let mut view = am_frame_view_t::default();
    assert_eq!(
        unsafe { crate::frames::amc_frame_get_view(frame, &mut view) },
        0
    );
    assert_eq!((view.width, view.height), (1920, 1080), "1080p 样本帧尺寸");
    let plane =
        unsafe { std::slice::from_raw_parts(view.planes[0], view.plane_size_bytes[0] as usize) };
    let center =
        (view.height as usize / 2) * view.strides[0] as usize + (view.width as usize / 2) * 4;
    let (r, _g, b) = (plane[center], plane[center + 1], plane[center + 2]);
    assert!(
        b as u32 > r as u32 + 50 && b > 100,
        "blue-1080 样本中心像素应为蓝主导: r={r} b={b}"
    );
    assert_eq!(unsafe { crate::frames::amc_frame_release(frame) }, 0);
    ms
}

fn percentile(mut sorted: Vec<f64>, p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted.sort_by(|a, b| a.total_cmp(b));
    let index = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

/// ctx 句柄跨浸泡线程按 usize 搬运（FFI 裸指针非 Send；SDK 侧对象线程安全）。
type CtxAddr = usize;

#[test]
#[ignore = "需要AM_RT_FFMPEG_DIR与AM_SDK_CODEC_SAMPLES（blue-1080.h264）；本机 Arc 核显定向 D3D11VA（V8.5）"]
fn decoder_device_selector_targets_intel_igpu_and_rejects_zero_match() {
    let directory = std::env::var("AM_RT_FFMPEG_DIR").expect("缺少隔离运行库目录");
    let samples = std::env::var_os("AM_SDK_CODEC_SAMPLES").expect("缺少合成样本目录");
    let sample = std::fs::read(std::path::Path::new(&samples).join("blue-1080.h264"))
        .expect("blue-1080.h264（1920x1080 单AU）");
    let path = CString::new(directory.clone()).unwrap();
    let dirs = [path.as_ptr()];
    let params = amc_init_params_t {
        abi_version: AM_ABI_VERSION,
        runtime_dirs_utf8: dirs.as_ptr(),
        runtime_dir_count: 1,
        ..Default::default()
    };
    let mut ctx = std::ptr::null_mut();
    assert_eq!(unsafe { crate::amc_init(&params, &mut ctx) }, 0);

    // 1) 定向 Arc 创建：选择报告命中 Intel 核显（vendor 0x8086+描述含 arc）。
    let dec = targeted_decoder(ctx, "Arc");
    let report = hardware_report(dec as usize);
    let adapter = &report["selection"]["selected"]["adapter"];
    let name = adapter["name"].as_str().unwrap_or_default().to_lowercase();
    let vendor = adapter["vendor_id"].as_u64().unwrap_or_default();
    assert_eq!(vendor, 0x8086, "定向应命中 Intel 核显: {adapter}");
    assert!(name.contains("arc"), "适配器描述含 arc: {name}");
    assert_eq!(report["selection"]["selected"]["backend"], "d3d11va");
    eprintln!(
        "V85_DEC_SEL adapter_index={} vendor={vendor:#x} name={name} luid={:#x}",
        adapter["index"].as_u64().unwrap_or_default(),
        adapter["luid"].as_u64().unwrap_or_default(),
    );

    // 2) 单帧硬解出帧：实际后端 d3d11va+核显身份保持。
    unsafe { decode_one(dec, &sample, 100) };
    {
        let entry = domain().get(dec as usize).unwrap();
        let decoder = entry.data::<Decoder>().unwrap();
        assert_eq!(decoder.codec_info.lock().unwrap().hw_accelerated, 1);
    }
    let verified = hardware_report(dec as usize);
    assert_eq!(verified["state"], "verified");
    assert_eq!(verified["actual_backend"], "ffmpeg-d3d11va");
    assert_eq!(verified["stream_verified"], true);

    // 3) 零命中子串：invalidArgument 同步拒绝（不静默回退其他适配器）。
    let (_ghost_text, ghost) = selector_of("nonexistent-gpu");
    let cfg = am_decoder_config_t {
        codec: AM_CODEC_H264,
        bitstream_format: AM_BSF_ANNEXB,
        output_pixel_format: AM_PF_RGBA8 as u32,
        hw_policy: AM_DHW_PREFER_HW as u32,
        time_base_num: 1,
        time_base_den: 1000,
        device: &ghost,
        ..Default::default()
    };
    let mut rejected = std::ptr::null_mut();
    assert_eq!(
        unsafe { amc_decoder_create(ctx, &cfg, &mut rejected) },
        AM_ERR_SDK_INVALID_ARGUMENT,
        "零命中应同步 invalidArgument"
    );
    assert!(rejected.is_null(), "拒绝不得产出解码器句柄");
    eprintln!(
        "V85_DEC_REJECT needle=nonexistent-gpu rc={}",
        AM_ERR_SDK_INVALID_ARGUMENT
    );

    // 4) 多路硬解浸泡：每路独立定向解码器（独立 D3D11 设备），默认 60s 冒烟；
    //    验收窗口 FB_V85_DEC_SOAK_MS=600000 与 vision 侧浸泡并发表跑。主解码器
    //    先收口（解码器/帧预算按 ctx 记账），浸泡路在窗口前串行创建、窗口内并解。
    let mut lifecycle = 0;
    assert_eq!(unsafe { amc_decoder_close(dec, 2000, &mut lifecycle) }, 0);
    assert_eq!(crate::amc_object_destroy(dec.cast()), 0);
    let soak_ms: u64 = std::env::var_os("FB_V85_DEC_SOAK_MS")
        .map(|v| v.to_string_lossy().parse().expect("数值环境变量"))
        .unwrap_or(60_000);
    let lanes = 4usize;
    let lane_decoders: Vec<*mut amc_decoder_t> = (0..lanes)
        .map(|lane| {
            let lane_dec = targeted_decoder(ctx, "Arc");
            let report = hardware_report(lane_dec as usize);
            assert_eq!(
                report["selection"]["selected"]["adapter"]["vendor_id"], 0x8086,
                "浸泡 lane {lane} 应保持核显定向"
            );
            eprintln!(
                "V85_DEC_LANE_CREATE lane={lane} adapter_index={}",
                report["selection"]["selected"]["adapter"]["index"]
            );
            lane_dec
        })
        .collect();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(soak_ms);
    let stats: Vec<(u64, Vec<f64>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = lane_decoders
            .iter()
            .enumerate()
            .map(|(lane, lane_dec)| {
                let sample = &sample;
                let lane_dec: CtxAddr = *lane_dec as usize;
                scope.spawn(move || {
                    let lane_dec = lane_dec as *mut amc_decoder_t;
                    let mut frames = 0u64;
                    let mut latencies = Vec::new();
                    let mut pts = 1000 + lane as i64 * 1_000_000;
                    while std::time::Instant::now() < deadline {
                        latencies.push(unsafe { decode_one(lane_dec, sample, pts) });
                        pts += 40;
                        frames += 1;
                    }
                    let mut lifecycle = 0;
                    assert_eq!(
                        unsafe { amc_decoder_close(lane_dec, 2000, &mut lifecycle) },
                        0
                    );
                    assert_eq!(crate::amc_object_destroy(lane_dec.cast()), 0);
                    (frames, latencies)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("lane 线程"))
            .collect()
    });
    let total: u64 = stats.iter().map(|(frames, _)| *frames).sum();
    let secs = soak_ms as f64 / 1000.0;
    eprintln!(
        "V85_DEC_SOAK window_ms={soak_ms} lanes={lanes} total_frames={total} agg_fps={:.1}",
        total as f64 / secs
    );
    for (lane, (frames, latencies)) in stats.iter().enumerate() {
        eprintln!(
            "V85_DEC_SOAK lane={lane} frames={frames} fps={:.1} p50_ms={:.2} p95_ms={:.2} p99_ms={:.2}",
            *frames as f64 / secs,
            percentile(latencies.clone(), 0.50),
            percentile(latencies.clone(), 0.95),
            percentile(latencies.clone(), 0.99),
        );
    }
    let floor = 10u64 * (soak_ms / 60_000).max(1);
    for (lane, (frames, _)) in stats.iter().enumerate() {
        assert!(
            *frames >= floor,
            "lane {lane} 帧数 {frames} 低于稳定下限 {floor}"
        );
    }

    let mut lifecycle = 0;
    assert_eq!(unsafe { crate::amc_shutdown(ctx, 100, &mut lifecycle) }, 0);
    assert_eq!(crate::amc_object_destroy(ctx.cast()), 0);
}
