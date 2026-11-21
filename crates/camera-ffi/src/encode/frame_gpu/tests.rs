//! amc_frame_encode_gpu 测试：合成校验（无 GPU）+ 真实 D3D11VA 硬解对拍门。
use super::super::{encode_frame_to_sink, validate_encode_opts};
use super::*;

fn nv12_strides(w: usize) -> [usize; 2] {
    [w, w]
}

fn jpeg_opts() -> am_frame_encode_opts_t {
    am_frame_encode_opts_t {
        struct_size: std::mem::size_of::<am_frame_encode_opts_t>() as u32,
        format: 1,
        quality: 80,
        reserved: [0; 5],
    }
}

fn gpu_ref(texture: *mut std::ffi::c_void) -> am_gpu_frame_ref_t {
    am_gpu_frame_ref_t {
        width: 32,
        height: 16,
        d3d_format: 103,
        subresource: 0,
        texture,
        ..Default::default()
    }
}

#[test]
fn gpu_ref_rejects_null_texture_bad_format_and_bounds() {
    let live = 0x1000 as *mut std::ffi::c_void;
    assert!(validate_gpu_ref(&gpu_ref(live)).is_ok());
    assert_eq!(
        validate_gpu_ref(&gpu_ref(std::ptr::null_mut())),
        Err(AM_ERR_SDK_INVALID_ARGUMENT),
        "texture 空"
    );
    let mut bad = gpu_ref(live);
    bad.d3d_format = 87;
    assert_eq!(validate_gpu_ref(&bad), Err(AM_ERR_SDK_INVALID_ARGUMENT));
    let mut bad = gpu_ref(live);
    bad.width = 0;
    assert_eq!(validate_gpu_ref(&bad), Err(AM_ERR_SDK_INVALID_ARGUMENT));
    let mut bad = gpu_ref(live);
    bad.height = 8193;
    assert_eq!(validate_gpu_ref(&bad), Err(AM_ERR_SDK_INVALID_ARGUMENT));
    let mut bad = gpu_ref(live);
    bad.width = 33;
    assert_eq!(
        validate_gpu_ref(&bad),
        Err(AM_ERR_SDK_INVALID_ARGUMENT),
        "NV12 奇数宽"
    );
    let mut bad = gpu_ref(live);
    bad.height = 17;
    assert_eq!(validate_gpu_ref(&bad), Err(AM_ERR_SDK_INVALID_ARGUMENT));
}

#[test]
fn opts_violations_flow_through_shared_validation() {
    let mut opts = jpeg_opts();
    opts.format = 9;
    assert_eq!(
        unsafe { validate_encode_opts(&opts) },
        Err(AM_ERR_SDK_INVALID_ARGUMENT)
    );
    let mut opts = jpeg_opts();
    opts.quality = 101;
    assert_eq!(
        unsafe { validate_encode_opts(&opts) },
        Err(AM_ERR_SDK_INVALID_ARGUMENT)
    );
    let mut opts = jpeg_opts();
    opts.reserved[4] = 1;
    assert_eq!(
        unsafe { validate_encode_opts(&opts) },
        Err(AM_ERR_SDK_INVALID_ARGUMENT)
    );
    let (format, quality) = unsafe { validate_encode_opts(&jpeg_opts()) }.unwrap();
    assert_eq!((format, quality), (1, 80));
}

/// 两段式容量协议（§2.12 与 amc_frame_encode 同款）：容量不足填所需返
/// limitOutOfRange，足量成功且字节数一致。
#[test]
fn capacity_two_phase_reports_required_then_encodes() {
    let (w, h) = (4usize, 4usize);
    let mut y = vec![0u8; w * h];
    let uv = vec![128u8; w * (h / 2)];
    for (i, b) in y.iter_mut().enumerate() {
        *b = (i * 7) as u8;
    }
    let strides = nv12_strides(w);
    let input = am_camera_core::frame_export::FrameInput {
        planes: &[&y, &uv],
        strides: &strides,
        width: w as u32,
        height: h as u32,
        format: am_camera_core::frame_export::PF_NV12,
        color_matrix: 0,
        color_range: 0,
    };
    let mut needed = 0u64;
    let mut sink = [0u8; 4];
    assert_eq!(
        encode_frame_to_sink(&input, 1, 0, sink.as_mut_ptr(), 4, &mut needed),
        Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE)
    );
    assert!(needed > 4, "required size must be reported, got {needed}");
    let mut out = vec![0u8; needed as usize];
    let mut written = 0u64;
    assert_eq!(
        encode_frame_to_sink(&input, 1, 0, out.as_mut_ptr(), needed, &mut written),
        Ok(())
    );
    assert_eq!(written, needed);
    assert_eq!((out[0], out[1]), (0xFF, 0xD8), "JPEG SOI");
    let mut png_needed = 0u64;
    let mut png_out = vec![0u8; 64 * 1024];
    let mut png_written = 0u64;
    assert_eq!(
        encode_frame_to_sink(
            &input,
            2,
            0,
            png_out.as_mut_ptr(),
            png_out.len() as u64,
            &mut png_needed
        ),
        Ok(())
    );
    assert_eq!(
        encode_frame_to_sink(
            &input,
            2,
            0,
            png_out.as_mut_ptr(),
            png_needed - 1,
            &mut png_written
        ),
        Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE)
    );
    assert_eq!(png_written, png_needed);
    assert!(png_out.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]));
}

/// ctx 名额守卫（encode/tests.rs 同款）：panic 路径也释放共享域准入名额。
struct CtxGuard(usize);
impl Drop for CtxGuard {
    fn drop(&mut self) {
        let _ = crate::amc_object_destroy(self.0 as *mut core::ffi::c_void);
    }
}

fn export_ctx() -> CtxGuard {
    let params = amc_init_params_t {
        abi_version: AM_ABI_VERSION,
        ..Default::default()
    };
    loop {
        match unsafe { am_sdk_support::context::initialize_camera(crate::domain(), &params) } {
            Ok(key) => return CtxGuard(key),
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED) => {
                std::thread::sleep(std::time::Duration::from_millis(5))
            }
            Err(other) => panic!("camera ctx init: {other}"),
        }
    }
}

/// 导出面三态：无效 ctx（校验先于 gpu 解引用，安全）、gpu 指针空、texture 空。
#[test]
fn frame_encode_gpu_rejects_invalid_ctx_and_references() {
    let opts = jpeg_opts();
    let mut written = 0u64;
    let mut sink = [0u8; 64];
    assert_eq!(
        unsafe {
            super::amc_frame_encode_gpu(
                0xdead as *mut amc_context_t,
                std::ptr::null(),
                &opts,
                sink.as_mut_ptr(),
                sink.len() as u64,
                &mut written,
            )
        },
        AM_ERR_SDK_HANDLE_INVALID
    );
    let guard = export_ctx();
    let ctx = guard.0 as *mut amc_context_t;
    assert_eq!(
        unsafe {
            super::amc_frame_encode_gpu(
                ctx,
                std::ptr::null(),
                &opts,
                sink.as_mut_ptr(),
                sink.len() as u64,
                &mut written,
            )
        },
        AM_ERR_SDK_NULL_ARGUMENT
    );
    let dead_gpu = gpu_ref(std::ptr::null_mut());
    assert_eq!(
        unsafe {
            super::amc_frame_encode_gpu(
                ctx,
                &dead_gpu,
                &opts,
                sink.as_mut_ptr(),
                sink.len() as u64,
                &mut written,
            )
        },
        AM_ERR_SDK_INVALID_ARGUMENT
    );
}

/// 真实 D3D11VA 硬解 GPU 帧 → amc_frame_encode_gpu，与 host 路对拍：
/// (a) 回读产物喂 amc_frame_encode 的 JPEG 与 GPU 路逐字节相同（同一编码链）；
/// (b) 与 FFmpeg copy_rgba 基准解码后像素近似（验证回读内容真实来自纹理）。
#[test]
#[ignore = "需要AM_SDK_FFMPEG_DIR与AM_RT_FFMPEG_DIR与AM_SDK_CODEC_SAMPLES（真实D3D11VA硬解+NVIDIA GPU 本机实测）"]
fn real_d3d11va_gpu_frame_encode_matches_host_path() {
    let runtime = std::env::var_os("AM_SDK_FFMPEG_DIR").expect("缺少隔离运行库目录");
    let samples = std::env::var_os("AM_SDK_CODEC_SAMPLES").expect("缺少合成样本目录");
    let rt_dir = std::env::var("AM_RT_FFMPEG_DIR").expect("缺少 ctx 运行库目录");
    let api = std::sync::Arc::new(
        am_camera_core::media::ffmpeg_lib::FfmpegApi::load_from(std::path::Path::new(&runtime))
            .expect("加载隔离FFmpeg运行库"),
    );
    let data = std::fs::read(std::path::Path::new(&samples).join("one.h264")).unwrap();
    let selected = am_camera_core::media::codec::CpuDecoder::create_with_policy(
        api,
        am_camera_core::media::codec::VideoCodec::H264,
        &[],
        256 * 144,
        am_camera_core::media::codec::selection::HardwarePolicy::PreferHardware,
        std::time::Duration::from_millis(1500),
    )
    .expect("硬解选择");
    assert!(
        selected.report.selected.is_some(),
        "本测试需要真实硬件，不将CPU当硬件验收"
    );
    let mut decoder = selected.decoder;
    decoder.send(&data, 90000, 90000).expect("送入码流");
    if decoder.peek().unwrap().is_none() {
        decoder.drain().unwrap();
    }
    let info = decoder.peek().unwrap().unwrap();
    let gpu_info = decoder.gpu_frame_info(info).expect("GPU元数据提取");
    assert_eq!(gpu_info.d3d_format, 103, "DXGI_FORMAT_NV12=103");
    let texture = decoder.gpu_texture().expect("纹理指针");
    assert!(!texture.is_null());

    let rt_path = std::ffi::CString::new(rt_dir).unwrap();
    let params = amc_init_params_t {
        abi_version: AM_ABI_VERSION,
        runtime_dirs_utf8: [rt_path.as_ptr()].as_ptr(),
        runtime_dir_count: 1,
        ..Default::default()
    };
    let mut ctx = std::ptr::null_mut();
    assert_eq!(unsafe { crate::amc_init(&params, &mut ctx) }, 0);
    let opts = jpeg_opts();

    // GPU 路：租约借用期内同步编码（帧仍被 decoder 持有）。
    let gpu_ref = am_gpu_frame_ref_t {
        width: gpu_info.info.width,
        height: gpu_info.info.height,
        d3d_format: gpu_info.d3d_format,
        subresource: gpu_info.subresource,
        texture,
        ..Default::default()
    };
    let mut needed = 0u64;
    let mut gpu_out = vec![0u8; 8 * 1024 * 1024];
    assert_eq!(
        unsafe {
            super::amc_frame_encode_gpu(
                ctx,
                &gpu_ref,
                &opts,
                gpu_out.as_mut_ptr(),
                gpu_out.len() as u64,
                &mut needed,
            )
        },
        0
    );
    gpu_out.truncate(needed as usize);
    assert_eq!((gpu_out[0], gpu_out[1]), (0xFF, 0xD8), "JPEG SOI");

    // 对拍 (a)：同帧回读产物组装 FrameInput 走共享编码主体，输出与 GPU 路
    // 逐字节一致（同一编码链；不经 pixel view 入口——read_pixel_view_any 对
    // NV12 UV 平面要求 stride≥2w，物理行距=w 的合法视图会被误拒，口径疑点
    // 另行上报）。
    let host = unsafe {
        crate::decoder::gpu_pool::readback_nv12(
            texture,
            gpu_info.subresource,
            gpu_info.info.width,
            gpu_info.info.height,
        )
    }
    .expect("受测路径回读");
    let (w, h) = (gpu_info.info.width as usize, gpu_info.info.height as usize);
    let pitch = host.row_pitch;
    let y_plane = &host.data[..pitch * (h - 1) + w];
    let uv_plane = &host.data[pitch * h..pitch * h + pitch * (h / 2 - 1) + w];
    let host_input = am_camera_core::frame_export::FrameInput {
        planes: &[y_plane, uv_plane],
        strides: &[pitch, pitch],
        width: gpu_info.info.width,
        height: gpu_info.info.height,
        format: am_camera_core::frame_export::PF_NV12,
        color_matrix: 0,
        color_range: 0,
    };
    let mut host_needed = 0u64;
    let mut host_out = vec![0u8; 8 * 1024 * 1024];
    assert_eq!(
        encode_frame_to_sink(
            &host_input,
            opts.format,
            opts.quality,
            host_out.as_mut_ptr(),
            host_out.len() as u64,
            &mut host_needed,
        ),
        Ok(())
    );
    host_out.truncate(host_needed as usize);
    assert_eq!(gpu_out, host_out, "同一 NV12 字节经两入口编码应逐字节一致");

    // 对拍 (b)：FFmpeg sws RGBA 基准 → amc_frame_encode，解码近似（回读内容
    // 与纹理真实内容一致；容差覆盖 sws 定点 vs 浮点 YUV→RGB + JPEG）。
    let rgba = decoder.copy_rgba(info.rgba_bytes).unwrap().unwrap();
    let rgba_view = Box::new(am_frame_view_t {
        width: info.width,
        height: info.height,
        format: AM_PF_RGBA8 as u32,
        plane_count: 1,
        planes: [
            rgba.data.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
        ],
        strides: [info.width * 4, 0, 0, 0],
        plane_size_bytes: [rgba.data.len() as u64, 0, 0, 0],
        ..Default::default()
    });
    let rgba_pixel = am_pixel_view_t {
        magic: AM_PIXEL_VIEW_MAGIC,
        view: &*rgba_view,
        ..Default::default()
    };
    let mut rgba_size = 0u64;
    let mut rgba_out = vec![0u8; 8 * 1024 * 1024];
    assert_eq!(
        unsafe {
            super::super::amc_frame_encode(
                ctx,
                &rgba_pixel,
                &opts,
                rgba_out.as_mut_ptr(),
                rgba_out.len() as u64,
                &mut rgba_size,
            )
        },
        0
    );
    rgba_out.truncate(rgba_size as usize);
    let gpu_image = image::load_from_memory(&gpu_out)
        .expect("GPU 路 JPEG 解码")
        .to_rgb8();
    let ref_image = image::load_from_memory(&rgba_out)
        .expect("基准 JPEG 解码")
        .to_rgb8();
    assert_eq!(gpu_image.dimensions(), ref_image.dimensions());
    let mut total = 0u64;
    let mut max_diff = 0u32;
    for (a, b) in gpu_image.as_raw().iter().zip(ref_image.as_raw()) {
        let diff = a.abs_diff(*b);
        total += diff as u64;
        max_diff = max_diff.max(u32::from(diff));
    }
    let mean = total / gpu_image.as_raw().len() as u64;
    eprintln!(
        "[证据] GPU 帧编码 {}x{} jpeg={}B 与 FFmpeg RGBA 基准解码对拍 mean={mean} max={max_diff}",
        info.width,
        info.height,
        gpu_out.len()
    );
    assert!(mean <= 8, "平均通道差 {mean} 超容差");
    assert!(max_diff <= 48, "最大通道差 {max_diff} 超容差");

    let mut lifecycle = 0u32;
    assert_eq!(unsafe { crate::amc_shutdown(ctx, 100, &mut lifecycle) }, 0);
    assert_eq!(crate::amc_object_destroy(ctx.cast()), 0);
}
