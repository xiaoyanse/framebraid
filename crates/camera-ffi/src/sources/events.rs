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
    queue_storage: Option<crate::frames::ControlCharge>,
}
mod queue;
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
        state.release_storage_if_idle();
        if state.stop {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        if cb.is_some() {
            state.prepare(&self.budget)?;
        }
        if cb.is_some() && !state.started {
            // 在线程启动前核句柄锁，失败时不能留下无人join的线程。
            let mut slot = match self.worker.lock() {
                Ok(slot) => slot,
                Err(_) => {
                    state.release_storage_if_idle();
                    return Err(AM_ERR_SDK_INTERNAL_ERROR);
                }
            };
            self.alive.store(true, Ordering::Release);
            let pump = self.clone();
            let worker = self
                .budget
                .metrics
                .threads
                .spawn("amc-source-events", move || pump.run(source));
            match worker {
                Ok(worker) => {
                    *slot = Some(worker);
                    state.started = true;
                }
                Err(_) => {
                    self.alive.store(false, Ordering::Release);
                    state.release_storage_if_idle();
                    return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
                }
            }
        }
        state.callback = cb;
        state.user = user;
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
                    if state.stop {
                        state.release_storage_if_idle();
                    }
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
            if state.queue_storage.is_none() {
                self.budget.metrics.dropped.fetch_add(1, Ordering::AcqRel);
                return;
            }
            if state.queue.len() + usize::from(state.busy) >= state.queue.capacity() {
                if state.queue.pop_front().is_none() {
                    self.budget.metrics.dropped.fetch_add(1, Ordering::AcqRel);
                    return;
                }
                self.budget.metrics.dropped.fetch_add(1, Ordering::AcqRel);
            }
            let charge = self.budget.control(0, false);
            let charge = match charge {
                Ok(charge) => charge,
                Err(_) => {
                    if state.queue.pop_front().is_some() {
                        self.budget.metrics.dropped.fetch_add(1, Ordering::AcqRel);
                    }
                    let Ok(charge) = self.budget.control(0, false) else {
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
            state.release_storage_if_idle();
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

#[cfg(test)]
mod tests;
