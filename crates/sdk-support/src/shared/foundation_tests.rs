use super::*;
use crate::{context, handles::Domain, shared::api};

fn fixture() -> (Domain, usize, usize, usize) {
    let domain = Domain::with_shared(false, true);
    let params = amc_init_params_t {
        abi_version: AM_ABI_VERSION,
        feature_flags: 1,
        ..Default::default()
    };
    let ctx = unsafe { context::initialize_camera(&domain, &params) }.unwrap();
    let config = am_shared_pool_config_t {
        slot_count: 1,
        max_frame_bytes: 64,
        total_bytes: 64,
        max_consumers: 8,
        max_outstanding_borrows: 32,
        ..Default::default()
    };
    let mut attach = am_shared_pool_attach_desc_t::default();
    let publisher = unsafe { api::create(&domain, ctx, &config, &mut attach) }.unwrap();
    let consumer = unsafe { api::attach(&domain, ctx, &attach) }.unwrap();
    let entry = domain.get(publisher).unwrap();
    let desc = am_shared_frame_desc_t {
        protocol_version: 1,
        pixel_format: AM_PF_BGR8 as u32,
        width: 4,
        height: 4,
        plane_count: 1,
        stride_bytes: [12, 0, 0, 0],
        plane_valid_bytes: [48, 0, 0, 0],
        assoc_valid: AM_ASSOC_SOURCE | AM_ASSOC_FRAME | AM_ASSOC_GENERATION,
        source_id: 7,
        frame_id: 900,
        source_generation: 2,
        pts: i64::MIN,
        captured_at_ms: i64::MIN,
        received_at_ms: i64::MIN,
        ..Default::default()
    };
    let pixels = [23u8; 48];
    entry
        .data::<Pool>()
        .unwrap()
        .publish(
            desc,
            [
                pixels.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
            ],
        )
        .unwrap();
    drop(entry);
    (domain, ctx, publisher, consumer)
}
fn finish(domain: Domain, ctx: usize, publisher: usize, consumer: usize) {
    assert_eq!(
        api::close(&domain, consumer, 10, false).unwrap(),
        AM_LIFECYCLE_CLOSED as u32
    );
    domain.destroy(consumer).unwrap();
    assert_eq!(
        api::close(&domain, publisher, 10, true).unwrap(),
        AM_LIFECYCLE_CLOSED as u32
    );
    domain.destroy(publisher).unwrap();
    domain.close(ctx, 10).unwrap();
    domain.destroy(ctx).unwrap();
}
#[test]
fn fixed_borrow_index_full_return_reborrow_and_retained_owner() {
    let (domain, ctx, publisher, consumer) = fixture();
    let mut views = std::array::from_fn::<_, 32, _>(|_| am_shared_frame_view_t::default());
    for view in &mut views {
        unsafe { api::borrow(&domain, consumer, 0, 0, view) }.unwrap();
    }
    let mut extra = am_shared_frame_view_t::default();
    assert_eq!(
        unsafe { api::borrow(&domain, consumer, 0, 0, &mut extra) },
        Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
    );
    let entry = domain.get(consumer).unwrap();
    let pool = entry.data::<Pool>().unwrap();
    let pointer = *pool
        .initial
        .lock()
        .unwrap()
        .get(&views[0].borrow_id)
        .unwrap();
    domain.buffers.retain(pointer).unwrap();
    unsafe { api::return_view(&domain, consumer, &views[0]) }.unwrap();
    assert_eq!(pool.initial.lock().unwrap().len(), 31);
    // 归还初始视图不提前释放外部retain仍持有的共享租约。
    assert_eq!(
        unsafe { api::borrow(&domain, consumer, 0, 0, &mut extra) },
        Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
    );
    domain.buffers.release(pointer).unwrap();
    unsafe { api::borrow(&domain, consumer, 0, 0, &mut extra) }.unwrap();
    assert_ne!(extra.borrow_id, views[0].borrow_id);
    for view in &views[1..] {
        unsafe { api::return_view(&domain, consumer, view) }.unwrap();
    }
    unsafe { api::return_view(&domain, consumer, &extra) }.unwrap();
    assert_eq!(pool.initial.lock().unwrap().len(), 0);
    assert_eq!(pool.local_leases.load(Ordering::Acquire), 0);
    drop(entry);
    finish(domain, ctx, publisher, consumer);
}
#[test]
fn failed_buffer_registration_returns_new_shared_lease() {
    struct Dummy;
    impl Buffer for Dummy {
        fn as_any(&self) -> &dyn Any {
            self
        }
    }
    let (domain, ctx, publisher, consumer) = fixture();
    let dummy = Arc::new(Dummy);
    for key in 1..=SDK_MAX_REGISTERED_HANDLES as usize {
        domain.buffers.insert(key, dummy.clone()).unwrap();
    }
    let mut view = am_shared_frame_view_t::default();
    assert_eq!(
        unsafe { api::borrow(&domain, consumer, 0, 0, &mut view) },
        Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
    );
    let entry = domain.get(consumer).unwrap();
    let pool = entry.data::<Pool>().unwrap();
    assert_eq!(pool.initial.lock().unwrap().len(), 0);
    assert_eq!(pool.local_leases.load(Ordering::Acquire), 0);
    let pair = pool.pair().unwrap();
    let state = pair.lock(0).unwrap();
    state.reap().unwrap();
    assert_eq!(state.stats().outstanding_borrows, 0);
    drop(state);
    domain.buffers.release(1).unwrap();
    unsafe { api::borrow(&domain, consumer, 0, 0, &mut view) }.unwrap();
    unsafe { api::return_view(&domain, consumer, &view) }.unwrap();
    for key in 2..=SDK_MAX_REGISTERED_HANDLES as usize {
        domain.buffers.release(key).unwrap();
    }
    drop(entry);
    finish(domain, ctx, publisher, consumer);
}
#[test]
fn fixed_shared_index_sizes_and_capacity() {
    let mut table = Table::<u64, usize, 32>::default();
    for key in 0..32 {
        table.insert(key, key as usize).unwrap();
    }
    assert_eq!(table.insert(32, 32), Err(AM_ERR_SDK_RESOURCE_EXHAUSTED));
    assert_eq!(table.remove(&7), Some(7));
    table.insert(32, 32).unwrap();
    println!(
        "shared_index_bytes ledger={} initial={} verified={}",
        std::mem::size_of::<Ledger>(),
        std::mem::size_of::<Table<u64, usize, 32>>(),
        std::mem::size_of::<Table<u64, Process, 8>>()
    );
}

#[test]
fn ledger_four_live_mappings_rejects_fifth_and_reuses_dead_slot() {
    let mut ledger = Ledger::default();
    let limits = default_limits(false);
    let config = am_shared_pool_config_t {
        slot_count: 1,
        max_frame_bytes: 64,
        total_bytes: 64,
        max_consumers: 8,
        max_outstanding_borrows: 32,
        ..Default::default()
    };
    let mut owners = std::array::from_fn::<_, 4, _>(|_| {
        Some(
            ledger
                .admit(Arc::new(Pair::create(config).unwrap()), &limits)
                .unwrap(),
        )
    });
    assert_eq!(ledger.maps.len(), 4);
    let fifth = Arc::new(Pair::create(config).unwrap());
    assert!(matches!(
        ledger.admit(fifth.clone(), &limits),
        Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
    ));
    let old_tag = owners[0].as_ref().unwrap().attach.identity_tag;
    owners[0].take();
    assert!(ledger.existing(&old_tag).is_none());
    ledger.admit(fifth.clone(), &limits).unwrap();
    assert_eq!(ledger.maps.len(), 4);
    assert!(Arc::ptr_eq(
        &ledger.existing(&fifth.attach.identity_tag).unwrap(),
        &fifth
    ));
}
