use super::*;
use std::sync::{atomic::AtomicUsize, Condvar};

fn hub() -> Arc<H264Hub> {
    H264Hub::new(Arc::new(|_| None), GOP_CACHE_MAX_BYTES)
}
fn core(url: &str) -> Arc<SessionCore> {
    Arc::new(SessionCore::new(url, GOP_CACHE_MAX_BYTES))
}
fn wait(check: impl Fn() -> bool) {
    let until = std::time::Instant::now() + Duration::from_secs(3);
    while !check() {
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn full_source_map_rejects_before_starting_another_worker() {
    let hub = hub();
    {
        let mut sessions = hub.sessions.lock().unwrap();
        for n in 0..MAX_SOURCES_PER_HUB {
            let url = format!("rtsp://127.0.0.1:9/{n}");
            let core = core(&url);
            core.thread_alive.store(true, Ordering::Release);
            sessions.insert(url, core);
        }
    }
    for _ in 0..32 {
        assert!(matches!(
            hub.subscribe("rtsp://127.0.0.1:9/excess"),
            Err(DenyReason::TooManySources)
        ));
    }
    assert_eq!(hub.sessions.lock().unwrap().len(), MAX_SOURCES_PER_HUB);
}
#[test]
fn stopped_but_live_core_is_not_replaced() {
    let hub = hub();
    let old = core("rtsp://127.0.0.1:9/stopped");
    old.thread_alive.store(true, Ordering::Release);
    old.stop.store(true, Ordering::Release);
    hub.sessions
        .lock()
        .unwrap()
        .insert(old.url.clone(), old.clone());
    for _ in 0..100 {
        assert!(matches!(hub.subscribe(&old.url), Err(DenyReason::NotReady)));
    }
    assert!(Arc::ptr_eq(&old, &hub.sessions.lock().unwrap()[&old.url]));
}
#[test]
fn old_subscription_drop_cannot_remove_same_id_from_replacement() {
    let hub = hub();
    let old = core("rtsp://127.0.0.1:9/reused");
    let new = core(&old.url);
    let old_fan = Arc::new(ClientFan::new());
    let new_fan = Arc::new(ClientFan::new());
    old.lock_clients().insert(1, old_fan.clone());
    new.lock_clients().insert(1, new_fan);
    new.thread_alive.store(true, Ordering::Release);
    hub.sessions
        .lock()
        .unwrap()
        .insert(old.url.clone(), new.clone());
    drop(H264Subscription {
        hub: hub.clone(),
        core: old.clone(),
        client_id: 1,
        queue: old_fan.queue.clone(),
    });
    assert!(old.lock_clients().is_empty());
    assert_eq!(new.lock_clients().len(), 1);
    assert!(!new.stop.load(Ordering::Acquire));
    assert!(Arc::ptr_eq(&new, &hub.sessions.lock().unwrap()[&new.url]));
}
#[test]
fn worker_guard_clears_ready_and_retained_configuration_on_unwind() {
    let hub = hub();
    let core = core("rtsp://127.0.0.1:9/panic");
    let fan = Arc::new(ClientFan::new());
    core.lock_clients().insert(1, fan.clone());
    core.ready.store(true, Ordering::Release);
    core.thread_alive.store(true, Ordering::Release);
    *core.config.lock().unwrap() = Some(Arc::new(StreamConfig {
        width: 1,
        height: 1,
        fps: None,
        generation: 1,
    }));
    *core.current_sps.lock().unwrap() = vec![7; 128];
    hub.sessions
        .lock()
        .unwrap()
        .insert(core.url.clone(), core.clone());
    let worker = SessionWorker {
        hub: hub.clone(),
        core: core.clone(),
    };
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _worker = worker;
            panic!("controlled worker unwind")
        }))
        .is_err()
    );
    assert!(!core.thread_alive.load(Ordering::Acquire));
    assert!(!hub.is_ready(&core.url));
    assert!(core.config.lock().unwrap().is_none());
    assert!(core.current_sps.lock().unwrap().is_empty());
    assert!(matches!(
        fan.queue.lock_for_test().front(),
        Some(H264Message::Closed(1001, _))
    ));
    hub.unsubscribe(&core, 1);
    assert!(hub.sessions.lock().unwrap().is_empty());
}
#[test]
fn concurrent_first_subscribers_share_one_blocked_resolver_and_stopping_slot() {
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let entered = Arc::new(AtomicUsize::new(0));
    let resolver: UrlResolver = {
        let gate = gate.clone();
        let entered = entered.clone();
        Arc::new(move |_| {
            entered.fetch_add(1, Ordering::AcqRel);
            let (lock, changed) = &*gate;
            let mut open = lock.lock().unwrap();
            while !*open {
                open = changed.wait(open).unwrap();
            }
            None
        })
    };
    let hub = H264Hub::new(resolver, GOP_CACHE_MAX_BYTES);
    let url = "rtsp://127.0.0.1:9/blocked";
    std::thread::scope(|scope| {
        for _ in 0..16 {
            let hub = hub.clone();
            scope.spawn(move || {
                for _ in 0..20 {
                    assert!(matches!(hub.subscribe(url), Err(DenyReason::NotReady)));
                }
            });
        }
    });
    wait(|| entered.load(Ordering::Acquire) > 0);
    let count = entered.load(Ordering::Acquire);
    let current = hub.sessions.lock().unwrap()[url].clone();
    current.stop.store(true, Ordering::Release);
    for _ in 0..20 {
        assert!(matches!(hub.subscribe(url), Err(DenyReason::NotReady)));
    }
    let same = Arc::ptr_eq(&current, &hub.sessions.lock().unwrap()[url]);
    *gate.0.lock().unwrap() = true;
    gate.1.notify_all();
    wait(|| !current.thread_alive.load(Ordering::Acquire));
    assert_eq!(count, 1);
    assert!(same);
    assert!(hub.sessions.lock().unwrap().is_empty());
}
#[test]
fn long_resource_backoff_honors_already_requested_stop() {
    let core = core("rtsp://127.0.0.1:9/backoff");
    core.stop.store(true, Ordering::Release);
    let at = std::time::Instant::now();
    assert!(wait_or_stop(&core, Duration::from_secs(30)));
    assert!(at.elapsed() < Duration::from_secs(2));
}
