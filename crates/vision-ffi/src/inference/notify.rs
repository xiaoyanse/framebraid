#![allow(clippy::missing_safety_doc)]
use super::{
    lock,
    models::ModelHandle,
    state::{ControlCharge, State},
};
use am_sdk_support::{abi::*, context::Context, handles::Entry, status};
use std::{
    collections::VecDeque,
    mem::size_of,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex, Weak,
    },
    thread::{JoinHandle, ThreadId},
};

type Notification = (u64, u32, ControlCharge);

fn reserve_notification(state: &State) -> am_sdk_support::Result<ControlCharge> {
    // 存储槽已预付字节；每个活通知仍占独立控制项和关闭屏障。
    state.control(0)
}

mod queue;

#[derive(Default)]
struct Data {
    callback: am_result_notify_fn,
    user: usize,
    queue: VecDeque<Notification>,
    busy: bool,
    stop: bool,
    thread_id: Option<ThreadId>,
    queue_storage: Option<ControlCharge>,
}
#[derive(Default)]
pub struct Notify {
    data: Mutex<Data>,
    wake: Condvar,
    handle: Mutex<Option<JoinHandle<()>>>,
    alive: AtomicBool,
}
impl Notify {
    pub fn stop(&self) {
        if let Ok(mut data) = self.data.lock() {
            data.stop = true;
            data.callback = None;
            data.queue.clear();
            data.release_storage_if_idle();
        }
        self.wake.notify_all();
    }
    pub fn drained(&self) -> bool {
        !self.alive.load(Ordering::Acquire)
    }
    pub fn join(&self) {
        if let Ok(mut handle) = self.handle.lock() {
            if let Some(thread) = handle.take() {
                let _ = thread.join();
            }
        }
    }
    pub fn on_thread(&self) -> bool {
        self.data
            .lock()
            .is_ok_and(|d| d.thread_id == Some(std::thread::current().id()))
    }
    pub fn enqueue(&self, model: &ModelHandle, id: u64, terminal: u32) {
        let Ok(mut data) = self.data.lock() else {
            return;
        };
        if data.stop || data.callback.is_none() {
            return;
        }
        if data.push(&model.state, id, terminal).is_err() {
            model.state.metrics.dropped.fetch_add(1, Ordering::AcqRel);
            return;
        }
        self.wake.notify_one();
    }
    fn run(self: Arc<Self>, model: Weak<Entry>) {
        if let Ok(mut data) = self.data.lock() {
            data.thread_id = Some(std::thread::current().id());
        }
        loop {
            let next = {
                let Ok(mut data) = self.data.lock() else {
                    break;
                };
                while data.queue.is_empty() && !data.stop {
                    data = match self.wake.wait(data) {
                        Ok(d) => d,
                        Err(_) => return,
                    };
                }
                if data.stop {
                    break;
                }
                let next = data
                    .queue
                    .pop_front()
                    .zip(data.callback)
                    .map(|((id, state, charge), cb)| (id, state, cb, data.user, charge));
                data.busy = next.is_some();
                next
            };
            if let Some((id, state, cb, user, _charge)) = next {
                if let Some(entry) = model.upgrade() {
                    if let Ok(_reader) = entry.readable() {
                        let _callback = am_sdk_support::callbacks::Scope::enter();
                        // 回调期间不持队列锁；拉取结果和异步请求关闭可安全重入。
                        unsafe {
                            cb(
                                entry
                                    .data::<ModelHandle>()
                                    .map_or(0, |m| m.info.key.load(Ordering::Acquire))
                                    as _,
                                id,
                                state,
                                user as _,
                            )
                        };
                    }
                }
                if let Ok(mut data) = self.data.lock() {
                    data.busy = false;
                    if data.stop {
                        data.release_storage_if_idle();
                    }
                }
                self.wake.notify_all();
            }
        }
        self.alive.store(false, Ordering::Release);
        self.wake.notify_all();
    }
}

#[no_mangle]
pub unsafe extern "C" fn amv_result_notify_callback_set(
    model: *mut amv_model_t,
    cb: am_result_notify_fn,
    user: *mut std::ffi::c_void,
) -> i32 {
    status(crate::domain(), model as usize, || {
        am_sdk_support::callbacks::reject_reentry()?;
        let entry = crate::domain().active(model as usize)?;
        let model = entry.data::<ModelHandle>()?;
        let notify = &model.notify;
        if notify.on_thread() {
            return Err(AM_ERR_SDK_CALLBACK_CONTRACT_VIOLATION);
        }
        let mut handle = lock(&notify.handle)?;
        let mut data = lock(&notify.data)?;
        data.callback = None;
        data.queue.clear();
        while data.busy {
            data = notify
                .wake
                .wait(data)
                .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        }
        data.release_storage_if_idle();
        if data.stop {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        if cb.is_some() {
            data.prepare(&model.state)?;
        }
        if cb.is_some() && handle.is_none() {
            let mut permit = match lock(&model.permit) {
                Ok(permit) => permit,
                Err(error) => {
                    data.release_storage_if_idle();
                    return Err(error);
                }
            };
            let Some(permit) = permit.as_mut() else {
                data.release_storage_if_idle();
                return Err(AM_ERR_SDK_HANDLE_CLOSED);
            };
            if let Err(error) = permit.add_callback_thread() {
                data.release_storage_if_idle();
                return Err(error);
            }
            notify.alive.store(true, Ordering::Release);
            let owned = notify.clone();
            let weak = Arc::downgrade(&entry.0);
            match std::thread::Builder::new()
                .name("am-sdk-notify".into())
                .spawn(move || owned.run(weak))
            {
                Ok(thread) => *handle = Some(thread),
                Err(_) => {
                    notify.alive.store(false, Ordering::Release);
                    permit.undo_callback_thread();
                    data.release_storage_if_idle();
                    return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
                }
            }
        }
        data.callback = cb;
        data.user = user as usize;
        Ok(())
    })
}

#[cfg(test)]
mod tests;
