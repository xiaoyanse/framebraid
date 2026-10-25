//! 合成单AU重复解码，对拍CPU与指定设备的真实输出；不连接现场相机。
use am_camera::media::{
    codec::{CpuDecoder, DecodeError, VideoCodec},
    ffmpeg_lib::FfmpegApi,
    hardware::{self, Adapter, Backend},
};
use std::{collections::BTreeMap, path::Path, sync::Arc, time::Instant};
#[path = "hardware_decode/observe.rs"]
mod observe;
type DecodeRun = (BTreeMap<i64, Vec<u8>>, f64, bool);

fn collect(decoder: &mut CpuDecoder, frames: &mut BTreeMap<i64, Vec<u8>>) -> Result<(), String> {
    loop {
        match decoder.peek() {
            Ok(None) | Err(DecodeError::Drained) => return Ok(()),
            Err(e) => return Err(format!("receive: {e:?}")),
            Ok(Some(info)) => {
                if !matches!(decoder.copy_rgba(1), Err(DecodeError::FrameTooLarge)) {
                    return Err("output budget was not enforced before transfer".into());
                }
                let copied = decoder.copy_rgba(8192 * 8192 * 4);
                let copied = if matches!(copied, Err(DecodeError::Runtime(-5)))
                    && observe::injected_failure()
                {
                    if decoder
                        .peek()
                        .map_err(|e| format!("retained frame: {e:?}"))?
                        .map(|f| f.pts)
                        != Some(info.pts)
                    {
                        return Err("failed transfer lost the pending frame".into());
                    }
                    decoder.copy_rgba(8192 * 8192 * 4)
                } else {
                    copied
                };
                let frame = copied
                    .map_err(|e| format!("copy: {e:?}"))?
                    .ok_or("pending frame disappeared")?;
                if frame.info.width != info.width || frame.info.height != info.height {
                    return Err("frame dimensions changed during copy".into());
                }
                if frames.insert(info.pts, frame.data).is_some() {
                    return Err("duplicate output timestamp".into());
                }
            }
        }
    }
}

fn run(
    api: Arc<FfmpegApi>,
    codec: VideoCodec,
    packet: &[u8],
    device: Option<(Backend, &Adapter)>,
) -> Result<DecodeRun, String> {
    let mut decoder = match device {
        Some((backend, adapter)) => {
            CpuDecoder::create_on_device(api, codec, &[], 8192 * 8192, backend, adapter)
        }
        None => CpuDecoder::create(api, codec, &[], 8192 * 8192),
    }
    .map_err(|e| format!("create: {e:?}"))?;
    if decoder.hardware_backend().is_some() {
        return Err("hardware reported before first decoded frame".into());
    }
    if device.is_some() {
        observe::inject_transfer_failure();
    }
    let mut frames = BTreeMap::new();
    let start = Instant::now();
    for index in 0..12 {
        if !decoder
            .send(packet, index, index)
            .map_err(|e| format!("send: {e:?}"))?
        {
            collect(&mut decoder, &mut frames)?;
            if !decoder
                .send(packet, index, index)
                .map_err(|e| format!("retry: {e:?}"))?
            {
                return Err("decoder remained backpressured after receiving".into());
            }
        }
        collect(&mut decoder, &mut frames)?;
    }
    if !decoder.drain().map_err(|e| format!("drain: {e:?}"))? {
        collect(&mut decoder, &mut frames)?;
        if !decoder.drain().map_err(|e| format!("drain retry: {e:?}"))? {
            return Err("drain remained backpressured".into());
        }
    }
    collect(&mut decoder, &mut frames)?;
    if frames.len() != 12 || frames.keys().copied().ne(0..12) {
        return Err(format!("missing output timestamps: {:?}", frames.keys()));
    }
    Ok((
        frames,
        start.elapsed().as_secs_f64() * 1000.0,
        decoder.hardware_backend().is_some(),
    ))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("usage: hardware_decode runtime_dir single_au.h264|h265 h264|h265".into());
    }
    let codec = match args[3].as_str() {
        "h264" => VideoCodec::H264,
        "h265" => VideoCodec::H265,
        _ => return Err("unsupported codec".into()),
    };
    let mut api = FfmpegApi::load_from(Path::new(&args[1]))?;
    observe::install(&mut api)?;
    let api = Arc::new(api);
    let packet = std::fs::read(&args[2])?;
    if packet.len() > 16 * 1024 * 1024 {
        return Err("packet budget exceeded".into());
    }
    let (reference, cpu_ms, _) = run(api.clone(), codec, &packet, None)?;
    let raw_reference = observe::take().ok_or("missing CPU YUV observation")?;
    let mut reports = Vec::new();
    for adapter in hardware::adapters()?.iter().filter(|a| !a.software) {
        for backend in [Backend::D3d11va, Backend::Dxva2] {
            observe::take();
            match run(api.clone(), codec, &packet, Some((backend, adapter))) {
                Err(error) => reports.push(serde_json::json!({"adapter": adapter.index, "backend": backend, "error": error})),
                Ok((frames, elapsed_ms, verified)) => {
                    let mut sum = 0u64;
                    let mut max = 0u8;
                    let mut count = 0u64;
                    for (pts, actual) in &frames {
                        let expected = reference.get(pts).ok_or("unexpected timestamp")?;
                        if expected.len() != actual.len() {
                            return Err("hardware output size mismatch".into());
                        }
                        for (a, b) in expected.iter().zip(actual) {
                            let difference = a.abs_diff(*b);
                            sum += u64::from(difference);
                            max = max.max(difference);
                            count += 1;
                        }
                    }
                    let mean = sum as f64 / count as f64;
                    let raw = observe::take().ok_or("missing hardware YUV observation")?;
                    if raw.len() != raw_reference.len() { return Err("YUV dimensions differ".into()); }
                    let raw_max = raw.iter().zip(&raw_reference).map(|(a,b)| a.abs_diff(*b)).max().ok_or("empty YUV")?;
                    let raw_mean = raw.iter().zip(&raw_reference).map(|(a,b)| f64::from(a.abs_diff(*b))).sum::<f64>() / raw.len() as f64;
                    if !verified || raw_max > 2 || raw_mean > 0.1 || mean > 1.0 {
                        return Err(format!("unverified or incorrect hardware output: YUV mean={raw_mean}, max={raw_max}; RGBA mean={mean}, max={max}").into());
                    }
                    reports.push(serde_json::json!({"adapter":adapter.index,"backend":backend,"stream_verified":verified,"frames":frames.len(),"decode_transfer_rgba_ms":elapsed_ms,"rgba_mean_abs_error":mean,"rgba_max_abs_error":max,"yuv_first_frame_mean_abs_error":raw_mean,"yuv_first_frame_max_abs_error":raw_max}));
                }
            }
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"codec":args[3],"cpu_decode_rgba_ms":cpu_ms,"frames":reference.len(),"attempts":reports})
        )?
    );
    Ok(())
}
