use super::*;
use crate::{frames::registry_tests::allocations, sources::*};
use std::{collections::HashMap, mem::size_of, time::Instant};

#[test]
fn full_snapshot_has_fixed_storage_and_allocation_free_unique_visits() {
    let values: Vec<_> = (0..CAPACITY).map(Arc::new).collect();
    let (mut snapshot, count) = allocations(|| {
        let mut snapshot = Snapshot::default();
        for n in 0..CAPACITY {
            snapshot.push(values[(n * 4051) % CAPACITY].clone());
        }
        snapshot
    });
    assert_eq!(count, 0);
    let mut visits = 0;
    let mut sum = 0;
    let (_, count) = allocations(|| {
        snapshot.for_each_unique(
            |value| *value,
            |value| {
                visits += 1;
                sum += **value;
            },
        );
    });
    assert_eq!(count, 0);
    assert_eq!(visits, CAPACITY);
    assert_eq!(sum, CAPACITY * (CAPACITY - 1) / 2);
    assert_eq!(
        size_of::<Snapshot<usize>>(),
        (CAPACITY + 1) * size_of::<usize>()
    );
    println!("source_snapshot_slots={CAPACITY}; snapshot_bytes={}; capture_allocations=0; sort_visit_allocations=0", size_of::<Snapshot<usize>>());
}

#[test]
fn worst_case_snapshot_comparison_and_small_stack() {
    // 单独小栈线程覆盖初始化、返回值移动、排序及释放，不把类型大小当栈峰值。
    std::thread::Builder::new().stack_size(512 * 1024).spawn(|| {
        let values: Vec<_> = (0..CAPACITY).map(Arc::new).collect();
        let rounds = 40;
        let started = Instant::now();
        for _ in 0..rounds {
            let mut snapshot = Snapshot::default();
            for n in 0..CAPACITY {
                snapshot.push(values[(n * 4051) % CAPACITY].clone());
            }
            let mut count = 0;
            snapshot.for_each_unique(|value| *value, |_| count += 1);
            assert_eq!(count, CAPACITY);
            std::hint::black_box(&snapshot);
        }
        let fixed = started.elapsed();
        let started = Instant::now();
        for _ in 0..rounds {
            let mut map = HashMap::new();
            for n in 0..CAPACITY {
                let value = &values[(n * 4051) % CAPACITY];
                map.entry(**value).or_insert_with(|| value.clone());
            }
            let rows: Vec<_> = map.into_values().collect();
            assert_eq!(rows.len(), CAPACITY);
            std::hint::black_box(&rows);
        }
        let previous = started.elapsed();
        let (_, old_allocations) = allocations(|| {
            let mut map = HashMap::new();
            for value in &values {
                map.entry(**value).or_insert_with(|| value.clone());
            }
            std::hint::black_box(map.into_values().collect::<Vec<_>>());
        });
        println!("source_snapshot_worst rounds={rounds}; entries={CAPACITY}; fixed_us={}; previous_hashmap_vec_us={}; previous_allocations={old_allocations}; tested_stack_bytes={}", fixed.as_micros(), previous.as_micros(), 512 * 1024);
    }).unwrap().join().unwrap();
}

fn source(shared: &Arc<SharedSource>, key: usize) -> Arc<Source> {
    shared.owners.fetch_add(1, Ordering::AcqRel);
    Arc::new(Source {
        shared: shared.clone(),
        key: AtomicUsize::new(key),
        closing: AtomicBool::new(false),
        closed: AtomicBool::new(false),
        events: Arc::new(events::Events::new(shared.state.clone())),
    })
}

#[test]
fn source_snapshots_keep_original_members_release_locks_and_deduplicate_identity() {
    let (_domain, state) = crate::frames::isolated_test_state();
    let shared = isolated_test_source(state.clone());
    let independent = isolated_test_source(state.clone());
    assert_eq!(shared.url, independent.url, "reuse=0相同URL仍是不同源");
    let first = source(&shared, 1);
    let alias = source(&shared, 2);
    let other = source(&independent, 3);
    {
        let mut rows = shared.sources.lock().unwrap();
        rows.try_insert(&state, 1, Arc::downgrade(&first), CAPACITY)
            .unwrap();
        rows.try_insert(&state, 2, Arc::downgrade(&alias), CAPACITY)
            .unwrap();
    }
    let (snapshot, count) = allocations(|| shared.source_snapshot());
    assert_eq!(count, 0);
    assert!(shared.sources.try_lock().is_ok());
    assert_eq!(snapshot.iter().count(), 2);
    let weak = Arc::downgrade(&first);
    std::thread::spawn(move || {
        first.request_close();
        drop(first);
    })
    .join()
    .unwrap();
    assert!(weak.upgrade().is_some(), "并发关闭后当次快照仍持原对象");
    let later = source(&shared, 4);
    shared
        .sources
        .lock()
        .unwrap()
        .try_insert(&state, 4, Arc::downgrade(&later), CAPACITY)
        .unwrap();
    assert_eq!(snapshot.iter().count(), 2, "不能混入快照完成后新增成员");
    drop(snapshot);
    assert!(weak.upgrade().is_none());

    let registry = Mutex::new(Registry {
        sources: (0..CAPACITY)
            .map(|n| Arc::downgrade(if n % 3 == 0 { &other } else { &alias }))
            .collect(),
        shared: Vec::new(),
    });
    let (mut snapshot, count) =
        allocations(|| registry.lock().unwrap().shared_snapshot(state.ctx_key));
    assert_eq!(count, 0);
    assert!(registry.try_lock().is_ok());
    let mut ids = [0; 2];
    let mut count = 0;
    let (_, allocs) = allocations(|| {
        snapshot.for_each_unique(
            |source| source.source_id,
            |source| {
                assert!(registry.try_lock().is_ok(), "读取Hub前必须放开全域登记锁");
                ids[count] = source.source_id;
                count += 1;
            },
        )
    });
    assert_eq!(allocs, 0);
    assert_eq!(count, 2, "共享别名计一次；同URL独立源分别计数");
    assert_eq!(
        ids,
        [
            shared.source_id.min(independent.source_id),
            shared.source_id.max(independent.source_id)
        ]
    );
}

#[test]
fn common_size_snapshot_comparison_reports_fixed_initialization_cost() {
    for size in [1, 16, 64] {
        let values: Vec<_> = (0..size).map(Arc::new).collect();
        let rounds = 2000;
        let started = Instant::now();
        for _ in 0..rounds {
            let mut snapshot = Snapshot::default();
            for value in values.iter().rev() {
                snapshot.push(value.clone());
            }
            let mut count = 0;
            snapshot.for_each_unique(|value| *value, |_| count += 1);
            assert_eq!(count, size);
            std::hint::black_box(&snapshot);
        }
        let fixed = started.elapsed();
        let started = Instant::now();
        for _ in 0..rounds {
            let mut map = HashMap::new();
            for value in values.iter().rev() {
                map.entry(**value).or_insert_with(|| value.clone());
            }
            let rows: Vec<_> = map.into_values().collect();
            assert_eq!(rows.len(), size);
            std::hint::black_box(&rows);
        }
        println!("source_snapshot_common rounds={rounds}; entries={size}; fixed_us={}; previous_hashmap_vec_us={}", fixed.as_micros(), started.elapsed().as_micros());
    }
}
