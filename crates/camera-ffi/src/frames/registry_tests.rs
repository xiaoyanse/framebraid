use super::*;
use am_sdk_support::handles::Domain;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

thread_local! {
    static MEASURING: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
struct Counter;
fn record() {
    if MEASURING.try_with(Cell::get).unwrap_or(false) {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
    }
}
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record();
        unsafe { System.realloc(pointer, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: Counter = Counter;

pub(crate) fn allocations<T>(action: impl FnOnce() -> T) -> (T, usize) {
    struct Stop;
    impl Drop for Stop {
        fn drop(&mut self) {
            MEASURING.with(|flag| flag.set(false));
        }
    }
    ALLOCATIONS.with(|count| count.set(0));
    MEASURING.with(|flag| assert!(!flag.replace(true)));
    let stop = Stop;
    let result = action();
    drop(stop);
    (result, ALLOCATIONS.with(Cell::get))
}
fn init(domain: &Domain) -> Result<usize> {
    let params = amc_init_params_t {
        abi_version: AM_ABI_VERSION,
        ..Default::default()
    };
    unsafe { am_sdk_support::context::initialize_camera(domain, &params) }
}

#[test]
fn state_registry_is_fixed_and_reuses_context_services() {
    let (mut registry, initialized) = allocations(StateRegistry::default);
    assert_eq!(initialized, 0);
    assert_eq!(registry.rows.len(), SDK_MAX_CONTEXTS as usize);
    let domain = Domain::new(false);
    let key = init(&domain).unwrap();
    let state = registry.state(&domain, key).unwrap();
    let usage = state.usage.clone();
    let metrics = state.metrics.clone();
    let (same, lookup_allocations) = allocations(|| registry.state(&domain, key).unwrap());
    assert!(Arc::ptr_eq(&state, &same));
    assert_eq!(lookup_allocations, 0);
    state.metrics.accepted.store(7, Ordering::Relaxed);
    drop((state, same));
    assert!(registry.idle());
    let replacement = registry.state(&domain, key).unwrap();
    assert!(Arc::ptr_eq(&usage, &replacement.usage));
    assert!(Arc::ptr_eq(&metrics, &replacement.metrics));
    assert_eq!(replacement.metrics.accepted.load(Ordering::Relaxed), 7);
    println!(
        "r3-state-registry {{\"slots\":{},\"registry_bytes\":{},\"mutex_bytes\":{},\"initialize_allocations\":{initialized},\"lookup_allocations\":{lookup_allocations}}}",
        registry.rows.len(),
        std::mem::size_of::<StateRegistry>(),
        std::mem::size_of::<States>()
    );
    drop(replacement);
    domain.close(key, 0).unwrap();
    domain.destroy(key).unwrap();
}

#[test]
fn retained_state_prevents_destroy_and_does_not_free_a_ninth_context_slot() {
    let domain = Domain::new(false);
    let mut registry = StateRegistry::default();
    let mut keys = Vec::new();
    let mut states = Vec::new();
    for _ in 0..SDK_MAX_CONTEXTS {
        let key = init(&domain).unwrap();
        keys.push(key);
        states.push(registry.state(&domain, key).unwrap());
    }
    let closed = keys[0];
    assert_eq!(domain.close(closed, 0).unwrap(), AM_LIFECYCLE_CLOSED as u32);
    assert_eq!(domain.destroy(closed), Err(AM_ERR_SDK_TRY_AGAIN));
    assert_eq!(domain.context_count().unwrap(), SDK_MAX_CONTEXTS as usize);
    assert_eq!(init(&domain), Err(AM_ERR_SDK_RESOURCE_EXHAUSTED));
    assert!(Arc::ptr_eq(
        &states[0],
        &registry.state(&domain, closed).unwrap()
    ));
    states.remove(0);
    domain.destroy(closed).unwrap();
    let next = init(&domain).unwrap();
    let replacement = registry.state(&domain, next).unwrap();
    assert_eq!(
        registry
            .rows
            .iter()
            .filter(|(_, state)| state.strong_count() > 0)
            .count(),
        SDK_MAX_CONTEXTS as usize
    );
    drop(replacement);
    domain.close(next, 0).unwrap();
    domain.destroy(next).unwrap();
    drop(states);
    for key in keys.into_iter().skip(1) {
        domain.close(key, 0).unwrap();
        domain.destroy(key).unwrap();
    }
    assert!(registry.idle());
}

#[test]
fn fixed_foundation_layout_snapshot() {
    println!("r3_media_foundation camera_state={} usage={} metrics={} arc_header_bytes=16 max_contexts=8", std::mem::size_of::<CameraState>(),std::mem::size_of::<Usage>(),std::mem::size_of::<CameraMetrics>());
}
