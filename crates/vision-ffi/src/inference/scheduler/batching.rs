//! 自适应攒批调度接线（R5-2.1）：逐模型登记配置/统计，聚合已接受项成窗口，
//! 由凑满、窗口截止或关闭排空触发成批。窗口项持有各自准入，等待期计入既有
//! 队列与在途账目，不占推理线程；执行复用既有 Job/worker 与同源核心组批预算。
use super::*;
use am_vision_core::sdk::batching::{self as core, Coordinator, WindowFlush};
use am_vision_core::sdk::Preprocess;
use std::time::Instant;

/// 聚合键：逐项输入 shape 签名 + 前处理参数完全一致才同批（张量项无前处理）。
/// 同一模型实例即同一实际后端选择；键不匹配项不等待、立即按现行规则执行。
pub(super) type AggKey = (
    super::super::submission::TensorSignature,
    Option<Preprocess>,
);

/// 从提交计划构造聚合键；AUTO 原生组内项目已按签名分组，此处再叠加完整前处理。
pub(in crate::inference) fn agg_key(model: &ModelHandle, plan: &Plan) -> AggKey {
    match plan {
        Plan::Image(p) => (
            plan.signature(&model.info.input_specs[0].name),
            Some(p.preprocess),
        ),
        Plan::Tensors(_) => (plan.signature(&model.info.input_specs[0].name), None),
    }
}

/// 已接受、仍在窗口内等待的单项；逐项 request_id/取消/TTL 语义保持不变。
pub(super) struct Pending {
    pub input: Input,
    pub id: u64,
    pub params: am_infer_params_t,
    pub admission: Admission,
}
// params.raw_preprocess 在接受前已复制进 Input 并置空，指针不跨线程使用；
// 其余字段与 Job 同构（Job 已有同样的 Send 声明）。
unsafe impl Send for Pending {}

/// 逐模型攒批登记：配置代次 + 累计统计 + 单开窗协调器。
pub(super) struct Registered {
    key: usize,
    entry: Arc<Entry>,
    parallel: usize,
    config: Option<core::Config>,
    generation: u64,
    pub stats: core::Stats,
    coord: Coordinator<AggKey, Pending>,
}

static REGISTRY: Mutex<Vec<Registered>> = Mutex::new(Vec::new());

fn registered(key: usize, entry: &Arc<Entry>) -> Result<usize> {
    let mut registry = lock(&REGISTRY)?;
    if let Some(index) = registry.iter().position(|r| r.key == key) {
        return Ok(index);
    }
    registry.push(Registered {
        key,
        entry: entry.clone(),
        parallel: 1,
        config: None,
        generation: 0,
        stats: core::Stats::default(),
        coord: Coordinator::default(),
    });
    Ok(registry.len() - 1)
}

/// 模型是否启用了攒批（仅动态批量轴可启用；未登记视为关闭）。
pub(in crate::inference) fn enabled(model: &ModelHandle) -> bool {
    let key = model.info.key.load(Ordering::Acquire);
    model.info.native_batch == 1
        && lock(&REGISTRY).is_ok_and(|r| r.iter().any(|x| x.key == key && x.config.is_some()))
}

/// 更新配置；生效值变化才推进代次（运行期切换只影响新请求，旧窗口按旧配置走完）。
/// 关闭（flags 未置 ENABLE）清零累计统计并放弃开窗，查询返回默认选项与全零计数。
pub(in crate::inference) fn configure(
    key: usize,
    entry: &Arc<Entry>,
    config: Option<core::Config>,
) -> Result<()> {
    let index = registered(key, entry)?;
    let mut registry = lock(&REGISTRY)?;
    let r = &mut registry[index];
    if r.config != config {
        r.generation += 1;
    }
    if config.is_none() {
        r.stats = core::Stats::default();
        r.coord = Coordinator::default();
    }
    r.config = config;
    Ok(())
}

/// 查询当前生效配置与累计统计；未登记模型返回关闭默认值。
pub(in crate::inference) fn snapshot(key: usize) -> (Option<core::Config>, core::Stats) {
    lock(&REGISTRY)
        .ok()
        .and_then(|r| r.iter().find(|x| x.key == key).map(|x| (x.config, x.stats)))
        .unwrap_or((None, core::Stats::default()))
}

/// 攒批启用期间的逐项拒绝（背压/校验拒绝）计数。
pub(in crate::inference) fn count_rejected(key: usize, count: u64) {
    if let Ok(mut registry) = lock(&REGISTRY) {
        if let Some(r) = registry.iter_mut().find(|r| r.key == key) {
            if r.config.is_some() {
                r.stats.rejected_items = r.stats.rejected_items.saturating_add(count);
            }
        }
    }
}

fn job_single(entry: Arc<Entry>, parallel: usize, pending: Pending) -> Job {
    let Pending {
        input,
        id,
        params,
        admission,
    } = pending;
    Job {
        model: entry,
        input: Payload::Single(input),
        id,
        params,
        parallel,
        _admissions: vec![admission],
    }
}

fn job_native(entry: Arc<Entry>, parallel: usize, items: Vec<Pending>) -> Job {
    let count = items.len();
    let mut ids = Vec::with_capacity(count);
    let mut inputs = Vec::with_capacity(count);
    let mut admissions = Vec::with_capacity(count);
    let mut params = am_infer_params_t::default();
    for (index, pending) in items.into_iter().enumerate() {
        if index == 0 {
            params = pending.params;
        }
        ids.push(pending.id);
        inputs.push(pending.input);
        admissions.push(pending.admission);
    }
    let id = ids.first().copied().unwrap_or(0);
    Job {
        model: entry,
        input: Payload::Native { inputs, ids },
        id,
        params,
        parallel,
        _admissions: admissions,
    }
}

fn flush_jobs(entry: &Arc<Entry>, parallel: usize, flush: WindowFlush<Pending>) -> Vec<Job> {
    if flush.batched {
        vec![job_native(entry.clone(), parallel, flush.items)]
    } else {
        // 截止不足 2 项：不组批，逐项执行。
        flush
            .items
            .into_iter()
            .map(|pending| job_single(entry.clone(), parallel, pending))
            .collect()
    }
}

fn bypass_jobs(entry: &Arc<Entry>, parallel: usize, items: Vec<Pending>) -> Vec<Job> {
    // 未启用/键或代次不匹配：维持现行逐次执行语义，多项目仍按单次原生组提交。
    if items.len() == 1 {
        vec![job_single(
            entry.clone(),
            parallel,
            items.into_iter().next().expect("single item"),
        )]
    } else {
        vec![job_native(entry.clone(), parallel, items)]
    }
}

/// 提交路由：聚合条件满足则入窗等待（或凑满/即发成批），否则按现行规则执行。
/// 返回需要入队执行的 Job；空表示全部在窗等待。
/// 提交路由：聚合条件满足则入窗等待（或凑满/即发成批），否则按现行规则执行。
/// 返回需要入队执行的 Job；空表示全部在窗等待。不可失败：请求此时已被记账，
/// 登记异常时降级为立即执行，不因攒批自身故障拒绝或搁置已接受请求。
pub(in crate::inference) fn route(
    key: usize,
    entry: &Arc<Entry>,
    parallel: usize,
    agg: AggKey,
    items: Vec<Pending>,
) -> Vec<Job> {
    let mut held = Some(items);
    let routed: Result<Vec<Job>> = (|| {
        let index = registered(key, entry)?;
        let mut registry = lock(&REGISTRY)?;
        let r = &mut registry[index];
        r.parallel = parallel;
        let now = Instant::now();
        let items = held.take().expect("items held until submit");
        Ok(
            match r
                .coord
                .submit(agg, items, r.config, r.generation, now, &mut r.stats)
            {
                core::Submitted::Bypass(items) => bypass_jobs(entry, parallel, items),
                core::Submitted::Parked => Vec::new(),
                core::Submitted::Flush(flush) => flush_jobs(entry, parallel, flush),
            },
        )
    })();
    routed.unwrap_or_else(|_| bypass_jobs(entry, parallel, held.take().unwrap_or_default()))
}

/// 周期触发：窗口截止成批；模型关闭排空。由 worker 持域锁调用，产出 Job 直接入队。
pub(in crate::inference) fn tick(now: Instant) -> Vec<Job> {
    tick_inner(now, |entry| {
        entry
            .data::<ModelHandle>()
            .map_or(true, |m| m.stopping.load(Ordering::Acquire))
    })
}

fn tick_inner(now: Instant, stopping_of: impl Fn(&Arc<Entry>) -> bool) -> Vec<Job> {
    let mut jobs = Vec::new();
    let Ok(mut registry) = lock(&REGISTRY) else {
        return jobs;
    };
    for r in registry.iter_mut() {
        // 已取消/已终态的等待项不进入后续批次；records仅try_lock，与既有
        // 退休扫描同样避开与准入的records→域锁锁序反转。
        if let Ok(model) = r.entry.data::<ModelHandle>() {
            match model.state.records.try_lock() {
                Ok(records) => {
                    for pending in r.coord.prune(|p| {
                        records
                            .get(&p.id)
                            .is_none_or(|r| !r.canceled && r.state < AM_REQ_DONE as u32)
                    }) {
                        finish_pending(model, pending);
                    }
                }
                Err(std::sync::TryLockError::WouldBlock) => {}
                Err(std::sync::TryLockError::Poisoned(_)) => {}
            }
        }
        let flush = if stopping_of(&r.entry) {
            r.coord.drain(&mut r.stats)
        } else {
            r.coord.tick(now, &mut r.stats)
        };
        if let Some(flush) = flush {
            jobs.extend(flush_jobs(&r.entry, r.parallel, flush));
        }
    }
    jobs
}

/// 窗口中止项逐项收口：与 retired::release 同构——不进批次、按关闭类错误
/// 终态化、归还输入保留与准入占用、递减在途计数。
fn finish_pending(model: &ModelHandle, pending: Pending) {
    execution::finish(model, pending.id, Err(AM_ERR_SDK_HANDLE_CLOSED));
    drop(pending.input);
    drop(pending.admission);
    model.inflight.fetch_sub(1, Ordering::AcqRel);
    model.state.changed.notify_all();
}

/// 最近一个开窗截止时间；worker 用它缩短睡眠，保证窗口及时触发。
pub(in crate::inference) fn next_deadline() -> Option<Instant> {
    let registry = lock(&REGISTRY).ok()?;
    registry
        .iter()
        .filter_map(|r| r.coord.next_deadline())
        .min()
}

/// 模型资源收尾：回收登记；仍在窗的项按关闭语义逐项收口（正常关闭时窗口
/// 已由排空触发清空，此处兜底异常路径的准入与在途账目）。
pub(in crate::inference) fn release(model: &ModelHandle) {
    let key = model.info.key.load(Ordering::Acquire);
    let Ok(mut registry) = lock(&REGISTRY) else {
        return;
    };
    let Some(index) = registry.iter().position(|r| r.key == key) else {
        return;
    };
    let mut removed = registry.remove(index);
    let flushed = removed.coord.drain(&mut core::Stats::default());
    let Some(flush) = flushed else {
        return;
    };
    let count = flush.items.len();
    for pending in flush.items {
        execution::finish(model, pending.id, Err(AM_ERR_SDK_HANDLE_CLOSED));
        drop(pending.input);
        drop(pending.admission);
    }
    model.inflight.fetch_sub(count, Ordering::AcqRel);
    model.state.changed.notify_all();
}

#[cfg(test)]
mod tests;
