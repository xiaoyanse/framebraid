use super::*;
use std::{collections::VecDeque, ffi::c_void, thread::JoinHandle};

struct Event {
    kind: i32,
    scope: i32,
    affected: usize,
    fault: u32,
    generation: u32,
    reason: &'static CStr,
    _charge: crate::frames::ControlCharge,
}
#[derive(Default)]
struct State {
    callback: am_source_event_fn,
    user: usize,
    busy: bool,
    stop: bool,
    started: bool,
    queue: VecDeque<Event>,
}
pub(super) struct Events {
    budget: Arc<CameraState>,
    state: Mutex<State>,
    changed: Condvar,
    alive: AtomicBool,
    worker: Mutex<Option<JoinHandle<()>>>,
}
impl Events {
    pub fn new(budget: Arc<CameraState>) -> Self {
        Self {
            budget,
            state: Mutex::default(),
            changed: Condvar::new(),
            alive: AtomicBool::new(false),
            worker: Mutex::default(),
        }
    }
}
impl Events {
    pub fn set(
        self: &Arc<Self>,
        source: Weak<Source>,
        cb: am_source_event_fn,
        user: usize,
    ) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        if state.stop {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        // 注销先切断新通知，再等待已经借出的 user_data 回调返回。
        state.callback = None;
        state.queue.clear();
        while state.busy {
            state = self
                .changed
                .wait(state)
                .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        }
        if state.stop {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        state.callback = cb;
        state.user = user;
        if cb.is_some() && !state.started {
            self.alive.store(true, Ordering::Release);
            let pump = self.clone();
            let worker = std::thread::Builder::new()
                .name("amc-source-events".into())
                .spawn(move || pump.run(source));
            match worker {
                Ok(worker) => {
                    *self.worker.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)? = Some(worker);
                    state.started = true;
                }
                Err(_) => {
                    self.alive.store(false, Ordering::Release);
                    state.callback = None;
                    return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
                }
            }
        }
        self.changed.notify_all();
        Ok(())
    }
    fn run(self: Arc<Self>, source: Weak<Source>) {
        loop {
            let job = {
                let Ok(mut state) = self.state.lock() else {
                    break;
                };
                while !state.stop && state.queue.is_empty() {
                    let Ok(next) = self.changed.wait(state) else {
                        self.alive.store(false, Ordering::Release);
                        return;
                    };
                    state = next;
                }
                if state.stop {
                    break;
                }
                match (state.callback, state.queue.pop_front()) {
                    (Some(cb), Some(event)) => {
                        state.busy = true;
                        Some((cb, state.user, event))
                    }
                    _ => None,
                }
            };
            if let Some((cb, user, event)) = job {
                let key = source.upgrade().map(|s| s.key.load(Ordering::Acquire));
                if let Some(key) = key {
                    let detail = am_event_detail_t {
                        generation: event.generation,
                        scope: event.scope,
                        affected_handle: event.affected as *mut c_void,
                        fault_class: event.fault,
                        reason_utf8: event.reason.as_ptr(),
                        ..Default::default()
                    };
                    let _scope = am_sdk_support::callbacks::Scope::enter();
                    unsafe {
                        cb(
                            key as *mut amc_source_t,
                            event.kind,
                            &detail,
                            user as *mut c_void,
                        )
                    };
                }
                if let Ok(mut state) = self.state.lock() {
                    state.busy = false;
                }
                self.changed.notify_all();
            }
        }
        self.alive.store(false, Ordering::Release);
        self.changed.notify_all();
    }
    pub fn push(
        &self,
        kind: i32,
        scope: i32,
        affected: usize,
        fault: u32,
        generation: u32,
        reason: &'static CStr,
    ) {
        if let Ok(mut state) = self.state.lock() {
            if state.stop || state.callback.is_none() {
                return;
            }
            // 事件通知是有界提示；状态查询始终提供最新事实，不让慢回调阻塞取流。
            if state.queue.len() >= 64 {
                state.queue.pop_front();
                self.budget.metrics.dropped.fetch_add(1, Ordering::AcqRel);
            }
            let charge = self.budget.control(std::mem::size_of::<Event>(), false);
            let charge = match charge {
                Ok(charge) => charge,
                Err(_) => {
                    if state.queue.pop_front().is_some() {
                        self.budget.metrics.dropped.fetch_add(1, Ordering::AcqRel);
                    }
                    let Ok(charge) = self.budget.control(std::mem::size_of::<Event>(), false)
                    else {
                        self.budget.metrics.dropped.fetch_add(1, Ordering::AcqRel);
                        return;
                    };
                    charge
                }
            };
            state.queue.push_back(Event {
                kind,
                scope,
                affected,
                fault,
                generation,
                reason,
                _charge: charge,
            });
            self.changed.notify_one();
        }
    }
    pub fn stop(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.stop = true;
            state.callback = None;
            state.queue.clear();
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
