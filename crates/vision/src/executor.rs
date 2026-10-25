//! 有界公平执行队列（6.2 队列下移）：池/Condvar 排队核心泛型化自 app-core 推理池，
//! 行为语义逐行保持一致。宿主与 DLL 共用此核心即可共享同一份并发/在途预算，
//! 防止宿主与 DLL 各排一层队形成双重积压（dual-dll-abi §6 末段）。
//! 无设备/进程/业务副作用；会话载体由宿主实现 FairSlot 注入。
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};

/// 回收器扫描间隔（沿用原池常量）。
const REAP_INTERVAL: Duration = Duration::from_secs(60);

/// 两级公平类别：全局 4 拍轮转，防快任务饿死慢任务。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestClass {
    Interactive,
    Background,
}

/// 并发预算配置：宿主按自身常量注入（原 MAX_WORKERS/MAX_WAITING/MAX_BYTES/IDLE_EXIT）。
#[derive(Clone, Copy, Debug)]
pub struct ExecutorConfig {
    /// 最大并行会话数（同键会话互斥，异键共享该上限）
    pub max_parallel: usize,
    /// 等待队列深度上限（超出立即拒绝）
    pub max_waiting: usize,
    /// 在途用量上限（等待+执行期间合计，如图像字节数）
    pub max_usage: usize,
    /// 会话闲置多久后可被回收器收走
    pub idle_exit: Duration,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            max_parallel: 2,
            max_waiting: 16,
            max_usage: usize::MAX,
            idle_exit: Duration::from_secs(300),
        }
    }
}

/// 执行槽最小接口：宿主自定义会话载体（进程/线程/句柄），执行器只看生命周期原语。
pub trait FairSlot: Send + Sync + 'static {
    fn is_busy(&self) -> bool;
    fn set_busy(&self, busy: bool);
    fn is_retired(&self) -> bool;
    fn set_retired(&self, retired: bool);
    /// 租约释放时刷新使用时刻（回收器依据）
    fn mark_used(&self);
    /// 距上次使用的时间
    fn idle_for(&self) -> Duration;
    /// 退役回收（幂等）：排干会话资源后返回
    fn evict(&self);
}

/// 排队票据：key=会话键；kind 参与两级公平轮转。
pub struct Ticket {
    pub id: u64,
    pub key: String,
    pub kind: RequestClass,
}

/// 池内状态：字段公开仅供宿主测试装配，运行期一律经 FairExecutor 访问。
/// max_parallel 为容量内联副本（available 纯函数判定需要），构造时取自配置。
pub struct PoolState<S: FairSlot> {
    pub slots: HashMap<String, Arc<S>>,
    pub waiting: VecDeque<Ticket>,
    pub usage: usize,
    pub turn: usize,
    pub version_turns: HashMap<String, usize>,
    pub closing: bool,
    pub max_parallel: usize,
}

impl<S: FairSlot> Default for PoolState<S> {
    fn default() -> Self {
        Self {
            slots: HashMap::new(),
            waiting: VecDeque::new(),
            usage: 0,
            turn: 0,
            version_turns: HashMap::new(),
            closing: false,
            max_parallel: ExecutorConfig::default().max_parallel,
        }
    }
}

/// 会话键可用性：已有键忙/退役即不可用；新键需有空位或存在空闲槽。
pub fn available<S: FairSlot>(p: &PoolState<S>, key: &str) -> bool {
    p.slots.get(key).map_or_else(
        || p.slots.len() < p.max_parallel || p.slots.values().any(|s| !s.is_busy()),
        |s| !s.is_busy() && !s.is_retired(),
    )
}

/// 公平轮转选队：全局 4 拍+每键 4 拍，同键缺本级时让位，空侧直接放行。
pub fn next<S: FairSlot>(p: &PoolState<S>) -> Option<usize> {
    let preferred = if p.turn == 0 {
        RequestClass::Interactive
    } else {
        RequestClass::Background
    };
    let eligible = |t: &Ticket| {
        if !available(p, &t.key) {
            return false;
        }
        let turn = p.version_turns.get(&t.key).copied().unwrap_or(0);
        let local = if turn == 0 {
            RequestClass::Interactive
        } else {
            RequestClass::Background
        };
        t.kind == local
            || !p
                .waiting
                .iter()
                .any(|other| other.key == t.key && other.kind == local)
    };
    p.waiting
        .iter()
        .position(|t| t.kind == preferred && eligible(t))
        .or_else(|| p.waiting.iter().position(eligible))
}

/// 准入失败分类；宿主映射为自身错误码（原 model.workerShuttingDown/capacityExceeded/workerTimeout/poolLockPoisoned）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcquireError {
    ShuttingDown,
    CapacityExceeded,
    DeadlineExceeded,
    LockPoisoned,
}

struct Inner<S: FairSlot> {
    state: Mutex<PoolState<S>>,
    cv: Condvar,
    cfg: ExecutorConfig,
    serial: AtomicU64,
}

/// 在途用量守卫：入队即计费，Drop 时退账并撤销等待票据（未轮到即超时/关闭同理）。
pub struct UsageGuard<S: FairSlot> {
    cost: usize,
    ticket: u64,
    inner: Arc<Inner<S>>,
}

impl<S: FairSlot> std::fmt::Debug for UsageGuard<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UsageGuard")
            .field("cost", &self.cost)
            .field("ticket", &self.ticket)
            .finish_non_exhaustive()
    }
}

impl<S: FairSlot> Drop for UsageGuard<S> {
    fn drop(&mut self) {
        if let Ok(mut p) = self.inner.state.lock() {
            p.usage = p.usage.saturating_sub(self.cost);
            p.waiting.retain(|t| t.id != self.ticket);
        }
        self.inner.cv.notify_all();
    }
}

/// 会话租约：Drop 释放占用并刷新使用时刻；退役槽从池内移除。
pub struct Lease<S: FairSlot> {
    pub key: String,
    pub slot: Arc<S>,
    inner: Arc<Inner<S>>,
}

impl<S: FairSlot> std::fmt::Debug for Lease<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lease")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

impl<S: FairSlot> Drop for Lease<S> {
    fn drop(&mut self) {
        self.slot.set_busy(false);
        self.slot.mark_used();
        if self.slot.is_retired() {
            if let Ok(mut p) = self.inner.state.lock() {
                if p.slots
                    .get(&self.key)
                    .is_some_and(|s| Arc::ptr_eq(s, &self.slot))
                {
                    p.slots.remove(&self.key);
                }
            }
        }
        self.inner.cv.notify_all();
    }
}

/// 有界公平执行器：克隆共享同一池状态。
pub struct FairExecutor<S: FairSlot> {
    inner: Arc<Inner<S>>,
}

impl<S: FairSlot> Clone for FairExecutor<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<S: FairSlot> FairExecutor<S> {
    pub fn new(cfg: ExecutorConfig) -> Self {
        let max_parallel = cfg.max_parallel;
        Self {
            inner: Arc::new(Inner {
                state: Mutex::new(PoolState {
                    max_parallel,
                    ..PoolState::default()
                }),
                cv: Condvar::new(),
                cfg,
                serial: AtomicU64::new(1),
            }),
        }
    }

    pub fn config(&self) -> ExecutorConfig {
        self.inner.cfg
    }

    /// 排队租借：入口做队列深度/用量预算门；轮到后惰性建槽并占用（make 在池锁内回调，
    /// 不得回头取本池锁）。守卫释放前在途用量持续占账。
    pub fn acquire(
        &self,
        key: &str,
        deadline: Instant,
        kind: RequestClass,
        usage: usize,
        make: impl FnOnce() -> Arc<S>,
    ) -> Result<(Lease<S>, UsageGuard<S>), AcquireError> {
        let id = self.inner.serial.fetch_add(1, Ordering::Relaxed);
        let mut p = match self.inner.state.lock() {
            Ok(p) => p,
            Err(poisoned) => {
                drop(poisoned.into_inner());
                return Err(AcquireError::LockPoisoned);
            }
        };
        if p.closing {
            return Err(AcquireError::ShuttingDown);
        }
        if p.waiting.len() >= self.inner.cfg.max_waiting
            || p.usage.saturating_add(usage) > self.inner.cfg.max_usage
        {
            return Err(AcquireError::CapacityExceeded);
        }
        p.usage += usage;
        p.waiting.push_back(Ticket {
            id,
            key: key.to_owned(),
            kind,
        });
        let live_keys = p
            .slots
            .keys()
            .chain(p.waiting.iter().map(|t| &t.key))
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        p.version_turns.retain(|key, _| live_keys.contains(key));
        let guard = UsageGuard {
            cost: usage,
            ticket: id,
            inner: self.inner.clone(),
        };
        loop {
            if p.closing {
                drop(p);
                return Err(AcquireError::ShuttingDown);
            }
            if Instant::now() >= deadline {
                drop(p);
                return Err(AcquireError::DeadlineExceeded);
            }
            if next(&p).is_some_and(|i| p.waiting[i].id == id) {
                p.waiting.retain(|t| t.id != id);
                p.turn = (p.turn + 1) % 4;
                let local_turn = p.version_turns.entry(key.to_owned()).or_default();
                *local_turn = (*local_turn + 1) % 4;
                let evicted = if !p.slots.contains_key(key) && p.slots.len() >= p.max_parallel {
                    let evict = p
                        .slots
                        .iter()
                        .find(|(_, s)| !s.is_busy())
                        .map(|(k, _)| k.clone());
                    evict.and_then(|k| p.slots.remove(&k))
                } else {
                    None
                };
                let slot = p.slots.entry(key.to_owned()).or_insert_with(make).clone();
                slot.set_busy(true);
                drop(p);
                if let Some(old) = evicted {
                    old.evict();
                }
                return Ok((
                    Lease {
                        key: key.to_owned(),
                        slot,
                        inner: self.inner.clone(),
                    },
                    guard,
                ));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            p = match self.inner.cv.wait_timeout(p, remaining) {
                Ok((guard, _)) => guard,
                Err(poisoned) => {
                    drop(poisoned.into_inner());
                    return Err(AcquireError::LockPoisoned);
                }
            };
        }
    }

    /// 启动闲置回收线程：非忙且闲置超时的会话先占位防新请求，再回收移除。
    pub fn start_reaper(&self) {
        let inner = self.inner.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(REAP_INTERVAL);
            let expired = if let Ok(p) = inner.state.lock() {
                let keys = p
                    .slots
                    .iter()
                    .filter(|(_, s)| !s.is_busy() && s.idle_for() >= inner.cfg.idle_exit)
                    .map(|(k, _)| k.clone())
                    .collect::<Vec<_>>();
                keys.into_iter()
                    .filter_map(|k| {
                        p.slots.get(&k).cloned().map(|slot| {
                            slot.set_busy(true);
                            slot.set_retired(true);
                            (k, slot)
                        })
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            for (key, slot) in expired {
                slot.evict();
                if let Ok(mut p) = inner.state.lock() {
                    if p.slots.get(&key).is_some_and(|s| Arc::ptr_eq(s, &slot)) {
                        p.slots.remove(&key);
                    }
                }
            }
            inner.cv.notify_all();
        });
    }

    /// 关闭屏障：拒绝新请求并排干全部会话（已租约的请求自行到达终态）。
    pub fn shutdown(&self) {
        let slots = self
            .inner
            .state
            .lock()
            .map(|mut p| {
                p.closing = true;
                p.slots.drain().map(|(_, s)| s).collect::<Vec<_>>()
            })
            .unwrap_or_default();
        self.inner.cv.notify_all();
        for slot in slots {
            slot.evict();
        }
    }

    /// 加锁读池状态；锁中毒返回 None（沿原 lock().is_ok_and 语义）。
    pub fn state<R>(&self, f: impl FnOnce(&PoolState<S>) -> R) -> Option<R> {
        self.inner.state.lock().ok().map(|p| f(&p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    struct Dummy {
        busy: AtomicBool,
        retired: AtomicBool,
        last_used: Mutex<Instant>,
    }
    impl Dummy {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                busy: AtomicBool::new(false),
                retired: AtomicBool::new(false),
                last_used: Mutex::new(Instant::now()),
            })
        }
    }
    impl FairSlot for Dummy {
        fn is_busy(&self) -> bool {
            self.busy.load(Ordering::Acquire)
        }
        fn set_busy(&self, busy: bool) {
            self.busy.store(busy, Ordering::Release);
        }
        fn is_retired(&self) -> bool {
            self.retired.load(Ordering::Acquire)
        }
        fn set_retired(&self, retired: bool) {
            self.retired.store(retired, Ordering::Release);
        }
        fn mark_used(&self) {
            if let Ok(mut used) = self.last_used.lock() {
                *used = Instant::now();
            }
        }
        fn idle_for(&self) -> Duration {
            self.last_used
                .lock()
                .map(|t| t.elapsed())
                .unwrap_or_default()
        }
        fn evict(&self) {
            self.set_retired(true);
        }
    }

    fn executor(parallel: usize, waiting: usize, usage: usize) -> FairExecutor<Dummy> {
        FairExecutor::new(ExecutorConfig {
            max_parallel: parallel,
            max_waiting: waiting,
            max_usage: usage,
            idle_exit: Duration::from_secs(300),
        })
    }
    fn soon(millis: u64) -> Instant {
        Instant::now() + Duration::from_millis(millis)
    }

    /// 迁移自 app-core：公平轮转与空侧放行断言原样。
    #[test]
    fn fair_turn_and_empty_side_yields() {
        let mut p = PoolState::<Dummy>::default();
        p.waiting.push_back(Ticket {
            id: 1,
            key: "a".into(),
            kind: RequestClass::Interactive,
        });
        p.waiting.push_back(Ticket {
            id: 2,
            key: "b".into(),
            kind: RequestClass::Background,
        });
        assert_eq!(next(&p), Some(0));
        for turn in 1..4 {
            p.turn = turn;
            assert_eq!(next(&p), Some(1));
        }
        p.waiting.pop_front();
        p.turn = 0;
        assert_eq!(next(&p), Some(0));
    }

    /// 迁移自 app-core：快键不得侵占慢键的交互拍。
    #[test]
    fn fast_key_cannot_consume_slow_keys_interactive_turn() {
        let mut p = PoolState::<Dummy> {
            turn: 1,
            ..PoolState::default()
        };
        p.waiting.push_back(Ticket {
            id: 1,
            key: "slow".into(),
            kind: RequestClass::Interactive,
        });
        p.waiting.push_back(Ticket {
            id: 2,
            key: "slow".into(),
            kind: RequestClass::Background,
        });
        for turn in [2, 3, 0, 1, 2, 3, 0, 1] {
            p.turn = turn;
            assert_eq!(next(&p), Some(0));
        }
        p.version_turns.insert("slow".into(), 1);
        assert_eq!(next(&p), Some(1));
    }

    /// 迁移自 app-core：在途键忙则同键不可入、异键可入。
    #[test]
    fn busy_key_unavailable_but_other_key_admissible() {
        let busy_slot = Dummy::new();
        busy_slot.set_busy(true);
        let mut p = PoolState::<Dummy>::default();
        p.slots.insert("old".into(), busy_slot);
        p.waiting.push_back(Ticket {
            id: 9,
            key: "new".into(),
            kind: RequestClass::Interactive,
        });
        assert!(available(&p, "new"));
        assert_eq!(next(&p), Some(0));
        assert!(!available(&p, "old"));
    }

    /// 满队列立即拒绝（未接受，无守卫）。
    #[test]
    fn queue_full_rejects_immediately() {
        let pool = executor(1, 0, usize::MAX);
        let err = pool
            .acquire("a", soon(200), RequestClass::Interactive, 0, Dummy::new)
            .unwrap_err();
        assert_eq!(err, AcquireError::CapacityExceeded);
    }

    /// 在途用量超上限立即拒绝；守卫释放后退账可再入。
    #[test]
    fn usage_budget_blocks_until_guard_released() {
        let pool = executor(1, 16, 100);
        let (lease, guard) = pool
            .acquire("a", soon(1000), RequestClass::Interactive, 60, Dummy::new)
            .unwrap();
        assert_eq!(
            pool.acquire("a", soon(200), RequestClass::Background, 50, Dummy::new)
                .unwrap_err(),
            AcquireError::CapacityExceeded
        );
        drop(lease);
        drop(guard);
        assert!(pool
            .acquire("a", soon(1000), RequestClass::Interactive, 100, Dummy::new)
            .is_ok());
    }

    /// 唯一槽被占时，等待者到期返回 DeadlineExceeded；释放后可复用同会话。
    #[test]
    fn deadline_exceeded_while_slot_busy_and_reuse_after_release() {
        let pool = executor(1, 16, usize::MAX);
        let (lease, guard) = pool
            .acquire("a", soon(1000), RequestClass::Interactive, 0, Dummy::new)
            .unwrap();
        assert_eq!(
            pool.acquire("b", soon(150), RequestClass::Background, 0, Dummy::new)
                .unwrap_err(),
            AcquireError::DeadlineExceeded
        );
        let slot = lease.slot.clone();
        drop(lease);
        drop(guard);
        let (again, again_guard) = pool
            .acquire("a", soon(1000), RequestClass::Interactive, 0, Dummy::new)
            .unwrap();
        // 同键复用池内既有会话，不重复建槽
        assert!(Arc::ptr_eq(&slot, &again.slot));
        drop(again);
        drop(again_guard);
    }

    /// 关闭屏障：shutdown 后新请求拒绝、既有会话被回收。
    #[test]
    fn shutdown_rejects_new_and_evicts_slots() {
        let pool = executor(2, 16, usize::MAX);
        let (lease, guard) = pool
            .acquire("a", soon(1000), RequestClass::Interactive, 0, Dummy::new)
            .unwrap();
        let slot = lease.slot.clone();
        drop(lease);
        drop(guard);
        pool.shutdown();
        assert!(slot.is_retired());
        assert_eq!(
            pool.acquire("a", soon(200), RequestClass::Interactive, 0, Dummy::new)
                .unwrap_err(),
            AcquireError::ShuttingDown
        );
    }
}
