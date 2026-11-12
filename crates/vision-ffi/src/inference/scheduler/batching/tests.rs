//! R5-2.1 攒批调度接线测试：复用真实 Domain/Context 作为登记宿主，准入与
//! Job 以 storage_tests 同款装配方式构造；引擎不派工作线程，触发全部由
//! 显式 tick 驱动，不依赖 ORT 运行库。
use super::super::exec_stage;
use super::*;
use am_sdk_support::abi::*;
use std::time::{Duration, Instant};

// REGISTRY 为模块级共享静态；用例串行避免跨键触发相互排空。
// 锁按毒化恢复获取：单用例失败毒锁后，后续用例照常运行不级联掩盖。
static TEST_LOCK: Mutex<()> = Mutex::new(());

struct Fixture {
    _domain: am_sdk_support::handles::Domain,
    key: usize,
    entry: Arc<Entry>,
    usage: Arc<super::super::super::state::Usage>,
    engine: Arc<Engine>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Ok(mut registry) = REGISTRY.lock() {
            registry.retain(|r| r.key != self.key);
        }
    }
}

fn fixture(key: usize, config: Option<core::Config>) -> Fixture {
    let budget = ThreadBudget {
        parallel: 2,
        total: 4,
        slots: 64,
    };
    // 不生成执行器线程：窗口触发全部由用例显式调用，避免后台tick竞争。
    let engine = Arc::new(Engine {
        data: Mutex::new(Data::new(budget).unwrap()),
        wake: Condvar::new(),
        budget,
        stop: AtomicBool::new(false),
        threads: Mutex::new(Vec::new()),
    });
    let domain = am_sdk_support::handles::Domain::new(true);
    let params = amv_init_params_t {
        abi_version: AM_ABI_VERSION,
        ..Default::default()
    };
    let ctx = unsafe { am_sdk_support::context::initialize_vision(&domain, &params) }.unwrap();
    let entry = domain.get(ctx).unwrap();
    let usage = Arc::new(super::super::super::state::Usage::default());
    configure(key, &entry, config).unwrap();
    Fixture {
        _domain: domain,
        key,
        entry,
        usage,
        engine,
    }
}

fn admissions(f: &Fixture, count: usize, bytes: usize) -> Vec<Admission> {
    // 与 budget::reserve 同步起账：真实路径在准入时即计入执行器与分类账。
    {
        let mut data = f.engine.data.lock().unwrap();
        data.inflight += count;
        data.bytes += count * bytes;
    }
    f.usage.preprocess.lock().unwrap()[0] += count * bytes;
    (0..count)
        .map(|_| Admission {
            engine: f.engine.clone(),
            usage: f.usage.clone(),
            class: 0,
            bytes,
            items: 1,
        })
        .collect()
}

fn pending(_f: &Fixture, ids: &[u64], guards: Vec<Admission>) -> Vec<Pending> {
    ids.iter()
        .zip(guards)
        .map(|(id, admission)| Pending {
            input: Input::Tensors(super::super::super::tensors::Input {
                tensors: Vec::new(),
                assoc: None,
            }),
            id: *id,
            params: am_infer_params_t::default(),
            admission,
            exec_charge: 0,
        })
        .collect()
}

fn agg(tag: u32) -> AggKey {
    (
        vec![(format!("x{tag}"), 0, vec![1, 2])],
        None,
        BatchEntry::Tensor,
    )
}

#[test]
fn route_parks_until_fill_then_forms_one_native_job() {
    let _lock = TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let f = fixture(
        0x5501,
        Some(core::Config {
            max_batch_items: 3,
            max_wait_ms: 5000,
        }),
    );
    // 未凑满：在窗等待、无 Job 入队；等待项已计入执行器在途账目，不占线程。
    let parked = route(
        f.key,
        &f.entry,
        2,
        agg(1),
        pending(&f, &[1, 2], admissions(&f, 2, 64)),
    );
    assert!(parked.is_empty());
    assert_eq!(f.engine.data.lock().unwrap().inflight, 2);
    assert_eq!(snapshot(f.key).1, core::Stats::default());
    // 凑满：恰一个原生 Job，逐项 request_id 保序，逐项准入合计计费、逐项
    // params 随 Job 携带（批 C：逐项 decode 参数逐项生效的载体）。
    let mut with_params = pending(&f, &[3], admissions(&f, 1, 64));
    with_params[0].params.confidence_threshold = 0.5;
    let jobs = route(f.key, &f.entry, 2, agg(1), with_params);
    assert_eq!(jobs.len(), 1);
    let Payload::Native {
        inputs,
        ids,
        params,
    } = &jobs[0].input
    else {
        panic!("凑满应为原生批量 Job");
    };
    assert_eq!(ids, &[1, 2, 3]);
    assert_eq!(inputs.len(), 3);
    assert_eq!(params.len(), 3);
    assert_eq!(params[2].confidence_threshold, 0.5);
    assert_eq!(Admission::totals(&jobs[0]._admissions), (3, 192));
    let stats = snapshot(f.key).1;
    assert_eq!(
        (
            stats.formed_batches,
            stats.batched_items,
            stats.timeout_batches
        ),
        (1, 3, 0)
    );
    drop(jobs);
    assert_eq!(f.engine.data.lock().unwrap().inflight, 0);
}

/// 批 E 分期计费：挂起项准入只入账实驻留份额，执行期补账随 Job 逐项携带；
/// 凑满/旁路两形态的准入合计、释放守恒（inflight 与 bytes 双归零）。
#[test]
fn staged_exec_charges_flow_into_jobs_and_conserve() {
    let _lock = TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let f = fixture(
        0x550a,
        Some(core::Config {
            max_batch_items: 3,
            max_wait_ms: 5000,
        }),
    );
    let mut parked = pending(&f, &[1, 2], admissions(&f, 2, 8));
    for item in &mut parked {
        item.exec_charge = 32;
    }
    // 挂起期：仅入账实驻留（2×8），执行期份额不提前占用。
    assert!(route(f.key, &f.entry, 2, agg(1), parked).is_empty());
    {
        let data = f.engine.data.lock().unwrap();
        assert_eq!((data.inflight, data.bytes), (2, 16));
    }
    // 凑满：原生 Job 逐项携带补账字节；准入合计仍为挂起份额。
    let mut fill = pending(&f, &[3], admissions(&f, 1, 8));
    fill[0].exec_charge = 32;
    let jobs = route(f.key, &f.entry, 2, agg(1), fill);
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].exec_charges, vec![32, 32, 32]);
    assert_eq!(Admission::totals(&jobs[0]._admissions), (3, 24));
    {
        let data = f.engine.data.lock().unwrap();
        assert_eq!((data.inflight, data.bytes), (3, 24));
    }
    // 释放守恒：Job 丢弃即全量归还，账目归零（含分类账）。
    drop(jobs);
    {
        let data = f.engine.data.lock().unwrap();
        assert_eq!((data.inflight, data.bytes), (0, 0));
    }
    assert_eq!(*f.usage.preprocess.lock().unwrap(), [0, 0]);
    // 旁路单项：窗口开着（挂起一项 agg(1)）时键不匹配项不等待、立即执行，
    // 补账同样逐项携带；随后排空窗内挂起项，账目全量归零。
    let mut holder = pending(&f, &[7], admissions(&f, 1, 8));
    holder[0].exec_charge = 32;
    assert!(route(f.key, &f.entry, 2, agg(1), holder).is_empty());
    let mut bypass = pending(&f, &[8], admissions(&f, 1, 8));
    bypass[0].exec_charge = 32;
    let jobs = route(f.key, &f.entry, 2, agg(2), bypass);
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].exec_charges, vec![32]);
    drop(jobs);
    let drained = tick_inner(Instant::now(), |_| true).jobs;
    assert_eq!(drained.len(), 1);
    drop(drained);
    let data = f.engine.data.lock().unwrap();
    assert_eq!((data.inflight, data.bytes), (0, 0));
}

/// 批 C：执行入口进聚合键——同签名同前处理的 CPU 图像与 GPU 帧不同键，
/// 不并窗（组装路径不同：CPU 张量合并 vs 设备侧组装）；键不匹配项不等待。
#[test]
fn execution_entry_dimension_separates_windows() {
    let _lock = TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let f = fixture(
        0x5509,
        Some(core::Config {
            max_batch_items: 2,
            max_wait_ms: 5000,
        }),
    );
    let image = |entry: BatchEntry| -> AggKey {
        (vec![("images".into(), 0, vec![1, 3, 64, 64])], None, entry)
    };
    assert!(image(BatchEntry::ImageCpu) != image(BatchEntry::ImageGpu));
    assert!(image(BatchEntry::ImageCpu) != agg(1));
    // CPU 图像项入窗等待；同签名 GPU 帧项键不匹配，立即按现行规则执行。
    assert!(route(
        f.key,
        &f.entry,
        2,
        image(BatchEntry::ImageCpu),
        pending(&f, &[1], admissions(&f, 1, 8))
    )
    .is_empty());
    let jobs = route(
        f.key,
        &f.entry,
        2,
        image(BatchEntry::ImageGpu),
        pending(&f, &[2], admissions(&f, 1, 8)),
    );
    assert_eq!(jobs.len(), 1);
    assert!(matches!(jobs[0].input, Payload::Single(_)));
    drop(jobs);
}

#[test]
fn disabled_route_keeps_current_per_request_semantics() {
    let _lock = TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let f = fixture(0x5502, None);
    // 未启用：单项成 Single Job，维持现行逐次执行、零攒批。
    let jobs = route(
        f.key,
        &f.entry,
        2,
        agg(1),
        pending(&f, &[7], admissions(&f, 1, 8)),
    );
    assert_eq!(jobs.len(), 1);
    assert!(matches!(jobs[0].input, Payload::Single(_)));
    assert_eq!(jobs[0].id, 7);
    drop(jobs);
    assert_eq!(f.engine.data.lock().unwrap().inflight, 0);
    assert_eq!(snapshot(f.key), (None, core::Stats::default()));
}

#[test]
fn key_mismatch_and_generation_switch_execute_immediately() {
    let _lock = TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let f = fixture(
        0x5504,
        Some(core::Config {
            max_batch_items: 4,
            max_wait_ms: 5000,
        }),
    );
    assert!(route(
        f.key,
        &f.entry,
        2,
        agg(1),
        pending(&f, &[1], admissions(&f, 1, 8))
    )
    .is_empty());
    // 聚合条件不满足（shape/前处理不一致）：不等待、立即按现行规则执行。
    let jobs = route(
        f.key,
        &f.entry,
        2,
        agg(2),
        pending(&f, &[2], admissions(&f, 1, 8)),
    );
    assert_eq!(jobs.len(), 1);
    assert!(matches!(jobs[0].input, Payload::Single(_)));
    assert_eq!(jobs[0].id, 2);
    drop(jobs);
    // 运行期切换：新请求不并入旧窗，旧窗按旧配置走完（截止排空）。
    configure(
        f.key,
        &f.entry,
        Some(core::Config {
            max_batch_items: 8,
            max_wait_ms: 5000,
        }),
    )
    .unwrap();
    let jobs = route(
        f.key,
        &f.entry,
        2,
        agg(1),
        pending(&f, &[3], admissions(&f, 1, 8)),
    );
    assert_eq!(jobs.len(), 1);
    assert!(matches!(jobs[0].input, Payload::Single(_)));
    drop(jobs);
    let drained = tick_inner(Instant::now() + Duration::from_secs(6), |_| false).jobs;
    assert_eq!(drained.len(), 1);
    assert!(matches!(drained[0].input, Payload::Single(_)));
    drop(drained);
}

#[test]
fn deadline_tick_forms_batch_and_under_two_runs_per_item() {
    let _lock = TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let f = fixture(
        0x5505,
        Some(core::Config {
            max_batch_items: 8,
            max_wait_ms: 2000,
        }),
    );
    assert!(route(
        f.key,
        &f.entry,
        2,
        agg(1),
        pending(&f, &[1, 2, 3], admissions(&f, 3, 8))
    )
    .is_empty());
    // 未到期不触发。
    assert!(
        tick_inner(Instant::now() + Duration::from_millis(10), |_| false)
            .jobs
            .is_empty()
    );
    // 截止：≥2 项合成一个原生 Job，计为超时批次。
    let jobs = tick_inner(Instant::now() + Duration::from_millis(2001), |_| false).jobs;
    assert_eq!(jobs.len(), 1);
    let Payload::Native { ids, .. } = &jobs[0].input else {
        panic!("截止≥2项应成批");
    };
    assert_eq!(ids, &[1, 2, 3]);
    let stats = snapshot(f.key).1;
    assert_eq!(
        (
            stats.formed_batches,
            stats.batched_items,
            stats.timeout_batches
        ),
        (1, 3, 1)
    );
    drop(jobs);
    // 不足 2 项：不组批、逐项执行。
    assert!(route(
        f.key,
        &f.entry,
        2,
        agg(1),
        pending(&f, &[9], admissions(&f, 1, 8))
    )
    .is_empty());
    let singles = tick_inner(Instant::now() + Duration::from_millis(2001), |_| false).jobs;
    assert_eq!(singles.len(), 1);
    assert!(matches!(singles[0].input, Payload::Single(_)));
    drop(singles);
}

#[test]
fn stopping_drains_window_and_disable_clears_stats() {
    let _lock = TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let f = fixture(
        0x5506,
        Some(core::Config {
            max_batch_items: 8,
            max_wait_ms: 5000,
        }),
    );
    assert!(route(
        f.key,
        &f.entry,
        2,
        agg(1),
        pending(&f, &[1, 2], admissions(&f, 2, 8))
    )
    .is_empty());
    // 模型关闭排空：立即触发，≥2 项按单一物理批次执行。
    let jobs = tick_inner(Instant::now(), |_| true).jobs;
    assert_eq!(jobs.len(), 1);
    assert!(matches!(jobs[0].input, Payload::Native { .. }));
    drop(jobs);
    assert_eq!(snapshot(f.key).1.formed_batches, 1);
    // 关闭攒批：累计统计清零、放弃开窗，查询返回默认选项与全零计数。
    configure(f.key, &f.entry, None).unwrap();
    let (config, stats) = snapshot(f.key);
    assert_eq!(config, None);
    assert_eq!(stats, core::Stats::default());
    // 关闭后新提交直接按现行规则执行。
    let jobs = route(
        f.key,
        &f.entry,
        2,
        agg(1),
        pending(&f, &[5], admissions(&f, 1, 8)),
    );
    assert_eq!(jobs.len(), 1);
    assert!(matches!(jobs[0].input, Payload::Single(_)));
    drop(jobs);
}

#[test]
fn rejected_items_accumulate_only_while_enabled() {
    let _lock = TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let f = fixture(
        0x5507,
        Some(core::Config {
            max_batch_items: 2,
            max_wait_ms: 0,
        }),
    );
    count_rejected(f.key, 2);
    count_rejected(f.key, 1);
    assert_eq!(snapshot(f.key).1.rejected_items, 3);
    // 关闭状态不累计；已清零的计数保持全零。
    configure(f.key, &f.entry, None).unwrap();
    count_rejected(f.key, 4);
    assert_eq!(snapshot(f.key).1.rejected_items, 0);
}

#[test]
fn next_deadline_tracks_open_windows() {
    let _lock = TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let f = fixture(
        0x5508,
        Some(core::Config {
            max_batch_items: 4,
            max_wait_ms: 300,
        }),
    );
    assert!(route(
        f.key,
        &f.entry,
        2,
        agg(1),
        pending(&f, &[1], admissions(&f, 1, 8))
    )
    .is_empty());
    let deadline = next_deadline().expect("开窗后应有截止");
    assert!(deadline <= Instant::now() + Duration::from_millis(300));
    // 排空后不再报告（其他用例串行收尾、登记已回收）。
    let jobs = tick_inner(Instant::now(), |_| true).jobs;
    drop(jobs);
    assert!(next_deadline().is_none());
}

/// B1.4 双 Model 交替全周期：两模型共享执行器，交替攒批排出带执行份额的批、
/// 交替完成（drop）归还，随后关闭排空收尾——全程账目精确、终态归零。
#[test]
fn dual_model_alternating_cycles_conserve_to_zero() {
    let _lock = TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let f = fixture(
        0x5601,
        Some(core::Config {
            max_batch_items: 2,
            max_wait_ms: 5000,
        }),
    );
    let second = 0x5602;
    configure(
        second,
        &f.entry,
        Some(core::Config {
            max_batch_items: 2,
            max_wait_ms: 5000,
        }),
    )
    .unwrap();
    struct Second(usize);
    impl Drop for Second {
        fn drop(&mut self) {
            if let Ok(mut registry) = REGISTRY.lock() {
                registry.retain(|r| r.key != self.0);
            }
        }
    }
    let _second = Second(second);
    // 交替挂起：A、B 各一项在窗（凑满线 2），挂起份额同源累计。
    assert!(route(f.key, &f.entry, 2, agg(1), staged_pending(&f, &[1])).is_empty());
    assert!(route(second, &f.entry, 2, agg(1), staged_pending(&f, &[2])).is_empty());
    assert_eq!(ledger(&f), (2, 16));
    // 交替凑满排出：各自 Native Job 逐项携带执行份额。
    let job_a = route(f.key, &f.entry, 2, agg(1), staged_pending(&f, &[3])).remove(0);
    let job_b = route(second, &f.entry, 2, agg(1), staged_pending(&f, &[4])).remove(0);
    assert_eq!(job_a.exec_charges, vec![32, 32]);
    assert_eq!(job_b.exec_charges, vec![32, 32]);
    assert_eq!(Admission::totals(&job_a._admissions), (2, 16));
    assert_eq!(Admission::totals(&job_b._admissions), (2, 16));
    assert_eq!(ledger(&f), (4, 32));
    // 交替完成：逐 Job 全量归还，账目精确回落。
    drop(job_a);
    assert_eq!(ledger(&f), (2, 16));
    drop(job_b);
    assert_eq!(ledger(&f), (0, 0));
    assert_eq!(*f.usage.preprocess.lock().unwrap(), [0, 0]);
    // 关闭排空：两模型窗口剩余项一次收口，账目归零。
    assert!(route(f.key, &f.entry, 2, agg(1), staged_pending(&f, &[5])).is_empty());
    assert!(route(second, &f.entry, 2, agg(1), staged_pending(&f, &[6])).is_empty());
    assert_eq!(ledger(&f), (2, 16));
    let drained = tick_inner(Instant::now(), |_| true).jobs;
    assert_eq!(drained.len(), 2);
    drop(drained);
    assert_eq!(ledger(&f), (0, 0));
    assert_eq!(*f.usage.preprocess.lock().unwrap(), [0, 0]);
}

fn staged_pending(f: &Fixture, ids: &[u64]) -> Vec<Pending> {
    let mut items = pending(f, ids, admissions(f, ids.len(), 8));
    for item in &mut items {
        item.exec_charge = 32;
    }
    items
}

/// 执行器共享账目快照（单次加锁读取，禁在断言元组内重复 lock）。
fn ledger(f: &Fixture) -> (usize, usize) {
    let data = f.engine.data.lock().unwrap();
    (data.inflight, data.bytes)
}

/// B1.4 错峰等待链：A 持槽执行中，B 批已排出只在挂起份额下等槽（执行份额
/// 未入账）；等槽期收口全量归还；槽释放后 B 补账执行再归零——挂起份额
/// 全链不泄漏。
#[test]
fn slot_wait_keeps_pending_share_and_frees_after_completion() {
    let _lock = TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let f = fixture(
        0x5603,
        Some(core::Config {
            max_batch_items: 2,
            max_wait_ms: 5000,
        }),
    );
    let holder = exec_stage::acquire(&f.engine).unwrap();
    assert!(f.engine.data.lock().unwrap().exec_slot);
    // B 批排出即等槽：仅挂起份额在账（2×8），执行份额随 Job 携带未入账。
    let waiting = route(f.key, &f.entry, 2, agg(1), staged_pending(&f, &[1, 2])).remove(0);
    assert_eq!(waiting.exec_charges, vec![32, 32]);
    assert_eq!(ledger(&f), (2, 16));
    // 等槽期收口（取消/TTL/关闭同出口）：挂起份额全量归还，槽不受影响。
    drop(waiting);
    assert_eq!(ledger(&f), (0, 0));
    assert!(f.engine.data.lock().unwrap().exec_slot);
    // 槽释放后 B 补账执行：同源补账（逐项 topup，stage_charges 同链）成功，
    // 执行完成（drop guard+topup+Job）账目归零。
    drop(holder);
    assert!(!f.engine.data.lock().unwrap().exec_slot);
    let retry = route(f.key, &f.entry, 2, agg(1), staged_pending(&f, &[3, 4])).remove(0);
    let guard = exec_stage::acquire(&f.engine).unwrap();
    let exec_guards: Vec<_> = retry
        ._admissions
        .iter()
        .zip(&retry.exec_charges)
        .map(|(admission, extra)| {
            budget::topup(admission, *extra, &limits(1024), &policy(1024)).unwrap()
        })
        .collect();
    assert_eq!(ledger(&f), (2, 80));
    drop(exec_guards);
    drop(retry);
    drop(guard);
    assert_eq!(ledger(&f), (0, 0));
    assert_eq!(*f.usage.preprocess.lock().unwrap(), [0, 0]);
}

fn limits(bytes: u64) -> am_limits_t {
    am_limits_t {
        preprocess_budget_bytes: bytes,
        ..Default::default()
    }
}

fn policy(bytes: u64) -> am_queue_policy_t {
    am_queue_policy_t {
        max_bytes: bytes,
        max_items: 16,
        ..Default::default()
    }
}

/// B1.4 等槽/挂起期取消与 TTL 收口（finish_pruned 统一出口）：剪除项的
/// 挂起份额全量归还、不残留执行份额。
#[test]
fn pruned_pending_share_returns_conserved() {
    let _lock = TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let f = fixture(0x5604, None);
    let items = staged_pending(&f, &[1, 2, 3]);
    assert_eq!(ledger(&f), (3, 24));
    finish_pruned(vec![(f.entry.clone(), items)]);
    assert_eq!(ledger(&f), (0, 0));
    assert_eq!(*f.usage.preprocess.lock().unwrap(), [0, 0]);
}

/// B1.4 关闭排空：等槽窗口项整批排出（执行份额仍随 Job 携带未入账），
/// Job 收尾全量归还。
#[test]
fn drained_pending_jobs_conserve_with_exec_charges() {
    let _lock = TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let f = fixture(
        0x5605,
        Some(core::Config {
            max_batch_items: 4,
            max_wait_ms: 5000,
        }),
    );
    assert!(route(f.key, &f.entry, 2, agg(1), staged_pending(&f, &[1, 2, 3])).is_empty());
    assert_eq!(ledger(&f), (3, 24));
    let drained = tick_inner(Instant::now(), |_| true).jobs;
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].exec_charges, vec![32, 32, 32]);
    assert_eq!(ledger(&f), (3, 24));
    drop(drained);
    assert_eq!(ledger(&f), (0, 0));
    assert_eq!(*f.usage.preprocess.lock().unwrap(), [0, 0]);
}
