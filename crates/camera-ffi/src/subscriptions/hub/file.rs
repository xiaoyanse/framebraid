//! FILE 源会话（P1-8 阶段 1）：一次打开顺序读完，无重连/凭据/重放。
//! 正常读完与中途失败都进入 `AM_SRC_EOF` 终态，fault_class 区分（NONE=正常读完，
//! DECODE/RESOURCE/NETWORK=异常收尾）；订阅队列保留待消费数据排空（PULL 排空后
//! 返回 `camera.streamEnded`），因此退出路径不调 `reset_subscribers`（那是流源
//! 重连的清队语义，会毁掉 FILE 的完整时间轴合同）。
use super::{Hub, SessionStamp};
use crate::{decoder::config, reader::BudgetedReader, sources::SharedSource};
use am_camera_core::{credentials::Secret, media::sdk_io::IoError};
use am_sdk_support::abi::*;
use std::{
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

pub(super) fn run(hub: &Arc<Hub>) {
    while let Some(source) = hub.source.upgrade() {
        if source.stop.load(std::sync::atomic::Ordering::Acquire) {
            break;
        }
        let revision = hub.revision.load(std::sync::atomic::Ordering::Acquire);
        let stamp = SessionStamp {
            revision,
            generation: source.generation.load(std::sync::atomic::Ordering::Acquire),
            credentials: source.credential_version(),
        };
        if !hub.current(&source, stamp) {
            // 配置换代（FILE 下罕见：无凭据变更）：停一拍再核，stop 优先退出。
            hub.pause(&source, revision, Duration::from_millis(100));
            continue;
        }
        let api = match config::runtime(&source.state) {
            Ok(api) => api,
            Err(code) => {
                source.runtime_unavailable(code);
                finish(
                    &source,
                    AM_FAULT_RESOURCE as u32,
                    c"camera.runtimeUnavailable",
                );
                return;
            }
        };
        let token = Arc::new(AtomicBool::new(false));
        if let Ok(mut slot) = hub.interrupt.lock() {
            *slot = Some(token.clone());
        }
        source.report(
            AM_SRC_CONNECTING as u32,
            AM_FAULT_NONE as u32,
            c"camera.connecting",
        );
        let secret = Secret {
            username: String::new(),
            password: String::new(),
        };
        let result = match BudgetedReader::open(
            source.state.clone(),
            api,
            &source.url,
            &secret,
            token,
            Duration::from_secs(5),
            true,
        ) {
            Ok(mut reader) => {
                if !hub.current(&source, stamp) {
                    continue;
                }
                hub.codec
                    .store(reader.info().codec, std::sync::atomic::Ordering::Release);
                source.report(
                    AM_SRC_LIVE as u32,
                    AM_FAULT_KEYFRAME_WAIT as u32,
                    c"camera.waitingKeyframe",
                );
                // 合同 §2.4 FILE 段：无订阅者时读取暂停——demux 抢跑会把包发给空订阅
                // 列表（短文件毫秒级读完尤其如此），首个订阅者注册前不进入读包循环。
                let mut waited = 0u32;
                while hub.list().iter().next().is_none() {
                    if source.stop.load(std::sync::atomic::Ordering::Acquire) {
                        return;
                    }
                    if !hub.current(&source, stamp) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                    waited += 1;
                }
                let _ = waited;
                if source.stop.load(std::sync::atomic::Ordering::Acquire) {
                    return;
                }
                hub.consume(&source, &mut reader, stamp)
            }
            // 打开失败（不存在/损坏/非容器）在 validate_file_path 已挡掉大部分；
            // demux 层失败为永久性（文件不会因等待变好），直接终态。
            Err(_) => Err(IoError::Invalid),
        };
        match result {
            // 正常读完（consume 对 FILE 源的 EOF 返回 Ok）。
            Ok(()) => {
                finish(&source, AM_FAULT_NONE as u32, c"camera.fileEnded");
                return;
            }
            Err(IoError::Closed) => {
                if source.stop.load(std::sync::atomic::Ordering::Acquire) {
                    return;
                }
                hub.pause(&source, revision, Duration::from_millis(100));
            }
            Err(error) => {
                let fault = match error {
                    IoError::Resource => AM_FAULT_RESOURCE,
                    IoError::Unsupported | IoError::Invalid => AM_FAULT_DECODE,
                    _ => AM_FAULT_NETWORK,
                } as u32;
                finish(&source, fault, c"camera.fileFailed");
                return;
            }
        }
    }
}

/// 终态收口：状态置 `AM_SRC_EOF` 并发状态变更事件；不关闭对象、不清订阅队列
/// （合同 §2.4 FILE 段：队列按序排空，PULL 排空后返回 streamEnded）。
fn finish(source: &Arc<SharedSource>, fault: u32, reason: &'static std::ffi::CStr) {
    source.report(AM_SRC_EOF as u32, fault, reason);
    source.event(
        AM_EVENT_STATE_CHANGED,
        AM_EVENT_SCOPE_SOURCE,
        0,
        fault,
        reason,
    );
}
