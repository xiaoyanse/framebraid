use super::*;
use std::{ffi::c_void, thread::JoinHandle, time::Instant};

#[derive(Clone, Copy)]
pub(super) struct Provider {
    fetch: unsafe extern "C" fn(*mut c_void, u32, *mut am_credential_data_t, *mut u32) -> i32,
    on_drop: am_credential_drop_fn,
    user: usize,
    timeout: u32,
}
impl Provider {
    pub unsafe fn read(value: *const am_credential_provider_t) -> Result<Self> {
        let value = unsafe { args::read(value, std::mem::size_of::<am_credential_provider_t>()) }?;
        let fetch = value.fetch.ok_or(AM_ERR_CAMERA_CREDENTIAL_INVALID)?;
        let timeout = if value.fetch_timeout_ms == 0 {
            5000
        } else {
            value.fetch_timeout_ms
        };
        if timeout > 60000 {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        Ok(Self {
            fetch,
            on_drop: value.on_drop,
            user: value.user_data as usize,
            timeout,
        })
    }
    fn same(&self, other: &Self) -> bool {
        self.fetch as usize == other.fetch as usize
            && self.user == other.user
            && self.on_drop.map(|f| f as usize) == other.on_drop.map(|f| f as usize)
            && self.timeout == other.timeout
    }
    fn release(self) {
        if let Some(callback) = self.on_drop {
            let _scope = am_sdk_support::callbacks::Scope::enter();
            unsafe { callback(self.user as *mut c_void) };
        }
    }
}
pub(super) unsafe fn copy_secret(value: *const am_credential_data_t) -> Result<Secret> {
    let value = unsafe { args::read(value, std::mem::size_of::<am_credential_data_t>()) }?;
    let username = unsafe { args::string(value.username_utf8) }?;
    let password = unsafe { args::string(value.password_utf8) }?;
    Ok(Secret { username, password })
}
struct State {
    secret: Option<Secret>,
    provider: Option<Provider>,
    version: u32,
    epoch: u64,
    pending: bool,
    busy: bool,
    changing: bool,
    stopping: bool,
    started: bool,
    fetch_started: Option<Instant>,
    require_new_version: bool,
    last_error: i32,
}
impl Default for State {
    fn default() -> Self {
        Self {
            secret: None,
            provider: None,
            version: 0,
            epoch: 0,
            pending: false,
            busy: false,
            changing: false,
            stopping: false,
            started: false,
            fetch_started: None,
            require_new_version: false,
            last_error: AM_ERR_CAMERA_CREDENTIAL_REQUIRED,
        }
    }
}
pub(super) struct Credentials {
    state: Mutex<State>,
    changed: Condvar,
    alive: AtomicBool,
    worker: Mutex<Option<JoinHandle<()>>>,
}
impl Default for Credentials {
    fn default() -> Self {
        Self {
            state: Mutex::default(),
            changed: Condvar::new(),
            alive: AtomicBool::new(false),
            worker: Mutex::default(),
        }
    }
}
impl Credentials {
    pub fn version(&self) -> u32 {
        self.state.lock().map(|s| s.version).unwrap_or(0)
    }
    pub fn secret(&self) -> Result<Secret> {
        self.snapshot().map(|(secret, _)| secret)
    }
    pub fn snapshot(&self) -> Result<(Secret, u32)> {
        let state = self.state.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        if state.stopping {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        if let (Some(started), Some(provider)) = (state.fetch_started, state.provider) {
            if started.elapsed().as_millis() > provider.timeout as u128 {
                return Err(AM_ERR_CAMERA_TIMEOUT);
            }
        }
        state
            .secret
            .clone()
            .map(|secret| (secret, state.version))
            .ok_or(state.last_error)
    }
    pub fn claim(
        &self,
        secret: Option<&Secret>,
        provider: Option<&Provider>,
        owners: &AtomicUsize,
        stop: &AtomicBool,
    ) -> bool {
        let Ok(state) = self.state.lock() else {
            return false;
        };
        if state.stopping || state.changing {
            return false;
        }
        let matches = match (secret, provider, &state.secret, &state.provider) {
            (Some(a), None, Some(b), None) => a.username == b.username && a.password == b.password,
            (None, Some(a), _, Some(b)) => a.same(b),
            (None, None, None, None) => true,
            _ => false,
        };
        if !matches || stop.load(Ordering::Acquire) {
            return false;
        }
        // 凭据匹配与领取共享所有权不可分开；最后一个关闭者若先减到零，禁止复活旧连接。
        owners
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                if count == 0 {
                    None
                } else {
                    count.checked_add(1)
                }
            })
            .is_ok()
    }
    pub fn replace(
        self: &Arc<Self>,
        owner: Weak<SharedSource>,
        secret: Option<Secret>,
        provider: Option<Provider>,
    ) -> Result<()> {
        let old = {
            let mut state = self.state.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
            while state.busy || state.changing {
                state = self
                    .changed
                    .wait(state)
                    .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
            }
            if state.stopping {
                return Err(AM_ERR_SDK_HANDLE_CLOSED);
            }
            // 同一提供器重新登记只刷新版本，不先释放仍将使用的 user_data。
            if provider.is_some_and(|new| state.provider.is_some_and(|old| new.same(&old))) {
                state.version = state
                    .version
                    .checked_add(1)
                    .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
                state.secret.take();
                state.epoch = state.epoch.wrapping_add(1);
                state.pending = false;
                state.require_new_version = false;
                state.last_error = AM_ERR_CAMERA_CREDENTIAL_REQUIRED;
                return Ok(());
            }
            state.changing = true;
            state.pending = false;
            state.secret.take();
            state.provider.take()
        };
        // 调用方的 on_drop 不在任何 SDK 锁内执行；changing 阻止工作线程取新任务。
        if let Some(old) = old {
            old.release();
        }
        let mut state = self.state.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        if state.stopping {
            state.changing = false;
            self.changed.notify_all();
            drop(state);
            if let Some(provider) = provider {
                provider.release();
            }
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        let Some(version) = state.version.checked_add(1) else {
            state.changing = false;
            self.changed.notify_all();
            drop(state);
            if let Some(provider) = provider {
                provider.release();
            }
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        };
        state.version = version;
        state.epoch = state.epoch.wrapping_add(1);
        state.secret = secret;
        state.provider = provider;
        state.pending = false;
        state.require_new_version = false;
        state.last_error = AM_ERR_CAMERA_CREDENTIAL_REQUIRED;
        state.changing = false;
        if provider.is_some() && !state.started {
            self.alive.store(true, Ordering::Release);
            let manager = self.clone();
            let worker = std::thread::Builder::new()
                .name("amc-credentials".into())
                .spawn(move || manager.run(owner));
            match worker {
                Ok(worker) => {
                    *self.worker.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)? = Some(worker);
                    state.started = true;
                }
                Err(_) => {
                    self.alive.store(false, Ordering::Release);
                    state.pending = false;
                    let provider = state.provider.take();
                    self.changed.notify_all();
                    drop(state);
                    if let Some(provider) = provider {
                        provider.release();
                    }
                    return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
                }
            }
        }
        self.changed.notify_all();
        Ok(())
    }
    fn run(self: Arc<Self>, owner: Weak<SharedSource>) {
        loop {
            let job = {
                let Ok(mut state) = self.state.lock() else {
                    break;
                };
                while !state.stopping && (!state.pending || state.changing) {
                    let Ok(next) = self.changed.wait(state) else {
                        self.alive.store(false, Ordering::Release);
                        return;
                    };
                    state = next;
                }
                if state.stopping {
                    let provider = state.provider.take();
                    drop(state);
                    if let Some(provider) = provider {
                        provider.release();
                    }
                    break;
                }
                state.pending = false;
                state.busy = true;
                state.fetch_started = Some(Instant::now());
                state.provider.map(|provider| {
                    (
                        provider,
                        state.version,
                        state.epoch,
                        state.require_new_version,
                    )
                })
            };
            let Some((provider, known, epoch, require_new)) = job else {
                continue;
            };
            let started = Instant::now();
            let result = {
                let _scope = am_sdk_support::callbacks::Scope::enter();
                let mut raw = am_credential_data_t::default();
                let mut version = known;
                let code = unsafe {
                    (provider.fetch)(provider.user as *mut c_void, known, &mut raw, &mut version)
                };
                if started.elapsed().as_millis() > provider.timeout as u128 {
                    Err(AM_ERR_CAMERA_TIMEOUT)
                } else if code != AM_OK as i32 {
                    Err(code)
                } else if version < known || require_new && version == known {
                    Err(AM_ERR_CAMERA_CREDENTIAL_INVALID)
                } else {
                    unsafe { copy_secret(&raw) }.map(|secret| (secret, version))
                }
            };
            let applied = {
                let Ok(mut state) = self.state.lock() else {
                    break;
                };
                state.fetch_started = None;
                let applied = !state.stopping && state.epoch == epoch;
                if applied {
                    if let Ok((secret, version)) = &result {
                        state.secret = Some(secret.clone());
                        state.version = *version;
                        state.require_new_version = false;
                        state.last_error = AM_ERR_CAMERA_CREDENTIAL_REQUIRED;
                    } else {
                        state.last_error = if matches!(result, Err(AM_ERR_CAMERA_TIMEOUT)) {
                            AM_ERR_CAMERA_TIMEOUT
                        } else {
                            AM_ERR_CAMERA_AUTH_FINAL_FAILED
                        };
                    }
                }
                state.busy = false;
                self.changed.notify_all();
                applied
            };
            if applied {
                if let Some(owner) = owner.upgrade() {
                    if result.is_ok() {
                        if owner.state_code.load(Ordering::Acquire) == AM_SRC_AUTH_WAITING as u32 {
                            let _ = owner.changed();
                        }
                        owner.credentials_ready();
                    } else {
                        owner.report(
                            AM_SRC_AUTH_WAITING as u32,
                            AM_FAULT_AUTH as u32,
                            if matches!(result, Err(AM_ERR_CAMERA_TIMEOUT)) {
                                c"camera.credentialTimeout"
                            } else {
                                c"camera.credentialProviderFailed"
                            },
                        );
                    }
                }
            }
        }
        self.alive.store(false, Ordering::Release);
        self.changed.notify_all();
    }
    pub fn invalidate(&self, known: u32) -> Result<bool> {
        let mut state = self.state.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        if known != state.version {
            return Ok(false);
        }
        state.secret.take();
        state.epoch = state.epoch.wrapping_add(1);
        state.pending = false;
        state.require_new_version = true;
        state.last_error = AM_ERR_CAMERA_CREDENTIAL_REQUIRED;
        self.changed.notify_all();
        Ok(true)
    }
    pub fn auth_failed(&self) -> bool {
        if let Ok(mut state) = self.state.lock() {
            if state.secret.is_some() && !state.stopping {
                state.secret.take();
                state.epoch = state.epoch.wrapping_add(1);
                state.pending = state.provider.is_some();
                state.require_new_version = true;
                state.last_error = AM_ERR_CAMERA_AUTH_FINAL_FAILED;
                self.changed.notify_all();
                return true;
            }
        }
        false
    }
    pub fn refresh(&self) {
        if let Ok(mut state) = self.state.lock() {
            if !state.stopping && !state.changing && state.provider.is_some() {
                state.pending = true;
                self.changed.notify_all();
            }
        }
    }
    pub fn stop(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.stopping = true;
            state.pending = false;
            state.secret.take();
        }
        self.changed.notify_all();
    }
    pub fn stopped(&self) -> bool {
        !self.alive.load(Ordering::Acquire)
    }
    pub fn join(&self) {
        if let Ok(mut worker) = self.worker.lock() {
            if let Some(worker) = worker.take() {
                let _ = worker.join();
            }
        }
    }
}
