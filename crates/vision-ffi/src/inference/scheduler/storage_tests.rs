use super::*;

#[test]
fn non_power_of_two_job_slots_have_exact_backing_capacity() {
    let _guard = TEST_ENGINE_LOCK.lock().unwrap();
    let budget = ThreadBudget {
        parallel: 2,
        total: 4,
        slots: 3,
    };
    let permit = reserve_model(budget, 0).unwrap();
    let engine = permit.engine.clone();
    let domain = am_sdk_support::handles::Domain::new(true);
    let params = amv_init_params_t {
        abi_version: AM_ABI_VERSION,
        ..Default::default()
    };
    let key = unsafe { am_sdk_support::context::initialize_vision(&domain, &params) }.unwrap();
    let object = domain.get(key).unwrap();
    let usage = Arc::new(super::super::state::Usage::default());
    let (capacity, jobs) = {
        let mut data = engine.data.lock().unwrap();
        for index in 0..3 {
            data.queue.push_back(Job {
                model: object.clone(),
                input: Payload::Native {
                    inputs: Vec::new(),
                    ids: Vec::new(),
                },
                id: index,
                params: Default::default(),
                parallel: 1,
                _admissions: vec![Admission {
                    engine: engine.clone(),
                    usage: usage.clone(),
                    class: 0,
                    bytes: 0,
                    items: 0,
                }],
            });
        }
        let capacity = data.queue.capacity();
        let jobs = data.queue.drain(..).collect::<Vec<_>>();
        (capacity, jobs)
    };
    drop(jobs);
    drop(permit);
    println!(
        "executor-queue slots=3 capacity={capacity} job_bytes={}",
        std::mem::size_of::<Job>()
    );
    assert_eq!(capacity, 3);
    assert!(engine.threads.lock().unwrap().is_empty());
}

fn state() -> Arc<State> {
    Arc::new(State {
        ctx: Weak::new(),
        prefix: 0,
        next: Default::default(),
        records: Mutex::default(),
        changed: Condvar::new(),
        usage: Arc::default(),
        models: Default::default(),
        running: Default::default(),
        metrics: Default::default(),
        backends: Mutex::default(),
    })
}

fn isolated_engine() -> Arc<Engine> {
    let budget = ThreadBudget {
        parallel: 2,
        total: 4,
        slots: 3,
    };
    Arc::new(Engine {
        data: Mutex::new(Data::new(budget).unwrap()),
        wake: Condvar::new(),
        budget,
        stop: AtomicBool::new(false),
        threads: Mutex::new(storage::thread_slots(2).unwrap()),
    })
}

#[test]
fn context_growth_retains_overlap_budget_and_idle_capacity() {
    let mut data = Data::new(ThreadBudget {
        parallel: 2,
        total: 4,
        slots: 3,
    })
    .unwrap();
    let base = data.storage_bytes;
    let mut states = (0..4).map(|_| state()).collect::<Vec<_>>();
    for value in &states {
        data.register_context(value).unwrap();
    }
    assert_eq!(data.contexts.capacity(), 4);
    assert_eq!(
        data.storage_bytes,
        base + 4 * std::mem::size_of::<Weak<State>>()
    );
    let next = state();
    let previous = data.storage_bytes;
    data.bytes = budget::EXECUTOR_BYTES - previous - 8 * std::mem::size_of::<Weak<State>>() + 1;
    assert_eq!(
        data.register_context(&next),
        Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
    );
    assert_eq!(
        (
            data.contexts.len(),
            data.contexts.capacity(),
            data.storage_bytes
        ),
        (4, 4, previous)
    );
    data.bytes = 0;
    data.register_context(&next).unwrap();
    assert_eq!(data.contexts.capacity(), 8);
    let retained = data.storage_bytes;
    states.clear();
    data.register_context(&next).unwrap();
    assert_eq!(
        (
            data.contexts.len(),
            data.contexts.capacity(),
            data.storage_bytes
        ),
        (1, 8, retained)
    );
    println!(
        "executor-storage base={base} context_capacity={} storage_bytes={retained}",
        data.contexts.capacity()
    );
}

#[test]
fn executor_storage_and_inflight_payload_share_the_existing_ceiling() {
    let engine = isolated_engine();
    let metadata = engine.data.lock().unwrap().storage_bytes;
    let usage = Arc::new(super::super::state::Usage::default());
    let limits = am_limits_t {
        preprocess_budget_bytes: budget::EXECUTOR_BYTES as u64,
        ..Default::default()
    };
    let policy = am_queue_policy_t {
        max_items: 3,
        max_bytes: budget::EXECUTOR_BYTES as u64,
        ..Default::default()
    };
    let held = budget::reserve(
        engine.clone(),
        usage.clone(),
        0,
        1,
        budget::EXECUTOR_BYTES - metadata,
        &limits,
        &policy,
    )
    .unwrap();
    assert!(matches!(
        budget::reserve(engine.clone(), usage, 1, 1, 1, &limits, &policy),
        Err(AM_ERR_SDK_QUEUE_FULL)
    ));
    drop(held);
    let data = engine.data.lock().unwrap();
    assert_eq!(
        (data.inflight, data.bytes, data.storage_bytes),
        (0, 0, metadata)
    );
}

#[test]
fn one_busy_record_table_does_not_block_cleanup_of_another_context() {
    let engine = isolated_engine();
    let first = state();
    let second = state();
    *second.records.lock().unwrap() = super::super::records::Records::isolated([(
        1,
        Record {
            model: 0,
            slot: 0,
            state: AM_REQ_DONE as u32,
            code: 0,
            created: 0,
            started: 0,
            finished: 0,
            expires: 0,
            result: None,
            canceled: false,
            offline: false,
        },
    )]);
    {
        let mut data = engine.data.lock().unwrap();
        data.register_context(&first).unwrap();
        data.register_context(&second).unwrap();
    }
    let guard = first.records.lock().unwrap();
    let returned = std::thread::scope(|scope| {
        let (sent, received) = std::sync::mpsc::sync_channel(1);
        let worker = scope.spawn(move || {
            storage::purge_contexts(&engine);
            sent.send(()).unwrap();
        });
        let returned = received
            .recv_timeout(std::time::Duration::from_secs(2))
            .is_ok();
        drop(guard);
        worker.join().unwrap();
        returned
    });
    assert!(returned);
    assert!(second.records.lock().unwrap().is_empty());
}
