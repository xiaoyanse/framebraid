use super::*;
#[test]
fn quarantine_never_grows_past_limit_and_child_snapshots_are_bounded() {
    let domain = Domain::new(false);
    let resource = Arc::new(Resource(AtomicUsize::new(0)));
    let mut old = VecDeque::new();
    for n in 0..SDK_HANDLE_QUARANTINE as usize + 3 {
        old.push_back(Box::new(n));
        if old.len() > SDK_HANDLE_QUARANTINE as usize {
            old.pop_front();
        }
        let key = domain.insert(0, resource.clone()).unwrap();
        assert_eq!(domain.close(key, 0).unwrap(), AM_LIFECYCLE_CLOSED as u32);
        domain.destroy(key).unwrap();
    }
    let registry = domain.registry.lock().unwrap();
    assert_eq!(registry.quarantine.len(), SDK_HANDLE_QUARANTINE as usize);
    assert_eq!(
        registry.quarantine.capacity(),
        SDK_HANDLE_QUARANTINE as usize
    );
    assert_eq!(
        registry.entries.capacity(),
        SDK_MAX_REGISTERED_HANDLES as usize
    );
    assert!(registry.entries.is_empty());
    println!(
        "old_quarantine_slots={} new_quarantine_slots={} handle_index_bytes={} entry_body_bytes={}",
        old.capacity(),
        registry.quarantine.capacity(),
        registry.entries.capacity() * std::mem::size_of::<(usize, Arc<Entry>)>(),
        std::mem::size_of::<Entry>()
    );
}
#[test]
fn multi_block_close_holds_children_until_each_visit_finishes() {
    let domain = Domain::new(false);
    let root = domain
        .insert(0, Arc::new(Resource(AtomicUsize::new(0))))
        .unwrap();
    let mut children = Vec::new();
    for _ in 0..97 {
        children.push(
            domain
                .insert(root, Arc::new(Resource(AtomicUsize::new(0))))
                .unwrap(),
        );
    }
    assert_eq!(domain.close(root, 0).unwrap(), AM_LIFECYCLE_CLOSED as u32);
    let mut visited = Vec::new();
    domain
        .children(root, |key, _entry| {
            assert_eq!(domain.destroy(key), Err(AM_ERR_SDK_TRY_AGAIN));
            visited.push(key);
            Ok(())
        })
        .unwrap();
    assert_eq!(visited.len(), 97);
    for key in children {
        domain.destroy(key).unwrap();
    }
    domain.destroy(root).unwrap();
    assert!(domain.idle_for_unload());
}
struct Resource(AtomicUsize);
impl Managed for Resource {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn drained(&self) -> bool {
        self.0.load(Ordering::Acquire) == 0
    }
}
#[test]
fn closing_keeps_inflight_resource_and_child_first_destroy() {
    let domain = Domain::new(false);
    let root = domain
        .insert(0, Arc::new(Resource(AtomicUsize::new(0))))
        .unwrap();
    let child = domain
        .insert(root, Arc::new(Resource(AtomicUsize::new(0))))
        .unwrap();
    let active = domain.active(child).unwrap();
    assert_eq!(
        domain.close(root, u32::MAX).unwrap_err(),
        AM_ERR_SDK_LIMIT_OUT_OF_RANGE
    );
    assert_eq!(domain.close(root, 0).unwrap(), AM_LIFECYCLE_CLOSING as u32);
    assert!(domain.active(child).is_err());
    drop(active);
    assert_eq!(domain.state(root).unwrap(), AM_LIFECYCLE_CLOSED as u32);
    assert!(domain.destroy(root).is_err());
    domain.destroy(child).unwrap();
    domain.destroy(root).unwrap();
    assert!(domain.get(root).is_err());
}
#[test]
fn live_external_reference_prevents_close_and_destroy_isolated_addresses() {
    let domain = Domain::new(false);
    let resource = Arc::new(Resource(AtomicUsize::new(1)));
    let key = domain.insert(0, resource.clone()).unwrap();
    assert_eq!(domain.close(key, 0).unwrap(), AM_LIFECYCLE_CLOSING as u32);
    resource.0.store(0, Ordering::Release);
    assert_eq!(domain.close(key, 0).unwrap(), AM_LIFECYCLE_CLOSED as u32);
    domain.destroy(key).unwrap();
    let next = domain
        .insert(0, Arc::new(Resource(AtomicUsize::new(0))))
        .unwrap();
    assert_ne!(key, next);
}

#[test]
fn closing_reader_is_fenced_again_at_final_resource_close() {
    struct Paused {
        calls: AtomicUsize,
        entered: std::sync::Barrier,
        proceed: std::sync::Barrier,
    }
    impl Managed for Paused {
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn drained(&self) -> bool {
            if self.calls.fetch_add(1, Ordering::AcqRel) == 0 {
                self.entered.wait();
                self.proceed.wait();
            }
            true
        }
    }
    let domain = Arc::new(Domain::new(true));
    let resource = Arc::new(Paused {
        calls: AtomicUsize::new(0),
        entered: std::sync::Barrier::new(2),
        proceed: std::sync::Barrier::new(2),
    });
    let key = domain.insert(0, resource.clone()).unwrap();
    domain.request_close(key).unwrap();
    let closer = domain.clone();
    let worker = std::thread::spawn(move || closer.state(key).unwrap());
    resource.entered.wait();
    let entry = domain.get(key).unwrap();
    let reader = entry.readable().unwrap();
    resource.proceed.wait();
    assert_eq!(worker.join().unwrap(), AM_LIFECYCLE_CLOSING as u32);
    drop(reader);
    assert_eq!(domain.state(key).unwrap(), AM_LIFECYCLE_CLOSED as u32);
    assert!(matches!(entry.readable(), Err(AM_ERR_SDK_HANDLE_CLOSED)));
    for _ in 0..20 {
        domain.request_close(key).unwrap();
        assert_eq!(
            entry.state.load(Ordering::Acquire),
            AM_LIFECYCLE_CLOSED as u32
        );
    }
    drop(entry);
    domain.destroy(key).unwrap();
}

struct ContextResource;
impl Managed for ContextResource {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn is_context(&self) -> bool {
        true
    }
}
#[test]
fn pending_context_admission_is_bounded_and_does_not_hold_the_registry_lock() {
    let domain = Domain::new(false);
    let entered = std::sync::Barrier::new(SDK_MAX_CONTEXTS as usize + 1);
    let release = std::sync::Barrier::new(SDK_MAX_CONTEXTS as usize + 1);
    std::thread::scope(|scope| {
        for _ in 0..SDK_MAX_CONTEXTS {
            scope.spawn(|| {
                let admission = domain.admit_context().unwrap();
                entered.wait();
                release.wait();
                drop(admission);
            });
        }
        entered.wait();
        assert_eq!(domain.context_count().unwrap(), 0);
        assert!(!domain.idle_for_unload());
        assert!(matches!(
            domain.admit_context(),
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
        ));
        assert_eq!(
            domain.insert(0, Arc::new(ContextResource)),
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
        );
        // 普通句柄操作在初始化占位期间仍可进行。
        let key = domain
            .insert(0, Arc::new(Resource(AtomicUsize::new(0))))
            .unwrap();
        domain.close(key, 0).unwrap();
        domain.destroy(key).unwrap();
        release.wait();
    });
    assert!(domain.idle_for_unload());
    assert_eq!(domain.registry.lock().unwrap().pending_contexts, 0);
}
#[test]
fn context_admission_transfers_once_and_reserves_its_handle_slot() {
    let domain = Domain::new(false);
    let admission = domain.admit_context().unwrap();
    let mut keys = Vec::new();
    for _ in 1..SDK_MAX_REGISTERED_HANDLES {
        keys.push(
            domain
                .insert(0, Arc::new(Resource(AtomicUsize::new(0))))
                .unwrap(),
        );
    }
    assert_eq!(
        domain.insert(0, Arc::new(Resource(AtomicUsize::new(0)))),
        Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
    );
    let context = admission.commit(Arc::new(ContextResource)).unwrap();
    assert_eq!(domain.context_count().unwrap(), 1);
    assert_eq!(domain.registry.lock().unwrap().pending_contexts, 0);
    for key in keys.into_iter().chain([context]) {
        domain.close(key, 0).unwrap();
        domain.destroy(key).unwrap();
    }
    assert!(domain.idle_for_unload());
    let admission = domain.admit_context().unwrap();
    assert_eq!(
        admission.commit(Arc::new(Resource(AtomicUsize::new(0)))),
        Err(AM_ERR_SDK_INVALID_ARGUMENT)
    );
    assert!(domain.idle_for_unload());
    let mut contexts = Vec::new();
    for _ in 1..SDK_MAX_CONTEXTS {
        contexts.push(domain.insert(0, Arc::new(ContextResource)).unwrap());
    }
    let last = domain
        .admit_context()
        .unwrap()
        .commit(Arc::new(ContextResource))
        .unwrap();
    assert_eq!(domain.context_count().unwrap(), SDK_MAX_CONTEXTS as usize);
    assert!(matches!(
        domain.admit_context(),
        Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
    ));
    for key in contexts.into_iter().chain([last]) {
        domain.close(key, 0).unwrap();
        domain.destroy(key).unwrap();
    }
}

#[test]
fn failed_admission_commit_returns_the_pending_slot() {
    let domain = Domain::new(false);
    domain.registry.lock().unwrap().sequence = u64::MAX;
    let admission = domain.admit_context().unwrap();
    assert_eq!(
        admission.commit(Arc::new(ContextResource)),
        Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
    );
    assert!(domain.idle_for_unload());
    assert_eq!(domain.registry.lock().unwrap().pending_contexts, 0);
    domain.registry.lock().unwrap().sequence = 0;
    let key = domain
        .admit_context()
        .unwrap()
        .commit(Arc::new(ContextResource))
        .unwrap();
    domain.close(key, 0).unwrap();
    domain.destroy(key).unwrap();
    assert!(domain.idle_for_unload());
}
