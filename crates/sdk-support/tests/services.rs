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
    static FAIL_NEXT: Cell<bool> = const { Cell::new(false) };
    static FAIL_AFTER: Cell<Option<usize>> = const { Cell::new(None) };
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
        let scheduled_failure = FAIL_AFTER
            .try_with(|v| match v.get() {
                Some(0) => {
                    v.set(None);
                    true
                }
                Some(remaining) => {
                    v.set(Some(remaining - 1));
                    false
                }
                None => false,
            })
            .unwrap_or(false);
        if FAIL_NEXT.try_with(|v| v.replace(false)).unwrap_or(false) || scheduled_failure {
            return std::ptr::null_mut();
        }
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
impl am_sdk_support::buffers::Buffer for Hook {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
#[test]
fn initial_registry_allocation_failures_do_not_publish_or_abort() {
    let domain = am_sdk_support::handles::Domain::new(false);
    let body: Arc<dyn Managed> = Arc::new(Hook);
    FAIL_NEXT.with(|v| v.set(true));
    let result = domain.insert(0, body.clone());
    assert_eq!(result, Err(AM_ERR_SDK_RESOURCE_EXHAUSTED));
    assert_eq!(domain.context_count().unwrap(), 0);
    assert!(domain.idle_for_unload());
    let key = domain.insert(0, body).unwrap();
    domain.close(key, 0).unwrap();
    domain.destroy(key).unwrap();
    let buffers = am_sdk_support::buffers::Buffers::default();
    let value: Arc<dyn am_sdk_support::buffers::Buffer> = Arc::new(Hook);
    FAIL_NEXT.with(|v| v.set(true));
    assert_eq!(
        buffers.insert(1, value.clone()),
        Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
    );
    assert!(buffers.idle_for_unload());
    assert!(buffers.get(1).is_err());
    buffers.insert(1, value).unwrap();
    buffers.release(1).unwrap();
    assert!(buffers.idle_for_unload());
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

#[test]
fn child_close_has_no_heap_snapshot_and_lookup_cost_is_reported() {
    use am_sdk_support::handles::Domain;
    use std::{sync::Mutex, time::Instant};
    let domain = Domain::new(false);
    let body = Arc::new(Hook);
    let mut root = 0;
    let startup = measure(|| root = domain.insert(0, body).unwrap());
    println!(
        "first_handle_calls={} first_handle_requested_bytes={}",
        startup.0, startup.1
    );
    let children: Vec<_> = (0..128)
        .map(|_| domain.insert(root, Arc::new(Hook)).unwrap())
        .collect();
    let mut snapshot = Vec::new();
    let baseline = measure(|| {
        snapshot = children
            .iter()
            .map(|key| (*key, domain.get(*key).unwrap()))
            .collect()
    });
    let candidate =
        measure(|| assert_eq!(domain.close(root, 0).unwrap(), AM_LIFECYCLE_CLOSED as u32));
    assert!(baseline.0 > 0);
    assert_eq!(candidate, (0, 0));
    drop(snapshot);
    for key in children {
        domain.destroy(key).unwrap();
    }
    domain.destroy(root).unwrap();
    assert!(domain.idle_for_unload());
    println!(
        "children=128 old_snapshot_calls={} new_close_snapshot_calls={}",
        baseline.0, candidate.0
    );
    for n in [8usize, 128, 4096] {
        let domain = Domain::new(false);
        let keys: Vec<_> = (0..n)
            .map(|_| domain.insert(0, Arc::new(Hook)).unwrap())
            .collect();
        let old = Mutex::new(
            keys.iter()
                .map(|key| (*key, domain.get(*key).unwrap()))
                .collect::<HashMap<_, _>>(),
        );
        for key in &keys {
            assert!(Arc::ptr_eq(
                &domain.get(*key).unwrap(),
                &old.lock().unwrap()[key]
            ));
        }
        for round in 0..3 {
            for sorted in if round % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                let at = Instant::now();
                for i in 0..50_000usize {
                    let key = keys[i.wrapping_mul(2654435761) % n];
                    let value = if sorted {
                        domain.get(key).unwrap()
                    } else {
                        old.lock().unwrap().get(&key).unwrap().clone()
                    };
                    std::hint::black_box(value);
                }
                println!(
                    "lookup_items={} round={} sorted={} calls=50000 total_ns={}",
                    n,
                    round,
                    sorted,
                    at.elapsed().as_nanos()
                );
            }
        }
    }
}

#[test]
fn runtime_directory_copy_is_exact_and_fallible_before_context_publication() {
    use am_sdk_support::handles::Domain;
    use std::ffi::CString;
    let long = format!(
        "{}{}",
        if cfg!(windows) { "C:/" } else { "/" },
        "a".repeat(SDK_MAX_STRING_BYTES as usize - 1 - if cfg!(windows) { 3 } else { 1 })
    );
    let strings: Vec<_> = (0..SDK_MAX_RUNTIME_DIRECTORIES)
        .map(|_| CString::new(long.as_bytes()).unwrap())
        .collect();
    let pointers: Vec<_> = strings.iter().map(|s| s.as_ptr()).collect();
    let params = amc_init_params_t {
        abi_version: AM_ABI_VERSION,
        runtime_dirs_utf8: pointers.as_ptr(),
        runtime_dir_count: pointers.len() as u32,
        ..Default::default()
    };
    // 第一项为Vec backing，后两项分别在已复制0/1个目录时失败。
    for failure_after in 0..=2 {
        let domain = Domain::new(false);
        FAIL_AFTER.with(|v| v.set(Some(failure_after)));
        let failed = unsafe { initialize_camera(&domain, &params) };
        FAIL_AFTER.with(|v| v.set(None));
        assert_eq!(failed, Err(AM_ERR_SDK_RESOURCE_EXHAUSTED));
        assert_eq!(domain.context_count().unwrap(), 0);
        assert!(domain.idle_for_unload());
        let mut keys = Vec::new();
        for _ in 0..SDK_MAX_CONTEXTS {
            keys.push(unsafe { initialize_camera(&domain, &params) }.unwrap());
        }
        let saturated = measure(|| {
            assert_eq!(
                unsafe { initialize_camera(&domain, &params) },
                Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
            )
        });
        assert_eq!(saturated, (0, 0));
        let legacy_copy = measure(|| {
            let mut old = Vec::with_capacity(strings.len());
            for string in &strings {
                old.push(string.to_str().unwrap().to_owned());
            }
            std::hint::black_box(old);
        });
        assert_eq!(
            legacy_copy,
            (
                1 + strings.len(),
                strings.len() * (std::mem::size_of::<String>() + SDK_MAX_STRING_BYTES as usize - 1)
            )
        );
        println!("saturated_init_legacy_directory_copy_calls={} legacy_directory_copy_requested_bytes={} candidate_calls={} candidate_requested_bytes={}",legacy_copy.0,legacy_copy.1,saturated.0,saturated.1);
        let entry = domain.get(keys[0]).unwrap();
        let context = entry.data::<Context>().unwrap();
        assert_eq!(
            context.runtime_dirs.capacity(),
            SDK_MAX_RUNTIME_DIRECTORIES as usize
        );
        for directory in &context.runtime_dirs {
            assert_eq!(directory.len(), SDK_MAX_STRING_BYTES as usize - 1);
            assert_eq!(directory.capacity(), directory.len());
        }
        let bytes = context.runtime_dirs.capacity() * std::mem::size_of::<String>()
            + context
                .runtime_dirs
                .iter()
                .map(String::capacity)
                .sum::<usize>();
        println!("runtime_dirs_failure_after={} max_contexts={} directories_per_context={} directory_len={} per_context_requested_backing_bytes={} domain_requested_backing_bytes={}",failure_after,SDK_MAX_CONTEXTS,SDK_MAX_RUNTIME_DIRECTORIES,long.len(),bytes,bytes * SDK_MAX_CONTEXTS as usize);
        drop(entry);
        for key in keys {
            domain.close(key, 0).unwrap();
            domain.destroy(key).unwrap();
        }
        assert!(domain.idle_for_unload());
    }
}
#[test]
fn invalid_runtime_directories_do_not_allocate_and_vision_failure_can_retry() {
    use am_sdk_support::{context::initialize_vision, handles::Domain};
    let invalid = c"relative".as_ptr();
    let params = amc_init_params_t {
        abi_version: AM_ABI_VERSION,
        runtime_dirs_utf8: &invalid,
        runtime_dir_count: 1,
        ..Default::default()
    };
    let domain = Domain::new(false);
    let allocations = measure(|| {
        assert_eq!(
            unsafe { initialize_camera(&domain, &params) },
            Err(AM_ERR_SDK_INVALID_ARGUMENT)
        )
    });
    assert_eq!(allocations, (0, 0));
    assert!(domain.idle_for_unload());
    let directory = if cfg!(windows) {
        c"C:/runtime"
    } else {
        c"/runtime"
    };
    let pointer = directory.as_ptr();
    let params = amv_init_params_t {
        abi_version: AM_ABI_VERSION,
        runtime_dirs_utf8: &pointer,
        runtime_dir_count: 1,
        infer_threads: 1,
        total_thread_budget: 1,
        domain_infer_slots: 1,
        ..Default::default()
    };
    let domain = Domain::new(true);
    FAIL_AFTER.with(|v| v.set(Some(1)));
    let failed = unsafe { initialize_vision(&domain, &params) };
    FAIL_AFTER.with(|v| v.set(None));
    assert_eq!(failed, Err(AM_ERR_SDK_RESOURCE_EXHAUSTED));
    assert_eq!(*domain.thread_config.lock().unwrap(), None);
    assert!(domain.idle_for_unload());
    let key = unsafe { initialize_vision(&domain, &params) }.unwrap();
    assert_eq!(domain.context_count().unwrap(), 1);
    domain.close(key, 0).unwrap();
    domain.destroy(key).unwrap();
}

#[test]
fn fixed_foundation_layout_snapshot() {
    println!(
        "r3_context_foundation context={} services={} close_hooks={} arc_header_bytes=16",
        std::mem::size_of::<Context>(),
        std::mem::size_of::<Services>(),
        std::mem::size_of::<CloseHooks>()
    );
}
