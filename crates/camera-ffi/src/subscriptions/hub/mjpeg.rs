use super::{sequence, Hub, SessionStamp};
use crate::sources::SharedSource;
use crate::subscriptions::{lock, pixels};
use am_camera_core::media::{
    mjpeg::MjpegStream,
    sdk_io::{decode_image, IoError, StreamInfo},
};
use am_sdk_support::{abi::*, clock::monotonic_ms};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

/// 建连与单次读超时（读超时同时承担黑洞流检出）。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// 帧级兜底上限；日常超时由 read_timeout 与解析器双超时先行触发。
const FRAME_DEADLINE: Duration = Duration::from_secs(30);
/// 连续解码失败达此值即断流重建（垃圾流不重连会烧 CPU）。
const DECODE_FAIL_LIMIT: u32 = 8;
/// 单帧像素上限，与 ISAPI 抓图一致。
const MAX_PIXELS: u64 = 16_777_216;

pub(super) fn run(hub: &Arc<Hub>) {
    let mut attempts = 0u32;
    let mut ever_connected = false;
    let mut previous_revision = hub.revision.load(Ordering::Acquire);
    loop {
        let Some(source) = hub.source.upgrade() else {
            break;
        };
        if source.stop.load(Ordering::Acquire) {
            break;
        }
        let revision = hub.revision.load(Ordering::Acquire);
        let mut stamp = SessionStamp {
            revision,
            generation: source.generation.load(Ordering::Acquire),
            credentials: source.credential_version(),
        };
        let secret = match source.secret() {
            Ok(secret) => secret,
            Err(_) => {
                hub.pause(&source, revision, Duration::from_millis(100));
                continue;
            }
        };
        if !hub.current(&source, stamp) {
            continue;
        }
        source.report(
            AM_SRC_CONNECTING as u32,
            AM_FAULT_NONE as u32,
            c"camera.connecting",
        );
        let opened = MjpegStream::open(&source.url, &secret, CONNECT_TIMEOUT);
        let result = match opened {
            Ok(mut stream) => {
                if !hub.current(&source, stamp) {
                    continue;
                }
                if ever_connected && revision == previous_revision {
                    let Some(next) = stamp.generation.checked_add(1) else {
                        break;
                    };
                    if source
                        .generation
                        .compare_exchange(
                            stamp.generation,
                            next,
                            Ordering::AcqRel,
                            Ordering::Acquire,
                        )
                        .is_err()
                    {
                        continue;
                    }
                    stamp.generation = next;
                    source
                        .state
                        .metrics
                        .reconnects
                        .fetch_add(1, Ordering::AcqRel);
                    source.event(
                        AM_EVENT_GENERATION_CHANGED,
                        AM_EVENT_SCOPE_ALL_CONSUMERS,
                        0,
                        AM_FAULT_NONE as u32,
                        c"camera.reconnected",
                    );
                }
                previous_revision = revision;
                ever_connected = true;
                attempts = 0;
                source.reconnect_attempts.store(0, Ordering::Release);
                source.report(AM_SRC_LIVE as u32, AM_FAULT_NONE as u32, c"camera.live");
                consume(&source, hub, &mut stream, stamp)
            }
            Err(error) => Err(error),
        };
        hub.reset_subscribers();
        if source.stop.load(Ordering::Acquire) {
            break;
        }
        if !hub.current(&source, stamp) {
            continue;
        }
        let error = result.err().unwrap_or(IoError::Network);
        if error == IoError::Auth {
            source.report(
                AM_SRC_AUTH_WAITING as u32,
                AM_FAULT_AUTH as u32,
                c"camera.authenticationRejected",
            );
            while !source.stop.load(Ordering::Acquire)
                && revision == hub.revision.load(Ordering::Acquire)
            {
                hub.pause(&source, revision, Duration::from_millis(100));
            }
            continue;
        }
        let fault = match error {
            IoError::Resource => AM_FAULT_RESOURCE,
            IoError::Unsupported | IoError::Invalid => AM_FAULT_DECODE,
            _ => AM_FAULT_NETWORK,
        };
        source.report(
            AM_SRC_RECONNECT_WAIT as u32,
            fault as u32,
            c"camera.streamInterrupted",
        );
        attempts = attempts.saturating_add(1);
        source.reconnect_attempts.store(attempts, Ordering::Release);
        let Ok(policy) = lock(&source.reconnect) else {
            break;
        };
        if policy.max_attempts != 0 && attempts >= policy.max_attempts {
            while !source.stop.load(Ordering::Acquire)
                && revision == hub.revision.load(Ordering::Acquire)
            {
                hub.pause(&source, revision, Duration::from_millis(100));
            }
            attempts = 0;
            continue;
        }
        let backoff = (policy.initial_backoff_ms as u64)
            .saturating_mul(1u64 << attempts.saturating_sub(1).min(16))
            .min(policy.max_backoff_ms as u64);
        // 抖动按源身份与重试号计算，避免同机所有相机同时重连。
        let jitter = if policy.jitter_ms == 0 {
            0
        } else {
            source
                .source_id
                .wrapping_mul(6364136223846793005)
                .wrapping_add(attempts as u64)
                % (policy.jitter_ms as u64 + 1)
        };
        hub.pause(
            &source,
            revision,
            Duration::from_millis(backoff.saturating_add(jitter).min(60_000)),
        );
    }
    hub.reset_subscribers();
}

fn consume(
    source: &Arc<SharedSource>,
    hub: &Arc<Hub>,
    stream: &mut MjpegStream,
    stamp: SessionStamp,
) -> std::result::Result<(), IoError> {
    // MJPEG 无时间戳与编码参数；time_base 仅填充元数据（pts 恒为 i64::MIN）。
    let info = StreamInfo {
        codec: 0,
        width: 0,
        height: 0,
        fps: 0.0,
        time_base_num: 1,
        time_base_den: 1000,
        extradata: Vec::new(),
    };
    let mut decode_failures = 0u32;
    loop {
        if !hub.current(source, stamp) {
            return Err(IoError::Closed);
        }
        let frame = match stream.next_frame(FRAME_DEADLINE) {
            Ok(Some(frame)) => frame,
            // close 分隔符=服务器收尾；按断连处理交外层重连。
            Ok(None) => return Err(IoError::Network),
            Err(error) => return Err(error),
        };
        let received = monotonic_ms();
        if !hub.current(source, stamp) {
            return Err(IoError::Closed);
        }
        let image = match decode_image(frame, MAX_PIXELS, true) {
            Ok(image) => {
                decode_failures = 0;
                image
            }
            Err(_) => {
                decode_failures += 1;
                source
                    .state
                    .metrics
                    .frames_dropped
                    .fetch_add(1, Ordering::AcqRel);
                if decode_failures >= DECODE_FAIL_LIMIT {
                    return Err(IoError::Unsupported);
                }
                continue;
            }
        };
        let Some(rgba) = image.rgba else {
            decode_failures += 1;
            source
                .state
                .metrics
                .frames_dropped
                .fetch_add(1, Ordering::AcqRel);
            if decode_failures >= DECODE_FAIL_LIMIT {
                return Err(IoError::Unsupported);
            }
            continue;
        };
        hub.observe_dimensions(source, image.width, image.height);
        let seq = sequence(&hub.frame_sequence).map_err(|_| IoError::Resource)?;
        let subs = hub.list();
        pixels::fanout(
            source,
            &subs,
            rgba,
            image.width,
            image.height,
            i64::MIN,
            &info,
            received,
            seq,
            stamp.generation,
            source.config_generation.load(Ordering::Acquire),
            hub.now(),
        )
        .map_err(|_| IoError::Resource)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{domain, frames::Frame, sources::api::amc_source_open};
    use am_sdk_support::context::initialize_camera;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Barrier;

    fn sample_jpeg(width: u32, height: u32) -> Vec<u8> {
        let image = image::DynamicImage::new_rgb8(width, height);
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
        let ctx = unsafe { initialize_camera(domain(), &params) }.unwrap();
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
}
