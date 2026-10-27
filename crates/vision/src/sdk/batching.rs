//! 自适应攒批（R5-2.1 合同，主文件 §3.5 末）：选项校验与逐模型攒批窗口纯状态机。
//! 不持有线程、锁或执行器；提交/窗口截止/关闭排空三触发由调度侧驱动。
use crate::{Error, Result};
use std::time::{Duration, Instant};

/// AM_BATCHING_ENABLE；未置该位的 flags 一律表示关闭。
pub const ENABLE: u32 = 1;
/// 动态批量轴的公开包络上限；max_batch_items=0 时按此解析。
pub const DYNAMIC_ITEM_LIMIT: u32 = 64;
/// max_wait_ms 上限（合同 0..5000）。
pub const WAIT_LIMIT_MS: u32 = 5000;

/// 已解析生效的攒批配置；生效 max_batch_items ∈ 2..=64。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Config {
    pub max_batch_items: u32,
    pub max_wait_ms: u32,
}

/// 累计只读计数；关闭状态清零。
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Stats {
    pub formed_batches: u64,
    pub batched_items: u64,
    pub timeout_batches: u64,
    pub rejected_items: u64,
}

/// 校验 configure 入参。返回 None 表示关闭（flags 未置 ENABLE，此时不校验范围）。
/// 启用仅接受动态批量轴模型（native_batch=1 且动态上限≥2）；固定轴/无批量能力
/// 返回 vision.batchUnsupported，不放宽尾项拒绝规则。
pub fn validate(
    native_batch: u32,
    max_batch_items: u32,
    max_wait_ms: u32,
    flags: u32,
) -> Result<Option<Config>> {
    if flags & !ENABLE != 0 {
        return Err(Error("sdk.invalidArgument".into()));
    }
    if flags & ENABLE == 0 {
        return Ok(None);
    }
    if native_batch != 1 {
        return Err(Error("vision.batchUnsupported".into()));
    }
    if (max_batch_items != 0 && !(2..=DYNAMIC_ITEM_LIMIT).contains(&max_batch_items))
        || max_wait_ms > WAIT_LIMIT_MS
    {
        return Err(Error("sdk.limitOutOfRange".into()));
    }
    Ok(Some(Config {
        max_batch_items: if max_batch_items == 0 {
            DYNAMIC_ITEM_LIMIT
        } else {
            max_batch_items
        },
        max_wait_ms,
    }))
}

/// 窗口排出内容的触发来源。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Trigger {
    /// 凑满 max_batch_items。
    Fill,
    /// max_wait_ms 窗口截止（含 max_wait_ms=0 即入即发）。
    Deadline,
    /// 模型关闭排空。
    Drain,
}

/// 窗口排出内容；batched=false 表示截止时不足 2 项，不组批逐项执行。
pub struct WindowFlush<T> {
    pub items: Vec<T>,
    pub batched: bool,
    pub trigger: Trigger,
}

fn flush_items<T>(items: Vec<T>, trigger: Trigger, stats: &mut Stats) -> WindowFlush<T> {
    let batched = items.len() >= 2;
    if batched {
        stats.formed_batches += 1;
        stats.batched_items += items.len() as u64;
        if trigger == Trigger::Deadline {
            stats.timeout_batches += 1;
        }
    }
    WindowFlush {
        items,
        batched,
        trigger,
    }
}

struct Window<K, T> {
    key: K,
    generation: u64,
    items: Vec<T>,
    capacity: usize,
    /// None=max_wait_ms=0，即入即发不等。
    deadline: Option<Instant>,
}

impl<K: PartialEq, T> Window<K, T> {
    fn open(key: K, config: Config, generation: u64, now: Instant) -> Self {
        Self {
            key,
            generation,
            items: Vec::new(),
            capacity: config.max_batch_items as usize,
            deadline: (config.max_wait_ms > 0)
                .then(|| now + Duration::from_millis(config.max_wait_ms as u64)),
        }
    }
    /// 已凑满，或窗口截止（含 max_wait_ms=0 即入即发）。
    fn ready(&self, now: Instant) -> bool {
        self.items.len() >= self.capacity || self.deadline.is_none_or(|d| now >= d)
    }
}

/// 提交裁决：Bypass=不聚合，立即按现行规则执行/拒绝；Parked=已入窗等待；
/// Flush=本次提交即触发（凑满或 max_wait_ms=0），可能只取走窗口前缀。
pub enum Submitted<T> {
    Bypass(Vec<T>),
    Parked,
    Flush(WindowFlush<T>),
}

/// 逐模型攒批协调器：单开窗，仅同键同代次聚合；窗口按开启时配置走完。
/// 运行期切换只影响新请求：代次不一致的新提交不并入旧窗，按现行规则立即执行。
pub struct Coordinator<K, T> {
    window: Option<Window<K, T>>,
}
impl<K, T> Default for Coordinator<K, T> {
    fn default() -> Self {
        Self { window: None }
    }
}

impl<K: PartialEq, T> Coordinator<K, T> {
    pub fn window_len(&self) -> usize {
        self.window.as_ref().map_or(0, |w| w.items.len())
    }
    pub fn next_deadline(&self) -> Option<Instant> {
        self.window.as_ref().and_then(|w| w.deadline)
    }
    /// 提交一组同键项。config=None（未启用/已关闭）→ 全组 Bypass。
    pub fn submit(
        &mut self,
        key: K,
        items: Vec<T>,
        config: Option<Config>,
        generation: u64,
        now: Instant,
        stats: &mut Stats,
    ) -> Submitted<T> {
        let Some(config) = config else {
            return Submitted::Bypass(items);
        };
        if let Some(window) = self.window.as_ref() {
            if window.key != key || window.generation != generation {
                // 聚合条件不满足：不等待、不并入，立即按现行规则执行/拒绝。
                return Submitted::Bypass(items);
            }
        } else {
            self.window = Some(Window::open(key, config, generation, now));
        }
        let window = self.window.as_mut().expect("window opened above");
        window.items.extend(items);
        if window.items.len() >= window.capacity {
            // 凑满即发：恰好取前 capacity 项成批，余量留窗继续等待。
            let capacity = window.capacity;
            let remainder = window.items.split_off(capacity);
            let batch = std::mem::take(&mut window.items);
            window.items = remainder;
            if window.items.is_empty() {
                self.window = None;
            }
            return Submitted::Flush(flush_items(batch, Trigger::Fill, stats));
        }
        if window.ready(now) {
            // 未凑满即触发：max_wait_ms=0 即入即发，或窗口已过截止。
            let window = self.window.take().expect("window present");
            return Submitted::Flush(flush_items(window.items, Trigger::Deadline, stats));
        }
        Submitted::Parked
    }
    /// 移除不再参与后续批次的等待项（已取消/已终态）；返回被移除项。
    /// 全部移除时窗口随之关闭，下一次提交按新窗口开启。
    pub fn prune(&mut self, mut keep: impl FnMut(&T) -> bool) -> Vec<T> {
        let Some(mut window) = self.window.take() else {
            return Vec::new();
        };
        let mut removed = Vec::new();
        let mut index = 0;
        while index < window.items.len() {
            if keep(&window.items[index]) {
                index += 1;
            } else {
                removed.push(window.items.remove(index));
            }
        }
        if !window.items.is_empty() {
            self.window = Some(window);
        }
        removed
    }
    /// 周期触发：窗口截止时排出；未到期窗口保持等待，等待期不占执行资源。
    pub fn tick(&mut self, now: Instant, stats: &mut Stats) -> Option<WindowFlush<T>> {
        let window = self.window.as_ref()?;
        if window.items.is_empty() || !window.ready(now) {
            return None;
        }
        let window = self.window.take().expect("checked above");
        Some(flush_items(window.items, Trigger::Deadline, stats))
    }
    /// 模型关闭排空：窗口按现状排出（≥2 成批，不足逐项），此后窗口关闭。
    pub fn drain(&mut self, stats: &mut Stats) -> Option<WindowFlush<T>> {
        self.window
            .take()
            .map(|w| flush_items(w.items, Trigger::Drain, stats))
    }
}
