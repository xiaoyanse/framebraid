use crate::{abi::*, Result};
use std::{
    any::Any,
    collections::VecDeque,
    sync::{
        atomic::{AtomicI32, AtomicU32, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

pub trait Managed: Any + Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn request_close(&self) {}
    fn drained(&self) -> bool {
        true
    }
    fn close_resources(&self) {}
    fn is_context(&self) -> bool {
        false
    }
}

pub struct Entry {
    token: Box<u64>,
    pub parent: usize,
    pub body: Arc<dyn Managed>,
    pub state: AtomicU32,
    pub active: AtomicUsize,
    pub last_error: AtomicI32,
    pub(crate) error: Mutex<(i32, Option<crate::diagnostics::Detail>)>,
    finish: Mutex<()>,
}
impl Entry {
    pub fn data<T: Managed>(&self) -> Result<&T> {
        self.body
            .as_any()
            .downcast_ref()
            .ok_or(AM_ERR_SDK_HANDLE_INVALID)
    }
    /// 关闭中仍可生成只读快照，但必须与最终资源关闭互斥登记。
    pub fn readable(self: &Arc<Self>) -> Result<Active> {
        let _finish = self.finish.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        if self.state.load(Ordering::Acquire) == AM_LIFECYCLE_CLOSED as u32 {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        self.active.fetch_add(1, Ordering::AcqRel);
        Ok(Active(self.clone()))
    }
}
pub struct Active(pub Arc<Entry>);
impl Drop for Active {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::AcqRel);
    }
}
impl std::ops::Deref for Active {
    type Target = Entry;
    fn deref(&self) -> &Entry {
        &self.0
    }
}

#[derive(Default)]
struct Registry {
    entries: crate::registry::Table<Arc<Entry>>,
    quarantine: VecDeque<Box<u64>>,
    sequence: u64,
}
pub struct Domain {
    registry: Mutex<Registry>,
    pub vision: bool,
    pub shared_compiled: bool,
    pub buffers: crate::buffers::Buffers,
    pub thread_config: Mutex<Option<crate::context::ThreadBudget>>,
}
impl Domain {
    /// 卸载钩子不得替调用方关对象，也不等待仍占用注册表的线程。
    pub fn idle_for_unload(&self) -> bool {
        self.registry.try_lock().is_ok_and(|r| r.entries.is_empty())
            && self.buffers.idle_for_unload()
    }
    pub fn new(vision: bool) -> Self {
        Self::with_shared(vision, false)
    }
    pub fn with_shared(vision: bool, shared_compiled: bool) -> Self {
        Self {
            registry: Mutex::new(Registry::default()),
            vision,
            shared_compiled,
            buffers: Default::default(),
            thread_config: Mutex::new(None),
        }
    }
    pub fn insert(&self, parent: usize, body: Arc<dyn Managed>) -> Result<usize> {
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        if registry.entries.len() >= SDK_MAX_REGISTERED_HANDLES as usize {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        if body.is_context()
            && registry
                .entries
                .values()
                .filter(|e| e.body.is_context())
                .count()
                >= SDK_MAX_CONTEXTS as usize
        {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        if parent != 0 {
            let p = registry
                .entries
                .get(&parent)
                .ok_or(AM_ERR_SDK_HANDLE_INVALID)?;
            if p.state.load(Ordering::Acquire) != AM_LIFECYCLE_RUNNING as u32 {
                return Err(AM_ERR_SDK_HANDLE_CLOSED);
            }
        }
        registry.entries.prepare()?;
        if registry.quarantine.capacity() == 0 {
            registry
                .quarantine
                .try_reserve_exact(SDK_HANDLE_QUARANTINE as usize)
                .map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        }
        registry.sequence = registry
            .sequence
            .checked_add(1)
            .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        let mut token = Box::new(registry.sequence);
        let key = (&mut *token as *mut u64) as usize;
        registry.entries.insert(
            key,
            Arc::new(Entry {
                token,
                parent,
                body,
                state: AtomicU32::new(AM_LIFECYCLE_RUNNING as u32),
                active: AtomicUsize::new(0),
                last_error: AtomicI32::new(0),
                error: Mutex::new((0, None)),
                finish: Mutex::new(()),
            }),
        )?;
        Ok(key)
    }
    pub fn context_count(&self) -> Result<usize> {
        Ok(self
            .registry
            .lock()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?
            .entries
            .values()
            .filter(|e| e.body.is_context())
            .count())
    }
    pub fn get(&self, key: usize) -> Result<Arc<Entry>> {
        self.registry
            .lock()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?
            .entries
            .get(&key)
            .cloned()
            .ok_or(AM_ERR_SDK_HANDLE_INVALID)
    }
    pub fn active(&self, key: usize) -> Result<Active> {
        let entry = self.get(key)?;
        let finish = entry.finish.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        if entry.state.load(Ordering::Acquire) != AM_LIFECYCLE_RUNNING as u32 {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        entry.active.fetch_add(1, Ordering::AcqRel);
        drop(finish);
        Ok(Active(entry))
    }
    pub fn record_error(&self, key: usize, code: i32) {
        self.record_error_detail(key, code, None);
    }
    pub fn record_error_detail(
        &self,
        key: usize,
        code: i32,
        detail: Option<crate::diagnostics::Detail>,
    ) {
        if let Ok(entry) = self.get(key) {
            if let Ok(mut record) = entry.error.lock() {
                *record = (code, detail);
            }
            entry.last_error.store(code, Ordering::Release);
        }
    }
    fn children(
        &self,
        parent: usize,
        mut visit: impl FnMut(usize, &Arc<Entry>) -> Result<()>,
    ) -> Result<()> {
        let mut cursor = 0;
        loop {
            let mut block: [Option<(usize, Arc<Entry>)>; 32] = std::array::from_fn(|_| None);
            {
                let registry = self
                    .registry
                    .lock()
                    .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
                for (slot, (key, entry)) in block.iter_mut().zip(
                    registry
                        .entries
                        .after(cursor)
                        .filter(|(_, entry)| entry.parent == parent),
                ) {
                    *slot = Some((*key, entry.clone()));
                }
            }
            let mut found = false;
            for (child, entry) in block.into_iter().flatten() {
                found = true;
                cursor = child;
                visit(child, &entry)?;
            }
            if !found {
                return Ok(());
            }
        }
    }
    pub fn request_close(&self, key: usize) -> Result<()> {
        let entry = self.get(key)?;
        match entry.state.compare_exchange(
            AM_LIFECYCLE_RUNNING as u32,
            AM_LIFECYCLE_CLOSING as u32,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Err(state) if state == AM_LIFECYCLE_CLOSED as u32 => return Ok(()),
            _ => {}
        }
        entry.body.request_close();
        self.children(key, |child, _entry| self.request_close(child))?;
        Ok(())
    }
    pub fn state(&self, key: usize) -> Result<u32> {
        let entry = self.get(key)?;
        if entry.state.load(Ordering::Acquire) == AM_LIFECYCLE_CLOSING as u32 {
            let mut done = true;
            self.children(key, |child, _entry| {
                done &= self.state(child)? == AM_LIFECYCLE_CLOSED as u32;
                Ok(())
            })?;
            if done && entry.active.load(Ordering::Acquire) == 0 && entry.body.drained() {
                let _finish = entry.finish.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
                if entry.state.load(Ordering::Acquire) != AM_LIFECYCLE_CLOSED as u32
                    && entry.active.load(Ordering::Acquire) == 0
                    && entry.body.drained()
                {
                    entry.body.close_resources();
                    entry
                        .state
                        .store(AM_LIFECYCLE_CLOSED as u32, Ordering::Release);
                }
            }
        }
        Ok(entry.state.load(Ordering::Acquire))
    }
    pub fn close(&self, key: usize, timeout_ms: u32) -> Result<u32> {
        if timeout_ms as u64 > SDK_SHUTDOWN_TIMEOUT_MS {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        self.request_close(key)?;
        let until = Instant::now() + Duration::from_millis(timeout_ms as u64);
        loop {
            let state = self.state(key)?;
            if state == AM_LIFECYCLE_CLOSED as u32 || Instant::now() >= until {
                return Ok(state);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    pub fn destroy(&self, key: usize) -> Result<()> {
        if self.state(key)? != AM_LIFECYCLE_CLOSED as u32 {
            return Err(AM_ERR_SDK_HANDLE_INVALID);
        }
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        if registry.entries.values().any(|e| e.parent == key) {
            return Err(AM_ERR_SDK_HANDLE_INVALID);
        }
        let entry = registry
            .entries
            .remove(&key)
            .ok_or(AM_ERR_SDK_HANDLE_INVALID)?;
        // token可在并发只读查询退出后才移入隔离区，避免复用仍可查询的地址。
        match Arc::try_unwrap(entry) {
            Ok(entry) => {
                if registry.quarantine.len() == SDK_HANDLE_QUARANTINE as usize {
                    registry.quarantine.pop_front();
                }
                registry.quarantine.push_back(entry.token);
            }
            Err(entry) => {
                registry.entries.insert(key, entry)?;
                return Err(AM_ERR_SDK_TRY_AGAIN);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
