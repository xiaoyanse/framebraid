mod admission;
mod batching;
pub(super) use batching::{
    configure as batching_configure, count_rejected as batching_count_rejected,
    enabled as batching_enabled, release as batching_release, snapshot as batching_snapshot,
};
mod budget;
mod execution;
mod native_execution;
mod retired;
mod storage;
use super::{
    error, lock,
    models::ModelHandle,
    results::ResultBuffer,
    state::{Record, State},
    submission::{Input, Plan},
};
use am_sdk_support::{
    abi::*,
    context::{Context, ThreadBudget},
    handles::Entry,
    Result,
};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex, Weak,
    },
};

static THREADS_PEAK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static ENGINE: Mutex<Option<Arc<Engine>>> = Mutex::new(None);
#[cfg(test)]
static TEST_ENGINE_LOCK: Mutex<()> = Mutex::new(());
struct Data {
    queue: VecDeque<Job>,
    storage_bytes: usize,
    retirement_pending: bool,
    contexts: Vec<Weak<State>>,
    inflight: usize,
    bytes: usize,
    extra_threads: usize,
    last: Option<(usize, u64)>,
    waiters: usize,
    models: usize,
}
struct Engine {
    data: Mutex<Data>,
    wake: Condvar,
    budget: ThreadBudget,
    stop: AtomicBool,
    threads: Mutex<Vec<std::thread::JoinHandle<()>>>,
}
pub struct ModelPermit {
    engine: Arc<Engine>,
    extra: usize,
}
impl ModelPermit {
    // 一个加载许可内调整候选额度，不创建第二执行器、不丢弃上一候选仍在读取的资源。
    pub fn set_extra(&mut self, extra: usize) -> Result<()> {
        let mut data = lock(&self.engine.data)?;
        let next = data.extra_threads - self.extra;
        let next = next
            .checked_add(extra)
            .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        let total = next
            .checked_add(self.engine.budget.parallel as usize)
            .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        if total > self.engine.budget.total as usize {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        data.extra_threads = next;
        self.extra = extra;
        THREADS_PEAK.fetch_max(total, Ordering::AcqRel);
        Ok(())
    }
    pub fn add_callback_thread(&mut self) -> Result<()> {
        let mut data = lock(&self.engine.data)?;
        if data.extra_threads + 1 + self.engine.budget.parallel as usize
            > self.engine.budget.total as usize
        {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        data.extra_threads += 1;
        THREADS_PEAK.fetch_max(
            data.extra_threads + self.engine.budget.parallel as usize,
            Ordering::AcqRel,
        );
        self.extra += 1;
        Ok(())
    }
    pub fn undo_callback_thread(&mut self) {
        if let Ok(mut data) = self.engine.data.lock() {
            data.extra_threads -= 1;
            self.extra -= 1;
        }
    }
}
impl Drop for ModelPermit {
    fn drop(&mut self) {
        release_model(&self.engine, self.extra);
    }
}
pub fn reserve_model(budget: ThreadBudget, extra: usize) -> Result<ModelPermit> {
    if extra
        .checked_add(budget.parallel as usize)
        .is_none_or(|n| n > budget.total as usize)
    {
        return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
    }
    let mut global = lock(&ENGINE)?;
    if global.is_none() {
        let data = Data::new(budget)?;
        let mut threads = storage::thread_slots(budget.parallel as usize)?;
        let engine = Arc::new(Engine {
            data: Mutex::new(data),
            wake: Condvar::new(),
            budget,
            stop: AtomicBool::new(false),
            threads: Mutex::default(),
        });
        for index in 0..budget.parallel {
            let owned = engine.clone();
            match std::thread::Builder::new()
                .name(format!("am-sdk-infer-{index}"))
                .spawn(move || execution::worker(owned))
            {
                Ok(t) => threads.push(t),
                Err(_) => {
                    engine.stop.store(true, Ordering::Release);
                    engine.wake.notify_all();
                    for t in threads {
                        let _ = t.join();
                    }
                    return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
                }
            }
        }
        *lock(&engine.threads)? = threads;
        *global = Some(engine);
    }
    let engine = global.as_ref().ok_or(AM_ERR_SDK_INTERNAL_ERROR)?;
    if engine.budget != budget {
        return Err(AM_ERR_SDK_DOMAIN_CONFLICT);
    }
    let mut data = lock(&engine.data)?;
    if data.extra_threads + extra + budget.parallel as usize > budget.total as usize {
        return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
    }
    data.extra_threads += extra;
    data.models += 1;
    THREADS_PEAK.fetch_max(
        data.extra_threads + budget.parallel as usize,
        Ordering::AcqRel,
    );
    Ok(ModelPermit {
        engine: engine.clone(),
        extra,
    })
}
fn release_model(engine: &Arc<Engine>, extra: usize) {
    let Ok(mut global) = ENGINE.lock() else {
        return;
    };
    if !global.as_ref().is_some_and(|e| Arc::ptr_eq(e, engine)) {
        return;
    }
    let last = if let Ok(mut data) = engine.data.lock() {
        data.extra_threads -= extra;
        data.models -= 1;
        data.models == 0
    } else {
        false
    };
    if last {
        // 最后模型会话已释放；持域锁完成join，防止新旧执行器短暂重叠超出预算。
        engine.stop.store(true, Ordering::Release);
        engine.wake.notify_all();
        if let Ok(mut threads) = engine.threads.lock() {
            for thread in threads.drain(..) {
                let _ = thread.join();
            }
        }
        *global = None;
    }
}
pub fn wake() {
    let engine = ENGINE.lock().ok().and_then(|global| global.clone());
    if let Some(engine) = engine {
        // 取消修改的是records；与wait的域锁握手，避免检查结束到睡眠之间丢唤醒。
        if let Ok(mut data) = engine.data.lock() {
            data.retirement_pending = true;
            engine.wake.notify_all();
        }
    }
}
fn engine() -> Result<Arc<Engine>> {
    lock(&ENGINE)?.clone().ok_or(AM_ERR_SDK_INTERNAL_ERROR)
}
pub(super) struct Admission {
    engine: Arc<Engine>,
    usage: Arc<super::state::Usage>,
    class: usize,
    bytes: usize,
    items: usize,
}
impl Admission {
    /// 多项合并（攒批窗口排出）时的合计计费基线：同源核心组批预算按合计输入计。
    pub(super) fn totals(guards: &[Admission]) -> (usize, usize) {
        guards.iter().fold((0, 0), |(items, bytes), a| {
            (items.saturating_add(a.items), bytes.saturating_add(a.bytes))
        })
    }
}
impl Drop for Admission {
    fn drop(&mut self) {
        if let Ok(mut d) = self.engine.data.lock() {
            if let Ok(mut used) = self.usage.preprocess.lock() {
                d.inflight -= self.items;
                d.bytes -= self.bytes;
                used[self.class] -= self.bytes;
            }
        }
        self.engine.wake.notify_all();
    }
}
struct Job {
    model: Arc<Entry>,
    input: Payload,
    id: u64,
    params: am_infer_params_t,
    parallel: usize,
    _admissions: Vec<Admission>,
}
enum Payload {
    Single(Input),
    Native { inputs: Vec<Input>, ids: Vec<u64> },
}
// params中raw_preprocess不跨线程使用，接受前已复制进Input；其余字段是纯值。
unsafe impl Send for Job {}
pub unsafe fn submit(entry: Arc<Entry>, plan: Plan) -> Result<u64> {
    let result = unsafe { admission::submit(entry.clone(), &[&plan], false, false, true, None) };
    if result
        .as_ref()
        .is_err_and(|e| matches!(*e, AM_ERR_SDK_QUEUE_FULL | AM_ERR_SDK_RESOURCE_EXHAUSTED))
    {
        let model = entry.data::<ModelHandle>()?;
        model.state.metrics.rejected.fetch_add(1, Ordering::AcqRel);
        if batching::enabled(model) {
            batching::count_rejected(model.info.key.load(Ordering::Acquire), 1);
        }
    }
    let ids = result?;
    ids.first().copied().ok_or(AM_ERR_SDK_INTERNAL_ERROR)
}
pub(super) unsafe fn submit_offline(
    entry: Arc<Entry>,
    plans: &[&Plan],
    native: bool,
    auto: bool,
    deadline: Option<std::time::Instant>,
) -> Result<Vec<u64>> {
    unsafe { admission::submit(entry, plans, native, true, native && auto, deadline) }
}
pub(super) fn reserve_metadata(state: &State, bytes: usize) -> Result<Admission> {
    let e = engine()?;
    let ctx = state.context()?;
    let c = ctx.data::<Context>()?;
    let limits = *lock(&c.limits)?;
    let policy = lock(&c.queues)?[AM_Q_OFFLINE_BATCH as usize];
    budget::reserve(e, state.usage.clone(), 1, 0, bytes, &limits, &policy)
}
pub(super) struct Waiter {
    engine: Arc<Engine>,
    state: Arc<State>,
}
impl Drop for Waiter {
    fn drop(&mut self) {
        if let Ok(mut d) = self.engine.data.lock() {
            d.waiters -= 1;
        }
        self.state.usage.waiters.fetch_sub(1, Ordering::AcqRel);
    }
}
pub(super) fn waiter(state: Arc<State>, maximum: usize) -> Result<Waiter> {
    let e = engine()?;
    let mut d = lock(&e.data)?;
    let max = maximum.min(16);
    if d.waiters >= 16 || state.usage.waiters.load(Ordering::Acquire) >= max {
        return Err(AM_ERR_SDK_QUEUE_FULL);
    }
    d.waiters += 1;
    state.usage.waiters.fetch_add(1, Ordering::AcqRel);
    Ok(Waiter {
        engine: e.clone(),
        state,
    })
}
pub(super) fn wait(duration: std::time::Duration) -> Result<()> {
    let e = engine()?;
    let data = lock(&e.data)?;
    let _ = e
        .wake
        .wait_timeout(data, duration.min(std::time::Duration::from_millis(50)))
        .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
    Ok(())
}

#[cfg(test)]
mod tests;

pub fn snapshot(state: &Arc<State>) -> Result<(usize, usize, usize, usize)> {
    let global = lock(&ENGINE)?;
    let Some(e) = global.as_ref() else {
        return Ok((0, 0, 0, THREADS_PEAK.load(Ordering::Acquire)));
    };
    let d = lock(&e.data)?;
    let (items, bytes) = d
        .queue
        .iter()
        .filter(|j| {
            j.model
                .data::<ModelHandle>()
                .is_ok_and(|m| Arc::ptr_eq(&m.state, state))
        })
        .fold((0, 0), |(items, bytes), job| {
            let (i, b) = Admission::totals(&job._admissions);
            (items + i, bytes + b)
        });
    Ok((
        items,
        bytes,
        d.extra_threads + e.budget.parallel as usize,
        THREADS_PEAK.load(Ordering::Acquire),
    ))
}

#[cfg(test)]
mod storage_tests;
