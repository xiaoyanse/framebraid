use super::*;

#[test]
fn last_model_joins_workers_and_concurrent_generations_do_not_overlap() {
    let budget = ThreadBudget {
        parallel: 2,
        total: 4,
        slots: 64,
    };
    assert!(reserve_model(budget, 3).is_err());
    assert!(ENGINE.lock().unwrap().is_none());
    let first = reserve_model(budget, 1).unwrap();
    let engine = first.engine.clone();
    let second = reserve_model(budget, 1).unwrap();
    assert!(Arc::ptr_eq(&first.engine, &second.engine));
    assert!(reserve_model(budget, 1).is_err());
    drop(first);
    assert_eq!(engine.data.lock().unwrap().models, 1);
    assert_eq!(engine.threads.lock().unwrap().len(), 2);
    drop(second);
    assert!(ENGINE.lock().unwrap().is_none());
    assert!(engine.threads.lock().unwrap().is_empty());
    assert_eq!(Arc::strong_count(&engine), 1);

    let mut callers = Vec::new();
    for _ in 0..4 {
        callers.push(std::thread::spawn(move || {
            for _ in 0..30 {
                let permit = reserve_model(budget, 0).unwrap();
                assert!(!permit.engine.stop.load(Ordering::Acquire));
                assert_eq!(permit.engine.budget, budget);
                std::thread::yield_now();
                drop(permit);
            }
        }));
    }
    for caller in callers {
        caller.join().unwrap();
    }
    assert!(ENGINE.lock().unwrap().is_none());
}
