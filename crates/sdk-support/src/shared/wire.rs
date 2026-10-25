use super::{layout::*, windows::*};
use crate::{abi::*, Result};
use std::{collections::HashMap, sync::Mutex};

pub struct Pair {
    pub control: Mapping,
    pub pixels: Mapping,
    mutex: NamedMutex,
    pub owner: Process,
    pub attach: am_shared_pool_attach_desc_t,
    pub config: am_shared_pool_config_t,
    verified: Mutex<HashMap<u64, Process>>,
}
// attach/config只包含定宽值；共享写入一律持命名锁，归还意向单独原子化。
unsafe impl Send for Pair {}
unsafe impl Sync for Pair {}
pub struct State<'a> {
    pub pair: &'a Pair,
    _guard: Guard<'a>,
}
impl Pair {
    pub fn create(config: am_shared_pool_config_t) -> Result<Self> {
        let tag = identity()?;
        let security = Security::current()?;
        let owner = Process::current()?;
        let control = Mapping::create(&name(&tag, "ctrl"), CONTROL_BYTES, &security)?;
        let pixels = Mapping::create(&name(&tag, "data"), config.total_bytes as usize, &security)?;
        let mutex = NamedMutex::create(&name(&tag, "lock"), &security)?;
        let attach = am_shared_pool_attach_desc_t {
            protocol_version: 1,
            pool_id: u64::from_le_bytes(
                tag[..8].try_into().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?,
            )
            .max(1),
            owner_pid: owner.pid,
            owner_started_at_100ns: owner.started,
            pool_generation: 1,
            identity_tag: tag,
            ..Default::default()
        };
        let pair = Self {
            control,
            pixels,
            mutex,
            owner,
            attach,
            config,
            verified: Mutex::new(HashMap::new()),
        };
        {
            let s = pair.lock(0)?;
            s.bytes(0, MAGIC);
            s.u32(8, 1);
            s.u32(12, HEADER_BYTES as u32);
            s.u64(16, CONTROL_BYTES as u64);
            s.u64(24, config.total_bytes);
            s.u64(32, attach.pool_id);
            s.u64(40, attach.owner_pid);
            s.u64(48, attach.owner_started_at_100ns);
            s.u32(56, 1);
            s.u32(60, config.slot_count);
            s.u32(64, config.max_frame_bytes);
            s.u32(68, config.max_consumers);
            s.u32(72, config.max_outstanding_borrows);
            s.u32(76, config.drop_policy);
            s.u32(80, config.publish_wait_ms);
            s.bytes(96, &tag);
        }
        Ok(pair)
    }
    pub fn open(
        attach: am_shared_pool_attach_desc_t,
        limits: &am_limits_t,
        mut admit: impl FnMut(u64) -> Result<()>,
    ) -> Result<Self> {
        if attach.protocol_version != 1 {
            return Err(AM_ERR_SDK_ABI_UNSUPPORTED);
        }
        if attach.pool_id == 0 || attach.pool_generation == 0 || attach.identity_tag == [0; 32] {
            return Err(AM_ERR_SDK_GENERATION_MISMATCH);
        }
        let owner = Process::open(attach.owner_pid, Some(attach.owner_started_at_100ns))?;
        if owner.exited() {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        let control = Mapping::open(&name(&attach.identity_tag, "ctrl"), CONTROL_BYTES, true)?;
        let mutex = NamedMutex::open(&name(&attach.identity_tag, "lock"))?;
        let (guard, abandoned) = mutex.lock(0)?;
        if abandoned {
            unsafe {
                std::ptr::copy_nonoverlapping(1u32.to_le_bytes().as_ptr(), control.ptr.add(84), 4);
            }
            return Err(AM_ERR_SDK_INTERNAL_ERROR);
        }
        // 打开像素段前，只在锁内读取显式小端头，并验证全部身份和容量。
        let raw = unsafe { std::slice::from_raw_parts(control.ptr, HEADER_BYTES) };
        let r32 = |o| u32::from_le_bytes([raw[o], raw[o + 1], raw[o + 2], raw[o + 3]]);
        let r64 = |o| {
            u64::from_le_bytes([
                raw[o],
                raw[o + 1],
                raw[o + 2],
                raw[o + 3],
                raw[o + 4],
                raw[o + 5],
                raw[o + 6],
                raw[o + 7],
            ])
        };
        if &raw[..8] != MAGIC
            || r32(8) != 1
            || r32(12) != HEADER_BYTES as u32
            || r64(16) != CONTROL_BYTES as u64
        {
            return Err(AM_ERR_SDK_ABI_UNSUPPORTED);
        }
        if r64(32) != attach.pool_id
            || r64(40) != attach.owner_pid
            || r64(48) != attach.owner_started_at_100ns
            || r32(56) != attach.pool_generation
            || raw[96..128] != attach.identity_tag
        {
            return Err(AM_ERR_SDK_GENERATION_MISMATCH);
        }
        if r32(84) != 0 {
            return Err(AM_ERR_SDK_INTERNAL_ERROR);
        }
        if r32(88) != 0 {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        let config = am_shared_pool_config_t {
            slot_count: r32(60),
            max_frame_bytes: r32(64),
            total_bytes: r64(24),
            max_consumers: r32(68),
            max_outstanding_borrows: r32(72),
            drop_policy: r32(76),
            publish_wait_ms: r32(80),
            ..Default::default()
        };
        validate_config(&config, limits)?;
        admit(config.total_bytes)?;
        let pixels = Mapping::open(
            &name(&attach.identity_tag, "data"),
            config.total_bytes as usize,
            false,
        )?;
        drop(guard);
        Ok(Self {
            control,
            pixels,
            mutex,
            owner,
            attach,
            config,
            verified: Mutex::new(HashMap::new()),
        })
    }
    pub fn lock(&self, wait: u32) -> Result<State<'_>> {
        let (guard, abandoned) = self.mutex.lock(wait)?;
        let state = State {
            pair: self,
            _guard: guard,
        };
        if abandoned {
            state.u32(84, 1);
        }
        if state.r32(84) != 0 {
            return Err(AM_ERR_SDK_INTERNAL_ERROR);
        }
        Ok(state)
    }
}
impl State<'_> {
    pub fn bytes(&self, at: usize, b: &[u8]) {
        assert!(at + b.len() <= CONTROL_BYTES);
        unsafe {
            std::ptr::copy_nonoverlapping(b.as_ptr(), self.pair.control.ptr.add(at), b.len());
        }
    }
    pub fn u32(&self, at: usize, v: u32) {
        self.bytes(at, &v.to_le_bytes());
    }
    pub fn u64(&self, at: usize, v: u64) {
        self.bytes(at, &v.to_le_bytes());
    }
    pub fn read<const N: usize>(&self, at: usize) -> [u8; N] {
        assert!(at + N <= CONTROL_BYTES);
        let mut b = [0; N];
        unsafe {
            std::ptr::copy_nonoverlapping(self.pair.control.ptr.add(at), b.as_mut_ptr(), N);
        }
        b
    }
    pub fn r32(&self, at: usize) -> u32 {
        u32::from_le_bytes(self.read(at))
    }
    pub fn r64(&self, at: usize) -> u64 {
        u64::from_le_bytes(self.read(at))
    }
    pub fn increment(&self, at: usize) {
        self.u64(at, self.r64(at).saturating_add(1));
    }
    pub fn reap(&self) -> Result<()> {
        let mut verified = self
            .pair
            .verified
            .lock()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        for c in 0..self.pair.config.max_consumers as usize {
            let at = CONSUMER_BASE + c * CONSUMER_BYTES;
            let token = self.r64(at);
            if token == 0 {
                continue;
            }
            if let std::collections::hash_map::Entry::Vacant(e) = verified.entry(token) {
                // 访问失败不是退出证明；保留该记录和所占槽。
                if let Ok(p) = Process::open(self.r64(at + 8), Some(self.r64(at + 16))) {
                    e.insert(p);
                }
            }
            let released = self.pair.control.release_read(at + 24) == token
                && !(0..self.pair.config.max_outstanding_borrows as usize)
                    .any(|l| self.r64(LEASE_BASE + l * LEASE_BYTES + 8) == token);
            if verified.get(&token).is_some_and(Process::exited) || released {
                for l in 0..self.pair.config.max_outstanding_borrows as usize {
                    let lease = LEASE_BASE + l * LEASE_BYTES;
                    if self.r64(lease + 8) == token {
                        self.clear_lease(lease)?;
                    }
                }
                self.u64(at, 0);
                self.pair.control.release_signal(at + 24, 0);
                verified.remove(&token);
            }
        }
        for l in 0..self.pair.config.max_outstanding_borrows as usize {
            let at = LEASE_BASE + l * LEASE_BYTES;
            let id = self.r64(at);
            if id != 0 && self.pair.control.release_read(at + RELEASE_OFFSET) == id {
                self.clear_lease(at)?;
            }
        }
        // 删除已正常detach的缓存进程对象，缓存不随重复attach增长。
        verified.retain(|token, _| {
            (0..self.pair.config.max_consumers as usize)
                .any(|c| self.r64(CONSUMER_BASE + c * CONSUMER_BYTES) == *token)
        });
        Ok(())
    }
    fn clear_lease(&self, at: usize) -> Result<()> {
        if self.r64(at) == 0 {
            return Ok(());
        }
        let slot = self.r32(at + 16);
        if slot >= self.pair.config.slot_count {
            self.u32(84, 1);
            return Err(AM_ERR_SDK_INTERNAL_ERROR);
        }
        let count_at = SLOT_BASE + slot as usize * SLOT_BYTES + 12;
        let count = self.r32(count_at);
        if count == 0 {
            self.u32(84, 1);
            return Err(AM_ERR_SDK_INTERNAL_ERROR);
        }
        self.u32(count_at, count - 1);
        self.u64(at, 0);
        self.u64(at + 8, 0);
        self.pair.control.release_signal(at + RELEASE_OFFSET, 0);
        Ok(())
    }
    pub fn stats(&self) -> am_shared_pool_stats_t {
        let mut s = am_shared_pool_stats_t {
            published: self.r64(144),
            dropped_stale: self.r64(152),
            dropped_full: self.r64(160),
            wait_timeouts: self.r64(168),
            bytes_in_use: self.pair.config.total_bytes,
            metadata_bytes: CONTROL_BYTES as u64,
            ..Default::default()
        };
        for i in 0..self.pair.config.slot_count as usize {
            let at = SLOT_BASE + i * SLOT_BYTES;
            s.slots_in_use += (self.r64(at) != 0) as u32;
            s.slots_borrowed += (self.r32(at + 12) != 0) as u32;
        }
        for i in 0..self.pair.config.max_consumers as usize {
            s.consumers_attached += (self.r64(CONSUMER_BASE + i * CONSUMER_BYTES) != 0) as u32;
        }
        for i in 0..self.pair.config.max_outstanding_borrows as usize {
            s.outstanding_borrows += (self.r64(LEASE_BASE + i * LEASE_BYTES) != 0) as u32;
        }
        s
    }
}
