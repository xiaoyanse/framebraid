//! 审计盲区回归：amc_stream_write 视图宽高必须与编码配置一致，不一致直接拒绝。
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

fn synthetic_object(width: u32, height: u32) -> Arc<StreamObject> {
    Arc::new(StreamObject {
        session: Mutex::new(None),
        state: AtomicU32::new(AM_LIFECYCLE_RUNNING as u32),
        width,
        height,
        stop: Arc::new(AtomicBool::new(false)),
        _permit: Mutex::new(None),
    })
}

#[test]
fn stream_write_rejects_view_dimensions_mismatching_encoder_config() {
    let ctx = open_camera_ctx();
    let mismatch = crate::domain().insert(ctx, synthetic_object(4, 4)).unwrap();
    let matching = crate::domain().insert(ctx, synthetic_object(8, 8)).unwrap();

    // 8x8 RGBA 合成视图：自洽且尺寸与 matching 配置一致
    let data = vec![0u8; 8 * 8 * 4];
    let view = am_frame_view_t {
        width: 8,
        height: 8,
        format: AM_PF_RGBA8 as u32,
        plane_count: 1,
        planes: [
            data.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
        ],
        strides: [8 * 4, 0, 0, 0],
        plane_size_bytes: [(8 * 8 * 4) as u64, 0, 0, 0],
        ..Default::default()
    };
    let pixel = am_pixel_view_t {
        magic: AM_PIXEL_VIEW_MAGIC,
        view: &view,
        ..Default::default()
    };

    // 空视图指针：NULL_ARGUMENT
    assert_eq!(
        unsafe { amc_stream_write(mismatch as *mut _, std::ptr::null()) },
        AM_ERR_SDK_NULL_ARGUMENT
    );
    // 视图宽高与编码配置不一致：直接拒绝，不得进入会话写入
    assert_eq!(
        unsafe { amc_stream_write(mismatch as *mut _, &pixel) },
        AM_ERR_SDK_INVALID_ARGUMENT,
        "视图自洽不等于可用：底层按创建配置尺寸读取，宽高不一致必须拒绝"
    );
    // 宽高一致：通过闸门；无会话按句柄已关闭收口，证明闸门放行且未触碰网络
    assert_eq!(
        unsafe { amc_stream_write(matching as *mut _, &pixel) },
        AM_ERR_SDK_HANDLE_CLOSED
    );

    for key in [mismatch, matching] {
        assert_eq!(
            crate::domain().close(key, 1000).unwrap(),
            AM_LIFECYCLE_CLOSED as u32
        );
        crate::domain().destroy(key).unwrap();
    }
    assert_eq!(
        crate::domain().close(ctx, 1000).unwrap(),
        AM_LIFECYCLE_CLOSED as u32
    );
    crate::domain().destroy(ctx).unwrap();
}
