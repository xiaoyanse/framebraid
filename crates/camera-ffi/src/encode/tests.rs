//! 编码/存档/推流会话测试：合成校验与真实编码（真实路径标 ignore 需运行库）。
use super::super::archive::{amc_archive_close, amc_archive_open, amc_archive_write};
use super::*;

/// 填充 w*h RGBA 渐变帧（合成数据，不依赖真实相机）。
pub(crate) fn fill_gradient(buffer: &mut [u8], w: u32, h: u32) {
    for y in 0..h as usize {
        for x in 0..w as usize {
            let i = (y * w as usize + x) * 4;
            buffer[i] = (x * 255 / w as usize) as u8;
            buffer[i + 1] = (y * 255 / h as usize) as u8;
            buffer[i + 2] = 128;
            buffer[i + 3] = 255;
        }
    }
}

/// 像素视图构造（调用方保证 buffer 与 view 在调用导出函数期间存活）：
/// `let view = frame_view(&buf, w, h); let pixel = pixel_view(&view);`
pub(crate) fn frame_view(buffer: &[u8], w: u32, h: u32) -> am_frame_view_t {
    am_frame_view_t {
        width: w,
        height: h,
        format: AM_PF_RGBA8 as u32,
        plane_count: 1,
        planes: [
            buffer.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
        ],
        strides: [w * 4, 0, 0, 0],
        plane_size_bytes: [buffer.len() as u64, 0, 0, 0],
        ..Default::default()
    }
}

pub(crate) fn pixel_view(view: &am_frame_view_t) -> am_pixel_view_t {
    am_pixel_view_t {
        magic: AM_PIXEL_VIEW_MAGIC,
        view,
        ..Default::default()
    }
}

#[test]
fn queue_budget_defaults_and_caps() {
    assert_eq!(
        normalize_queue_budget(0, 0).unwrap(),
        (64, SDK_DECODER_INPUT_BYTES)
    );
    assert_eq!(normalize_queue_budget(8, 4096).unwrap(), (8, 4096));
    assert_eq!(
        normalize_queue_budget(65, 0).unwrap_err(),
        AM_ERR_SDK_LIMIT_OUT_OF_RANGE
    );
    assert_eq!(
        normalize_queue_budget(0, SDK_DECODER_INPUT_BYTES + 1).unwrap_err(),
        AM_ERR_SDK_LIMIT_OUT_OF_RANGE
    );
}

#[test]
fn encode_error_mapping_matches_contract_codes() {
    assert_eq!(
        encode_error_status(EncodeError::Library("media.ffmpegEncoderMissing")),
        AM_ERR_SDK_NOT_SUPPORTED
    );
    assert_eq!(
        encode_error_status(EncodeError::Config("camera.encoder.axisRange")),
        AM_ERR_SDK_INVALID_ARGUMENT
    );
    assert_eq!(
        encode_error_status(EncodeError::Ffmpeg(-1)),
        AM_ERR_SDK_INTERNAL_ERROR
    );
    assert_eq!(
        encode_error_status(EncodeError::PixelInvalid),
        AM_ERR_SDK_INVALID_ARGUMENT
    );
    assert_eq!(
        encode_error_status(EncodeError::InputAfterFlush),
        AM_ERR_SDK_DRAINED
    );
}

#[test]
fn pixel_view_rejects_bad_magic_format_and_bounds() {
    let mut buffer = vec![0u8; 32 * 16 * 4];
    fill_gradient(&mut buffer, 32, 16);
    let view = frame_view(&buffer, 32, 16);
    let mut pixel = pixel_view(&view);
    unsafe {
        assert!(read_pixel_view(&pixel).is_ok());
        pixel.magic = 0;
        assert_eq!(
            read_pixel_view(&pixel).unwrap_err(),
            AM_ERR_SDK_INVALID_ARGUMENT
        );
        let mut wrong = view;
        wrong.format = AM_PF_NV12 as u32;
        let wrong_pixel = pixel_view(&wrong);
        assert_eq!(
            read_pixel_view(&wrong_pixel).unwrap_err(),
            AM_ERR_CAMERA_PIXEL_FORMAT_UNSUPPORTED
        );
        let mut short = frame_view(&buffer[..31 * 16 * 4], 32, 16);
        short.strides[0] = 32 * 4;
        let short_pixel = pixel_view(&short);
        assert_eq!(
            read_pixel_view(&short_pixel).unwrap_err(),
            AM_ERR_SDK_INVALID_ARGUMENT
        );
    }
}

#[test]
fn segment_template_substitution() {
    assert_eq!(
        super::super::archive::segment_path("out%d.mp4", 0),
        "out0.mp4"
    );
    assert_eq!(
        super::super::archive::segment_path("out%d.mp4", 12),
        "out12.mp4"
    );
    assert_eq!(
        super::super::archive::segment_path("plain.mp4", 3),
        "plain.mp4"
    );
}

/// 真实路径公共入口：带 S5 运行库目录的 amc_init；返回 ctx 句柄。
unsafe fn init_with_runtime_dirs(directory: &std::path::Path) -> *mut amc_context_t {
    let path = std::ffi::CString::new(directory.to_str().unwrap()).unwrap();
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

unsafe fn encoder_config(w: u32, h: u32, items: u32) -> am_encoder_config_t {
    am_encoder_config_t {
        width: w,
        height: h,
        bitrate_kbps: 1500,
        gop_size: 10,
        fps_num: 30,
        fps_den: 1,
        queue_max_items: items,
        queue_max_bytes: 1 << 20,
        ..Default::default()
    }
}

const TEST_W: u32 = 320;
const TEST_H: u32 = 240;

#[test]
#[ignore = "显式设置 AM_RT_FFMPEG_DIR 指向 S5 GPL 运行库目录，真实 libx264 编码"]
fn encoder_lifecycle_queue_budget_and_drain_state_machine() {
    let directory = std::env::var("AM_RT_FFMPEG_DIR").unwrap();
    let directory = std::path::PathBuf::from(directory);
    let ctx = unsafe { init_with_runtime_dirs(&directory) };
    let cfg = unsafe { encoder_config(TEST_W, TEST_H, 8) };
    let mut enc = std::ptr::null_mut();
    assert_eq!(unsafe { amc_encoder_create(ctx, &cfg, &mut enc) }, 0);
    let mut buffer = vec![0u8; (TEST_W * TEST_H * 4) as usize];
    fill_gradient(&mut buffer, TEST_W, TEST_H);
    let view = frame_view(&buffer, TEST_W, TEST_H);
    let pixel = pixel_view(&view);
    for round in 0..8 {
        assert_eq!(
            unsafe { amc_encoder_encode(enc, &pixel) },
            0,
            "round {round}"
        );
    }
    {
        let entry = domain().get(enc as usize).unwrap();
        let object = entry.data::<EncoderObject>().unwrap();
        let guard = object.inner.lock().unwrap();
        let inner = guard.as_ref().unwrap();
        assert_eq!(inner.queue.len(), 8);
        let expected: u64 = inner.queue.iter().map(|p| p.data.len() as u64 + 64).sum();
        assert_eq!(inner.queue_bytes, expected);
        assert!(inner.queue.front().unwrap().keyframe, "首包应为 IDR 关键帧");
    }
    assert_eq!(
        unsafe { amc_encoder_encode(enc, &pixel) },
        AM_ERR_SDK_QUEUE_FULL,
        "超过 queue_max_items 应拒收"
    );
    assert_eq!(unsafe { amc_encoder_drain(enc) }, 0);
    assert_eq!(
        unsafe { amc_encoder_encode(enc, &pixel) },
        AM_ERR_SDK_DRAINED,
        "drain 后 encode 应报 DRAINED"
    );
    {
        let entry = domain().get(enc as usize).unwrap();
        let object = entry.data::<EncoderObject>().unwrap();
        let guard = object.inner.lock().unwrap();
        assert!(guard.as_ref().unwrap().drained);
    }
    let mut lifecycle = 0u32;
    assert_eq!(unsafe { amc_encoder_close(enc, 100, &mut lifecycle) }, 0);
    assert_eq!(lifecycle, AM_LIFECYCLE_CLOSED as u32);
    assert_eq!(crate::amc_object_destroy(enc.cast()), 0);
    assert_eq!(unsafe { crate::amc_shutdown(ctx, 100, &mut lifecycle) }, 0);
    assert_eq!(crate::amc_object_destroy(ctx.cast()), 0);
}

#[test]
#[ignore = "显式设置 AM_RT_FFMPEG_DIR 指向 S5 GPL 运行库目录，真实 MP4 落盘"]
fn archive_writes_and_segments_mp4_files() {
    let directory = std::env::var("AM_RT_FFMPEG_DIR").unwrap();
    let directory = std::path::PathBuf::from(directory);
    let ctx = unsafe { init_with_runtime_dirs(&directory) };
    let temp = tempfile::tempdir().unwrap();
    let template = temp.path().join("seg%d.mp4");
    let template = std::ffi::CString::new(template.to_str().unwrap()).unwrap();
    let cfg = unsafe { encoder_config(TEST_W, TEST_H, 8) };
    let mut arch = std::ptr::null_mut();
    assert_eq!(
        unsafe { amc_archive_open(ctx, template.as_ptr(), &cfg, 10, &mut arch) },
        0
    );
    let mut buffer = vec![0u8; (TEST_W * TEST_H * 4) as usize];
    fill_gradient(&mut buffer, TEST_W, TEST_H);
    let view = frame_view(&buffer, TEST_W, TEST_H);
    let pixel = pixel_view(&view);
    for round in 0..25 {
        assert_eq!(
            unsafe { amc_archive_write(arch, &pixel) },
            0,
            "frame {round}"
        );
    }
    let mut lifecycle = 0u32;
    assert_eq!(unsafe { amc_archive_close(arch, 100, &mut lifecycle) }, 0);
    assert_eq!(lifecycle, AM_LIFECYCLE_CLOSED as u32);
    assert_eq!(crate::amc_object_destroy(arch.cast()), 0);
    let seg0 = std::fs::read(temp.path().join("seg0.mp4")).unwrap();
    let seg1 = std::fs::read(temp.path().join("seg1.mp4")).unwrap();
    let seg2 = std::fs::read(temp.path().join("seg2.mp4")).unwrap();
    assert!(!seg0.is_empty() && !seg1.is_empty() && !seg2.is_empty());
    // MP4 魔数：ftyp 盒在文件头。
    assert_eq!(&seg0[4..8], b"ftyp");
    assert_eq!(&seg2[4..8], b"ftyp");
    eprintln!(
        "[证据] 分段 25 帧 gop=10 → 3 文件：seg0={}B seg1={}B seg2={}B",
        seg0.len(),
        seg1.len(),
        seg2.len()
    );
    assert_eq!(unsafe { crate::amc_shutdown(ctx, 100, &mut lifecycle) }, 0);
    assert_eq!(crate::amc_object_destroy(ctx.cast()), 0);
}

#[test]
#[ignore = "显式设置 AM_RT_FFMPEG_DIR 指向 S5 GPL 运行库目录，真实 libx264 MP4 落盘"]
fn plain_archive_single_file_valid_mp4() {
    let directory = std::env::var("AM_RT_FFMPEG_DIR").unwrap();
    let directory = std::path::PathBuf::from(directory);
    let ctx = unsafe { init_with_runtime_dirs(&directory) };
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("plain.mp4");
    let path = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
    let cfg = unsafe { encoder_config(TEST_W, TEST_H, 8) };
    let mut arch = std::ptr::null_mut();
    assert_eq!(
        unsafe { amc_archive_open(ctx, path.as_ptr(), &cfg, 0, &mut arch) },
        0
    );
    let mut buffer = vec![0u8; (TEST_W * TEST_H * 4) as usize];
    fill_gradient(&mut buffer, TEST_W, TEST_H);
    let view = frame_view(&buffer, TEST_W, TEST_H);
    let pixel = pixel_view(&view);
    for round in 0..12 {
        assert_eq!(
            unsafe { amc_archive_write(arch, &pixel) },
            0,
            "frame {round}"
        );
    }
    let mut lifecycle = 0u32;
    assert_eq!(unsafe { amc_archive_close(arch, 100, &mut lifecycle) }, 0);
    assert_eq!(crate::amc_object_destroy(arch.cast()), 0);
    let bytes = std::fs::read(temp.path().join("plain.mp4")).unwrap();
    assert!(!bytes.is_empty());
    assert_eq!(&bytes[4..8], b"ftyp");
    eprintln!("[证据] 单文件 12 帧 gop=10 → plain.mp4={}B", bytes.len());
    assert_eq!(unsafe { crate::amc_shutdown(ctx, 100, &mut lifecycle) }, 0);
    assert_eq!(crate::amc_object_destroy(ctx.cast()), 0);
}

#[test]
#[ignore = "显式设置 AM_RT_FFMPEG_DIR 指向 S5 GPL 运行库目录，真实 libx264 编码"]
fn archive_open_rejects_missing_segment_template_without_runtime() {
    let directory = std::env::var("AM_RT_FFMPEG_DIR").unwrap();
    let directory = std::path::PathBuf::from(directory);
    let ctx = unsafe { init_with_runtime_dirs(&directory) };
    let temp = tempfile::tempdir().unwrap();
    let path = std::ffi::CString::new(temp.path().join("plain.mp4").to_str().unwrap()).unwrap();
    let cfg = unsafe { encoder_config(TEST_W, TEST_H, 8) };
    let mut arch = std::ptr::null_mut();
    assert_eq!(
        unsafe { amc_archive_open(ctx, path.as_ptr(), &cfg, 10, &mut arch) },
        AM_ERR_SDK_INVALID_ARGUMENT,
        "分段模式缺 %d 模板应拒绝"
    );
    let mut lifecycle = 0u32;
    assert_eq!(unsafe { crate::amc_shutdown(ctx, 100, &mut lifecycle) }, 0);
    assert_eq!(crate::amc_object_destroy(ctx.cast()), 0);
}
