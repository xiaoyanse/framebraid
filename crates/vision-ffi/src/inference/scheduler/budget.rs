//! 分类、本Context及共享执行器三层准入，真实归还输入后一起出账。
use super::*;

pub(super) const EXECUTOR_BYTES: usize = 64 * 1024 * 1024;

pub(super) fn reserve(
    engine: Arc<Engine>,
    usage: Arc<super::super::state::Usage>,
    class: usize,
    count: usize,
    bytes: usize,
    limits: &am_limits_t,
    policy: &am_queue_policy_t,
) -> Result<Admission> {
    let mut data = lock(&engine.data)?;
    // 与Admission析构同序；不在锁内读取调用方平面或执行拥有者回调。
    let mut used = lock(&usage.preprocess)?;
    let next_class = used[class].checked_add(bytes);
    let next_context = next_class.and_then(|n| n.checked_add(used[1 - class]));
    let next_total = data
        .bytes
        .checked_add(bytes)
        .and_then(|value| value.checked_add(data.storage_bytes));
    let next_items = data.inflight.checked_add(count);
    if count > policy.max_items as usize
        || next_class.is_none_or(|n| n as u64 > policy.max_bytes)
        || next_context.is_none_or(|n| n as u64 > limits.preprocess_budget_bytes)
        || next_total.is_none_or(|n| n > EXECUTOR_BYTES)
        || next_items.is_none_or(|n| n > engine.budget.slots as usize)
    {
        return Err(AM_ERR_SDK_QUEUE_FULL);
    }
    used[class] += bytes;
    data.bytes += bytes;
    data.inflight += count;
    drop(used);
    drop(data);
    Ok(Admission {
        engine,
        usage,
        class,
        bytes,
        items: count,
    })
}

#[cfg(test)]
mod tests {
    use super::super::super::state::Usage;
    use super::*;

    fn engine() -> Arc<Engine> {
        Arc::new(Engine {
            data: Mutex::new(Data {
                queue: VecDeque::new(),
                // 分类账定向测试不分配执行器容器；真实存储另由storage_tests覆盖。
                storage_bytes: 0,
                retirement_pending: false,
                contexts: Vec::new(),
                inflight: 0,
                bytes: 0,
                extra_threads: 0,
                last: None,
                waiters: 0,
                models: 0,
            }),
            wake: Condvar::new(),
            budget: ThreadBudget {
                parallel: 2,
                total: 4,
                slots: 16,
            },
            stop: AtomicBool::new(false),
            threads: Mutex::default(),
        })
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
    fn snapshot(e: &Engine, u: &Usage) -> (usize, usize, [usize; 2]) {
        let d = e.data.lock().unwrap();
        (d.inflight, d.bytes, *u.preprocess.lock().unwrap())
    }

    #[test]
    fn classes_and_contexts_do_not_apply_a_small_budget_to_other_owners() {
        let e = engine();
        let a = Arc::new(Usage::default());
        let b = Arc::new(Usage::default());
        let x = reserve(
            e.clone(),
            a.clone(),
            0,
            1,
            4096,
            &limits(8192),
            &policy(8192),
        )
        .unwrap();
        let y = reserve(e.clone(), a.clone(), 1, 1, 64, &limits(8192), &policy(64)).unwrap();
        let z = reserve(e.clone(), b.clone(), 0, 1, 64, &limits(64), &policy(64)).unwrap();
        assert_eq!(snapshot(&e, &a), (3, 4224, [4096, 64]));
        assert_eq!(snapshot(&e, &b).2, [64, 0]);
        drop((x, y, z));
        assert_eq!(snapshot(&e, &a), (0, 0, [0, 0]));
        assert_eq!(snapshot(&e, &b).2, [0, 0]);
    }

    #[test]
    fn class_context_and_global_limits_reject_without_partial_charges() {
        let e = engine();
        let a = Arc::new(Usage::default());
        let held = reserve(e.clone(), a.clone(), 0, 1, 300, &limits(512), &policy(400)).unwrap();
        for (class, bytes, cap) in [(0, 101, 400), (1, 213, 400), (1, usize::MAX, u64::MAX)] {
            assert!(matches!(
                reserve(
                    e.clone(),
                    a.clone(),
                    class,
                    1,
                    bytes,
                    &limits(512),
                    &policy(cap)
                ),
                Err(AM_ERR_SDK_QUEUE_FULL)
            ));
            assert_eq!(snapshot(&e, &a), (1, 300, [300, 0]));
        }
        drop(held);
        let large = reserve(
            e.clone(),
            a.clone(),
            0,
            1,
            EXECUTOR_BYTES,
            &limits(EXECUTOR_BYTES as u64),
            &policy(EXECUTOR_BYTES as u64),
        )
        .unwrap();
        let b = Arc::new(Usage::default());
        assert!(reserve(e.clone(), b.clone(), 1, 1, 1, &limits(1), &policy(1)).is_err());
        assert_eq!(snapshot(&e, &b), (1, EXECUTOR_BYTES, [0, 0]));
        drop(large);
        let recovered = reserve(e.clone(), b.clone(), 1, 1, 1, &limits(1), &policy(1)).unwrap();
        drop(recovered);
        assert_eq!(snapshot(&e, &b), (0, 0, [0, 0]));
    }

    #[test]
    fn metadata_and_inputs_share_class_budget_until_both_are_released() {
        let e = engine();
        let u = Arc::new(Usage::default());
        let metadata =
            reserve(e.clone(), u.clone(), 1, 0, 300, &limits(512), &policy(512)).unwrap();
        let input = reserve(e.clone(), u.clone(), 1, 1, 212, &limits(512), &policy(512)).unwrap();
        assert!(reserve(e.clone(), u.clone(), 1, 0, 1, &limits(512), &policy(512)).is_err());
        drop(metadata);
        assert_eq!(snapshot(&e, &u), (1, 212, [0, 212]));
        assert!(reserve(e.clone(), u.clone(), 1, 1, 1, &limits(512), &policy(128)).is_err());
        drop(input);
        assert_eq!(snapshot(&e, &u), (0, 0, [0, 0]));
    }

    #[test]
    fn concurrent_admissions_cannot_exceed_shared_slots_or_context_bytes() {
        for (bytes, accepted) in [(512, 8), (4096, 16)] {
            let e = engine();
            let u = Arc::new(Usage::default());
            let barrier = Arc::new(std::sync::Barrier::new(32));
            let mut callers = Vec::new();
            for i in 0..32 {
                let (e, u, barrier) = (e.clone(), u.clone(), barrier.clone());
                callers.push(std::thread::spawn(move || {
                    let result = reserve(e, u, i % 2, 1, 64, &limits(bytes), &policy(bytes));
                    barrier.wait();
                    result
                }));
            }
            let held = callers
                .into_iter()
                .filter_map(|t| t.join().unwrap().ok())
                .collect::<Vec<_>>();
            assert_eq!(held.len(), accepted);
            assert_eq!(snapshot(&e, &u).0, accepted);
            assert_eq!(snapshot(&e, &u).1, accepted * 64);
            drop(held);
            assert_eq!(snapshot(&e, &u), (0, 0, [0, 0]));
        }
    }
}
