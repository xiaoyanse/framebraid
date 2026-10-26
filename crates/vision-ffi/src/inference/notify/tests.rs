use super::*;

fn scope() -> (am_sdk_support::handles::Domain, Arc<Entry>, State) {
    let domain = am_sdk_support::handles::Domain::new(false);
    let params = amv_init_params_t {
        abi_version: AM_ABI_VERSION,
        ..Default::default()
    };
    let key = unsafe { am_sdk_support::context::initialize_vision(&domain, &params) }.unwrap();
    let entry = domain.get(key).unwrap();
    let state = State {
        ctx: Arc::downgrade(&entry),
        prefix: 0,
        next: 1.into(),
        records: Mutex::default(),
        changed: Condvar::new(),
        usage: Arc::default(),
        models: 0.into(),
        running: 0.into(),
        metrics: Default::default(),
        backends: Mutex::default(),
    };
    (domain, entry, state)
}

#[test]
fn queue_storage_is_reserved_before_allocation_and_not_double_charged() {
    let (_domain, entry, state) = scope();
    let ctx = entry.data::<Context>().unwrap();
    let bytes = size_of::<Notification>();
    let notify = Notify::default();
    let mut data = notify.data.lock().unwrap();
    ctx.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_bytes = bytes as u64 - 1;
    assert!(matches!(
        data.prepare(&state),
        Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
    ));
    assert_eq!(data.queue.capacity(), 0);
    assert_eq!(state.usage.control_bytes.load(Ordering::Acquire), 0);
    // 仅调整本测试私有Context，不提高公开set_limits已降低的上限。
    ctx.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_bytes = bytes as u64;
    data.prepare(&state).unwrap();
    assert_eq!(data.queue.capacity(), 1);
    assert_eq!(state.usage.control_bytes.load(Ordering::Acquire), bytes);
    for id in 1..=100 {
        data.push(&state, id, 2).unwrap();
        assert_eq!(state.usage.snapshots.load(Ordering::Acquire), 2);
        assert_eq!(ctx.buffers.load(Ordering::Acquire), 1);
        assert_eq!(state.usage.control_bytes.load(Ordering::Acquire), bytes);
        assert!(matches!(
            data.push(&state, 999, 2),
            Err(AM_ERR_SDK_QUEUE_FULL)
        ));
        drop(data.queue.pop_front());
        assert_eq!(state.usage.control_bytes.load(Ordering::Acquire), bytes);
        assert_eq!(data.queue.capacity(), 1);
    }
    assert_eq!(ctx.buffers.load(Ordering::Acquire), 0);
    drop(data);
    notify.stop();
    assert_eq!(notify.data.lock().unwrap().queue.capacity(), 0);
    assert_eq!(state.usage.control_bytes.load(Ordering::Acquire), 0);
    assert_eq!(state.usage.snapshots.load(Ordering::Acquire), 0);
    println!("notification-storage {{\"entry_bytes\":{bytes},\"fixed_capacity\":1,\"cycles\":100,\"after_stop_bytes\":0}}");
}

#[test]
fn stop_keeps_storage_and_close_barrier_until_busy_callback_returns() {
    let (_domain, entry, state) = scope();
    let ctx = entry.data::<Context>().unwrap();
    let bytes = 2 * size_of::<Notification>();
    ctx.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_bytes = bytes as u64;
    let notify = Notify::default();
    let mut data = notify.data.lock().unwrap();
    data.prepare(&state).unwrap();
    data.push(&state, 1, 2).unwrap();
    let executing = data.queue.pop_front().unwrap();
    data.busy = true;
    data.push(&state, 2, 2).unwrap();
    assert!(data.push(&state, 3, 2).is_err());
    drop(data);
    notify.stop();
    assert_eq!(state.usage.control_bytes.load(Ordering::Acquire), bytes);
    assert_eq!(ctx.buffers.load(Ordering::Acquire), 1);
    assert_eq!(state.usage.snapshots.load(Ordering::Acquire), 2);
    drop(executing);
    let mut data = notify.data.lock().unwrap();
    data.busy = false;
    data.release_storage_if_idle();
    assert_eq!(data.queue.capacity(), 0);
    assert_eq!(state.usage.control_bytes.load(Ordering::Acquire), 0);
    assert_eq!(ctx.buffers.load(Ordering::Acquire), 0);
}

#[test]
fn repeated_registration_releases_capacity_and_one_item_cannot_enable_callback() {
    let (_domain, entry, state) = scope();
    let ctx = entry.data::<Context>().unwrap();
    ctx.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_items = 1;
    let mut data = Data::default();
    assert!(data.prepare(&state).is_err());
    assert_eq!(state.usage.snapshots.load(Ordering::Acquire), 0);
    ctx.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_items = 3;
    for _ in 0..100 {
        data.prepare(&state).unwrap();
        assert_eq!(data.queue.capacity(), 2);
        data.push(&state, 1, 2).unwrap();
        data.queue.clear();
        data.release_storage_if_idle();
        assert_eq!(data.queue.capacity(), 0);
        assert_eq!(state.usage.snapshots.load(Ordering::Acquire), 0);
        assert_eq!(state.usage.control_bytes.load(Ordering::Acquire), 0);
    }
}
