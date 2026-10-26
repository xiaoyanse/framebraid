//! 执行器自己的容器也占域总额，扩容时保留旧存储的重叠用量。
use super::*;
use std::mem::size_of;

impl Data {
    pub(super) fn new(budget: ThreadBudget) -> Result<Self> {
        if budget.slots == 0
            || budget.slots > SDK_DOMAIN_INFER_SLOTS
            || budget.parallel == 0
            || budget.parallel > 4
            || budget.parallel > budget.total
        {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        let storage_bytes = size_of::<Engine>()
            + budget.slots as usize * size_of::<Job>()
            + budget.parallel as usize * size_of::<std::thread::JoinHandle<()>>();
        if storage_bytes > budget::EXECUTOR_BYTES {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        let mut queue = VecDeque::new();
        queue
            .try_reserve_exact(budget.slots as usize)
            .map_err(|_| AM_ERR_SDK_OUT_OF_MEMORY)?;
        if queue.capacity() > budget.slots as usize {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        Ok(Self {
            queue,
            storage_bytes,
            retirement_pending: false,
            contexts: Vec::new(),
            inflight: 0,
            bytes: 0,
            extra_threads: 0,
            last: None,
            waiters: 0,
            models: 0,
        })
    }

    pub(super) fn register_context(&mut self, state: &Arc<State>) -> Result<()> {
        self.contexts.retain(|value| value.strong_count() > 0);
        let reference = Arc::downgrade(state);
        if self.contexts.iter().any(|value| value.ptr_eq(&reference)) {
            return Ok(());
        }
        if self.contexts.len() == self.contexts.capacity() {
            let capacity = self
                .contexts
                .len()
                .checked_add(1)
                .and_then(usize::checked_next_power_of_two)
                .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?
                .max(4);
            let bytes = capacity
                .checked_mul(size_of::<Weak<State>>())
                .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
            let peak = self
                .storage_bytes
                .checked_add(self.bytes)
                .and_then(|value| value.checked_add(bytes));
            if peak.is_none_or(|value| value > budget::EXECUTOR_BYTES) {
                return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
            }
            let retained = self
                .storage_bytes
                .checked_sub(self.contexts.capacity() * size_of::<Weak<State>>())
                .ok_or(AM_ERR_SDK_INTERNAL_ERROR)?;
            let mut next = Vec::new();
            next.try_reserve_exact(capacity)
                .map_err(|_| AM_ERR_SDK_OUT_OF_MEMORY)?;
            if next.capacity() > capacity {
                return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
            }
            next.append(&mut self.contexts);
            self.contexts = next;
            self.storage_bytes = retained + bytes;
        }
        self.contexts.push(reference);
        Ok(())
    }
}

pub(super) fn thread_slots(count: usize) -> Result<Vec<std::thread::JoinHandle<()>>> {
    let mut threads = Vec::new();
    threads
        .try_reserve_exact(count)
        .map_err(|_| AM_ERR_SDK_OUT_OF_MEMORY)?;
    if threads.capacity() > count {
        return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
    }
    Ok(threads)
}

pub(super) fn purge_contexts(engine: &Engine) {
    let (mut begin, mut remaining) = (0usize, None);
    loop {
        let states: [Option<Arc<State>>; 16] = {
            let Ok(data) = engine.data.lock() else { return };
            let count = *remaining.get_or_insert(data.contexts.len());
            if begin >= count {
                return;
            }
            std::array::from_fn(|i| {
                (begin + i < count)
                    .then(|| data.contexts.get(begin + i).and_then(Weak::upgrade))
                    .flatten()
            })
        };
        // 不持执行器锁查records，不为一个繁忙Context阻塞其他工作。
        for state in states.into_iter().flatten() {
            if let Ok(mut records) = state.records.try_lock() {
                state.purge(&mut records);
            }
        }
        begin += 16;
        if remaining.is_some_and(|count| begin >= count) {
            return;
        }
    }
}
