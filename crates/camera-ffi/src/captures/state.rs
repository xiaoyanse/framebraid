use super::*;
use crate::frames::ControlCharge;
use am_sdk_support::{
    context::{CloseHooks, Context},
    handles::Entry,
};
use std::{
    any::TypeId,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Condvar, Weak,
    },
};

static IDENTITIES: AtomicU64 = AtomicU64::new(1);
pub struct Record {
    pub state: u32,
    pub code: i32,
    pub created: i64,
    pub finished: i64,
    pub expires: i64,
    pub result: Option<Arc<images::Image>>,
    pub _charge: ControlCharge,
}
pub struct State {
    pub ctx: Weak<Entry>,
    prefix: u64,
    next: AtomicU64,
    pub records: Mutex<tables::Table<u64, Record>>,
    pub flights: Mutex<tables::Table<u64, Arc<io::Flight>>>,
    pub recent: Mutex<tables::Table<(u64, u32), Arc<io::Raw>>>,
    pub running: AtomicUsize,
    pub continuous: AtomicUsize,
    pub consumers: Mutex<tables::Table<(u64, u32), usize>>,
    pub subscriptions: Mutex<tables::Table<usize, Weak<super::continuous::Subscription>>>,
    pub closed: AtomicBool,
    pub changed: Condvar,
}
impl Managed for State {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn request_close(&self) {
        self.closed.store(true, Ordering::Release);
        if let Ok(mut records) = self.records.lock() {
            records.clear();
        }
        if let Ok(mut flights) = self.flights.lock() {
            flights.clear();
        }
        if let Ok(mut recent) = self.recent.lock() {
            recent.clear();
        }
        self.changed.notify_all();
    }
    fn drained(&self) -> bool {
        self.running.load(Ordering::Acquire) == 0
    }
}
impl State {
    pub fn remove_consumer(&self, key: (u64, u32)) {
        if let Ok(mut consumers) = self.consumers.lock() {
            if let Some(count) = consumers.get_mut(&key) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    consumers.remove(&key);
                    if let Ok(mut recent) = self.recent.lock() {
                        recent.remove(&key);
                    }
                }
            }
        }
    }
    pub fn get(camera: &Arc<CameraState>) -> Result<Arc<Self>> {
        let context = camera.ctx.data::<Context>()?;
        let mut services = lock(&context.services)?;
        if let Some(value) = services.get(&TypeId::of::<Self>()) {
            return value
                .clone()
                .downcast()
                .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR);
        }
        if camera.ctx.state.load(Ordering::Acquire) != AM_LIFECYCLE_RUNNING as u32 {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        let prefix = IDENTITIES.fetch_add(1, Ordering::AcqRel);
        if prefix > 65535 {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        let hooks = services
            .get_or_insert_with(TypeId::of::<CloseHooks>(), || {
                Arc::new(CloseHooks::default())
            })?
            .downcast::<CloseHooks>()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        let value = services
            .try_get_or_insert_with(TypeId::of::<Self>(), || {
                let value = Arc::new(Self {
                    ctx: Arc::downgrade(&camera.ctx),
                    prefix: prefix << 48,
                    next: AtomicU64::new(1),
                    records: Mutex::default(),
                    flights: Mutex::default(),
                    recent: Mutex::default(),
                    running: AtomicUsize::new(0),
                    continuous: AtomicUsize::new(0),
                    consumers: Mutex::default(),
                    subscriptions: Mutex::default(),
                    closed: AtomicBool::new(false),
                    changed: Condvar::new(),
                });
                let managed: Arc<dyn Managed> = value.clone();
                hooks.register(Arc::downgrade(&managed))?;
                Ok(value)
            })?
            .downcast::<Self>()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        Ok(value)
    }
    pub fn context(key: usize) -> Result<Arc<Self>> {
        Self::get(&crate::frames::camera_state(key)?)
    }
    pub fn id(&self) -> Result<u64> {
        let id = self.next.fetch_add(1, Ordering::AcqRel);
        if id >= 0xffffffffffff {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        Ok(self.prefix | id)
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
    pub fn purge(&self, records: &mut tables::Table<u64, Record>) {
        let now = monotonic_ms();
        records.retain(|_, r| r.state < AM_REQ_DONE as u32 || now < r.expires);
    }
    pub fn reserve_worker(self: &Arc<Self>) -> Result<Worker> {
        if self.closed.load(Ordering::Acquire) {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        self.running
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 4).then_some(n + 1)
            })
            .map_err(|_| AM_ERR_CAMERA_CAPTURE_CAPACITY)?;
        Ok(Worker(self.clone()))
    }
    pub fn result_policy(&self) -> Result<am_queue_policy_t> {
        let entry = self.ctx.upgrade().ok_or(AM_ERR_SDK_HANDLE_CLOSED)?;
        let context = entry.data::<Context>()?;
        let policy = lock(&context.queues)?[AM_Q_RESULTS as usize];
        Ok(policy)
    }
}
pub struct Worker(pub Arc<State>);
impl Drop for Worker {
    fn drop(&mut self) {
        self.0.running.fetch_sub(1, Ordering::AcqRel);
        self.0.changed.notify_all();
    }
}
