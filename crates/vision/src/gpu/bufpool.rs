//! GPU 直传内部资源池（A 线三靶点优化）：设备缓冲按容量复用、kernel 完成
//! 事件池化。池条目进程级存活（GpuPipeline 单例持有，与 CUDA ctx/kernel
//! 模块同策略，不显式释放，进程退出由驱动回收）。
//! 正确性契约：缓冲归还发生在 kernel 完成同步之后（pipeline 编排保证），
//! 借用期间的 D2D 拷贝与 kernel 均在 legacy 默认流提交，流内顺序天然串行。
use super::cuda_sys::{CudaApi, CudaResult};
use super::driver_sys::DriverApi;
use crate::Error;
use std::ffi::c_void;
use std::sync::{Arc, Mutex};

/// 复用条目：设备指针与分配容量（可大于请求，按最小够用挑选）。
struct PoolEntry {
    ptr: *mut c_void,
    capacity: usize,
}

/// 池上限：R1.3 多路形态并发 lane 峰值 16；超上限的归还直接 cudaFree。
const POOL_LIMIT: usize = 16;

/// 设备缓冲复用池：取最小够用容量条目；归还在限内入池。
pub struct DeviceBufPool {
    free_list: Vec<PoolEntry>,
    limit: usize,
}

// 裸设备指针仅在持锁时移动；CUDA 分配器线程安全（分配/释放在持锁线程
// 之外执行，指针本身只做转运）。
unsafe impl Send for DeviceBufPool {}

impl DeviceBufPool {
    pub fn new() -> Self {
        Self {
            free_list: Vec::new(),
            limit: POOL_LIMIT,
        }
    }

    /// 借出容量 >= need 的最小空闲条目；无命中 None（调用方新分配）。
    fn take(&mut self, need: usize) -> Option<PoolEntry> {
        let mut best: Option<usize> = None;
        for (i, e) in self.free_list.iter().enumerate() {
            if e.capacity >= need && best.is_none_or(|b| e.capacity < self.free_list[b].capacity) {
                best = Some(i);
            }
        }
        best.map(|i| self.free_list.swap_remove(i))
    }

    /// 归还入池；限满 false（调用方释放）。
    fn give(&mut self, entry: PoolEntry) -> bool {
        if self.free_list.len() >= self.limit {
            return false;
        }
        self.free_list.push(entry);
        true
    }
}

impl Default for DeviceBufPool {
    fn default() -> Self {
        Self::new()
    }
}

/// 借用守卫：Drop 归还池（限满时 cudaFree）。分配/归还须在已设置 CUDA
/// current context 的管线线程（与既有 CudaBuf 同约束）。
pub struct PooledBuf<'a> {
    ptr: *mut c_void,
    capacity: usize,
    pool: &'a Mutex<DeviceBufPool>,
    free: unsafe extern "C" fn(*mut c_void) -> CudaResult,
}

impl<'a> PooledBuf<'a> {
    pub fn acquire(
        pool: &'a Mutex<DeviceBufPool>,
        api: &Arc<CudaApi>,
        need: usize,
        what: &str,
    ) -> Result<Self, Error> {
        if let Some(entry) = pool.lock().unwrap_or_else(|e| e.into_inner()).take(need) {
            return Ok(Self {
                ptr: entry.ptr,
                capacity: entry.capacity,
                pool,
                free: api.free,
            });
        }
        let mut ptr: *mut c_void = std::ptr::null_mut();
        api.check(unsafe { (api.malloc)(&mut ptr, need) }, what)?;
        Ok(Self {
            ptr,
            capacity: need,
            pool,
            free: api.free,
        })
    }

    pub fn ptr(&self) -> *mut c_void {
        self.ptr
    }
}

impl Drop for PooledBuf<'_> {
    fn drop(&mut self) {
        let entry = PoolEntry {
            ptr: self.ptr,
            capacity: self.capacity,
        };
        let mut pool = self.pool.lock().unwrap_or_else(|e| e.into_inner());
        if !pool.give(entry) {
            drop(pool);
            // 析构路径错误无法传播；释放结果按既有惯例忽略，不构成吞错。
            let _ = unsafe { (self.free)(self.ptr) };
        }
    }
}

/// kernel 完成事件池：record/synchronize 后归还（guard 化，错误路径不丢）。
#[derive(Default)]
pub struct EventPool {
    events: Vec<*mut c_void>,
}

// 事件句柄仅在持锁时转运；record/synchronize 调用在持锁线程之外执行。
unsafe impl Send for EventPool {}

impl EventPool {
    fn take(&mut self) -> Option<*mut c_void> {
        self.events.pop()
    }
    fn give(&mut self, event: *mut c_void) {
        self.events.push(event);
    }
}

/// 事件借用守卫：Drop 归还事件池。
pub struct EventLease<'a> {
    event: *mut c_void,
    pool: &'a Mutex<EventPool>,
}

impl<'a> EventLease<'a> {
    /// 池空时创建新事件（CU_EVENT_DEFAULT=0）。
    pub fn acquire(pool: &'a Mutex<EventPool>, driver: &Arc<DriverApi>) -> Result<Self, Error> {
        if let Some(event) = pool.lock().unwrap_or_else(|e| e.into_inner()).take() {
            return Ok(Self { event, pool });
        }
        let mut event: *mut c_void = std::ptr::null_mut();
        driver.check(
            unsafe { (driver.event_create)(&mut event, 0) },
            "kernel事件创建",
        )?;
        Ok(Self { event, pool })
    }

    pub fn get(&self) -> *mut c_void {
        self.event
    }
}

impl Drop for EventLease<'_> {
    fn drop(&mut self) {
        self.pool
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .give(self.event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_takes_smallest_sufficient_entry() {
        let mut pool = DeviceBufPool::new();
        assert!(pool.take(100).is_none(), "空池无命中");
        pool.give(PoolEntry {
            ptr: 3 as *mut c_void,
            capacity: 300,
        });
        pool.give(PoolEntry {
            ptr: std::ptr::dangling_mut::<c_void>(),
            capacity: 100,
        });
        pool.give(PoolEntry {
            ptr: 2 as *mut c_void,
            capacity: 200,
        });
        let got = pool.take(150).expect("应有 200 容量条目");
        assert_eq!(got.capacity, 200, "取最小够用而非最大");
        let got = pool.take(50).expect("应有 100 容量条目");
        assert_eq!(got.capacity, 100);
        let got = pool.take(300).expect("应有 300 容量条目");
        assert_eq!(got.capacity, 300);
        assert!(pool.take(1).is_none(), "取尽后无命中");
    }

    #[test]
    fn pool_give_respects_limit() {
        let mut pool = DeviceBufPool::new();
        for i in 0..pool.limit {
            assert!(pool.give(PoolEntry {
                ptr: i as *mut c_void,
                capacity: 1,
            }));
        }
        assert!(
            !pool.give(PoolEntry {
                ptr: 99 as *mut c_void,
                capacity: 1,
            }),
            "限满拒绝归还（调用方释放）"
        );
    }

    static FREED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    unsafe extern "C" fn counting_free(_p: *mut c_void) -> CudaResult {
        FREED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        0
    }

    /// issue #5 池化版：守卫 Drop 恰好归还一次；限满路径才真释放。
    /// 计数 free 验证释放语义（真实 cudaMalloc 失败注入需 CUDA 环境，未测）。
    #[test]
    fn pooled_buf_drop_returns_to_pool_once() {
        let pool = Mutex::new(DeviceBufPool::new());
        {
            let _y = PooledBuf {
                ptr: std::ptr::dangling_mut::<c_void>(),
                capacity: 8,
                pool: &pool,
                free: counting_free,
            };
            let _uv = PooledBuf {
                ptr: 2 as *mut c_void,
                capacity: 4,
                pool: &pool,
                free: counting_free,
            };
        }
        assert_eq!(pool.lock().unwrap().free_list.len(), 2, "两条目入池");
        assert_eq!(
            FREED.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "限内归还不释放"
        );
    }

    #[test]
    fn event_pool_roundtrip() {
        let mut pool = EventPool::default();
        assert!(pool.take().is_none());
        pool.give(7 as *mut c_void);
        assert_eq!(pool.take(), Some(7 as *mut c_void));
        assert!(pool.take().is_none());
    }
}
