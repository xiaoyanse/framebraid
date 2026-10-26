use super::*;
use std::mem::size_of;

impl State {
    pub(super) fn prepare(&mut self, budget: &Arc<CameraState>) -> Result<()> {
        if self.busy || !self.queue.is_empty() || self.queue_storage.is_some() {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        let limits = budget.limits()?;
        let context = budget.ctx.data::<am_sdk_support::context::Context>()?;
        let policy = context
            .queues
            .lock()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?[AM_Q_CONTROL as usize];
        let (used_items, used_bytes) = budget.control_usage_snapshot()?;
        let maximum = (limits.control_max_items.min(policy.max_items) as usize)
            .saturating_sub(used_items)
            .saturating_sub(1)
            .min((policy.max_bytes as usize).saturating_sub(used_bytes) / size_of::<Event>())
            .min(64);
        if maximum == 0 {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        let storage = budget.control(maximum * size_of::<Event>(), false)?;
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

    pub(super) fn release_storage_if_idle(&mut self) {
        if !self.busy && self.queue.is_empty() {
            // 先销毁引用槽，再归还包含空闲容量的额度。
            self.queue = VecDeque::new();
            self.queue_storage.take();
        }
    }
}
