use super::*;
use am_sdk_support::{context::Context, handles::Domain};
use std::sync::{atomic::Ordering, Arc};

fn scope() -> (Domain, Arc<am_sdk_support::handles::Entry>, State) {
    let domain = Domain::new(false);
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
        records: Default::default(),
        changed: Default::default(),
        usage: Arc::default(),
        models: 0.into(),
        running: 0.into(),
        metrics: Default::default(),
        backends: Default::default(),
    };
    (domain, entry, state)
}

fn record() -> Record {
    Record {
        model: 1,
        slot: 1,
        state: AM_REQ_DONE as u32,
        code: 0,
        created: 0,
        started: 0,
        finished: 0,
        expires: 0,
        result: None,
        canceled: false,
        offline: false,
    }
}

#[test]
fn exact_growth_reserves_overlap_and_keeps_idle_capacity_charged() {
    let (_domain, entry, state) = scope();
    let ctx = entry.data::<Context>().unwrap();
    let unit = size_of::<(u64, Record)>();
    let mut table = Records::default();
    ctx.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_bytes = (unit - 1) as u64;
    assert_eq!(
        table.reserve(&state, 1, 10),
        Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
    );
    assert_eq!(table.rows.capacity(), 0);
    assert_eq!(state.usage.control_bytes.load(Ordering::Acquire), 0);
    ctx.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_bytes = unit as u64;
    table.reserve(&state, 1, 10).unwrap();
    table.insert(1, record());
    assert_eq!(table.rows.capacity(), 1);
    // 最终两槽可放下不代表新旧共三槽的增长峰值可放下。
    ctx.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_bytes = (3 * unit - 1) as u64;
    assert_eq!(
        table.reserve(&state, 1, 10),
        Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
    );
    assert_eq!((table.len(), table.rows.capacity()), (1, 1));
    assert_eq!(state.usage.control_bytes.load(Ordering::Acquire), unit);
    ctx.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_bytes = (3 * unit) as u64;
    table.reserve(&state, 1, 10).unwrap();
    assert_eq!(table.rows.capacity(), 2);
    table.insert(2, record());
    assert_eq!(state.usage.control_bytes.load(Ordering::Acquire), 2 * unit);
    state.purge(&mut table);
    assert!(table.is_empty());
    assert_eq!(table.rows.capacity(), 2);
    assert_eq!(state.usage.control_bytes.load(Ordering::Acquire), 2 * unit);
    assert_eq!(state.usage.snapshots.load(Ordering::Acquire), 0);
    assert_eq!(ctx.buffers.load(Ordering::Acquire), 0);
    println!(
        "record-storage unit={unit} retained={} capacity={}",
        2 * unit,
        table.rows.capacity()
    );
    drop(table);
    assert_eq!(state.usage.control_bytes.load(Ordering::Acquire), 0);
}

#[test]
fn ordering_removal_and_non_power_of_two_record_limit_are_preserved() {
    let (_domain, _entry, state) = scope();
    let mut table = Records::default();
    table.reserve(&state, 3, 3).unwrap();
    assert_eq!(table.rows.capacity(), 3);
    for key in [3, 1, 2] {
        table.insert(key, record());
    }
    assert_eq!(
        table.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert_eq!(table.reserve(&state, 1, 3), Err(AM_ERR_SDK_QUEUE_FULL));
    assert!(table.remove(&2).is_some());
    assert!(table.get(&2).is_none());
    table.get_mut(&3).unwrap().canceled = true;
    assert!(table.get(&3).unwrap().canceled);
    table.reserve(&state, 1, 3).unwrap();
    table.insert(4, record());
    assert_eq!(table.rows.capacity(), 3);
}

#[test]
fn records_and_public_snapshots_share_bytes_but_not_external_lifetimes() {
    let (_domain, entry, state) = scope();
    let ctx = entry.data::<Context>().unwrap();
    let unit = size_of::<(u64, Record)>();
    ctx.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_bytes = (unit + 20) as u64;
    let mut table = Records::default();
    table.reserve(&state, 1, 1).unwrap();
    let snapshot = state.control(20).unwrap();
    assert!(matches!(
        state.control(1),
        Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
    ));
    assert_eq!(state.usage.snapshots.load(Ordering::Acquire), 1);
    assert_eq!(ctx.buffers.load(Ordering::Acquire), 1);
    drop(snapshot);
    assert_eq!(state.usage.control_bytes.load(Ordering::Acquire), unit);
    assert_eq!(state.usage.snapshots.load(Ordering::Acquire), 0);
    assert_eq!(ctx.buffers.load(Ordering::Acquire), 0);
    drop(table);
    assert_eq!(state.usage.control_bytes.load(Ordering::Acquire), 0);
}
