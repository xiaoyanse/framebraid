use crate::{
    domain,
    frames::{camera_state, CameraState},
    status,
};
use am_camera_core::credentials::Secret;
use am_sdk_support::{
    abi::*,
    args,
    handles::{Entry, Managed},
    Result,
};
use std::{
    any::Any,
    ffi::{c_void, CStr},
    mem::size_of,
    sync::{
        atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, AtomicUsize, Ordering},
        Arc, Condvar, Mutex, Weak,
    },
};
mod api;
mod credentials;
mod events;
pub(crate) mod hardware;
#[cfg(test)]
mod tests;
use credentials::{Credentials, Provider};

pub struct Source {
    pub shared: Arc<SharedSource>,
    pub key: AtomicUsize,
    closing: AtomicBool,
    closed: AtomicBool,
    events: Arc<events::Events>,
}
pub struct SharedSource {
    pub state: Arc<CameraState>,
    pub url: String,
    pub isapi: bool,
    pub source_id: u64,
    pub generation: AtomicU32,
    pub config_generation: AtomicU32,
    pub stop: Arc<AtomicBool>,
    pub reconnect: Mutex<am_reconnect_policy_t>,
    pub stream: Mutex<Option<Arc<crate::subscriptions::Hub>>>,
    pub state_code: AtomicU32,
    pub fault: AtomicU32,
    pub reconnect_attempts: AtomicU32,
    pub last_frame: AtomicI64,
    pub(crate) decode: Mutex<hardware::DecodeState>,
    owners: AtomicUsize,
    credentials: Arc<Credentials>,
    sources: Mutex<crate::captures::tables::Table<usize, Weak<Source>>>,
    myself: Weak<SharedSource>,
}
impl SharedSource {
    pub fn runtime_unavailable(&self, code: i32) {
        if let Ok((_, detail)) = am_sdk_support::diagnostics::snapshot(domain(), self.state.ctx_key)
        {
            if let Ok(sources) = self.sources.lock() {
                for source in sources.values().filter_map(Weak::upgrade) {
                    domain().record_error_detail(source.key.load(Ordering::Acquire), code, detail);
                }
            }
        }
    }
    pub fn secret(&self) -> Result<Secret> {
        self.credentials.secret()
    }
    pub fn credential_version(&self) -> u32 {
        self.credentials.version()
    }
    pub fn credential_snapshot(&self) -> Result<(Secret, u32, u32, u32)> {
        let generation = self.generation.load(Ordering::Acquire);
        let configuration = self.config_generation.load(Ordering::Acquire);
        let (secret, version) = self.credentials.snapshot()?;
        if generation != self.generation.load(Ordering::Acquire)
            || configuration != self.config_generation.load(Ordering::Acquire)
        {
            return Err(AM_ERR_SDK_GENERATION_MISMATCH);
        }
        Ok((secret, version, generation, configuration))
    }
    pub fn report(&self, state: u32, fault: u32, reason: &'static CStr) {
        if self.stop.load(Ordering::Acquire) {
            return;
        }
        let changed = self.state_code.swap(state, Ordering::AcqRel) != state;
        let fault_changed = self.fault.swap(fault, Ordering::AcqRel) != fault;
        if changed || fault_changed {
            self.event(
                AM_EVENT_STATE_CHANGED,
                AM_EVENT_SCOPE_ALL_CONSUMERS,
                0,
                fault,
                reason,
            );
            if state == AM_SRC_AUTH_WAITING as u32 {
                self.event(
                    AM_EVENT_AUTH_WAITING,
                    AM_EVENT_SCOPE_ALL_CONSUMERS,
                    0,
                    fault,
                    reason,
                );
                if self.credentials.auth_failed() {
                    let _ = self.changed();
                }
            }
        }
    }
    pub fn received(&self, at: i64) {
        self.last_frame.fetch_max(at, Ordering::AcqRel);
        self.state
            .metrics
            .newest_received
            .fetch_max(at, Ordering::AcqRel);
        self.report(AM_SRC_LIVE as u32, AM_FAULT_NONE as u32, c"camera.live");
    }
    pub fn event(&self, kind: i32, scope: i32, affected: usize, fault: u32, reason: &'static CStr) {
        let sources = self
            .sources
            .lock()
            .map(|mut sources| {
                sources.retain(|_, s| s.strong_count() > 0);
                sources
                    .values()
                    .filter_map(Weak::upgrade)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for source in sources {
            if !source.closing.load(Ordering::Acquire) {
                source.events.push(
                    kind,
                    scope,
                    affected,
                    fault,
                    self.generation.load(Ordering::Acquire),
                    reason,
                );
            }
        }
    }
    pub fn credentials_ready(&self) {
        if self.stop.load(Ordering::Acquire) || self.secret().is_err() {
            return;
        }
        self.report(
            AM_SRC_CONNECTING as u32,
            AM_FAULT_NONE as u32,
            c"camera.connecting",
        );
        if !self.isapi {
            if let Ok(hub) = self.stream.lock() {
                if let Some(hub) = hub.as_ref() {
                    hub.credentials_changed();
                }
            }
            if let Some(owner) = self.myself.upgrade() {
                if let Err(code) = crate::subscriptions::start(&owner) {
                    self.report(
                        AM_SRC_RECONNECT_WAIT as u32,
                        AM_FAULT_RESOURCE as u32,
                        if code == AM_ERR_SDK_RESOURCE_EXHAUSTED {
                            c"camera.resourceExhausted"
                        } else {
                            c"camera.connectionStartFailed"
                        },
                    );
                }
            }
        }
    }
    fn changed(&self) -> Result<()> {
        let invalidation = crate::probe::invalidate(self.state.ctx_key);
        // reuse=1 的所有句柄属于同一权限域；轮换使旧流与旧帧代次失效。
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.event(
            AM_EVENT_GENERATION_CHANGED,
            AM_EVENT_SCOPE_ALL_CONSUMERS,
            0,
            AM_FAULT_AUTH as u32,
            c"camera.credentialsChanged",
        );
        if let Ok(hub) = self.stream.lock() {
            if let Some(hub) = hub.as_ref() {
                hub.credentials_changed();
            }
        }
        invalidation
    }
    fn stopped(&self) -> bool {
        self.credentials.stopped()
            && self
                .stream
                .lock()
                .map(|hub| hub.as_ref().is_none_or(|hub| hub.is_stopped()))
                .unwrap_or(false)
    }
}
impl Managed for Source {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn request_close(&self) {
        if self.closing.swap(true, Ordering::AcqRel) {
            return;
        }
        self.events.stop();
        if self.shared.owners.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.shared.stop.store(true, Ordering::Release);
            self.shared
                .state_code
                .store(AM_SRC_CLOSING as u32, Ordering::Release);
            self.shared.credentials.stop();
            if let Ok(hub) = self.shared.stream.lock() {
                if let Some(hub) = hub.as_ref() {
                    hub.stop();
                }
            }
        }
    }
    fn drained(&self) -> bool {
        self.events.stopped()
            && (self.shared.owners.load(Ordering::Acquire) > 0 || self.shared.stopped())
    }
    fn close_resources(&self) {
        self.events.join();
        if self.shared.owners.load(Ordering::Acquire) == 0 {
            self.shared.credentials.join();
            if let Ok(mut hub) = self.shared.stream.lock() {
                hub.take();
            }
            self.shared
                .state_code
                .store(AM_SRC_CLOSED as u32, Ordering::Release);
        }
        self.closed.store(true, Ordering::Release);
    }
}
impl Drop for Source {
    fn drop(&mut self) {
        // 登记失败或提前返回也必须归还已领取的共享所有权。
        self.request_close();
    }
}
#[derive(Default)]
struct Registry {
    sources: Vec<Weak<Source>>,
    shared: Vec<Weak<SharedSource>>,
}
static REGISTRY: am_sdk_support::unload::UnloadCell<Mutex<Registry>> =
    am_sdk_support::unload::UnloadCell::new();
#[cfg(all(windows, not(test)))]
pub fn unload_ready() -> bool {
    REGISTRY.get().is_none_or(|registry| {
        registry.try_lock().is_ok_and(|values| {
            values
                .sources
                .iter()
                .all(|source| source.strong_count() == 0)
                && values
                    .shared
                    .iter()
                    .all(|source| source.strong_count() == 0)
        })
    })
}
#[cfg(all(windows, not(test)))]
pub unsafe fn unload_static() {
    let _ = unsafe {
        REGISTRY.clear_if(|registry| {
            registry.try_lock().is_ok_and(|values| {
                values
                    .sources
                    .iter()
                    .all(|source| source.strong_count() == 0)
                    && values
                        .shared
                        .iter()
                        .all(|source| source.strong_count() == 0)
            })
        })
    };
}
// 全域基础登记独立于Context载荷额度；发布句柄前一次预留原域硬上限。
fn reserve_registry<T>(rows: &mut Vec<Weak<T>>) -> Result<()> {
    let maximum = SDK_MAX_REGISTERED_HANDLES as usize;
    if rows.len() >= maximum {
        return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
    }
    if rows.capacity() == 0 {
        rows.try_reserve_exact(maximum)
            .map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
    }
    if rows.capacity() != maximum {
        return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
    }
    Ok(())
}
fn registry() -> &'static Mutex<Registry> {
    REGISTRY.get_or_init(Mutex::default)
}
pub fn queue_usage(ctx_key: usize) -> (u64, u64) {
    let shared = registry()
        .lock()
        .map(|registry| {
            let mut sources = std::collections::HashMap::new();
            for source in registry.sources.iter().filter_map(Weak::upgrade) {
                if source.shared.state.ctx_key == ctx_key {
                    sources
                        .entry(source.shared.source_id)
                        .or_insert_with(|| source.shared.clone());
                }
            }
            sources.into_values().collect::<Vec<_>>()
        })
        .unwrap_or_default();
    // registry 与 Hub 的锁分开，避免统计和连接状态回调形成反向锁序。
    shared.into_iter().fold((0, 0), |(items, bytes), source| {
        let usage = source
            .stream
            .lock()
            .ok()
            .and_then(|hub| hub.as_ref().map(|hub| hub.queue_usage()))
            .unwrap_or_default();
        (items + usage.0, bytes + usage.1)
    })
}
pub fn get(key: usize) -> Result<Arc<Entry>> {
    let entry = domain().get(key)?;
    entry.data::<Source>()?;
    Ok(entry)
}
static SOURCE_ID: AtomicU64 = AtomicU64::new(1 << 48);
fn make_shared(state: Arc<CameraState>, url: String, isapi: bool) -> Result<Arc<SharedSource>> {
    let source_id = SOURCE_ID
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
        .map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
    Ok(Arc::new_cyclic(|myself| SharedSource {
        state: state.clone(),
        url,
        isapi,
        source_id,
        generation: AtomicU32::new(1),
        config_generation: AtomicU32::new(1),
        stop: Arc::new(AtomicBool::new(false)),
        reconnect: Mutex::new(am_reconnect_policy_t {
            initial_backoff_ms: 1000,
            max_backoff_ms: 30000,
            jitter_ms: 500,
            max_attempts: 0,
            ..Default::default()
        }),
        stream: Mutex::default(),
        state_code: AtomicU32::new(AM_SRC_WAITING_CREDENTIALS as u32),
        fault: AtomicU32::new(AM_FAULT_NONE as u32),
        reconnect_attempts: AtomicU32::new(0),
        last_frame: AtomicI64::new(0),
        decode: Mutex::default(),
        owners: AtomicUsize::new(1),
        credentials: Arc::new(Credentials::new(state.metrics.threads.clone())),
        sources: Mutex::default(),
        myself: myself.clone(),
    }))
}
#[cfg(test)]
pub(crate) fn isolated_test_source(state: Arc<CameraState>) -> Arc<SharedSource> {
    make_shared(state, String::new(), false).unwrap()
}
