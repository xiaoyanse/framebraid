use super::*;
use crate::{
    frames::ControlCharge,
    sources::{self, SharedSource, Source},
};
use std::{
    collections::VecDeque,
    ffi::c_void,
    mem::size_of,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Condvar,
    },
};

struct Queue {
    items: VecDeque<(u64, Arc<images::Image>)>,
    bytes: u64,
    callback: am_capture_cb_fn,
    user: usize,
    busy: bool,
    last_received: i64,
    last_frame_id: u64,
    received: VecDeque<i64>,
    failures: u32,
    skipped: u64,
}
fn metadata_bytes(items: usize) -> usize {
    size_of::<Subscription>()
        + items * size_of::<(u64, Arc<images::Image>)>()
        + 120 * size_of::<i64>()
}
impl Queue {
    fn new(maximum: usize) -> Result<Self> {
        let mut items = VecDeque::new();
        items
            .try_reserve_exact(maximum)
            .map_err(|_| AM_ERR_SDK_OUT_OF_MEMORY)?;
        let mut received = VecDeque::new();
        received
            .try_reserve_exact(120)
            .map_err(|_| AM_ERR_SDK_OUT_OF_MEMORY)?;
        if items.capacity() > maximum || received.capacity() > 120 {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        Ok(Self {
            items,
            bytes: 0,
            callback: None,
            user: 0,
            busy: false,
            last_received: i64::MIN,
            last_frame_id: 0,
            received,
            failures: 0,
            skipped: 0,
        })
    }
    fn remember(&mut self, last: i64) {
        while self.received.len() >= 120 || self.received.front().is_some_and(|t| *t < last - 5000)
        {
            self.received.pop_front();
        }
        self.received.push_back(last);
    }
}
fn reserve_consumer(
    state: &state::State,
    camera: &Arc<CameraState>,
    source: u64,
    interval: u32,
    max: usize,
) -> Result<()> {
    state
        .continuous
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < max).then_some(n + 1)
        })
        .map_err(|_| AM_ERR_CAMERA_CAPTURE_CAPACITY)?;
    // 注册锁失败还没有Subscription拥有许可，须在此归还容量。
    let result = (|| {
        let mut consumers = lock(&state.consumers)?;
        if let Some(count) = consumers.get_mut(&(source, interval)) {
            *count += 1;
        } else {
            consumers.try_insert(camera, (source, interval), 1, max)?;
        }
        Ok(())
    })();
    if result.is_err() {
        state.continuous.fetch_sub(1, Ordering::AcqRel);
    }
    result
}
pub struct Subscription {
    source: Arc<SharedSource>,
    state: Arc<state::State>,
    config: am_capture_continuous_desc_t,
    queue: Mutex<Queue>,
    changed: Condvar,
    stop: AtomicBool,
    alive: AtomicBool,
    key: AtomicUsize,
    next: AtomicU64,
    waiters: AtomicUsize,
    reserved: AtomicBool,
    _charge: ControlCharge,
}
impl Managed for Subscription {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn request_close(&self) {
        self.stop.store(true, Ordering::Release);
        if let Ok(mut q) = self.queue.lock() {
            q.items.clear();
            q.bytes = 0;
            q.callback = None;
        }
        self.changed.notify_all();
    }
    fn drained(&self) -> bool {
        !self.alive.load(Ordering::Acquire)
    }
    fn close_resources(&self) {
        self.release_capacity();
    }
}
impl Drop for Subscription {
    fn drop(&mut self) {
        self.release_capacity();
    }
}
impl Subscription {
    pub(super) fn queue_usage(&self) -> Result<(u64, u64)> {
        let queue = lock(&self.queue)?;
        Ok((queue.items.len() as u64, queue.bytes))
    }
    fn release_capacity(&self) {
        if self.reserved.swap(false, Ordering::AcqRel) {
            self.state.continuous.fetch_sub(1, Ordering::AcqRel);
            self.state
                .remove_consumer((self.source.source_id, self.config.target_interval_ms));
        }
    }
}
fn get(key: usize) -> Result<Arc<am_sdk_support::handles::Entry>> {
    let entry = domain().get(key)?;
    entry.data::<Subscription>()?;
    Ok(entry)
}
#[no_mangle]
pub unsafe extern "C" fn amc_capture_continuous_start(
    src: *mut amc_source_t,
    desc: *const am_capture_continuous_desc_t,
    out: *mut *mut amc_capture_sub_t,
) -> i32 {
    status(domain(), src as usize, || unsafe {
        am_sdk_support::callbacks::reject_reentry()?;
        args::pointer(out)?;
        out.write(std::ptr::null_mut());
        let mut config = args::read(desc, size_of::<am_capture_continuous_desc_t>())?;
        kind(config.output_kind)?;
        if config.mode > AM_SUB_PULL || config.reserved.iter().any(|n| *n != 0) {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        if config.target_interval_ms == 0 {
            config.target_interval_ms = 1000;
        }
        if config.max_inflight == 0 {
            config.max_inflight = 1;
        }
        if config.request_timeout_ms == 0 {
            config.request_timeout_ms = SDK_CAPTURE_TIMEOUT_MS as u32;
        }
        if config.queue_max_items == 0 {
            config.queue_max_items = 2;
        }
        if config.queue_max_bytes == 0 {
            config.queue_max_bytes = 32 * 1024 * 1024;
        }
        if config.target_interval_ms < 20
            || config.target_interval_ms > 60000
            || config.max_inflight > 4
            || config.queue_max_items > 4
            || config.queue_max_bytes > 128 * 1024 * 1024
        {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        deadline(config.request_timeout_ms)?;
        let _active = domain().active(src as usize)?;
        let entry = sources::get(src as usize)?;
        let source = entry.data::<Source>()?.shared.clone();
        if !source.isapi {
            return Err(AM_ERR_CAMERA_MODE_CONFLICT);
        }
        let state = state::State::get(&source.state)?;
        let charge = source
            .state
            .control(metadata_bytes(config.queue_max_items as usize), false)?;
        let queue = Queue::new(config.queue_max_items as usize)?;
        let max = source.state.limits()?.max_capture_continuous as usize;
        reserve_consumer(
            &state,
            &source.state,
            source.source_id,
            config.target_interval_ms,
            max,
        )?;
        let sub = Arc::new(Subscription {
            source,
            state,
            config,
            queue: Mutex::new(queue),
            changed: Condvar::new(),
            stop: AtomicBool::new(false),
            alive: AtomicBool::new(false),
            key: AtomicUsize::new(0),
            next: AtomicU64::new(1),
            waiters: AtomicUsize::new(0),
            reserved: AtomicBool::new(true),
            _charge: charge,
        });
        let key = domain().insert(src as usize, sub.clone())?;
        let registered = (|| -> Result<()> {
            let mut subscriptions = lock(&sub.state.subscriptions)?;
            subscriptions.retain(|_, sub| {
                sub.upgrade()
                    .is_some_and(|s| s.reserved.load(Ordering::Acquire))
            });
            subscriptions.try_insert(&sub.source.state, key, Arc::downgrade(&sub), max)?;
            Ok(())
        })();
        if let Err(code) = registered {
            domain().close(key, 0)?;
            domain().destroy(key)?;
            return Err(code);
        }
        sub.key.store(key, Ordering::Release);
        sub.alive.store(true, Ordering::Release);
        if let Err(code) = launch(sub.clone()) {
            sub.alive.store(false, Ordering::Release);
            domain().close(key, 0)?;
            domain().destroy(key)?;
            return Err(code);
        }
        out.write(key as *mut _);
        Ok(())
    })
}
fn launch(sub: Arc<Subscription>) -> Result<()> {
    std::thread::Builder::new()
        .name("amc-isapi-sub".into())
        .spawn(move || {
            struct Running(Arc<Subscription>);
            impl Drop for Running {
                fn drop(&mut self) {
                    self.0.alive.store(false, Ordering::Release);
                    self.0.changed.notify_all();
                }
            }
            let _running = Running(sub.clone());
            let interval = Duration::from_millis(sub.config.target_interval_ms as u64);
            let mut due = Instant::now();
            while !sub.stop.load(Ordering::Acquire)
                && !sub.source.stop.load(Ordering::Acquire)
                && !sub.state.closed.load(Ordering::Acquire)
            {
                if Instant::now() < due {
                    let Ok(q) = sub.queue.lock() else { break };
                    let Ok(_) = sub
                        .changed
                        .wait_timeout(q, due.saturating_duration_since(Instant::now()))
                    else {
                        break;
                    };
                    continue;
                }
                let result = images::Image::continuous(
                    &sub.state,
                    &sub.source,
                    sub.config.output_kind,
                    Duration::from_millis(sub.config.request_timeout_ms as u64),
                    sub.config.target_interval_ms,
                );
                if sub.stop.load(Ordering::Acquire) || sub.state.closed.load(Ordering::Acquire) {
                    break;
                }
                if let Ok(mut q) = sub.queue.lock() {
                    match result {
                        Ok(image) if image.raw.meta.frame_id == q.last_frame_id => {}
                        Ok(image) => {
                            q.last_frame_id = image.raw.meta.frame_id;
                            q.failures = 0;
                            q.last_received = image.raw.meta.received_at_ms;
                            let last = q.last_received;
                            q.remember(last);
                            let bytes = image.bytes();
                            while !q.items.is_empty()
                                && (q.items.len() >= sub.config.queue_max_items as usize
                                    || bytes > sub.config.queue_max_bytes.saturating_sub(q.bytes))
                            {
                                if let Some((_, old)) = q.items.pop_front() {
                                    q.bytes -= old.bytes();
                                }
                                sub.source
                                    .state
                                    .metrics
                                    .dropped
                                    .fetch_add(1, Ordering::AcqRel);
                            }
                            if bytes <= sub.config.queue_max_bytes {
                                q.bytes += bytes;
                                q.items
                                    .push_back((sub.next.fetch_add(1, Ordering::AcqRel), image));
                            } else {
                                sub.source
                                    .state
                                    .metrics
                                    .dropped
                                    .fetch_add(1, Ordering::AcqRel);
                            }
                        }
                        Err(_) => q.failures = q.failures.saturating_add(1),
                    }
                }
                sub.changed.notify_all();
                deliver(&sub);
                due += interval;
                if due < Instant::now() {
                    let missed = (Instant::now().duration_since(due).as_millis()
                        / interval.as_millis()) as u64
                        + 1;
                    if let Ok(mut q) = sub.queue.lock() {
                        q.skipped = q.skipped.saturating_add(missed);
                    }
                    due = Instant::now() + interval;
                }
            }
        })
        .map(|_| ())
        .map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)
}
fn deliver(sub: &Arc<Subscription>) {
    if sub.config.mode != AM_SUB_PUSH {
        return;
    }
    let item = (|| -> Result<_> {
        let mut q = lock(&sub.queue)?;
        let Some(cb) = q.callback else {
            return Ok(None);
        };
        let Some((id, image)) = q.items.front() else {
            return Ok(None);
        };
        let out = images::transfer(image.clone(), &sub.source.state, *id)?;
        let bytes = image.bytes();
        q.items.pop_front();
        q.bytes -= bytes;
        q.busy = true;
        Ok(Some((cb, q.user, out)))
    })();
    if let Ok(Some((callback, user, image))) = item {
        let _scope = am_sdk_support::callbacks::Scope::enter();
        unsafe {
            callback(
                sub.key.load(Ordering::Acquire) as *mut _,
                image,
                user as *mut c_void,
            );
        }
        let _ = images::release(image as usize, false);
        if let Ok(mut q) = sub.queue.lock() {
            q.busy = false;
        }
        sub.changed.notify_all();
    }
}
#[no_mangle]
pub unsafe extern "C" fn amc_capture_continuous_stop(
    sub: *mut amc_capture_sub_t,
    timeout: u32,
    out: *mut u32,
) -> i32 {
    status(domain(), sub as usize, || unsafe {
        am_sdk_support::callbacks::reject_reentry()?;
        args::pointer(out)?;
        get(sub as usize)?;
        out.write(domain().close(sub as usize, timeout)?);
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_capture_sub_set_callback(
    sub: *mut amc_capture_sub_t,
    callback: am_capture_cb_fn,
    user: *mut c_void,
) -> i32 {
    status(domain(), sub as usize, || {
        am_sdk_support::callbacks::reject_reentry()?;
        let entry = domain().active(sub as usize)?;
        let sub = entry.data::<Subscription>()?;
        if sub.config.mode != AM_SUB_PUSH {
            return Err(AM_ERR_CAMERA_MODE_CONFLICT);
        }
        let mut q = lock(&sub.queue)?;
        q.callback = None;
        while q.busy {
            q = sub.changed.wait(q).map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        }
        q.items.clear();
        q.bytes = 0;
        q.callback = callback;
        q.user = user as usize;
        sub.changed.notify_all();
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_capture_sub_pull(
    sub: *mut amc_capture_sub_t,
    timeout: u32,
    out: *mut *mut am_captured_image_t,
) -> i32 {
    status(domain(), sub as usize, || unsafe {
        args::pointer(out)?;
        out.write(std::ptr::null_mut());
        if timeout > 30000 {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        if timeout > 0 {
            am_sdk_support::callbacks::reject_reentry()?;
        }
        let entry = domain().active(sub as usize)?;
        let sub = entry.data::<Subscription>()?;
        if sub.config.mode != AM_SUB_PULL {
            return Err(AM_ERR_CAMERA_MODE_CONFLICT);
        }
        let policy = {
            let context = sub
                .source
                .state
                .ctx
                .data::<am_sdk_support::context::Context>()?;
            let p = lock(&context.queues)?[0];
            p
        };
        if timeout > 0 && timeout > policy.wait_deadline_ms {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        sub.waiters
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < policy.max_waiters as usize || timeout == 0).then_some(n + 1)
            })
            .map_err(|_| AM_ERR_SDK_QUEUE_FULL)?;
        struct Waiter<'a>(&'a AtomicUsize);
        impl Drop for Waiter<'_> {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::AcqRel);
            }
        }
        let _waiter = Waiter(&sub.waiters);
        let until = Instant::now() + Duration::from_millis(timeout as u64);
        let mut q = lock(&sub.queue)?;
        loop {
            if sub.stop.load(Ordering::Acquire) {
                return Err(AM_ERR_SDK_HANDLE_CLOSED);
            }
            if let Some((id, image)) = q.items.front() {
                let output = images::transfer(image.clone(), &sub.source.state, *id)?;
                let bytes = image.bytes();
                q.items.pop_front();
                q.bytes -= bytes;
                out.write(output);
                return Ok(());
            }
            let remaining = until.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(if timeout == 0 {
                    AM_ERR_SDK_TRY_AGAIN
                } else {
                    AM_ERR_SDK_DEADLINE_EXCEEDED
                });
            }
            q = sub
                .changed
                .wait_timeout(q, remaining)
                .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?
                .0;
        }
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_capture_sub_get_stats(
    sub: *mut amc_capture_sub_t,
    out: *mut am_capture_stats_t,
) -> i32 {
    status(domain(), sub as usize, || unsafe {
        let entry = get(sub as usize)?;
        let sub = entry.data::<Subscription>()?;
        let q = lock(&sub.queue)?;
        let fps = match (q.received.front(), q.received.back()) {
            (Some(first), Some(last)) if last > first => {
                if monotonic_ms().saturating_sub(*last) > 5000 {
                    0.0
                } else {
                    (q.received.len() - 1) as f64 * 1000.0 / (last - first) as f64
                }
            }
            _ => 0.0,
        };
        sub.source
            .state
            .metrics
            .capture_fps_bits
            .store(fps.to_bits(), Ordering::Release);
        let consumers = lock(&sub.state.consumers)?
            .iter()
            .filter(|((source, _), _)| *source == sub.source.source_id)
            .map(|(_, count)| *count as u64)
            .sum();
        args::write(
            out,
            am_capture_stats_t {
                actual_fps: fps,
                last_received_at_ms: q.last_received,
                consecutive_failures: q.failures,
                newest_age_ms: if q.last_received == i64::MIN {
                    i64::MIN
                } else {
                    monotonic_ms().saturating_sub(q.last_received)
                },
                skipped_beats: q.skipped,
                shared_consumers: consumers,
                ..Default::default()
            },
        )
    })
}

#[cfg(test)]
mod storage_tests;
