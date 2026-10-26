use super::*;
use crate::frames::ControlCharge;
use am_sdk_support::{buffers::Buffer, callbacks, handles::Managed};
use std::{any::Any, thread::JoinHandle};

pub struct Payload {
    pub abi: am_encoded_packet_t,
    pub bytes: Vec<u8>,
    pub _charge: Charge,
}
#[cfg(test)]
mod tests;
unsafe impl Send for Payload {}
unsafe impl Sync for Payload {}
pub struct Packet {
    pub abi: am_encoded_packet_t,
    _payload: Arc<Payload>,
    _external: Charge,
}
unsafe impl Send for Packet {}
unsafe impl Sync for Packet {}
impl Buffer for Packet {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
pub enum Item {
    Encoded(Arc<Payload>),
    Frame(Arc<Frame>),
}
impl Item {
    pub fn bytes(&self) -> u64 {
        match self {
            Self::Encoded(p) => p.bytes.len() as u64,
            Self::Frame(f) => f.data.len() as u64,
        }
    }
    pub fn transfer(self, state: &Arc<CameraState>) -> Result<usize> {
        match self {
            Self::Frame(f) => {
                let key = Arc::as_ptr(&f) as usize;
                domain().buffers.transfer(key, f)?;
                Ok(key)
            }
            Self::Encoded(p) => {
                let external = state.charge(p.bytes.len() as u64, true)?;
                let mut abi = p.abi;
                abi.data = p.bytes.as_ptr();
                abi.size = p.bytes.len() as u64;
                let body = Arc::new(Packet {
                    abi,
                    _payload: p,
                    _external: external,
                });
                let key = &body.abi as *const am_encoded_packet_t as usize;
                domain().buffers.insert(key, body)?;
                Ok(key)
            }
        }
    }
}
#[derive(Clone)]
pub enum Kind {
    Encoded {
        mask: u32,
    },
    Frames {
        format: u32,
        fps: u32,
        presets: Vec<(u32, u32)>,
    },
}
#[derive(Clone, Copy)]
pub enum Callback {
    Encoded(unsafe extern "C" fn(*mut amc_encoded_sub_t, *mut am_encoded_packet_t, *mut c_void)),
    Frame(unsafe extern "C" fn(*mut amc_frame_sub_t, *mut amc_frame_t, *mut c_void)),
}
#[derive(Default)]
struct Data {
    queue: VecDeque<Item>,
    bytes: u64,
    callback: Option<Callback>,
    user: usize,
    busy: bool,
    stopped: bool,
    changing: bool,
    waiting_key: bool,
    generation: u32,
    last_frame: Option<Instant>,
    #[cfg(test)]
    selections: VecDeque<Instant>,
    waiters: u32,
}
pub struct Subscription {
    pub key: AtomicUsize,
    pub kind: Kind,
    pub mode: u32,
    pub state: Arc<CameraState>,
    pub hub: Weak<Hub>,
    maximum: usize,
    max_bytes: u64,
    max_waiters: u32,
    deadline_ms: u32,
    data: Mutex<Data>,
    wake: Condvar,
    alive: AtomicBool,
    handle: Mutex<Option<JoinHandle<()>>>,
    _metadata: ControlCharge,
}
impl Managed for Subscription {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn request_close(&self) {
        if let Ok(mut d) = self.data.lock() {
            d.stopped = true;
            d.callback = None;
            d.queue.clear();
            d.bytes = 0;
        }
        self.wake.notify_all();
    }
    fn drained(&self) -> bool {
        !self.alive.load(Ordering::Acquire)
    }
    fn close_resources(&self) {
        if let Ok(mut handle) = self.handle.lock() {
            if let Some(thread) = handle.take() {
                let _ = thread.join();
            }
        }
        if let Some(hub) = self.hub.upgrade() {
            hub.changed.notify_all();
        }
    }
}
impl Subscription {
    pub fn new(
        state: Arc<CameraState>,
        hub: &Arc<Hub>,
        kind: Kind,
        mode: u32,
        items: u32,
        bytes: u64,
    ) -> Result<Arc<Self>> {
        if mode > AM_SUB_PULL {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        let encoded = matches!(kind, Kind::Encoded { .. });
        let ctx = state.ctx.data::<am_sdk_support::context::Context>()?;
        let policy = lock(&ctx.queues)?[if encoded {
            AM_Q_ENCODED_STREAM
        } else {
            AM_Q_REALTIME_FRAMES
        } as usize];
        let default_items = if encoded {
            SDK_ENCODED_QUEUE_ITEMS
        } else if mode == AM_SUB_PUSH {
            SDK_FRAME_PUSH_ITEMS
        } else {
            SDK_FRAME_PULL_ITEMS
        };
        let maximum = if items == 0 {
            default_items.min(policy.max_items as u64) as u32
        } else {
            items
        };
        let max_bytes = if bytes == 0 { policy.max_bytes } else { bytes };
        if maximum == 0
            || max_bytes == 0
            || maximum > policy.max_items
            || max_bytes > policy.max_bytes
        {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        let presets = match &kind {
            Kind::Frames { presets, .. } => presets.capacity() * size_of::<(u32, u32)>(),
            _ => 0,
        };
        let metadata = state.control(
            size_of::<Self>() + maximum as usize * size_of::<Item>() + presets,
            false,
        )?;
        // 固定引用槽预留一次，控制计费覆盖整个订阅寿命。
        let mut queue = VecDeque::new();
        queue
            .try_reserve_exact(maximum as usize)
            .map_err(|_| AM_ERR_SDK_OUT_OF_MEMORY)?;
        Ok(Arc::new(Self {
            key: AtomicUsize::new(0),
            kind,
            mode,
            state,
            hub: Arc::downgrade(hub),
            maximum: maximum as usize,
            max_bytes,
            max_waiters: policy.max_waiters,
            deadline_ms: policy.wait_deadline_ms,
            data: Mutex::new(Data {
                queue,
                waiting_key: encoded,
                ..Default::default()
            }),
            wake: Condvar::new(),
            alive: AtomicBool::new(false),
            handle: Mutex::default(),
            _metadata: metadata,
        }))
    }
    pub fn stopped(&self) -> bool {
        lock(&self.data).map_or(true, |d| d.stopped)
    }
    pub fn occupies_slot(&self) -> bool {
        !self.stopped() || self.alive.load(Ordering::Acquire)
    }
    pub fn wants_frames(&self) -> bool {
        matches!(self.kind, Kind::Frames { .. })
            && lock(&self.data)
                .is_ok_and(|d| !d.stopped && (self.mode == AM_SUB_PULL || d.callback.is_some()))
    }
    pub fn queue_usage(&self) -> (u64, u64) {
        lock(&self.data).map_or((0, 0), |d| (d.queue.len() as u64, d.bytes))
    }
    pub fn reset(&self) {
        if let Ok(mut d) = self.data.lock() {
            d.queue.clear();
            d.bytes = 0;
            d.waiting_key = matches!(self.kind, Kind::Encoded { .. });
            d.last_frame = None;
            #[cfg(test)]
            d.selections.clear();
        }
        self.wake.notify_all();
    }
    fn next_at(&self, data: &Data, now: Instant) -> Option<Instant> {
        let Kind::Frames { fps, .. } = self.kind else {
            return None;
        };
        if data.stopped || data.changing || self.mode == AM_SUB_PUSH && data.callback.is_none() {
            return None;
        }
        if fps == 0 {
            return Some(now);
        }
        match data.last_frame {
            None => Some(now),
            // 向上取整，不以浮点容差把帧率上限变成可提前发布。
            Some(at) => at
                .checked_add(Duration::from_nanos(1_000_000_000u64.div_ceil(fps as u64)))
                .map(|at| at.max(now)),
        }
    }
    fn eligible(&self, data: &Data, now: Instant) -> bool {
        self.next_at(data, now).is_some_and(|at| at <= now)
    }
    pub fn next_pixels_at(&self, now: Instant) -> Option<Instant> {
        lock(&self.data)
            .ok()
            .and_then(|data| self.next_at(&data, now))
    }
    /// 转换前只查询需求，不提前占掉失败或未发布帧的采样机会。
    #[cfg(test)]
    pub fn needs_pixels(&self, now: Instant) -> bool {
        lock(&self.data).is_ok_and(|data| self.eligible(&data, now))
    }
    pub fn due(&self, now: Instant) -> bool {
        let Ok(mut d) = self.data.lock() else {
            return false;
        };
        if !self.eligible(&d, now) {
            return false;
        }
        d.last_frame = Some(now);
        #[cfg(test)]
        {
            if d.selections.len() == 128 {
                d.selections.pop_front();
            }
            d.selections.push_back(now);
        }
        true
    }
    #[cfg(test)]
    pub fn selection_times(&self) -> Vec<Instant> {
        lock(&self.data)
            .unwrap()
            .selections
            .iter()
            .copied()
            .collect()
    }
    pub fn enqueue(&self, item: Item) {
        let mut resync = false;
        let Ok(mut d) = self.data.lock() else {
            return;
        };
        if d.stopped || d.changing || self.mode == AM_SUB_PUSH && d.callback.is_none() {
            return;
        }
        let generation = match &item {
            Item::Encoded(p) => p.abi.source_generation,
            Item::Frame(f) => f.meta.source_generation,
        };
        if self
            .hub
            .upgrade()
            .and_then(|h| h.source.upgrade())
            .is_some_and(|s| {
                s.stop.load(Ordering::Acquire) || s.generation.load(Ordering::Acquire) != generation
            })
        {
            return;
        }
        let size = item.bytes();
        let is_encoded = matches!(item, Item::Encoded(_));
        if let Item::Encoded(p) = &item {
            let Kind::Encoded { mask } = self.kind else {
                return;
            };
            if mask & p.abi.codec == 0 {
                let changed = d.generation != p.abi.source_generation;
                d.generation = p.abi.source_generation;
                d.queue.clear();
                d.bytes = 0;
                d.waiting_key = true;
                drop(d);
                if changed {
                    if let Some(source) = self.hub.upgrade().and_then(|h| h.source.upgrade()) {
                        source.event(
                            AM_EVENT_STATE_CHANGED,
                            AM_EVENT_SCOPE_SINGLE_SUB,
                            self.key.load(Ordering::Acquire),
                            AM_FAULT_DECODE as u32,
                            c"camera.codecUnsupported",
                        );
                    }
                }
                return;
            }
            if d.generation != p.abi.source_generation {
                d.queue.clear();
                d.bytes = 0;
                d.waiting_key = true;
                d.generation = p.abi.source_generation;
            }
            if d.waiting_key && p.abi.flags & AM_PKT_KEYFRAME == 0 {
                self.state.metrics.dropped.fetch_add(1, Ordering::AcqRel);
                return;
            }
        }
        if size > self.max_bytes
            || d.queue.len() >= self.maximum
            || d.bytes.saturating_add(size) > self.max_bytes
        {
            if is_encoded {
                self.state
                    .metrics
                    .dropped
                    .fetch_add(d.queue.len() as u64, Ordering::AcqRel);
                d.queue.clear();
                d.bytes = 0;
                d.waiting_key = true;
                resync = true;
            } else {
                while !d.queue.is_empty()
                    && (d.queue.len() >= self.maximum
                        || d.bytes.saturating_add(size) > self.max_bytes)
                {
                    if let Some(old) = d.queue.pop_front() {
                        d.bytes -= old.bytes();
                        self.state
                            .metrics
                            .frames_dropped
                            .fetch_add(1, Ordering::AcqRel);
                    }
                }
            }
        }
        let admissible = size <= self.max_bytes
            && (!is_encoded
                || !d.waiting_key
                || matches!(&item, Item::Encoded(p) if p.abi.flags & AM_PKT_KEYFRAME != 0));
        if admissible {
            self.state.metrics.accepted.fetch_add(1, Ordering::AcqRel);
            d.waiting_key = false;
            d.bytes += size;
            d.queue.push_back(item);
        } else {
            self.state.metrics.dropped.fetch_add(1, Ordering::AcqRel);
        }
        drop(d);
        self.wake.notify_one();
        if resync {
            self.state.metrics.resyncs.fetch_add(1, Ordering::AcqRel);
            if let Some(source) = self.hub.upgrade().and_then(|h| h.source.upgrade()) {
                source.event(
                    AM_EVENT_ENCODED_RESYNC,
                    AM_EVENT_SCOPE_SINGLE_SUB,
                    self.key.load(Ordering::Acquire),
                    AM_FAULT_KEYFRAME_WAIT as u32,
                    c"camera.subscriberBackpressure",
                );
            }
        }
    }
    #[cfg(test)]
    pub fn pull(&self, timeout: u32) -> Result<Item> {
        self.pull_map(timeout, Ok)
    }
    pub fn pull_export(&self, timeout: u32) -> Result<usize> {
        self.pull_map(timeout, |item| item.transfer(&self.state))
    }
    fn pull_map<T>(&self, timeout: u32, deliver: impl FnOnce(Item) -> Result<T>) -> Result<T> {
        callbacks::reject_reentry()?;
        if self.mode != AM_SUB_PULL {
            return Err(AM_ERR_CAMERA_MODE_CONFLICT);
        }
        if timeout > self.deadline_ms {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        let until = Instant::now() + Duration::from_millis(timeout as u64);
        let mut d = lock(&self.data)?;
        if d.stopped {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        let waiting = d.queue.is_empty() && timeout != 0;
        if waiting && d.waiters >= self.max_waiters {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        if waiting {
            d.waiters += 1;
        }
        let result = loop {
            if d.stopped {
                break Err(AM_ERR_SDK_HANDLE_CLOSED);
            }
            if let Some(item) = d.queue.pop_front() {
                d.bytes -= item.bytes();
                break Ok(item);
            }
            let now = Instant::now();
            if now >= until {
                break Err(AM_ERR_SDK_DEADLINE_EXCEEDED);
            }
            match self.wake.wait_timeout(d, until - now) {
                Ok((data, _)) => d = data,
                Err(_) => return Err(AM_ERR_SDK_INTERNAL_ERROR),
            }
        };
        if waiting {
            d.waiters -= 1;
        }
        let had_item = result.is_ok();
        // 同一拉取订阅的预算移交与出队原子进行，不能先放行后续P帧再发现前帧移交失败。
        let result = result.and_then(deliver);
        let resync = had_item && result.is_err() && matches!(self.kind, Kind::Encoded { .. });
        if resync {
            d.queue.clear();
            d.bytes = 0;
            d.waiting_key = true;
        }
        drop(d);
        if resync {
            self.report_resync(c"camera.outputPacketBudgetExceeded");
        }
        result
    }
    fn report_resync(&self, reason: &'static std::ffi::CStr) {
        self.state.metrics.resyncs.fetch_add(1, Ordering::AcqRel);
        if let Some(source) = self.hub.upgrade().and_then(|h| h.source.upgrade()) {
            source.event(
                AM_EVENT_ENCODED_RESYNC,
                AM_EVENT_SCOPE_SINGLE_SUB,
                self.key.load(Ordering::Acquire),
                AM_FAULT_KEYFRAME_WAIT as u32,
                reason,
            );
        }
    }
    pub fn set_callback(self: &Arc<Self>, callback: Option<Callback>, user: usize) -> Result<()> {
        callbacks::reject_reentry()?;
        if self.mode != AM_SUB_PUSH {
            return Err(AM_ERR_CAMERA_MODE_CONFLICT);
        }
        let mut d = lock(&self.data)?;
        while d.changing && !d.stopped {
            d = self.wake.wait(d).map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        }
        if d.stopped {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        d.changing = true;
        d.callback = None;
        while d.busy {
            d = self.wake.wait(d).map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        }
        d.queue.clear();
        d.bytes = 0;
        d.waiting_key = matches!(self.kind, Kind::Encoded { .. });
        if d.stopped {
            d.changing = false;
            self.wake.notify_all();
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        d.callback = callback;
        d.user = user;
        if callback.is_some() && !self.alive.load(Ordering::Acquire) {
            self.alive.store(true, Ordering::Release);
            let this = self.clone();
            match self
                .state
                .metrics
                .threads
                .spawn("amc-subscription", move || {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| this.run()));
                    if let Ok(mut data) = this.data.lock() {
                        data.busy = false;
                        data.stopped = true;
                        data.callback = None;
                        data.queue.clear();
                        data.bytes = 0;
                    }
                    this.alive.store(false, Ordering::Release);
                    this.wake.notify_all();
                }) {
                Ok(thread) => *lock(&self.handle)? = Some(thread),
                Err(_) => {
                    self.alive.store(false, Ordering::Release);
                    d.callback = None;
                    d.changing = false;
                    self.wake.notify_all();
                    return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
                }
            }
        }
        d.changing = false;
        self.wake.notify_all();
        Ok(())
    }
    fn run(&self) {
        loop {
            let next = {
                let Ok(mut d) = self.data.lock() else {
                    break;
                };
                while !d.stopped && (d.changing || d.callback.is_none() || d.queue.is_empty()) {
                    d = match self.wake.wait(d) {
                        Ok(v) => v,
                        Err(_) => return,
                    };
                }
                if d.stopped {
                    break;
                }
                let next = d.queue.pop_front().zip(d.callback).map(|(item, cb)| {
                    d.bytes -= item.bytes();
                    (item, cb, d.user)
                });
                d.busy = next.is_some();
                next
            };
            if let Some((item, callback, user)) = next {
                let key = self.key.load(Ordering::Acquire);
                if let Ok(entry) = domain().get(key) {
                    if let Ok(_active) = entry.readable() {
                        if let Ok(item_key) = item.transfer(&self.state) {
                            let _scope = callbacks::Scope::enter();
                            unsafe {
                                match callback {
                                    Callback::Encoded(cb) => cb(key as _, item_key as _, user as _),
                                    Callback::Frame(cb) => cb(key as _, item_key as _, user as _),
                                }
                            }
                            let _ = domain().buffers.release(item_key);
                        } else if matches!(self.kind, Kind::Encoded { .. }) {
                            self.reset();
                            self.report_resync(c"camera.outputPacketBudgetExceeded");
                        }
                    }
                }
                if let Ok(mut d) = self.data.lock() {
                    d.busy = false;
                }
                self.wake.notify_all();
            }
        }
    }
}
