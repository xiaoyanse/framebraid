//! Media的Context工作槽：创建前准入，真实线程返回后归还。
use am_sdk_support::{abi::*, args, callbacks, status, Result};
use std::sync::{Arc, Mutex};

struct Counts {
    limit: u32,
    active: u32,
    peak: u32,
}
pub(crate) struct Budget(Mutex<Counts>);
impl Default for Budget {
    fn default() -> Self {
        Self(Mutex::new(Counts {
            limit: AM_MEDIA_WORKER_LIMIT,
            active: 0,
            peak: 0,
        }))
    }
}
pub(crate) struct Permit(Arc<Budget>);
impl Drop for Permit {
    fn drop(&mut self) {
        if let Ok(mut counts) = self.0 .0.lock() {
            counts.active -= 1;
        }
    }
}
impl Budget {
    pub(crate) fn reserve(self: &Arc<Self>) -> Result<Permit> {
        let mut counts = self.0.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        if counts.active >= counts.limit {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        counts.active += 1;
        counts.peak = counts.peak.max(counts.active);
        Ok(Permit(self.clone()))
    }
    pub fn snapshot(&self) -> Result<(u32, u32)> {
        let counts = self.0.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        Ok((counts.active, counts.peak))
    }
    pub(crate) fn limit(&self) -> Result<u32> {
        Ok(self.0.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?.limit)
    }
    fn set(&self, limit: u32) -> Result<()> {
        let mut counts = self.0.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        if limit > counts.limit {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        if limit < counts.active {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        counts.limit = limit;
        Ok(())
    }
    pub fn spawn<F, T>(self: &Arc<Self>, name: &str, work: F) -> Result<std::thread::JoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let permit = self.reserve()?;
        std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let _permit = permit;
                work()
            })
            .map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)
    }
    pub fn spawn_scoped<'scope, 'env, F, T>(
        self: &Arc<Self>,
        scope: &'scope std::thread::Scope<'scope, 'env>,
        name: &str,
        work: F,
    ) -> Result<std::thread::ScopedJoinHandle<'scope, T>>
    where
        F: FnOnce() -> T + Send + 'scope,
        T: Send + 'scope,
    {
        self.reserve()?.spawn_scoped(scope, name, work)
    }
}
impl Permit {
    pub fn spawn_scoped<'scope, 'env, F, T>(
        self,
        scope: &'scope std::thread::Scope<'scope, 'env>,
        name: &str,
        work: F,
    ) -> Result<std::thread::ScopedJoinHandle<'scope, T>>
    where
        F: FnOnce() -> T + Send + 'scope,
        T: Send + 'scope,
    {
        std::thread::Builder::new()
            .name(name.into())
            .spawn_scoped(scope, move || {
                let _permit = self;
                work()
            })
            .map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)
    }
}

#[no_mangle]
pub unsafe extern "C" fn amc_get_worker_budget(ctx: *mut amc_context_t, out: *mut u32) -> i32 {
    status(crate::domain(), ctx as usize, || unsafe {
        args::pointer(out)?;
        out.write(0);
        let entry = crate::domain().get(ctx as usize)?;
        let _read = entry.readable()?;
        out.write(
            crate::frames::camera_state(ctx as usize)?
                .metrics
                .threads
                .limit()?,
        );
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_set_worker_budget(ctx: *mut amc_context_t, limit: u32) -> i32 {
    status(crate::domain(), ctx as usize, || {
        callbacks::reject_reentry()?;
        let _active = crate::domain().active(ctx as usize)?;
        crate::frames::camera_state(ctx as usize)?
            .metrics
            .threads
            .set(limit)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_budget_limits_all_permits_and_rejects_live_shrink_without_mutation() {
        let budget = Arc::new(Budget::default());
        assert_eq!(budget.limit(), Ok(64));
        budget.set(3).unwrap();
        let a = budget.reserve().unwrap();
        let b = budget.reserve().unwrap();
        assert_eq!(budget.set(1), Err(AM_ERR_SDK_RESOURCE_EXHAUSTED));
        assert_eq!(budget.limit(), Ok(3));
        let c = budget.reserve().unwrap();
        assert!(matches!(
            budget.reserve(),
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
        ));
        assert_eq!(budget.snapshot(), Ok((3, 3)));
        drop((a, b, c));
        budget.set(0).unwrap();
        assert_eq!(budget.snapshot(), Ok((0, 3)));
        assert!(matches!(
            budget.reserve(),
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
        ));
        assert_eq!(budget.set(1), Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE));
    }
    #[test]
    fn captured_work_is_dropped_on_refusal_and_panics_return_the_slot() {
        let first = Arc::new(Budget::default());
        let second = Arc::new(Budget::default());
        first.set(0).unwrap();
        let marker = Arc::new(());
        let captured = marker.clone();
        assert!(first.spawn("rejected", move || drop(captured)).is_err());
        assert_eq!(Arc::strong_count(&marker), 1);
        let worker = second
            .spawn("test-panic", || panic!("controlled worker unwind"))
            .unwrap();
        assert!(worker.join().is_err());
        assert_eq!(second.snapshot(), Ok((0, 1)));
        assert_eq!(first.snapshot(), Ok((0, 0)));
    }
    #[test]
    fn concurrent_scoped_threads_cannot_exceed_the_context_limit() {
        let budget = Arc::new(Budget::default());
        budget.set(2).unwrap();
        std::thread::scope(|scope| {
            let (tx, rx) = std::sync::mpsc::channel();
            let a = budget
                .spawn_scoped(scope, "a", move || rx.recv().unwrap())
                .unwrap();
            let (tx2, rx2) = std::sync::mpsc::channel();
            let b = budget
                .spawn_scoped(scope, "b", move || rx2.recv().unwrap())
                .unwrap();
            assert!(budget.spawn_scoped(scope, "c", || ()).is_err());
            assert_eq!(budget.snapshot(), Ok((2, 2)));
            tx.send(()).unwrap();
            tx2.send(()).unwrap();
            a.join().unwrap();
            b.join().unwrap();
        });
        assert_eq!(budget.snapshot(), Ok((0, 2)));
    }
}
