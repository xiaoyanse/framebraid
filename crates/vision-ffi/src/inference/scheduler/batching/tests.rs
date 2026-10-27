//! R5-2.1 攒批调度接线测试：复用真实 Domain/Context 作为登记宿主，准入与
//! Job 以 storage_tests 同款装配方式构造；引擎不派工作线程，触发全部由
//! 显式 tick 驱动，不依赖 ORT 运行库。
use super::*;
use am_sdk_support::abi::*;
use std::time::{Duration, Instant};

// REGISTRY 为模块级共享静态；用例串行避免跨键触发相互排空。
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
        })
        .collect()
}

fn agg(tag: u32) -> AggKey {
    (vec![(format!("x{tag}"), 0, vec![1, 2])], None)
}

#[test]
fn route_parks_until_fill_then_forms_one_native_job() {
    let _lock = TEST_LOCK.lock().unwrap();
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
    // 凑满：恰一个原生 Job，逐项 request_id 保序，逐项准入合计计费。
    let jobs = route(
        f.key,
        &f.entry,
        2,
        agg(1),
        pending(&f, &[3], admissions(&f, 1, 64)),
    );
    assert_eq!(jobs.len(), 1);
    let Payload::Native { inputs, ids } = &jobs[0].input else {
        panic!("凑满应为原生批量 Job");
    };
    assert_eq!(ids, &[1, 2, 3]);
    assert_eq!(inputs.len(), 3);
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

#[test]
fn disabled_route_keeps_current_per_request_semantics() {
    let _lock = TEST_LOCK.lock().unwrap();
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
    let _lock = TEST_LOCK.lock().unwrap();
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
    let _lock = TEST_LOCK.lock().unwrap();
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
    let _lock = TEST_LOCK.lock().unwrap();
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
    let _lock = TEST_LOCK.lock().unwrap();
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
    let _lock = TEST_LOCK.lock().unwrap();
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
