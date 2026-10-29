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
mod tests;
