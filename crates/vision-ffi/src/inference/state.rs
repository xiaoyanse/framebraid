use super::{lock, records::Records, results::ResultBuffer};
use am_sdk_support::{abi::*, context::Context, handles::Entry, Result};
use std::{
    any::TypeId,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, Condvar, Mutex, Weak,
    },
};

static IDENTITIES: AtomicU64 = AtomicU64::new(1);
pub struct Record {
    pub model: usize,
    pub slot: u64,
    pub state: u32,
    pub code: i32,
    pub created: i64,
    pub started: i64,
    pub finished: i64,
    pub expires: i64,
    pub result: Option<Arc<ResultBuffer>>,
    pub canceled: bool,
    pub offline: bool,
}
pub struct State {
    pub ctx: Weak<Entry>,
    pub prefix: u64,
    pub next: AtomicU64,
    pub records: Mutex<Records>,
    pub changed: Condvar,
    pub usage: Arc<Usage>,
    pub models: AtomicUsize,
    pub running: AtomicUsize,
    pub metrics: super::telemetry::Metrics,
    pub backends: Mutex<super::backends::Rows>,
}
#[derive(Default)]
pub struct Usage {
    pub preprocess: Mutex<[usize; 2]>,
    pub bytes: AtomicUsize,
    pub items: AtomicUsize,
    pub snapshots: AtomicUsize,
    pub external_items: AtomicUsize,
    pub external_bytes: AtomicUsize,
    pub control_bytes: AtomicUsize,
    pub waiters: AtomicUsize,
}
pub struct ResultCharge {
    pub ctx: Weak<Entry>,
    pub usage: Arc<Usage>,
    pub bytes: usize,
    items: usize,
}
pub struct ControlCharge {
    external: bool,
    counted: bool,
    ctx: Weak<Entry>,
    usage: Arc<Usage>,
    bytes: usize,
}
impl Drop for ControlCharge {
    fn drop(&mut self) {
        if self.counted {
            self.usage.snapshots.fetch_sub(1, Ordering::AcqRel);
        }
        self.usage
            .control_bytes
            .fetch_sub(self.bytes, Ordering::AcqRel);
        if self.external {
            if let Some(ctx) = self.ctx.upgrade() {
                if let Ok(c) = ctx.data::<Context>() {
                    c.buffers.fetch_sub(1, Ordering::AcqRel);
                }
            }
        }
    }
}
impl Drop for ResultCharge {
    fn drop(&mut self) {
        self.usage.bytes.fetch_sub(self.bytes, Ordering::AcqRel);
        self.usage.items.fetch_sub(self.items, Ordering::AcqRel);
        if let Some(ctx) = self.ctx.upgrade() {
            if let Ok(c) = ctx.data::<Context>() {
                c.buffers.fetch_sub(1, Ordering::AcqRel);
            }
        }
    }
}
impl State {
    pub fn get(key: usize) -> Result<Arc<Self>> {
        let ctx = crate::domain().get(key)?;
        let _read = ctx.readable()?;
        let context = ctx.data::<Context>()?;
        let mut services = lock(&context.services)?;
        if let Some(existing) = services.get(&TypeId::of::<State>()) {
            return existing
                .clone()
                .downcast()
                .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR);
        }
        let prefix = IDENTITIES.fetch_add(1, Ordering::Relaxed);
        if prefix > 65535 {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        let value = Arc::new(Self {
            ctx: Arc::downgrade(&ctx),
            prefix: prefix << 48,
            next: AtomicU64::new(1),
            records: Mutex::default(),
            changed: Condvar::new(),
            usage: Arc::default(),
            models: AtomicUsize::new(0),
            running: AtomicUsize::new(0),
            metrics: Default::default(),
            backends: Mutex::default(),
        });
        services.insert(TypeId::of::<State>(), value.clone());
        Ok(value)
    }
    pub fn context(&self) -> Result<Arc<Entry>> {
        self.ctx.upgrade().ok_or(AM_ERR_SDK_HANDLE_CLOSED)
    }
    pub fn unknown(&self, id: u64) -> i32 {
        if id & 0xffff000000000000 == self.prefix
            && id & 0xffffffffffff > 0
            && id & 0xffffffffffff < self.next.load(Ordering::Acquire)
        {
            AM_ERR_SDK_RESULT_EXPIRED
        } else {
            AM_ERR_SDK_REQUEST_UNKNOWN
        }
    }
    pub fn purge(&self, records: &mut Records) {
        let now = am_sdk_support::clock::monotonic_ms();
        records.retain(|_, r| {
            let keep = r.state < AM_REQ_DONE as u32 || now < r.expires;
            if !keep {
                self.metrics.expired.fetch_add(1, Ordering::AcqRel);
            }
            keep
        });
    }
    pub fn charge(&self, bytes: usize) -> Result<ResultCharge> {
        self.reserve_result(bytes, 1)
    }
    pub fn temporary(&self, bytes: usize) -> Result<ResultCharge> {
        self.reserve_result(bytes, 0)
    }
    pub fn control(&self, bytes: usize) -> Result<ControlCharge> {
        self.reserve_control(bytes, true, true)
    }
    pub fn cached_control(&self, bytes: usize) -> Result<ControlCharge> {
        self.reserve_control(bytes, false, true)
    }
    pub fn record_storage(&self, bytes: usize) -> Result<ControlCharge> {
        self.reserve_control(bytes, false, false)
    }
    fn reserve_control(
        &self,
        bytes: usize,
        external: bool,
        counted: bool,
    ) -> Result<ControlCharge> {
        let ctx = self.context()?;
        let _read = ctx.readable()?;
        let c = ctx.data::<Context>()?;
        let limit = *lock(&c.limits)?;
        let queue = lock(&c.queues)?[AM_Q_CONTROL as usize];
        let max = limit.control_max_items.min(queue.max_items) as usize;
        let maximum = queue.max_bytes as usize;
        let mut used = self.usage.control_bytes.load(Ordering::Acquire);
        loop {
            let next = used
                .checked_add(bytes)
                .filter(|v| *v <= maximum)
                .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
            match self.usage.control_bytes.compare_exchange_weak(
                used,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(n) => used = n,
            }
        }
        if counted && self.usage.snapshots.fetch_add(1, Ordering::AcqRel) >= max {
            self.usage.snapshots.fetch_sub(1, Ordering::AcqRel);
            self.usage.control_bytes.fetch_sub(bytes, Ordering::AcqRel);
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        if external {
            c.buffers.fetch_add(1, Ordering::AcqRel);
        }
        Ok(ControlCharge {
            external,
            counted,
            ctx: self.ctx.clone(),
            usage: self.usage.clone(),
            bytes,
        })
    }
    fn reserve_result(&self, bytes: usize, items: usize) -> Result<ResultCharge> {
        let ctx = self.context()?;
        let context = ctx.data::<Context>()?;
        let limits = *lock(&context.limits)?;
        let queue = lock(&context.queues)?[AM_Q_RESULTS as usize];
        let max = limits.result_budget_bytes.min(queue.max_bytes) as usize;
        let count = limits.result_max_items.min(queue.max_items) as usize;
        let mut used = self.usage.bytes.load(Ordering::Acquire);
        loop {
            let next = used
                .checked_add(bytes)
                .filter(|v| *v <= max)
                .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
            match self.usage.bytes.compare_exchange_weak(
                used,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(n) => used = n,
            }
        }
        let old = self.usage.items.fetch_add(items, Ordering::AcqRel);
        if items != 0 && old >= count {
            self.usage.items.fetch_sub(items, Ordering::AcqRel);
            self.usage.bytes.fetch_sub(bytes, Ordering::AcqRel);
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        context.buffers.fetch_add(1, Ordering::AcqRel);
        Ok(ResultCharge {
            ctx: self.ctx.clone(),
            usage: self.usage.clone(),
            bytes,
            items,
        })
    }
}
