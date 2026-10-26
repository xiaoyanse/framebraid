use super::*;

#[test]
fn continuous_history_and_reference_slots_fit_the_admitted_storage() {
    let mut queue = Queue::new(3).unwrap();
    for received in 0..10_000 {
        queue.remember(received);
    }
    let required = size_of::<Subscription>()
        + queue.items.capacity() * size_of::<(u64, Arc<images::Image>)>()
        + queue.received.capacity() * size_of::<i64>();
    println!(
        "capture-storage items={} history={} allocated={required} reserved={}",
        queue.items.capacity(),
        queue.received.capacity(),
        metadata_bytes(3)
    );
    assert_eq!(queue.received.len(), 120);
    assert_eq!(queue.received.front(), Some(&9880));
    assert_eq!(queue.received.back(), Some(&9999));
    assert!(required <= metadata_bytes(3));
    assert_eq!(queue.items.capacity(), 3);
    queue.remember(20_000);
    assert_eq!(
        queue.received.iter().copied().collect::<Vec<_>>(),
        vec![20_000]
    );
}

#[test]
fn continuous_metadata_keeps_idle_capacity_charged_until_storage_is_dropped() {
    let (_domain, camera) = crate::frames::isolated_test_state();
    let context = camera
        .ctx
        .data::<am_sdk_support::context::Context>()
        .unwrap();
    let bytes = metadata_bytes(3);
    context.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_bytes = bytes as u64 - 1;
    assert!(matches!(
        camera.control(bytes, false),
        Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
    ));
    assert_eq!(camera.control_usage_snapshot().unwrap(), (0, 0));
    context.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_bytes = bytes as u64;
    let charge = camera.control(bytes, false).unwrap();
    let mut queue = Queue::new(3).unwrap();
    for i in 0..10_000 {
        queue.remember(i);
    }
    queue.received.clear();
    assert_eq!(queue.received.capacity(), 120);
    assert_eq!(camera.control_usage_snapshot().unwrap(), (1, bytes));
    drop(queue);
    drop(charge);
    assert_eq!(camera.control_usage_snapshot().unwrap(), (0, 0));
}

#[test]
fn failed_consumer_registration_returns_its_capacity() {
    let (_domain, camera) = crate::frames::isolated_test_state();
    let state = state::State::get(&camera).unwrap();
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = state.consumers.lock().unwrap();
        panic!("injected registration poison");
    }));
    assert_eq!(
        reserve_consumer(&state, 1, 1000, 1),
        Err(AM_ERR_SDK_INTERNAL_ERROR)
    );
    assert_eq!(state.continuous.load(Ordering::Acquire), 0);
    assert!(state.subscriptions.lock().unwrap().is_empty());
}
