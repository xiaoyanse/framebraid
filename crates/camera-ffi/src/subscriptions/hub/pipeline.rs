//! 每会话一个有界像素worker；等待原生返回，不为卡住的调用创建替代线程。
use super::*;
use crate::frames::{ControlCharge, DecoderPermit};
use std::thread::{Scope, ScopedJoinHandle};
mod decode;
#[cfg(test)]
mod live_sampling_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod timing_tests;

#[cfg(test)]
#[derive(Default)]
pub(super) struct Block {
    entered: AtomicBool,
    released: Mutex<bool>,
    wake: Condvar,
}
#[cfg(test)]
impl Block {
    fn wait(&self) {
        self.entered.store(true, Ordering::Release);
        let mut released = self.released.lock().unwrap();
        while !*released {
            released = self.wake.wait(released).unwrap();
        }
    }
}

pub(super) struct Stream {
    pub info: StreamInfo,
    pub _charge: Charge,
}
pub(super) struct Configuration {
    pub stream: Arc<Stream>,
    pub extra: Vec<u8>,
    pub generation: u32,
    pub _charge: Charge,
}
pub(super) struct Job {
    pub packet: Arc<queue::Payload>,
    pub config: Arc<Configuration>,
    pub received: i64,
    pub epoch: u64,
}
#[derive(Default)]
struct Data {
    queue: VecDeque<Job>,
    bytes: u64,
    config: u32,
    waiting_key: bool,
}
pub(super) struct Pipeline {
    state: Arc<CameraState>,
    maximum: usize,
    max_bytes: u64,
    data: Mutex<Data>,
    wake: Condvar,
    stopping: AtomicBool,
    epoch: AtomicU64,
    _metadata: ControlCharge,
}
impl Pipeline {
    fn new(state: Arc<CameraState>) -> Result<Arc<Self>> {
        let context = state.ctx.data::<am_sdk_support::context::Context>()?;
        let policy = lock(&context.queues)?[AM_Q_ENCODED_STREAM as usize];
        let maximum = (policy.max_items as u64).min(SDK_ENCODED_QUEUE_ITEMS) as usize;
        let max_bytes = policy.max_bytes.min(SDK_ENCODED_QUEUE_BYTES);
        let metadata = state.control(
            std::mem::size_of::<Self>() + maximum * std::mem::size_of::<Job>(),
            false,
        )?;
        Ok(Arc::new(Self {
            state,
            maximum,
            max_bytes,
            data: Mutex::default(),
            wake: Condvar::new(),
            stopping: AtomicBool::new(false),
            epoch: AtomicU64::new(1),
            _metadata: metadata,
        }))
    }
    fn clear(&self, data: &mut Data) {
        let count = data.queue.len() as u64;
        self.state
            .metrics
            .decoder_queue_items
            .fetch_sub(count, Ordering::AcqRel);
        self.state
            .metrics
            .decoder_queue_bytes
            .fetch_sub(data.bytes, Ordering::AcqRel);
        self.state
            .metrics
            .dropped
            .fetch_add(count, Ordering::AcqRel);
        data.queue.clear();
        data.bytes = 0;
    }
    fn invalidate(&self, data: &mut Data) -> Result<()> {
        self.clear(data);
        data.waiting_key = true;
        self.epoch
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        Ok(())
    }
    /// 返回true表示参考链丢失；调用方只重置像素订阅，不清压缩消费者。
    fn submit(&self, mut job: Job) -> Result<bool> {
        let mut data = lock(&self.data)?;
        if self.stopping.load(Ordering::Acquire) {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        if data.config != job.config.generation {
            self.invalidate(&mut data)?;
            data.config = job.config.generation;
        }
        let bytes = job.packet.bytes.len() as u64;
        let overflow = bytes > self.max_bytes
            || data.queue.len() >= self.maximum
            || bytes > self.max_bytes.saturating_sub(data.bytes);
        if overflow {
            self.invalidate(&mut data)?;
            self.state.metrics.resyncs.fetch_add(1, Ordering::AcqRel);
        }
        let random =
            job.packet.abi.codec == AM_CODEC_MJPEG || job.packet.abi.flags & AM_PKT_KEYFRAME != 0;
        if bytes > self.max_bytes || self.maximum == 0 || data.waiting_key && !random {
            self.state.metrics.dropped.fetch_add(1, Ordering::AcqRel);
            return Ok(overflow);
        }
        data.waiting_key = false;
        job.epoch = self.epoch.load(Ordering::Acquire);
        data.bytes += bytes;
        data.queue.push_back(job);
        self.state
            .metrics
            .decoder_queue_items
            .fetch_add(1, Ordering::AcqRel);
        self.state
            .metrics
            .decoder_queue_bytes
            .fetch_add(bytes, Ordering::AcqRel);
        drop(data);
        self.wake.notify_one();
        Ok(overflow)
    }
    fn pop(&self) -> Result<Option<Job>> {
        let mut data = lock(&self.data)?;
        loop {
            if self.stopping.load(Ordering::Acquire) {
                return Ok(None);
            }
            if let Some(job) = data.queue.pop_front() {
                let bytes = job.packet.bytes.len() as u64;
                data.bytes -= bytes;
                self.state
                    .metrics
                    .decoder_queue_items
                    .fetch_sub(1, Ordering::AcqRel);
                self.state
                    .metrics
                    .decoder_queue_bytes
                    .fetch_sub(bytes, Ordering::AcqRel);
                return Ok(Some(job));
            }
            data = self
                .wake
                .wait(data)
                .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        }
    }
    pub(super) fn current(&self, epoch: u64) -> bool {
        !self.stopping.load(Ordering::Acquire) && self.epoch.load(Ordering::Acquire) == epoch
    }
    pub(super) fn has_newer_backlog(&self, epoch: u64) -> bool {
        self.current(epoch) && lock(&self.data).is_ok_and(|data| !data.queue.is_empty())
    }
    pub(super) fn wait_until(&self, epoch: u64, deadline: Instant) -> Result<bool> {
        if deadline.saturating_duration_since(Instant::now()) > Duration::from_millis(50) {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        loop {
            if !self.current(epoch) {
                return Ok(false);
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(true);
            }
            // Windows条件变量的毫秒超时可能跨系统tick；标准sleep使用可用的高精度计时器。
            // 分片只为重验取消，不自旋，也不持队列/配置/订阅锁。
            std::thread::sleep((deadline - now).min(Duration::from_millis(2)));
        }
    }
    fn resync(&self) -> Result<()> {
        let mut data = lock(&self.data)?;
        self.invalidate(&mut data)?;
        self.state.metrics.resyncs.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }
    fn stop(&self) {
        self.stopping.store(true, Ordering::Release);
        if let Ok(mut data) = self.data.lock() {
            self.clear(&mut data);
        }
        self.wake.notify_all();
    }
}

pub(super) struct Delivery<'a> {
    pub pipe: &'a Pipeline,
    pub epoch: u64,
    pub config: u32,
}
impl Delivery<'_> {
    pub fn current(&self, source: &SharedSource) -> bool {
        self.pipe.current(self.epoch)
            && source.config_generation.load(Ordering::Acquire) == self.config
    }
}

pub(super) struct Execution<'scope, 'env: 'scope> {
    scope: &'scope Scope<'scope, 'env>,
    hub: &'scope Hub,
    source: &'scope Arc<SharedSource>,
    stamp: SessionStamp,
    pipe: Option<Arc<Pipeline>>,
    handle: Option<ScopedJoinHandle<'scope, ()>>,
}
impl<'scope, 'env: 'scope> Execution<'scope, 'env> {
    pub fn new(
        scope: &'scope Scope<'scope, 'env>,
        hub: &'scope Hub,
        source: &'scope Arc<SharedSource>,
        stamp: SessionStamp,
    ) -> Self {
        Self {
            scope,
            hub,
            source,
            stamp,
            pipe: None,
            handle: None,
        }
    }
    pub fn stop(&mut self) {
        if let Some(pipe) = &self.pipe {
            pipe.stop();
        }
    }
    pub fn submit(&mut self, job: Job) -> Result<()> {
        if self.handle.as_ref().is_some_and(|h| h.is_finished()) {
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
            self.pipe.take();
        }
        if self
            .pipe
            .as_ref()
            .is_some_and(|p| p.stopping.load(Ordering::Acquire))
        {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        if self.pipe.is_none() {
            let permit = Arc::new(self.source.state.decoder()?);
            let pipe = Pipeline::new(self.source.state.clone())?;
            let worker = decode::Worker::new(self.hub, self.source, self.stamp, permit)?;
            let work = pipe.clone();
            self.handle = Some(
                std::thread::Builder::new()
                    .name("amc-source-pixels".into())
                    .spawn_scoped(self.scope, move || {
                        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            worker.run(&work)
                        }));
                        work.stop();
                    })
                    .map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)?,
            );
            self.pipe = Some(pipe);
        }
        let _configuration = lock(&self.hub.configuration)?;
        if job.config.generation != self.source.config_generation.load(Ordering::Acquire) {
            self.source
                .state
                .metrics
                .dropped
                .fetch_add(1, Ordering::AcqRel);
            return Ok(());
        }
        if self
            .pipe
            .as_ref()
            .ok_or(AM_ERR_SDK_INTERNAL_ERROR)?
            .submit(job)?
        {
            for sub in self
                .hub
                .list()
                .iter()
                .filter(|s| matches!(s.kind, Kind::Frames { .. }))
            {
                sub.reset();
            }
            self.source.event(
                AM_EVENT_STATE_CHANGED,
                AM_EVENT_SCOPE_ALL_CONSUMERS,
                0,
                AM_FAULT_RESOURCE as u32,
                c"camera.pixelQueueBackpressure",
            );
        }
        Ok(())
    }
}
impl Drop for Execution<'_, '_> {
    fn drop(&mut self) {
        self.stop();
    }
}
