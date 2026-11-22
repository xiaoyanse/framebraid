use super::*;
use crate::{domain, frames::Frame, sources::api::amc_source_open};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Barrier;

/// 不设 FB_AI_PHOTOS 时纯色合成（无夹具环境可跑）；设置后读 AI 夹具
/// raw/face2-1.png 经 image crate 缩放至请求尺寸内存转 JPEG——解码帧尺寸
/// 语义不变（断言不动），仅像素内容由纯色换为真实照片纹理。
fn sample_jpeg(width: u32, height: u32) -> Vec<u8> {
    let image = match std::env::var("FB_AI_PHOTOS").ok() {
        Some(dir) => {
            let photo = image::open(format!("{dir}\\raw\\face2-1.png"))
                .expect("FB_AI_PHOTOS 夹具 raw/face2-1.png 应可读")
                .resize_exact(width, height, image::imageops::FilterType::Triangle);
            image::DynamicImage::from(photo.to_rgb8())
        }
        None => image::DynamicImage::new_rgb8(width, height),
    };
    let mut cursor = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut cursor, image::ImageFormat::Jpeg)
        .unwrap();
    cursor.into_inner()
}

/// 吐固定两帧的最小 multipart 服务器；Barrier 保证写响应晚于订阅登记；
/// 不写 close 分隔符并保持连接（断连会清空未取走帧，属合同语义）；
/// 测试收帧并关源后放行再退出。
fn multipart_server(port: u16, jpeg: Vec<u8>, ready: Arc<Barrier>, done: Arc<Barrier>) {
    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).unwrap();
    let (mut stream, _) = listener.accept().unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut request = Vec::new();
    while !request.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap(), 1);
        request.push(byte[0]);
        assert!(request.len() < 16 * 1024);
    }
    ready.wait();
    let mut response =
        b"HTTP/1.1 200 OK\r\nContent-Type: multipart/x-mixed-replace; boundary=frame\r\nConnection: close\r\n\r\n".to_vec();
    // 三帧：前两帧的交付依赖帧后分隔符到达（流式分帧语义）。
    for _ in 0..3 {
        response.extend(b"--frame\r\nContent-Type: image/jpeg\r\nContent-Length: ");
        response.extend(jpeg.len().to_string().as_bytes());
        response.extend(b"\r\n\r\n");
        response.extend_from_slice(&jpeg);
        response.extend(b"\r\n");
    }
    stream.write_all(&response).unwrap();
    done.wait();
}

#[test]
fn mjpeg_stream_source_subscribes_frames_and_closes_cleanly() {
    let jpeg = sample_jpeg(4, 3);
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    drop(TcpListener::bind(format!("127.0.0.1:{port}")));
    let ready = Arc::new(Barrier::new(2));
    let done = Arc::new(Barrier::new(2));
    let server_ready = ready.clone();
    let server_done = done.clone();
    let server_jpeg = jpeg.clone();
    let server = std::thread::spawn(move || {
        multipart_server(port, server_jpeg, server_ready, server_done);
    });
    let params = amc_init_params_t {
        abi_version: AM_ABI_VERSION,
        ..Default::default()
    };
    let ctx = crate::open_shared_camera_ctx(&params);
    let url = std::ffi::CString::new(format!("http://127.0.0.1:{port}/video")).unwrap();
    let desc = am_source_desc_t {
        kind: AM_SRC_MJPEG_URL as u32,
        url_utf8: url.as_ptr(),
        ..Default::default()
    };
    let mut source = std::ptr::null_mut();
    assert_eq!(unsafe { amc_source_open(ctx as _, &desc, &mut source) }, 0);
    let creds = am_credential_data_t {
        username_utf8: c"sdk-user".as_ptr(),
        password_utf8: c"sdk-local-test-only".as_ptr(),
        ..Default::default()
    };
    assert_eq!(
        unsafe { crate::sources::api::amc_credentials_set(source, &creds) },
        0
    );
    let sub_desc = am_frame_sub_desc_t {
        pixel_format: AM_PF_RGBA8 as u32,
        mode: AM_SUB_PULL,
        ..Default::default()
    };
    let mut sub = std::ptr::null_mut();
    assert_eq!(
        unsafe { crate::subscriptions::amc_subscribe_frames(source, &sub_desc, &mut sub) },
        0
    );
    ready.wait();
    let mut received = 0;
    for _ in 0..2 {
        let mut frame = std::ptr::null_mut();
        assert_eq!(
            unsafe { crate::subscriptions::amc_frame_pull(sub, 5000, &mut frame) },
            0
        );
        let stored = domain().buffers.get(frame as usize).unwrap();
        let view = stored
            .as_any()
            .downcast_ref::<Frame>()
            .unwrap()
            .view
            .as_ref();
        assert_eq!((view.width, view.height), (4, 3));
        assert_eq!(view.format, AM_PF_RGBA8 as u32);
        assert_eq!(view.plane_size_bytes[0], 48);
        received += 1;
        assert_eq!(unsafe { crate::frames::amc_frame_release(frame) }, 0);
    }
    assert_eq!(received, 2);
    let mut status = am_source_status_t::default();
    assert_eq!(
        unsafe { crate::sources::api::amc_source_get_status(source, &mut status) },
        0
    );
    assert_eq!(status.state, AM_SRC_LIVE);
    let mut lifecycle = 0;
    assert_eq!(
        unsafe { crate::subscriptions::amc_frame_unsubscribe(sub, 5000, &mut lifecycle) },
        0
    );
    done.wait();
    assert_eq!(
        unsafe { crate::sources::api::amc_source_close(source, 5000, &mut lifecycle) },
        0
    );
    assert_eq!(lifecycle, AM_LIFECYCLE_CLOSED as u32);
    domain().destroy(sub as usize).unwrap();
    domain().destroy(source as usize).unwrap();
    domain().close(ctx, 2000).unwrap();
    domain().destroy(ctx).unwrap();
    server.join().unwrap();
}

fn multipart_response(jpeg: &[u8], frame_count: usize) -> Vec<u8> {
    let mut response =
        b"HTTP/1.1 200 OK\r\nContent-Type: multipart/x-mixed-replace; boundary=frame\r\nConnection: close\r\n\r\n".to_vec();
    for _ in 0..frame_count {
        response.extend(b"--frame\r\nContent-Type: image/jpeg\r\nContent-Length: ");
        response.extend(jpeg.len().to_string().as_bytes());
        response.extend(b"\r\n\r\n");
        response.extend_from_slice(jpeg);
        response.extend(b"\r\n");
    }
    response
}

/// 连接 1 送两帧后硬断（无 close 分隔符=断流）；连接 2 续供帧并保持到 done；
/// ready 保证首拍响应晚于订阅登记。
fn reconnect_server(port: u16, jpeg: Vec<u8>, ready: Arc<Barrier>, done: Arc<Barrier>) {
    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).unwrap();
    let read_headers = |socket: &mut TcpStream| {
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            assert_eq!(socket.read(&mut byte).unwrap(), 1);
            request.push(byte[0]);
            assert!(request.len() < 16 * 1024);
        }
    };
    {
        let (mut socket, _) = listener.accept().unwrap();
        read_headers(&mut socket);
        ready.wait();
        socket.write_all(&multipart_response(&jpeg, 2)).unwrap();
    }
    {
        let (mut socket, _) = listener.accept().unwrap();
        read_headers(&mut socket);
        socket.write_all(&multipart_response(&jpeg, 3)).unwrap();
        done.wait();
    }
}

#[test]
fn mjpeg_stream_reconnects_after_connection_drop() {
    let jpeg = sample_jpeg(4, 3);
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    drop(TcpListener::bind(format!("127.0.0.1:{port}")));
    let ready = Arc::new(Barrier::new(2));
    let done = Arc::new(Barrier::new(2));
    let server_ready = ready.clone();
    let server_done = done.clone();
    let server_jpeg = jpeg.clone();
    let server = std::thread::spawn(move || {
        reconnect_server(port, server_jpeg, server_ready, server_done);
    });
    let params = amc_init_params_t {
        abi_version: AM_ABI_VERSION,
        ..Default::default()
    };
    let ctx = crate::open_shared_camera_ctx(&params);
    let url = std::ffi::CString::new(format!("http://127.0.0.1:{port}/video")).unwrap();
    let desc = am_source_desc_t {
        kind: AM_SRC_MJPEG_URL as u32,
        url_utf8: url.as_ptr(),
        ..Default::default()
    };
    let mut source = std::ptr::null_mut();
    assert_eq!(unsafe { amc_source_open(ctx as _, &desc, &mut source) }, 0);
    let creds = am_credential_data_t {
        username_utf8: c"sdk-user".as_ptr(),
        password_utf8: c"sdk-local-test-only".as_ptr(),
        ..Default::default()
    };
    assert_eq!(
        unsafe { crate::sources::api::amc_credentials_set(source, &creds) },
        0
    );
    let sub_desc = am_frame_sub_desc_t {
        pixel_format: AM_PF_RGBA8 as u32,
        mode: AM_SUB_PULL,
        ..Default::default()
    };
    let mut sub = std::ptr::null_mut();
    assert_eq!(
        unsafe { crate::subscriptions::amc_subscribe_frames(source, &sub_desc, &mut sub) },
        0
    );
    ready.wait();
    // 断流前仅首帧可交付：末帧数据无后续分隔符随断连丢弃（流式分帧合同），
    // 后两帧来自重连后的连接；generation 递增为重连实证。
    for _ in 0..3 {
        let mut frame = std::ptr::null_mut();
        // 重连退避阶梯（1s/2s/4s…）累计可超单次 pull 截止；队列 wait_deadline_ms
        // 默认 5000 且合同只可收紧，故按耐心消费者模型在截止超时上重试（3×5s 包络）
        let mut rc = -1;
        for _ in 0..3 {
            rc = unsafe { crate::subscriptions::amc_frame_pull(sub, 5000, &mut frame) };
            if rc != AM_ERR_SDK_DEADLINE_EXCEEDED {
                break;
            }
        }
        assert_eq!(rc, 0);
        let stored = domain().buffers.get(frame as usize).unwrap();
        let view = stored
            .as_any()
            .downcast_ref::<Frame>()
            .unwrap()
            .view
            .as_ref();
        assert_eq!((view.width, view.height), (4, 3));
        assert_eq!(view.format, AM_PF_RGBA8 as u32);
        assert_eq!(view.plane_size_bytes[0], 48);
        assert_eq!(unsafe { crate::frames::amc_frame_release(frame) }, 0);
    }
    let mut status = am_source_status_t::default();
    assert_eq!(
        unsafe { crate::sources::api::amc_source_get_status(source, &mut status) },
        0
    );
    assert_eq!(status.state, AM_SRC_LIVE);
    assert!(status.generation >= 1);
    let mut lifecycle = 0;
    assert_eq!(
        unsafe { crate::subscriptions::amc_frame_unsubscribe(sub, 5000, &mut lifecycle) },
        0
    );
    done.wait();
    assert_eq!(
        unsafe { crate::sources::api::amc_source_close(source, 5000, &mut lifecycle) },
        0
    );
    assert_eq!(lifecycle, AM_LIFECYCLE_CLOSED as u32);
    domain().destroy(sub as usize).unwrap();
    domain().destroy(source as usize).unwrap();
    domain().close(ctx, 2000).unwrap();
    domain().destroy(ctx).unwrap();
    server.join().unwrap();
}
