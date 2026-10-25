//! 搬运分发原语（批 1.2）：per-client 有界队列（满丢旧→重同步信令→等关键帧
//! 续发）与 GOP 有界缓存（新客户端关键帧起发）。慢客户端语义对齐 WEB_0815。
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::Notify;

use super::session::{H264Message, H264Packet, StreamConfig};

/// per-client 队列容量（满丢旧保新）。按 GOP 定容：典型 GOP=30 帧（1s@30fps），
/// 容量≈1 个 GOP 让慢客户端在 1 秒量级触发重同步（AIeng 用 8 帧钉延迟，
/// H.264 不能任意跳帧故取 GOP 尺寸），而非旧值 256 帧积压数秒旧画面。
pub const CLIENT_QUEUE_MAX: usize = 36;

/// 连续溢出踢线：持续消费不动=死客户端（半开连接），AIeng 32 帧连丢即踢。
const CLIENT_STALL_KICK_AFTER: u32 = 2;

/// 客户端消息队列（有界语义在 ClientFan 投递侧实现）。
pub(crate) struct ClientQueue {
    queue: Mutex<VecDeque<H264Message>>,
    notify: Notify,
    closed: AtomicBool,
    stall_overflows: std::sync::atomic::AtomicU32,
}

impl ClientQueue {
    fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            notify: Notify::new(),
            closed: AtomicBool::new(false),
            stall_overflows: std::sync::atomic::AtomicU32::new(0),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, VecDeque<H264Message>> {
        self.queue.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// 测试锚点：窥视队列内容（断言重同步语义用）。
    #[cfg(test)]
    pub(crate) fn lock_for_test(&self) -> std::sync::MutexGuard<'_, VecDeque<H264Message>> {
        self.lock()
    }

    fn poll_one(&self) -> Option<H264Message> {
        let message = self.lock().pop_front();
        if message.is_some() {
            // 消费在动：清除死客户端踢除累计
            self.stall_overflows.store(0, Ordering::Relaxed);
        }
        if message.is_none() && self.closed.load(Ordering::Acquire) {
            return None;
        }
        message
    }

    /// 客户端侧等待下一消息；关闭且排空后返回 None。
    pub async fn recv(&self) -> Option<H264Message> {
        loop {
            if let Some(message) = self.poll_one() {
                return Some(message);
            }
            if self.closed.load(Ordering::Acquire) {
                return None;
            }
            self.notify.notified().await;
        }
    }

    /// 关闭队列（终态消息入队并唤醒全部等待者）。
    pub(crate) fn close(&self, message: H264Message) {
        self.lock().push_back(message);
        self.closed.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }
}

/// 单客户端投递位：队列+重同步状态（满丢旧→信令一次→等关键帧续发）。
pub(crate) struct ClientFan {
    pub(crate) queue: Arc<ClientQueue>,
    await_keyframe: AtomicBool,
    resync_sent: AtomicBool,
}

impl ClientFan {
    pub(crate) fn new() -> Self {
        Self {
            queue: Arc::new(ClientQueue::new()),
            await_keyframe: AtomicBool::new(false),
            resync_sent: AtomicBool::new(false),
        }
    }

    /// 订阅种子：配置+GOP 缓存直入（容量内）；缓存缺失或超容量则等关键帧起流。
    pub(crate) fn seed(&self, config: &Arc<StreamConfig>, gop: Vec<Arc<H264Packet>>) {
        let mut queue = self.queue.lock();
        queue.push_back(H264Message::Config(config.clone()));
        if gop.is_empty() || gop.len() > CLIENT_QUEUE_MAX {
            self.await_keyframe.store(true, Ordering::Relaxed);
        } else {
            for packet in gop {
                queue.push_back(H264Message::Packet(packet));
            }
        }
        drop(queue);
        self.queue.notify.notify_one();
    }

    /// 活跃包投递：满丢旧保新；溢出即重同步信令（一次）并等关键帧续发。
    pub(crate) fn deliver(&self, message: H264Message) {
        let mut queue = self.queue.lock();
        if let H264Message::Packet(packet) = &message {
            if self.await_keyframe.load(Ordering::Relaxed) && !packet.keyframe {
                return; // 丢后等关键帧
            }
            if queue.len() >= CLIENT_QUEUE_MAX {
                queue.pop_front();
                if !self.resync_sent.load(Ordering::Relaxed) {
                    let generation = packet.generation;
                    queue.push_back(H264Message::ResyncRequired {
                        reason: super::protocol::RESYNC_SLOW_CLIENT,
                        generation,
                    });
                    self.resync_sent.store(true, Ordering::Relaxed);
                }
                if !packet.keyframe {
                    self.await_keyframe.store(true, Ordering::Relaxed);
                    return;
                }
                // 关键帧触发溢出：消费侧持续不动（每帧都被顶掉）——连续满
                // CLIENT_STALL_KICK_AFTER 个 GOP 仍未消费即判死客户端，主动踢除
                let stalls = self.queue.stall_overflows.fetch_add(1, Ordering::Relaxed) + 1;
                if stalls >= CLIENT_STALL_KICK_AFTER {
                    let message: Arc<str> = Arc::from("media.clientStalled");
                    drop(queue);
                    self.queue.close(H264Message::Closed(1011, message));
                    return;
                }
            }
            if packet.keyframe {
                // 关键帧即恢复点：清除等待与信令去重标记
                self.await_keyframe.store(false, Ordering::Relaxed);
                self.resync_sent.store(false, Ordering::Relaxed);
            }
        }
        queue.push_back(message);
        drop(queue);
        self.queue.notify.notify_one();
    }

    /// 新代次配置：清空旧代次残留（旧包不可混入新流），等新代次关键帧起流。
    pub(crate) fn deliver_config(&self, config: Arc<StreamConfig>) {
        let mut queue = self.queue.lock();
        queue.clear();
        self.await_keyframe.store(true, Ordering::Relaxed);
        self.resync_sent.store(false, Ordering::Relaxed);
        queue.push_back(H264Message::Config(config));
        drop(queue);
        self.queue.notify.notify_one();
    }

    /// 检测叠层帧投递（批③）：低频 JSON 直入队尾；队列满即丢，
    /// 不挤占视频帧窗口、不触发重同步（无消费者语义=丢，不积压）。
    pub(crate) fn deliver_overlay(&self, json: Arc<str>) {
        let mut queue = self.queue.lock();
        if queue.len() >= CLIENT_QUEUE_MAX {
            return; // 队列满保视频帧：overlay 本 tick 丢弃
        }
        queue.push_back(H264Message::Overlay(json));
        drop(queue);
        self.queue.notify.notify_one();
    }
}

/// GOP 缓存：自最近关键帧起的有界帧序列（字节上限，超限整组弃置）。
pub(crate) struct GopCache {
    frames: VecDeque<Arc<H264Packet>>,
    bytes: usize,
    max_bytes: usize,
}

impl GopCache {
    pub(crate) fn new(max_bytes: usize) -> Self {
        Self {
            frames: VecDeque::new(),
            bytes: 0,
            max_bytes,
        }
    }

    /// 关键帧开启新组；非头帧仅在组内追加；超字节上限整组弃置。
    pub(crate) fn push(&mut self, packet: &Arc<H264Packet>) {
        if packet.keyframe {
            self.frames.clear();
            self.bytes = 0;
        }
        if self.frames.is_empty() && !packet.keyframe {
            return; // 无关键帧起点不缓存
        }
        if self.bytes + packet.data.len() > self.max_bytes {
            self.frames.clear();
            self.bytes = 0;
            return;
        }
        self.bytes += packet.data.len();
        self.frames.push_back(packet.clone());
    }

    pub(crate) fn snapshot(&self) -> Vec<Arc<H264Packet>> {
        self.frames.iter().cloned().collect()
    }

    pub(crate) fn clear(&mut self) {
        self.frames.clear();
        self.bytes = 0;
    }
}
