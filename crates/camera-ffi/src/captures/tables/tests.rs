use super::*;
use am_sdk_support::context::Context;

#[test]
fn actual_capacity_growth_overlap_and_idle_storage_are_charged() {
    let (_domain, camera) = crate::frames::isolated_test_state();
    let ctx = camera.ctx.data::<Context>().unwrap();
    let mut table = Table::<u64, u64>::default();
    let unit = size_of::<(u64, u64)>();
    ctx.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_bytes = (unit - 1) as u64;
    assert_eq!(
        table.try_insert(&camera, 1, 10, 4),
        Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
    );
    assert_eq!((table.len(), table.rows.capacity()), (0, 0));
    assert_eq!(camera.control_usage_snapshot().unwrap(), (0, 0));
    ctx.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_bytes = unit as u64;
    table.try_insert(&camera, 1, 10, 4).unwrap();
    assert_eq!(table.rows.capacity(), 1);
    ctx.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_bytes = (3 * unit - 1) as u64;
    assert_eq!(
        table.try_insert(&camera, 2, 20, 4),
        Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
    );
    assert_eq!((table.len(), table.rows.capacity()), (1, 1));
    assert_eq!(table.get(&1), Some(&10));
    ctx.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_bytes = (3 * unit) as u64;
    table.try_insert(&camera, 2, 20, 4).unwrap();
    assert_eq!(camera.control_usage_snapshot().unwrap(), (0, 2 * unit));
    table.clear();
    assert_eq!(table.rows.capacity(), 2);
    assert_eq!(camera.control_usage_snapshot().unwrap(), (0, 2 * unit));
    assert_eq!(ctx.buffers.load(std::sync::atomic::Ordering::Acquire), 0);
    drop(table);
    assert_eq!(camera.control_usage_snapshot().unwrap(), (0, 0));
}

#[test]
fn bounded_non_power_capacity_and_ordered_reuse_preserve_values() {
    let (_domain, camera) = crate::frames::isolated_test_state();
    let mut table = Table::<u64, u64>::default();
    table.reserve(&camera, 3, 3).unwrap();
    assert_eq!(table.rows.capacity(), 3);
    for id in [3, 1, 2] {
        table.insert(id, id * 10);
    }
    assert_eq!(table.iter().map(|(k, _)| *k).collect::<Vec<_>>(), [1, 2, 3]);
    assert_eq!(
        table.try_insert(&camera, 4, 40, 3),
        Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
    );
    assert_eq!(table.remove(&2), Some(20));
    table.try_insert(&camera, 4, 40, 3).unwrap();
    table.try_insert(&camera, 3, 300, 3).unwrap();
    assert_eq!(table[&3], 300);
    assert_eq!(table.rows.capacity(), 3);
    table.retain(|key, _| *key > 1);
    assert_eq!(table.values().copied().collect::<Vec<_>>(), [300, 40]);
}

#[test]
fn record_tokens_are_separate_from_storage_and_snapshots_still_keep_context_open() {
    let (_domain, camera) = crate::frames::isolated_test_state();
    let ctx = camera.ctx.data::<Context>().unwrap();
    let mut table = Table::<u64, u64>::default();
    table.reserve(&camera, 1, 1).unwrap();
    let stored = size_of::<(u64, u64)>();
    ctx.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_bytes = (stored + 8) as u64;
    let token = camera.control(0, false).unwrap();
    let snapshot = camera.control(8, true).unwrap();
    assert_eq!(camera.control_usage_snapshot().unwrap(), (2, stored + 8));
    assert!(matches!(
        camera.control(1, false),
        Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
    ));
    assert_eq!(ctx.buffers.load(std::sync::atomic::Ordering::Acquire), 1);
    drop(snapshot);
    drop(token);
    assert_eq!(ctx.buffers.load(std::sync::atomic::Ordering::Acquire), 0);
    assert_eq!(camera.control_usage_snapshot().unwrap(), (0, stored));
    println!(
        "capture-table entry={stored} record_slot={} flight_body={}",
        size_of::<(u64, super::super::state::Record)>(),
        size_of::<super::super::io::Flight>()
    );
}
