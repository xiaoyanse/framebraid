//! 审计盲区回归：amc_frame_get_scaled 参数闸（method/宽高/句柄/格式）与合法缩放输出。
use super::*;

fn open_camera_ctx() -> usize {
    let params = amc_init_params_t {
        abi_version: AM_ABI_VERSION,
        ..Default::default()
    };
    // 并行测试共享进程域的上下文准入上限；名额被占满时等待其它测试释放
    for _ in 0..600 {
        match unsafe { am_sdk_support::context::initialize_camera(crate::domain(), &params) } {
            Ok(key) => return key,
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED) => {
                std::thread::sleep(std::time::Duration::from_millis(5))
            }
            Err(other) => panic!("camera ctx init: {other}"),
        }
    }
    panic!("camera ctx admission never freed")
}

fn synthetic_frame(
    state: &Arc<CameraState>,
    width: u32,
    height: u32,
    format: u32,
    bytes: usize,
) -> Result<Arc<Frame>> {
    let charge = state.charge(bytes as u64, false)?;
    Frame::with_format(
        vec![0xFF; bytes],
        width,
        height,
        format,
        am_frame_meta_t::default(),
        charge,
    )
}

#[test]
fn frame_get_scaled_rejects_invalid_params_and_scales_uniform_frame() {
    let ctx = open_camera_ctx();
    let state = camera_state(ctx).unwrap();
    let rgba = synthetic_frame(&state, 8, 8, AM_PF_RGBA8 as u32, 8 * 8 * 4).unwrap();
    let rgba_key = Arc::as_ptr(&rgba) as usize;
    crate::domain()
        .buffers
        .insert(rgba_key, rgba.clone())
        .unwrap();
    let nv12 = synthetic_frame(&state, 8, 8, AM_PF_NV12 as u32, 8 * 8 * 3 / 2).unwrap();
    let nv12_key = Arc::as_ptr(&nv12) as usize;
    crate::domain()
        .buffers
        .insert(nv12_key, nv12.clone())
        .unwrap();
    let mut out: *mut amc_frame_t = std::ptr::null_mut();
    fn scaled(frame: usize, w: u32, h: u32, method: u32, out: &mut *mut amc_frame_t) -> i32 {
        unsafe { scale::amc_frame_get_scaled(frame as *const _, w, h, method, out) }
    }

    // 参数闸：非法method、零/超限宽高、未知句柄、目标格式约束
    assert_eq!(
        scaled(rgba_key, 4, 4, 2, &mut out),
        AM_ERR_SDK_INVALID_ARGUMENT
    );
    assert_eq!(
        scaled(rgba_key, 0, 4, 0, &mut out),
        AM_ERR_SDK_LIMIT_OUT_OF_RANGE
    );
    assert_eq!(
        scaled(rgba_key, 4, 0, 0, &mut out),
        AM_ERR_SDK_LIMIT_OUT_OF_RANGE
    );
    assert_eq!(
        scaled(rgba_key, 8193, 4, 0, &mut out),
        AM_ERR_SDK_LIMIT_OUT_OF_RANGE
    );
    assert_eq!(
        scaled(rgba_key, 4, 8193, 0, &mut out),
        AM_ERR_SDK_LIMIT_OUT_OF_RANGE
    );
    assert_eq!(
        scaled(0x777_0000, 4, 4, 0, &mut out),
        AM_ERR_SDK_HANDLE_INVALID
    );
    assert_eq!(
        scaled(nv12_key, 3, 3, 0, &mut out),
        AM_ERR_CAMERA_PIXEL_FORMAT_UNSUPPORTED,
        "NV12目标宽高必须为偶数"
    );
    assert!(out.is_null(), "拒绝路径不得写出半成品句柄");

    // 合法缩放：8x8纯白RGBA -> 4x4，尺寸/格式/字节正确且内容保持均匀
    assert_eq!(scaled(rgba_key, 4, 4, 0, &mut out), AM_OK as i32);
    {
        let body = crate::domain().buffers.get(out as usize).unwrap();
        let value = body.as_any().downcast_ref::<Frame>().unwrap();
        assert_eq!(value.view.width, 4);
        assert_eq!(value.view.height, 4);
        assert_eq!(value.view.format, AM_PF_RGBA8 as u32);
        assert_eq!(value.view.plane_size_bytes[0], (4 * 4 * 4) as u64);
        assert!(
            value.data.iter().all(|b| *b == 0xFF),
            "均匀图缩放后必须保持均匀"
        );
    }
    assert_eq!(unsafe { amc_frame_release(out) }, AM_OK as i32);
    // NV12 偶数目标同样可缩放
    assert_eq!(scaled(nv12_key, 4, 4, 0, &mut out), AM_OK as i32);
    {
        let body = crate::domain().buffers.get(out as usize).unwrap();
        let value = body.as_any().downcast_ref::<Frame>().unwrap();
        assert_eq!((value.view.width, value.view.height), (4, 4));
        assert_eq!(value.view.format, AM_PF_NV12 as u32);
        assert_eq!(
            value.view.plane_size_bytes.iter().sum::<u64>(),
            (4 * 4 * 3 / 2) as u64
        );
    }
    assert_eq!(unsafe { amc_frame_release(out) }, AM_OK as i32);

    assert_eq!(
        unsafe { amc_frame_release(rgba_key as *mut _) },
        AM_OK as i32
    );
    assert_eq!(
        unsafe { amc_frame_release(nv12_key as *mut _) },
        AM_OK as i32
    );
    drop((rgba, nv12, state));
    assert_eq!(
        crate::domain().close(ctx, 1000).unwrap(),
        AM_LIFECYCLE_CLOSED as u32
    );
    crate::domain().destroy(ctx).unwrap();
}
