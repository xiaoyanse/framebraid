use super::{
    layout::*,
    windows::Process,
    wire::{Pair, State},
};
use crate::{abi::*, buffers::Buffer, handles::Managed, Result};
use std::{
    any::Any,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex, Weak,
    },
    time::{Duration, Instant},
};

#[path = "table.rs"]
pub(super) mod table;
use table::Table;
#[cfg(test)]
#[path = "foundation_tests.rs"]
mod foundation_tests;

#[derive(Default)]
pub struct Ledger {
    maps: Table<[u8; 32], Weak<Pair>, 4>,
}
impl Ledger {
    pub fn summary(&self) -> Result<(u64, u64, u32, u32)> {
        let mut result = (0, 0, 0, 0);
        for pair in self.maps.values().filter_map(Weak::upgrade) {
            let state = pair.lock(0)?;
            state.reap()?;
            result.0 += pair.config.total_bytes;
            result.1 += CONTROL_BYTES as u64;
            result.2 += 1;
            result.3 += state.stats().outstanding_borrows;
        }
        Ok(result)
    }
    pub fn check(&mut self, additional: u64, l: &am_limits_t) -> Result<()> {
        self.maps.retain(|_, p| p.strong_count() != 0);
        let bytes: u64 = self
            .maps
            .values()
            .filter_map(Weak::upgrade)
            .map(|p| p.config.total_bytes)
            .sum();
        if self.maps.len() >= l.shared_pool_max_pools.min(4) as usize
            || bytes + additional > l.shared_pool_max_total_bytes
            || (self.maps.len() as u64 + 1) * CONTROL_BYTES as u64 > l.shared_pool_metadata_bytes
        {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        Ok(())
    }
    pub fn existing(&mut self, tag: &[u8; 32]) -> Option<Arc<Pair>> {
        self.maps.retain(|_, p| p.strong_count() != 0);
        self.maps.get(tag).and_then(Weak::upgrade)
    }
    pub fn admit(&mut self, pair: Arc<Pair>, l: &am_limits_t) -> Result<Arc<Pair>> {
        self.check(pair.config.total_bytes, l)?;
        self.maps
            .insert(pair.attach.identity_tag, Arc::downgrade(&pair))?;
        Ok(pair)
    }
}
pub struct Pool {
    mapping: Mutex<Option<Arc<Pair>>>,
    pub publisher: bool,
    pub consumer: u64,
    consumer_at: usize,
    consumer_done: AtomicBool,
    pub initial: Mutex<Table<u64, usize, 32>>,
    pub local_leases: Arc<AtomicUsize>,
    pub stopping: AtomicBool,
    waiters: Arc<AtomicUsize>,
}
impl Pool {
    pub fn new(pair: Arc<Pair>, publisher: bool, waiters: Arc<AtomicUsize>) -> Result<Self> {
        let (consumer, consumer_at) = if publisher {
            (0, 0)
        } else {
            let s = pair.lock(0)?;
            s.reap()?;
            if s.r32(88) != 0 || pair.control.release_read(176) != 0 || pair.owner.exited() {
                return Err(AM_ERR_SDK_HANDLE_CLOSED);
            }
            let c = (0..pair.config.max_consumers as usize)
                .find(|i| s.r64(CONSUMER_BASE + i * CONSUMER_BYTES) == 0)
                .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
            // 随机消费者号不依赖本地递增计数，避免重复attach复用旧身份。
            let random = super::windows::identity()?;
            let token = u64::from_le_bytes(
                random[..8]
                    .try_into()
                    .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?,
            )
            .max(1);
            if (0..pair.config.max_consumers as usize)
                .any(|i| s.r64(CONSUMER_BASE + i * CONSUMER_BYTES) == token)
            {
                return Err(AM_ERR_SDK_TRY_AGAIN);
            }
            let process = Process::current()?;
            let at = CONSUMER_BASE + c * CONSUMER_BYTES;
            pair.control.release_signal(at + 24, 0);
            s.u64(at, token);
            s.u64(at + 8, process.pid);
            s.u64(at + 16, process.started);
            (token, at)
        };
        Ok(Self {
            mapping: Mutex::new(Some(pair)),
            publisher,
            consumer,
            consumer_at,
            consumer_done: AtomicBool::new(false),
            initial: Mutex::new(Table::default()),
            local_leases: Arc::new(AtomicUsize::new(0)),
            stopping: AtomicBool::new(false),
            waiters,
        })
    }
    pub fn pair(&self) -> Result<Arc<Pair>> {
        self.mapping
            .lock()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?
            .clone()
            .ok_or(AM_ERR_SDK_HANDLE_CLOSED)
    }
    fn wait<T>(
        &self,
        wait: u32,
        mut operation: impl FnMut(&State<'_>) -> Result<Option<T>>,
    ) -> Result<T> {
        if wait as u64 > SDK_SHARED_WAIT_MAX_MS {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        let _waiting = Waiter::new(&self.waiters, wait)?;
        let until = Instant::now() + Duration::from_millis(wait as u64);
        let pair = self.pair()?;
        loop {
            if self.stopping.load(Ordering::Acquire) {
                return Err(AM_ERR_SDK_HANDLE_CLOSED);
            }
            // 每次只尝试锁，截止涵盖争锁，绝不持锁sleep或超时后偷偷接受。
            match pair.lock(0) {
                Ok(s) => {
                    if wait != 0 && Instant::now() >= until {
                        s.increment(168);
                        return Err(AM_ERR_SDK_DEADLINE_EXCEEDED);
                    }
                    s.reap()?;
                    if s.r32(88) != 0 || pair.control.release_read(176) != 0 || pair.owner.exited()
                    {
                        return Err(AM_ERR_SDK_HANDLE_CLOSED);
                    }
                    if let Some(value) = operation(&s)? {
                        return Ok(value);
                    }
                }
                Err(AM_ERR_SDK_TRY_AGAIN) => {}
                Err(e) => return Err(e),
            }
            if wait == 0 {
                return Err(AM_ERR_SDK_TRY_AGAIN);
            }
            if Instant::now() >= until {
                if let Ok(s) = pair.lock(0) {
                    s.increment(168);
                }
                return Err(AM_ERR_SDK_DEADLINE_EXCEEDED);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    pub fn publish(&self, mut desc: am_shared_frame_desc_t, planes: [*const u8; 4]) -> Result<u64> {
        if !self.publisher {
            return Err(AM_ERR_SDK_HANDLE_INVALID);
        }
        let pair = self.pair()?;
        let bytes = validate_desc(&desc, pair.config.max_frame_bytes as u64)?;
        let wait = if pair.config.drop_policy == AM_SFP_DROP_WAIT {
            pair.config.publish_wait_ms
        } else {
            0
        };
        let result = self.wait(wait, |s| {
            let free = (0..pair.config.slot_count as usize)
                .filter(|i| s.r32(SLOT_BASE + i * SLOT_BYTES + 12) == 0)
                .min_by_key(|i| s.r64(SLOT_BASE + i * SLOT_BYTES));
            let Some(slot) = free else {
                if wait == 0 {
                    s.increment(160);
                }
                return Ok(None);
            };
            let at = SLOT_BASE + slot * SLOT_BYTES;
            let id = s
                .r64(128)
                .checked_add(1)
                .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
            desc.publication_id = id;
            desc.pool_generation = pair.attach.pool_generation;
            // 校验先完成，之后只复制已验证范围；提交序号是最后一步。
            for from in planes.iter().take(desc.plane_count as usize) {
                if from.is_null() {
                    return Err(AM_ERR_SDK_NULL_ARGUMENT);
                }
            }
            let base = slot * pair.config.max_frame_bytes as usize;
            unsafe {
                std::ptr::write_bytes(pair.pixels.ptr.add(base), 0, bytes);
                for (i, from) in planes.iter().enumerate().take(desc.plane_count as usize) {
                    std::ptr::copy_nonoverlapping(
                        *from,
                        pair.pixels.ptr.add(base + desc.offset_bytes[i] as usize),
                        desc.plane_valid_bytes[i] as usize,
                    );
                }
            }
            if s.r64(at) != 0 {
                s.increment(152);
            }
            s.bytes(at + 32, &encode(&desc));
            s.u32(at + 8, bytes as u32);
            s.u64(at, id);
            s.u64(128, id);
            s.increment(144);
            Ok(Some(id))
        });
        match result {
            Err(AM_ERR_SDK_TRY_AGAIN) if wait == 0 => Err(AM_ERR_SDK_QUEUE_FULL),
            other => other,
        }
    }
    pub fn borrow(&self, id: u64, wait: u32) -> Result<Arc<Lease>> {
        if self.publisher {
            return Err(AM_ERR_SDK_HANDLE_INVALID);
        }
        let pair = self.pair()?;
        self.wait(wait, |s| {
            let wanted = if id == 0 { s.r64(128) } else { id };
            let slot = (0..pair.config.slot_count as usize)
                .find(|i| s.r64(SLOT_BASE + i * SLOT_BYTES) == wanted && wanted != 0);
            let Some(slot) = slot else {
                if id != 0 && id <= s.r64(128) {
                    return Err(AM_ERR_SDK_RESULT_EXPIRED);
                }
                return Ok(None);
            };
            let at = SLOT_BASE + slot * SLOT_BYTES;
            let desc = decode(&s.read::<DESC_BYTES>(at + 32));
            if desc.publication_id != wanted || desc.pool_generation != pair.attach.pool_generation
            {
                return Err(AM_ERR_SDK_GENERATION_MISMATCH);
            }
            let len = validate_desc(&desc, pair.config.max_frame_bytes as u64)?;
            if s.r32(at + 8) as usize != len {
                return Err(AM_ERR_SDK_INVALID_ARGUMENT);
            }
            let record = (0..pair.config.max_outstanding_borrows as usize)
                .find(|i| s.r64(LEASE_BASE + i * LEASE_BYTES) == 0)
                .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
            let borrow = s
                .r64(136)
                .checked_add(1)
                .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
            let refs = s
                .r32(at + 12)
                .checked_add(1)
                .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
            let lease_at = LEASE_BASE + record * LEASE_BYTES;
            s.pair.control.release_signal(lease_at + RELEASE_OFFSET, 0);
            s.u64(lease_at, borrow);
            s.u64(lease_at + 8, self.consumer);
            s.u32(lease_at + 16, slot as u32);
            s.u32(at + 12, refs);
            s.u64(136, borrow);
            self.local_leases.fetch_add(1, Ordering::AcqRel);
            let mut frame = am_frame_view_t {
                width: desc.width,
                height: desc.height,
                format: desc.pixel_format,
                plane_count: desc.plane_count,
                strides: desc.stride_bytes,
                color_matrix: desc.color_matrix,
                color_range: desc.color_range,
                ..Default::default()
            };
            let mut view = am_shared_frame_view_t {
                desc,
                borrow_id: borrow,
                ..Default::default()
            };
            for i in 0..desc.plane_count as usize {
                let ptr = unsafe {
                    pair.pixels.ptr.add(
                        slot * pair.config.max_frame_bytes as usize + desc.offset_bytes[i] as usize,
                    )
                };
                frame.planes[i] = ptr;
                frame.plane_size_bytes[i] = desc.plane_valid_bytes[i] as u64;
                view.plane_data[i] = ptr.cast();
            }
            let assoc = am_frame_assoc_t {
                source_id: desc.source_id,
                frame_id: desc.frame_id,
                source_generation: desc.source_generation,
                config_generation: desc.config_generation,
                pts: desc.pts,
                time_base_num: desc.time_base_num,
                time_base_den: desc.time_base_den,
                captured_at_ms: desc.captured_at_ms,
                received_at_ms: desc.received_at_ms,
                clock_mask: desc.time_flags,
                clock_quality: desc.clock_quality,
                valid: desc.assoc_valid,
                ..Default::default()
            };
            Ok(Some(Arc::new(Lease {
                pair: pair.clone(),
                offset: lease_at,
                frame,
                assoc,
                view,
                local: self.local_leases.clone(),
            })))
        })
    }
}
impl Managed for Pool {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn request_close(&self) {
        self.stopping.store(true, Ordering::Release);
        if self.publisher {
            if let Ok(pair) = self.pair() {
                pair.control.release_signal(176, 1);
            }
        }
    }
    fn drained(&self) -> bool {
        if self.local_leases.load(Ordering::Acquire) != 0 {
            return false;
        }
        let Ok(pair) = self.pair() else {
            return true;
        };
        let Ok(s) = pair.lock(0) else {
            return false;
        };
        if s.reap().is_err() {
            return false;
        }
        if self.publisher {
            s.u32(88, 1);
            let stats = s.stats();
            stats.consumers_attached == 0 && stats.outstanding_borrows == 0
        } else {
            for i in 0..pair.config.max_consumers as usize {
                let at = CONSUMER_BASE + i * CONSUMER_BYTES;
                if s.r64(at) == self.consumer {
                    self.consumer_done.store(true, Ordering::Release);
                    s.u64(at, 0);
                }
            }
            true
        }
    }
    fn close_resources(&self) {
        if let Ok(mut mapping) = self.mapping.lock() {
            mapping.take();
        }
    }
}
impl Drop for Pool {
    fn drop(&mut self) {
        if !self.publisher && !self.consumer_done.load(Ordering::Acquire) {
            if let Ok(pair) = self.pair() {
                // 只写自己的独立归还意向，不在析构中等待跨进程锁。
                pair.control
                    .release_signal(self.consumer_at + 24, self.consumer);
            }
        }
    }
}
pub struct Lease {
    pub pair: Arc<Pair>,
    offset: usize,
    pub frame: am_frame_view_t,
    pub assoc: am_frame_assoc_t,
    pub view: am_shared_frame_view_t,
    local: Arc<AtomicUsize>,
}
unsafe impl Send for Lease {}
unsafe impl Sync for Lease {}
impl Buffer for Lease {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.pair
            .control
            .release_signal(self.offset + RELEASE_OFFSET, self.view.borrow_id);
        self.local.fetch_sub(1, Ordering::AcqRel);
    }
}
struct Waiter<'a> {
    count: &'a AtomicUsize,
    registered: bool,
}
impl<'a> Waiter<'a> {
    fn new(count: &'a AtomicUsize, wait: u32) -> Result<Self> {
        if wait > 0
            && count
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |c| {
                    (c < SDK_QUEUE_MAX_WAITERS as usize).then_some(c + 1)
                })
                .is_err()
        {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        Ok(Self {
            count,
            registered: wait > 0,
        })
    }
}
impl Drop for Waiter<'_> {
    fn drop(&mut self) {
        if self.registered {
            self.count.fetch_sub(1, Ordering::AcqRel);
        }
    }
}
