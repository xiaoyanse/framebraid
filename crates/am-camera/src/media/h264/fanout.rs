//! 搬运分发原语（批 1.2）：per-client 有界队列（满清受损链→重同步→等关键帧
//! 续发）与 GOP 有界缓存（新客户端关键帧起发）。慢客户端语义对齐 WEB_0815。
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use tokio::sync::Notify;

use super::session::{H264Message, H264Packet, StreamConfig};
#[cfg(test)]
mod tests;

/// per-client 队列容量（满清受损链等关键帧）。按 GOP 定容：典型 GOP=30 帧（1s@30fps），
/// 容量≈1 个 GOP 让慢客户端在 1 秒量级触发重同步（AIeng 用 8 帧钉延迟，
/// H.264 不能任意跳帧故取 GOP 尺寸），而非旧值 256 帧积压数秒旧画面。
pub const CLIENT_QUEUE_MAX: usize = 36;
/// 每订阅载荷上限；消息元数据另受36项限制，不以包数代替字节限制。
pub const CLIENT_QUEUE_MAX_BYTES: usize = 8 * 1024 * 1024;

/// 连续溢出踢线：持续消费不动=死客户端（半开连接），AIeng 32 帧连丢即踢。
const CLIENT_STALL_KICK_AFTER: u32 = 2;

fn message_bytes(message: &H264Message) -> usize {
    match message {
        H264Message::Packet(p) => p.data.len(),
        H264Message::Overlay(s) | H264Message::Closed(_, s) => s.len(),
        H264Message::Config(_) | H264Message::ResyncRequired { .. } => 0,
    }
}
fn fits(queue: &VecDeque<H264Message>, message: &H264Message) -> bool {
    queue.len() < CLIENT_QUEUE_MAX
        && queue
            .iter()
            .fold(0usize, |n, m| n.saturating_add(message_bytes(m)))
            .saturating_add(message_bytes(message))
            <= CLIENT_QUEUE_MAX_BYTES
}

/// 客户端消息队列（有界语义在 ClientFan 投递侧实现）。
pub(crate) struct ClientQueue {
    queue: Mutex<VecDeque<H264Message>>,
    notify: Notify,
    closed: AtomicBool,
    stall_overflows: std::sync::atomic::AtomicU32,
    #[cfg(test)]
    before_wait: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl ClientQueue {
    fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            notify: Notify::new(),
            closed: AtomicBool::new(false),
            stall_overflows: std::sync::atomic::AtomicU32::new(0),
            #[cfg(test)]
            before_wait: Mutex::default(),
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

    fn poll_one(&self) -> (Option<H264Message>, bool) {
        let mut queue = self.lock();
        let message = queue.pop_front();
        if message.is_some() {
            // 消费在动：清除死客户端踢除累计
            self.stall_overflows.store(0, Ordering::Relaxed);
        }
        // 与close在同一队列锁内读终态，避免空检查后跳过刚入队的终态消息。
        (message, self.closed.load(Ordering::Acquire))
    }

    /// 客户端侧等待下一消息；关闭且排空后返回 None。
    pub async fn recv(&self) -> Option<H264Message> {
        loop {
            // 先登记等待再查状态；close的广播不能落在空队列检查与睡眠之间。
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let (message, closed) = self.poll_one();
            if let Some(message) = message {
                return Some(message);
            }
            if closed {
                return None;
            }
            #[cfg(test)]
            if let Some(hook) = self.before_wait.lock().unwrap().take() {
                hook();
            }
            notified.await;
        }
    }

    /// 关闭队列（终态消息入队并唤醒全部等待者）。
    pub(crate) fn close(&self, message: H264Message) {
        self.terminate(self.lock(), message);
    }
    fn terminate(&self, mut queue: MutexGuard<'_, VecDeque<H264Message>>, message: H264Message) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        queue.clear();
        let message = if message_bytes(&message) > CLIENT_QUEUE_MAX_BYTES {
            H264Message::Closed(1011, Arc::from("media.closeReasonTooLarge"))
        } else {
            message
        };
        queue.push_back(message);
        self.closed.store(true, Ordering::Release);
        drop(queue);
        self.notify.notify_waiters();
    }
}

/// 单客户端投递位：队列+重同步状态（满清受损链→信令一次→等关键帧续发）。
pub(crate) struct ClientFan {
    pub(crate) queue: Arc<ClientQueue>,
    await_keyframe: AtomicBool,
}

impl ClientFan {
    pub(crate) fn new() -> Self {
        Self {
            queue: Arc::new(ClientQueue::new()),
            await_keyframe: AtomicBool::new(false),
        }
    }

    /// 订阅种子：配置+GOP 缓存直入（容量内）；缓存缺失或超容量则等关键帧起流。
    pub(crate) fn seed(&self, config: &Arc<StreamConfig>, gop: Vec<Arc<H264Packet>>) {
        let mut queue = self.queue.lock();
        if self.queue.closed.load(Ordering::Acquire) {
            return;
        }
        queue.clear();
        queue.push_back(H264Message::Config(config.clone()));
        let bytes = gop
            .iter()
            .fold(0usize, |n, p| n.saturating_add(p.data.len()));
        if gop.is_empty()
            || gop.len() >= CLIENT_QUEUE_MAX
            || bytes > CLIENT_QUEUE_MAX_BYTES
            || !gop[0].keyframe
            || gop.iter().any(|p| p.generation != config.generation)
        {
            self.await_keyframe.store(true, Ordering::Relaxed);
        } else {
            self.await_keyframe.store(false, Ordering::Relaxed);
            for packet in gop {
                queue.push_back(H264Message::Packet(packet));
            }
        }
        drop(queue);
        self.queue.notify.notify_one();
    }

    /// 活跃包投递：溢出清受损链，保留当前配置并等新关键帧。
    pub(crate) fn deliver(&self, message: H264Message) {
        match message {
            H264Message::Config(config) => return self.deliver_config(config),
            H264Message::Overlay(json) => return self.deliver_overlay(json),
            H264Message::Closed(_, _) => return self.queue.close(message),
            _ => {}
        }
        let mut queue = self.queue.lock();
        if self.queue.closed.load(Ordering::Acquire) {
            return;
        }
        if let H264Message::Packet(packet) = &message {
            if self.await_keyframe.load(Ordering::Relaxed) && !packet.keyframe {
                return; // 丢后等关键帧
            }
            if packet.data.len() > CLIENT_QUEUE_MAX_BYTES {
                self.queue.terminate(
                    queue,
                    H264Message::Closed(1009, Arc::from("media.packetTooLarge")),
                );
                return;
            }
            if !fits(&queue, &message) {
                // 低优先级叠层不应挤掉仍可解码的视频参考链。
                queue.retain(|m| !matches!(m, H264Message::Overlay(_)));
            }
            if !fits(&queue, &message) {
                let stalls = self.queue.stall_overflows.fetch_add(1, Ordering::Relaxed) + 1;
                if stalls >= CLIENT_STALL_KICK_AFTER {
                    // 持队列锁原子发布终态，不能放锁后让生产者穿过关闭屏障。
                    self.queue.terminate(
                        queue,
                        H264Message::Closed(1011, Arc::from("media.clientStalled")),
                    );
                    return;
                }
                queue.retain(
                    |m| matches!(m, H264Message::Config(c) if c.generation == packet.generation),
                );
                queue.push_back(H264Message::ResyncRequired {
                    reason: super::protocol::RESYNC_SLOW_CLIENT,
                    generation: packet.generation,
                });
                self.await_keyframe.store(true, Ordering::Relaxed);
                if !packet.keyframe || !fits(&queue, &message) {
                    drop(queue);
                    self.queue.notify.notify_one();
                    return;
                }
            }
            if packet.keyframe {
                self.await_keyframe.store(false, Ordering::Relaxed);
            }
        }
        if fits(&queue, &message) {
            queue.push_back(message);
        }
        drop(queue);
        self.queue.notify.notify_one();
    }

    /// 新代次配置：清空旧代次残留（旧包不可混入新流），等新代次关键帧起流。
    pub(crate) fn deliver_config(&self, config: Arc<StreamConfig>) {
        let mut queue = self.queue.lock();
        if self.queue.closed.load(Ordering::Acquire) {
            return;
        }
        queue.clear();
        self.await_keyframe.store(true, Ordering::Relaxed);
        queue.push_back(H264Message::Config(config));
        drop(queue);
        self.queue.notify.notify_one();
    }

    /// 检测叠层帧投递（批③）：只保留最新一项；队列满即丢，
    /// 不挤占视频帧窗口、不触发重同步（无消费者语义=丢，不积压）。
    pub(crate) fn deliver_overlay(&self, json: Arc<str>) {
        let mut queue = self.queue.lock();
        if self.queue.closed.load(Ordering::Acquire) {
            return;
        }
        queue.retain(|m| !matches!(m, H264Message::Overlay(_)));
        let message = H264Message::Overlay(json);
        if !fits(&queue, &message) {
            return; // 队列满保视频帧：overlay 本 tick 丢弃
        }
        queue.push_back(message);
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
        if self.frames.len() >= CLIENT_QUEUE_MAX - 1
            || packet.data.len() > self.max_bytes.saturating_sub(self.bytes)
        {
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
