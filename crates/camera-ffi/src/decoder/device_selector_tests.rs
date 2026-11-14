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

/// Q1 诊断（sdk-q1-20261115）：4 路并发 `amc_decoder_create` 个别 rc=8 的复现与归因。
/// 每变体独立 ctx；逐轮快照 control 队列账本（controls/control_bytes）定位拒绝源。
/// 变体（FB_Q1_MODE，默认 all）：lanes4=仅 4 路并发（V8.5 原始形态）；fifth=先存活
/// 第 5 个解码器再并发 4 路（算术边界：恰 1 路 rc=8）；fifth_smallq=同 fifth 但浸泡路
/// queue_max_items=16（验证约束源=control 字节数而非解码器个数）；destroy_race=第 5 个
/// 解码器销毁与 4 路创建同窗竞速（V8.5 疑似时序，只统计不断言）。
struct Q1Create {
    rc: i32,
    handle: *mut amc_decoder_t,
}
unsafe impl Send for Q1Create {}

fn q1_setup() -> *mut amc_context_t {
    let directory = std::env::var("AM_RT_FFMPEG_DIR").expect("缺少隔离运行库目录");
    let path = CString::new(directory).unwrap();
    let dirs = [path.as_ptr()];
    let params = amc_init_params_t {
        abi_version: AM_ABI_VERSION,
        runtime_dirs_utf8: dirs.as_ptr(),
        runtime_dir_count: 1,
        ..Default::default()
    };
    let mut ctx = std::ptr::null_mut();
    assert_eq!(unsafe { crate::amc_init(&params, &mut ctx) }, 0);
    ctx
}

/// 同款定向 config（默认队列=64 槽；queue_items>0 时覆盖）；返回 (rc, 句柄)。
unsafe fn q1_create(ctx: *mut amc_context_t, queue_items: u32) -> Q1Create {
    let (_text, selector) = selector_of("Arc");
    let cfg = am_decoder_config_t {
        codec: AM_CODEC_H264,
        bitstream_format: AM_BSF_ANNEXB,
        output_pixel_format: AM_PF_RGBA8 as u32,
        hw_policy: AM_DHW_PREFER_HW as u32,
        time_base_num: 1,
        time_base_den: 1000,
        queue_max_items: queue_items,
        device: &selector,
        ..Default::default()
    };
    let mut dec = std::ptr::null_mut();
    let rc = unsafe { amc_decoder_create(ctx, &cfg, &mut dec) };
    Q1Create { rc, handle: dec }
}

fn q1_ledger(ctx: *mut amc_context_t) -> (usize, usize) {
    crate::frames::camera_state(ctx as usize)
        .unwrap()
        .control_usage_snapshot()
        .unwrap()
}

fn q1_teardown(dec: *mut amc_decoder_t) {
    if dec.is_null() {
        return;
    }
    let mut lifecycle = 0;
    assert_eq!(unsafe { amc_decoder_close(dec, 2000, &mut lifecycle) }, 0);
    assert_eq!(crate::amc_object_destroy(dec.cast()), 0);
}

/// 4 路并发创建一轮；destroy_race 模式下附带一个与创建同窗销毁的第 5 个解码器。
fn q1_round(
    ctx: *mut amc_context_t,
    queue_items: u32,
    racer: Option<*mut amc_decoder_t>,
) -> Vec<Q1Create> {
    let barrier = std::sync::Barrier::new(4 + usize::from(racer.is_some()));
    let results: std::sync::Mutex<Vec<Q1Create>> = std::sync::Mutex::new(Vec::new());
    let ctx_addr: CtxAddr = ctx as usize;
    let racer_addr: Option<usize> = racer.map(|handle| handle as usize);
    std::thread::scope(|scope| {
        let barrier = &barrier;
        if let Some(racer_addr) = racer_addr {
            scope.spawn(move || {
                barrier.wait();
                q1_teardown(racer_addr as *mut amc_decoder_t);
            });
        }
        let lanes: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(move || {
                    barrier.wait();
                    unsafe { q1_create(ctx_addr as *mut amc_context_t, queue_items) }
                })
            })
            .collect();
        for lane in lanes {
            let created = lane.join().expect("lane 线程");
            results.lock().unwrap().push(Q1Create {
                rc: created.rc,
                handle: created.handle,
            });
        }
    });
    let mut created = results.into_inner().unwrap();
    created.sort_by_key(|lane| lane.rc);
    created
}

#[test]
#[ignore = "需要AM_RT_FFMPEG_DIR（本机 Arc 核显定向 D3D11VA）；Q1 rc8 诊断（sdk-q1-20261115）"]
fn decoder_concurrent_create_rc8_reproduction() {
    let rounds: usize = std::env::var_os("FB_Q1_ROUNDS")
        .map(|v| v.to_string_lossy().parse().expect("数值环境变量"))
        .unwrap_or(12);
    let mode = std::env::var_os("FB_Q1_MODE")
        .map(|v| v.to_string_lossy().into_owned())
        .unwrap_or_else(|| "all".into());
    let variants: &[&str] = if mode == "all" {
        &["lanes4", "fifth", "fifth_smallq", "destroy_race"]
    } else {
        &[mode.as_str()]
    };
    for variant in variants {
        let ctx = q1_setup();
        let baseline = q1_ledger(ctx);
        eprintln!(
            "Q1 variant={variant} rounds={rounds} baseline_ledger=(controls={},bytes={})",
            baseline.0, baseline.1
        );
        let mut anomalies = 0usize;
        for round in 0..rounds {
            let (queue_items, racer, mut extra_cleanup) = match *variant {
                // 第 5 个解码器默认队列存活，占满 headroom 后并发 4 路。
                "fifth" | "fifth_smallq" => {
                    let extra = unsafe { q1_create(ctx, 0) };
                    assert_eq!(extra.rc, 0, "前置第 5 解码器创建应成功");
                    let items = if *variant == "fifth_smallq" { 16 } else { 0 };
                    (items, None, Some(extra.handle))
                }
                // 销毁竞速：先建第 5 个，销毁线程与 4 路创建同 barrier 起跑；
                // 句柄由竞速线程在轮内销毁，轮末不再重复清理。
                "destroy_race" => {
                    let extra = unsafe { q1_create(ctx, 0) };
                    assert_eq!(extra.rc, 0, "前置第 5 解码器创建应成功");
                    (0, Some(extra.handle), None)
                }
                _ => (0, None, None),
            };
            let lanes = q1_round(ctx, queue_items, racer);
            let mut codes = [0i32; 4];
            for (index, lane) in lanes.iter().enumerate() {
                codes[index] = lane.rc;
            }
            let exhausted = codes
                .iter()
                .filter(|rc| **rc == AM_ERR_SDK_RESOURCE_EXHAUSTED)
                .count();
            eprintln!(
                "Q1 {variant} round={round} rcs={codes:?} ledger=({:?})",
                q1_ledger(ctx)
            );
            for lane in &lanes {
                q1_teardown(lane.handle);
            }
            if let Some(extra) = extra_cleanup.take() {
                q1_teardown(extra);
            }
            match *variant {
                // 4 路独占（V8.5 原始形态）：全部应成功——出现任何 rc=8 即真竞态信号。
                "lanes4" => assert_eq!(
                    exhausted, 0,
                    "lanes4 第 {round} 轮出现 rc=8：并发创建存在真实竞态 codes={codes:?}"
                ),
                // 算术边界：5 份 13408B 必然恰拒 1 路（control 队列上限 65536）。
                "fifth" => assert_eq!(
                    exhausted, 1,
                    "fifth 第 {round} 轮应恰 1 路 rc=8（其余成功）codes={codes:?}"
                ),
                // 队列 16 槽时单路计费缩小，5 路共存应全部成功。
                "fifth_smallq" => assert_eq!(
                    exhausted, 0,
                    "fifth_smallq 第 {round} 轮不应拒绝 codes={codes:?}"
                ),
                // 竞速窗口：结果取决于销毁释放与创建准入的先后，只统计。
                _ => anomalies += usize::from(exhausted > 0),
            }
            assert_eq!(
                q1_ledger(ctx),
                baseline,
                "{variant} 第 {round} 轮销毁后账本未回落基线（计费泄漏）"
            );
        }
        if *variant == "destroy_race" {
            eprintln!(
                "Q1 destroy_race rounds={rounds} rounds_with_rc8={anomalies} rate={:.0}%",
                anomalies as f64 / rounds as f64 * 100.0
            );
        }
        let mut lifecycle = 0;
        assert_eq!(unsafe { crate::amc_shutdown(ctx, 100, &mut lifecycle) }, 0);
        assert_eq!(crate::amc_object_destroy(ctx.cast()), 0);
    }
    eprintln!("Q1 done");
}
