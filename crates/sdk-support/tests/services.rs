//! 单进程/单测试比较同样登记操作，分配计数不外推RSS或端到端速度。
use am_sdk_support::{
    abi::*,
    context::{initialize_camera, Context},
    handles::Managed,
    services::{CloseHooks, Services},
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    any::{Any, TypeId},
    cell::Cell,
    collections::HashMap,
    sync::Arc,
};

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static CALLS: Cell<usize> = const { Cell::new(0) };
    static BYTES: Cell<usize> = const { Cell::new(0) };
}
struct Counter;
fn count(bytes: usize) {
    if ENABLED.try_with(Cell::get).unwrap_or(false) {
        let _ = CALLS.try_with(|v| v.set(v.get() + 1));
        let _ = BYTES.try_with(|v| v.set(v.get() + bytes));
    }
}
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count(size);
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: Counter = Counter;
fn measure(action: impl FnOnce()) -> (usize, usize) {
    CALLS.with(|v| v.set(0));
    BYTES.with(|v| v.set(0));
    ENABLED.with(|v| v.set(true));
    action();
    ENABLED.with(|v| v.set(false));
    (CALLS.with(Cell::get), BYTES.with(Cell::get))
}
struct Hook;
impl Managed for Hook {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
#[test]
fn same_five_services_and_close_hook_have_no_index_heap_allocation() {
    let ids = [
        TypeId::of::<u8>(),
        TypeId::of::<u16>(),
        TypeId::of::<u32>(),
        TypeId::of::<u64>(),
        TypeId::of::<usize>(),
    ];
    let body: Arc<dyn Any + Send + Sync> = Arc::new(7u32);
    let mut old = HashMap::new();
    let old_index = measure(|| {
        for key in ids {
            old.insert(key, body.clone());
        }
    });
    let mut new = Services::default();
    let new_index = measure(|| {
        for key in ids {
            new.get_or_insert_with(key, || body.clone()).unwrap();
        }
    });
    assert!(old_index.0 > 0);
    assert_eq!(new_index, (0, 0));
    for key in ids {
        assert_eq!(
            old[&key].downcast_ref::<u32>(),
            new.get(&key).unwrap().downcast_ref::<u32>()
        );
    }
    let hook: Arc<dyn Managed> = Arc::new(Hook);
    let mut old_hooks = Vec::new();
    let old_registration = measure(|| old_hooks.push(Arc::downgrade(&hook)));
    let hooks = Arc::new(CloseHooks::default());
    let new_registration = measure(|| hooks.register(Arc::downgrade(&hook)).unwrap());
    let mut old_snapshot = Vec::new();
    let old_close = measure(|| {
        old_snapshot = old_hooks
            .iter()
            .filter_map(std::sync::Weak::upgrade)
            .collect()
    });
    assert_eq!(old_snapshot.len(), 1);
    assert_eq!(new_registration, (0, 0));
    assert!(old_registration.0 > 0);
    assert!(old_close.0 > 0);
    let domain = am_sdk_support::handles::Domain::new(false);
    let params = amc_init_params_t {
        abi_version: AM_ABI_VERSION,
        ..Default::default()
    };
    let key = unsafe { initialize_camera(&domain, &params) }.unwrap();
    let entry = domain.get(key).unwrap();
    let context = entry.data::<Context>().unwrap();
    context
        .services
        .lock()
        .unwrap()
        .get_or_insert_with(TypeId::of::<CloseHooks>(), || hooks.clone())
        .unwrap();
    let new_close = measure(|| context.request_close());
    assert_eq!(new_close, (0, 0));
    println!("old_index_calls={} old_index_requested_bytes={} old_index_capacity={} new_index_calls={} new_index_inline_bytes={} old_hook_calls={} old_close_snapshot_calls={} new_hook_calls={} new_close_snapshot_calls={} new_hooks_inline_bytes={} context_inline_bytes={}",old_index.0,old_index.1,old.capacity(),new_index.0,std::mem::size_of::<Services>(),old_registration.0,old_close.0,new_registration.0,new_close.0,std::mem::size_of::<CloseHooks>(),std::mem::size_of::<Context>());
}
