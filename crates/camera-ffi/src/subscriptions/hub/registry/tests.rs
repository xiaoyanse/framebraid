use super::*;
use crate::frames::registry_tests::allocations;
use am_sdk_support::handles::Managed;

#[test]
fn subscriber_registry_and_snapshots_have_fixed_storage_and_no_heap_allocations() {
    let (domain, source, hub, _) = super::super::tests::fixture();
    let context = source
        .state
        .ctx
        .data::<am_sdk_support::context::Context>()
        .unwrap();
    context.limits.lock().unwrap().control_max_items = CAPACITY as u32;
    context.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_items = CAPACITY as u32;
    let mut owners = Vec::new();
    for _ in 0..CAPACITY {
        owners.push(super::super::tests::sub(&source, &hub, 5, AM_SUB_PULL));
    }
    let (registry, initialize_allocations) = allocations(Subscribers::default);
    assert_eq!(initialize_allocations, 0);
    *hub.subscribers.lock().unwrap() = registry;
    let (_, registration_allocations) = allocations(|| {
        let mut registry = hub.subscribers.lock().unwrap();
        for sub in &owners {
            registry.push(Arc::downgrade(sub)).unwrap();
        }
    });
    assert_eq!(registration_allocations, 0);
    assert!(hub.subscribers.lock().unwrap().is_full());
    assert_eq!(
        hub.subscribers.lock().unwrap().push(Weak::new()),
        Err(AM_ERR_CAMERA_SUBSCRIBER_LIMIT_REACHED)
    );
    let (old, baseline_allocations) = allocations(|| {
        let registry = hub.subscribers.lock().unwrap();
        registry
            .iter()
            .filter_map(Weak::upgrade)
            .filter(|sub| !sub.stopped())
            .collect::<Vec<_>>()
    });
    assert_eq!(old.len(), CAPACITY);
    assert!(baseline_allocations > 0);
    drop(old);
    let (snapshot, snapshot_allocations) = allocations(|| hub.list());
    assert_eq!(snapshot_allocations, 0);
    assert_eq!(snapshot.len(), CAPACITY);
    assert!(hub.subscribers.try_lock().is_ok());
    for (expected, actual) in owners.iter().zip(snapshot.iter()) {
        assert!(Arc::ptr_eq(expected, actual));
        assert_eq!(Arc::strong_count(expected), 2);
    }
    let (_, repeated_allocations) = allocations(|| {
        for _ in 0..1_000 {
            let current = hub.list();
            assert_eq!(current.len(), CAPACITY);
            std::hint::black_box(&current);
        }
    });
    assert_eq!(repeated_allocations, 0);
    println!("r3-hub-registry {{\"slots\":{CAPACITY},\"registry_bytes\":{},\"snapshot_bytes\":{},\"initialize_allocations\":{initialize_allocations},\"register_64_allocations\":{registration_allocations},\"old_collect_allocations\":{baseline_allocations},\"new_snapshot_allocations\":{snapshot_allocations},\"repeated_1000_allocations\":{repeated_allocations}}}", std::mem::size_of::<Subscribers>(), std::mem::size_of::<Snapshot>());
    let weak = Arc::downgrade(&owners[0]);
    drop(owners);
    assert!(weak.upgrade().is_some());
    drop(snapshot);
    assert!(weak.upgrade().is_none());
    assert!(hub.list().is_empty());
    assert_eq!(hub.subscribers.lock().unwrap().len(), 0);
    assert_eq!(source.state.control_usage_snapshot().unwrap(), (0, 0));
    let key = source.state.ctx_key;
    drop((source, hub));
    domain.close(key, 0).unwrap();
    domain.destroy(key).unwrap();
}

#[test]
fn snapshot_prunes_dead_and_drained_slots_and_resets_outside_registry_lock() {
    let (domain, source, hub, _) = super::super::tests::fixture();
    let first = super::super::tests::sub(&source, &hub, 5, AM_SUB_PULL);
    let closed = super::super::tests::sub(&source, &hub, 5, AM_SUB_PULL);
    let dead = super::super::tests::sub(&source, &hub, 5, AM_SUB_PULL);
    {
        let mut registry = hub.subscribers.lock().unwrap();
        for sub in [&first, &closed, &dead] {
            registry.push(Arc::downgrade(sub)).unwrap();
        }
    }
    closed.request_close();
    drop(dead);
    let snapshot = hub.list();
    assert_eq!(snapshot.len(), 1);
    assert!(Arc::ptr_eq(&first, snapshot.iter().next().unwrap()));
    assert_eq!(hub.subscribers.lock().unwrap().len(), 1);
    assert!(first.due(hub.now()));
    assert!(!first.due(hub.now()));
    hub.reset_subscribers();
    assert!(first.due(hub.now()));
    drop((snapshot, first, closed));
    assert!(hub.list().is_empty());
    let key = source.state.ctx_key;
    drop((source, hub));
    domain.close(key, 0).unwrap();
    domain.destroy(key).unwrap();
}
