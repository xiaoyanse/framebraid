use super::batching::*;
use std::time::{Duration, Instant};

fn config(items: u32, wait_ms: u32) -> Config {
    validate(1, items, wait_ms, ENABLE).unwrap().unwrap()
}
fn zero_stats() -> Stats {
    Stats::default()
}

#[test]
fn validate_bounds_and_applicability_follow_the_contract() {
    // 默认关闭：flags 未置 ENABLE 即关闭，不校验范围，任何模型可关闭。
    assert_eq!(validate(0, 999, 9999, 0).unwrap(), None);
    assert_eq!(validate(4, 999, 9999, 0).unwrap(), None);
    // 仅动态批量轴（native_batch=1）接受启用；固定轴/无批量能力拒绝。
    assert_eq!(
        validate(4, 4, 100, ENABLE).unwrap_err().0,
        "vision.batchUnsupported"
    );
    assert_eq!(
        validate(0, 4, 100, ENABLE).unwrap_err().0,
        "vision.batchUnsupported"
    );
    // 范围：max_batch_items 2..=64（0=动态上限）；max_wait_ms 0..=5000。
    assert_eq!(
        validate(1, 1, 100, ENABLE).unwrap_err().0,
        "sdk.limitOutOfRange"
    );
    assert_eq!(
        validate(1, 65, 100, ENABLE).unwrap_err().0,
        "sdk.limitOutOfRange"
    );
    assert_eq!(
        validate(1, 4, 5001, ENABLE).unwrap_err().0,
        "sdk.limitOutOfRange"
    );
    assert_eq!(config(0, 0).max_batch_items, DYNAMIC_ITEM_LIMIT);
    assert_eq!(
        config(64, 5000),
        Config {
            max_batch_items: 64,
            max_wait_ms: 5000
        }
    );
    // 未知 flag 位一律拒绝。
    assert_eq!(
        validate(1, 4, 100, ENABLE | 2).unwrap_err().0,
        "sdk.invalidArgument"
    );
}

#[test]
fn disabled_or_unmatched_submissions_bypass_without_waiting() {
    let mut c: Coordinator<u32, u32> = Coordinator::default();
    let mut stats = zero_stats();
    // 未启用：全组 Bypass，零攒批。
    assert!(matches!(
        c.submit(1, vec![7], None, 0, Instant::now(), &mut stats),
        Submitted::Bypass(v) if v == vec![7]
    ));
    assert_eq!(c.window_len(), 0);
    // 启用后首项入窗等待。
    let cfg = config(3, 50);
    assert!(matches!(
        c.submit(1, vec![1, 2], Some(cfg), 1, Instant::now(), &mut stats),
        Submitted::Parked
    ));
    assert_eq!(c.window_len(), 2);
    // 键不一致项不等待、不并入。
    assert!(matches!(
        c.submit(2, vec![9], Some(cfg), 1, Instant::now(), &mut stats),
        Submitted::Bypass(v) if v == vec![9]
    ));
    // 配置代次不一致（运行期切换后的新请求）不并入旧窗。
    assert!(matches!(
        c.submit(1, vec![8], Some(cfg), 2, Instant::now(), &mut stats),
        Submitted::Bypass(v) if v == vec![8]
    ));
    assert_eq!(c.window_len(), 2);
    assert_eq!(stats, zero_stats());
}

#[test]
fn fill_trigger_takes_exactly_capacity_and_keeps_remainder_parked() {
    let mut c: Coordinator<u32, u32> = Coordinator::default();
    let mut stats = zero_stats();
    let cfg = config(2, 50);
    assert!(matches!(
        c.submit(1, vec![1], Some(cfg), 1, Instant::now(), &mut stats),
        Submitted::Parked
    ));
    let Submitted::Flush(flush) = c.submit(1, vec![2, 3], Some(cfg), 1, Instant::now(), &mut stats)
    else {
        panic!("凑满应立即触发");
    };
    assert!(flush.batched);
    assert_eq!(flush.trigger, Trigger::Fill);
    assert_eq!(flush.items, vec![1, 2]);
    assert_eq!(stats.formed_batches, 1);
    assert_eq!(stats.batched_items, 2);
    assert_eq!(stats.timeout_batches, 0);
    // 余量留窗继续等待，不再立即触发。
    assert_eq!(c.window_len(), 1);
    assert!(c.tick(Instant::now(), &mut stats).is_none());
}

#[test]
fn deadline_with_fewer_than_two_items_executes_per_item() {
    let mut c: Coordinator<u32, u32> = Coordinator::default();
    let mut stats = zero_stats();
    let start = Instant::now();
    assert!(matches!(
        c.submit(1, vec![5], Some(config(4, 40)), 1, start, &mut stats),
        Submitted::Parked
    ));
    assert!(c
        .tick(start + Duration::from_millis(10), &mut stats)
        .is_none());
    let flush = c
        .tick(start + Duration::from_millis(41), &mut stats)
        .expect("窗口截止应排出");
    assert!(!flush.batched);
    assert_eq!(flush.trigger, Trigger::Deadline);
    assert_eq!(flush.items, vec![5]);
    // 不足 2 项不组批：零批次计数。
    assert_eq!(stats.formed_batches, 0);
    assert_eq!(stats.timeout_batches, 0);
    assert_eq!(c.window_len(), 0);
}

#[test]
fn deadline_with_two_or_more_forms_timeout_batch() {
    let mut c: Coordinator<u32, u32> = Coordinator::default();
    let mut stats = zero_stats();
    let start = Instant::now();
    assert!(matches!(
        c.submit(1, vec![1, 2, 3], Some(config(8, 20)), 1, start, &mut stats),
        Submitted::Parked
    ));
    let flush = c
        .tick(start + Duration::from_millis(21), &mut stats)
        .expect("截止触发");
    assert!(flush.batched);
    assert_eq!(flush.items.len(), 3);
    assert_eq!(stats.formed_batches, 1);
    assert_eq!(stats.batched_items, 3);
    assert_eq!(stats.timeout_batches, 1);
}

#[test]
fn zero_wait_flushes_immediately_without_forming_single_batches() {
    let mut c: Coordinator<u32, u32> = Coordinator::default();
    let mut stats = zero_stats();
    let now = Instant::now();
    // 单项即入即发：不足 2 项逐项执行。
    let Submitted::Flush(single) = c.submit(1, vec![1], Some(config(4, 0)), 1, now, &mut stats)
    else {
        panic!("max_wait_ms=0 应即发");
    };
    assert!(!single.batched);
    assert_eq!(stats.formed_batches, 0);
    // 同调多项目仍按单一物理批次执行。
    let Submitted::Flush(batch) = c.submit(1, vec![2, 3], Some(config(4, 0)), 1, now, &mut stats)
    else {
        panic!("多项目应即发成批");
    };
    assert!(batch.batched);
    assert_eq!(batch.items, vec![2, 3]);
    assert_eq!(stats.batched_items, 2);
}

#[test]
fn drain_flushes_current_window_state_on_close() {
    let mut c: Coordinator<u32, u32> = Coordinator::default();
    let mut stats = zero_stats();
    assert!(matches!(
        c.submit(
            1,
            vec![1, 2],
            Some(config(4, 5000)),
            1,
            Instant::now(),
            &mut stats
        ),
        Submitted::Parked
    ));
    let flush = c.drain(&mut stats).expect("关闭排空");
    assert!(flush.batched);
    assert_eq!(flush.trigger, Trigger::Drain);
    assert_eq!(stats.formed_batches, 1);
    assert_eq!(stats.timeout_batches, 0);
    assert!(c.drain(&mut stats).is_none());
    assert!(c.next_deadline().is_none());
}

#[test]
fn next_deadline_reports_open_window_for_worker_sleep_planning() {
    let mut c: Coordinator<u32, u32> = Coordinator::default();
    let mut stats = zero_stats();
    let start = Instant::now();
    assert!(c.next_deadline().is_none());
    assert!(matches!(
        c.submit(1, vec![1], Some(config(4, 30)), 1, start, &mut stats),
        Submitted::Parked
    ));
    let deadline = c.next_deadline().expect("开窗后应有截止时间");
    assert!(deadline > start && deadline <= start + Duration::from_millis(30));
    // 排空后不再报告。
    c.drain(&mut stats);
    assert!(c.next_deadline().is_none());
}

#[test]
fn prune_removes_canceled_items_before_any_later_batch() {
    let mut c: Coordinator<u32, u32> = Coordinator::default();
    let mut stats = zero_stats();
    let start = Instant::now();
    assert!(matches!(
        c.submit(1, vec![1, 2, 3], Some(config(4, 30)), 1, start, &mut stats),
        Submitted::Parked
    ));
    // 已取消项（id=2）不进入后续批次：剪除后仅存活动项。
    let removed = c.prune(|id| *id != 2);
    assert_eq!(removed, vec![2]);
    assert_eq!(c.window_len(), 2);
    // 全部取消时窗口关闭，后续提交按新窗口开启。
    let removed = c.prune(|_| false);
    assert_eq!(removed, vec![1, 3]);
    assert_eq!(c.window_len(), 0);
    assert!(c
        .tick(start + Duration::from_millis(31), &mut stats)
        .is_none());
    // 空窗剪除不产生项。
    assert!(c.prune(|_| false).is_empty());
    assert_eq!(stats, zero_stats());
}

#[test]
fn prune_keeps_remaining_items_flushable_by_deadline_tick() {
    // 部分取消后，剩余活动项仍须被窗口截止正常成批（真实DLL发现的组合缺陷回归）。
    let mut c: Coordinator<u32, u32> = Coordinator::default();
    let mut stats = zero_stats();
    let start = Instant::now();
    assert!(matches!(
        c.submit(1, vec![1, 2, 3], Some(config(4, 30)), 1, start, &mut stats),
        Submitted::Parked
    ));
    assert_eq!(c.prune(|id| *id != 2), vec![2]);
    assert_eq!(c.window_len(), 2);
    let flush = c
        .tick(start + Duration::from_millis(31), &mut stats)
        .expect("截止后剩余2项必须成批flush");
    assert!(flush.batched && flush.items == vec![1, 3]);
    assert_eq!(stats.formed_batches, 1);
    assert_eq!(stats.batched_items, 2);
    assert_eq!(stats.timeout_batches, 1);
}
