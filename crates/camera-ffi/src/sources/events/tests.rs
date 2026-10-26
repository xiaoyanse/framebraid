use super::*;
use am_sdk_support::context::Context;
use std::mem::size_of;

unsafe extern "C" fn callback(
    _: *mut amc_source_t,
    _: i32,
    _: *const am_event_detail_t,
    _: *mut c_void,
) {
}

#[test]
fn stopped_source_event_queue_releases_backing_capacity() {
    let (_domain, budget) = crate::frames::isolated_test_state();
    let events = Events::new(budget.clone());
    {
        let mut state = events.state.lock().unwrap();
        state.prepare(&budget).unwrap();
        state.callback = Some(callback);
    }
    events.push(1, 0, 0, 0, 1, c"fixture");
    assert!(events.state.lock().unwrap().queue.capacity() > 0);
    events.stop();
    assert_eq!(events.state.lock().unwrap().queue.capacity(), 0);
    assert_eq!(budget.control_usage_snapshot().unwrap(), (0, 0));
}

#[test]
fn tiny_budget_rejects_before_allocation_and_latest_event_replaces_only_pending() {
    let (_domain, budget) = crate::frames::isolated_test_state();
    let ctx = budget.ctx.data::<Context>().unwrap();
    let bytes = size_of::<Event>();
    let events = Events::new(budget.clone());
    {
        let mut state = events.state.lock().unwrap();
        ctx.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_bytes = bytes as u64 - 1;
        assert!(matches!(
            state.prepare(&budget),
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
        ));
        assert_eq!(state.queue.capacity(), 0);
        assert_eq!(budget.control_usage_snapshot().unwrap(), (0, 0));
        // 只变更隔离测试Context，不提高公开set_limits的额度。
        ctx.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_bytes = bytes as u64;
        state.prepare(&budget).unwrap();
        state.callback = Some(callback);
    }
    for index in 1..=100 {
        events.push(index, 0, 0, 0, index as u32, c"fixture");
        let state = events.state.lock().unwrap();
        assert_eq!(state.queue.capacity(), 1);
        assert_eq!(state.queue.len(), 1);
        assert_eq!(state.queue.front().unwrap().kind, index);
        assert_eq!(budget.control_usage_snapshot().unwrap(), (2, bytes));
    }
    assert_eq!(budget.metrics.dropped.load(Ordering::Acquire), 99);
    events.stop();
    assert_eq!(budget.control_usage_snapshot().unwrap(), (0, 0));
    println!("source-event-storage {{\"entry_bytes\":{bytes},\"events\":100,\"dropped\":99,\"capacity\":1,\"after_stop_bytes\":0}}");
}

#[test]
fn busy_event_is_not_replaced_and_storage_survives_until_its_return() {
    let (_domain, budget) = crate::frames::isolated_test_state();
    let bytes = 2 * size_of::<Event>();
    budget.ctx.data::<Context>().unwrap().queues.lock().unwrap()[AM_Q_CONTROL as usize].max_bytes =
        bytes as u64;
    let events = Events::new(budget.clone());
    {
        let mut state = events.state.lock().unwrap();
        state.prepare(&budget).unwrap();
        state.callback = Some(callback);
    }
    events.push(1, 0, 0, 0, 1, c"busy");
    let executing = {
        let mut state = events.state.lock().unwrap();
        let value = state.queue.pop_front().unwrap();
        state.busy = true;
        value
    };
    events.push(2, 0, 0, 0, 2, c"pending");
    events.push(3, 0, 0, 0, 3, c"latest");
    assert_eq!(executing.kind, 1);
    assert_eq!(events.state.lock().unwrap().queue.front().unwrap().kind, 3);
    assert_eq!(budget.metrics.dropped.load(Ordering::Acquire), 1);
    events.stop();
    assert_eq!(budget.control_usage_snapshot().unwrap(), (2, bytes));
    drop(executing);
    let mut state = events.state.lock().unwrap();
    state.busy = false;
    state.release_storage_if_idle();
    assert_eq!(state.queue.capacity(), 0);
    assert_eq!(budget.control_usage_snapshot().unwrap(), (0, 0));
}

#[test]
fn failed_registration_starts_no_thread_and_repeated_registration_reuses_one_worker() {
    let (_domain, budget) = crate::frames::isolated_test_state();
    let ctx = budget.ctx.data::<Context>().unwrap();
    ctx.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_items = 1;
    let events = Arc::new(Events::new(budget.clone()));
    assert!(matches!(
        events.set(Weak::new(), Some(callback), 0),
        Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
    ));
    assert!(events.worker.lock().unwrap().is_none());
    assert!(events.stopped());
    assert_eq!(budget.control_usage_snapshot().unwrap(), (0, 0));
    ctx.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_items = 3;
    let mut worker_id = None;
    for _ in 0..100 {
        events.set(Weak::new(), Some(callback), 0).unwrap();
        let id = events
            .worker
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .thread()
            .id();
        assert_eq!(*worker_id.get_or_insert(id), id);
        events.set(Weak::new(), None, 0).unwrap();
        assert_eq!(events.state.lock().unwrap().queue.capacity(), 0);
        assert_eq!(budget.control_usage_snapshot().unwrap(), (0, 0));
    }
    events.stop();
    events.join();
    assert!(events.stopped());
    assert!(events.worker.lock().unwrap().is_none());
}

#[test]
fn one_busy_slot_drops_new_hints_without_reclaiming_the_active_event() {
    let (_domain, budget) = crate::frames::isolated_test_state();
    budget.ctx.data::<Context>().unwrap().queues.lock().unwrap()[AM_Q_CONTROL as usize].max_bytes =
        size_of::<Event>() as u64;
    let events = Events::new(budget.clone());
    let executing = {
        let mut state = events.state.lock().unwrap();
        state.prepare(&budget).unwrap();
        state.callback = Some(callback);
        drop(state);
        events.push(1, 0, 0, 0, 1, c"busy");
        let mut state = events.state.lock().unwrap();
        state.busy = true;
        state.queue.pop_front().unwrap()
    };
    events.push(2, 0, 0, 0, 2, c"drop");
    assert_eq!(executing.kind, 1);
    assert!(events.state.lock().unwrap().queue.is_empty());
    assert_eq!(budget.metrics.dropped.load(Ordering::Acquire), 1);
    drop(executing);
    let mut state = events.state.lock().unwrap();
    state.busy = false;
    state.release_storage_if_idle();
    assert_eq!(budget.control_usage_snapshot().unwrap(), (0, 0));
}
