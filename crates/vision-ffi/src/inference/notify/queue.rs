use super::*;

impl Data {
    pub(super) fn prepare(&mut self, state: &State) -> am_sdk_support::Result<()> {
        if self.busy || !self.queue.is_empty() || self.queue_storage.is_some() {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        let entry = state.context()?;
        let context = entry.data::<Context>()?;
        let limits = *lock(&context.limits)?;
        let policy = lock(&context.queues)?[AM_Q_CONTROL as usize];
        // 存储许可也占一个控制项；不预占其他对象已使用的额度。
        let slots = (limits.control_max_items.min(policy.max_items) as usize)
            .saturating_sub(state.usage.snapshots.load(Ordering::Acquire))
            .saturating_sub(1);
        let available = policy
            .max_bytes
            .saturating_sub(state.usage.control_bytes.load(Ordering::Acquire) as u64);
        let maximum = slots.min((available / size_of::<Notification>() as u64) as usize);
        if maximum == 0 {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        let storage = state.cached_control(maximum * size_of::<Notification>())?;
        let mut queue = VecDeque::new();
        queue
            .try_reserve_exact(maximum)
            .map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        if queue.capacity() > maximum {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        self.queue = queue;
        self.queue_storage = Some(storage);
        Ok(())
    }

    pub(super) fn push(
        &mut self,
        state: &State,
        id: u64,
        terminal: u32,
    ) -> am_sdk_support::Result<()> {
        // 忙回调仍占一个已预留槽；入队不能触发VecDeque扩容。
        if self.queue_storage.is_none()
            || self.queue.len() + usize::from(self.busy) >= self.queue.capacity()
        {
            return Err(AM_ERR_SDK_QUEUE_FULL);
        }
        let charge = reserve_notification(state)?;
        self.queue.push_back((id, terminal, charge));
        Ok(())
    }

    pub(super) fn release_storage_if_idle(&mut self) {
        if !self.busy && self.queue.is_empty() {
            // 先销毁缓冲，再归还其额度。
            self.queue = VecDeque::new();
            self.queue_storage.take();
        }
    }
}
