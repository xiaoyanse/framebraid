use super::*;
use std::sync::atomic::{AtomicI32, AtomicUsize};

fn context() -> (usize, Arc<CameraState>) {
    let params = amc_init_params_t {
        abi_version: AM_ABI_VERSION,
        ..Default::default()
    };
    let key = unsafe { am_sdk_support::context::initialize_camera(domain(), &params) }.unwrap();
    (key, crate::frames::camera_state(key).unwrap())
}
fn subscription(state: Arc<CameraState>, mode: u32) -> Arc<Subscription> {
    Arc::new(Subscription {
        _metadata: state
            .control(size_of::<Subscription>() + 2 * size_of::<Item>(), false)
            .unwrap(),
        key: AtomicUsize::new(0),
        kind: Kind::Encoded {
            mask: AM_CODEC_H264,
        },
        mode,
        state,
        hub: Weak::new(),
        maximum: 2,
        max_bytes: 32,
        max_waiters: 2,
        deadline_ms: 1000,
        data: Mutex::new(Data {
            queue: VecDeque::with_capacity(2),
            waiting_key: true,
            ..Default::default()
        }),
        wake: Condvar::new(),
        alive: AtomicBool::new(false),
        handle: Mutex::default(),
    })
}
fn packet(state: &Arc<CameraState>, key: bool, seq: u64) -> Item {
    Item::Encoded(Arc::new(Payload {
        abi: am_encoded_packet_t {
            codec: AM_CODEC_H264,
            source_generation: 1,
            seq,
            flags: AM_PKT_AU_COMPLETE
                | if key {
                    AM_PKT_KEYFRAME | AM_PKT_PARAMSETS
                } else {
                    0
                },
            ..Default::default()
        },
        bytes: vec![0, 0, 0, 1, if key { 0x65 } else { 0x61 }, 0],
        _charge: state.charge(6, false).unwrap(),
    }))
}
fn cleanup(key: usize, state: Arc<CameraState>) {
    drop(state);
    assert_eq!(
        domain().close(key, 1000).unwrap(),
        AM_LIFECYCLE_CLOSED as u32
    );
    domain().destroy(key).unwrap();
}
#[test]
fn encoded_overflow_discards_reference_chain_only_for_slow_subscriber() {
    let (ctx, state) = context();
    let slow = subscription(state.clone(), AM_SUB_PULL);
    let fast = subscription(state.clone(), AM_SUB_PULL);
    for seq in 1..=4 {
        slow.enqueue(packet(&state, seq == 1, seq));
        fast.enqueue(packet(&state, seq == 1, seq));
        assert!(fast.pull(0).is_ok());
    }
    assert!(matches!(slow.pull(0), Err(AM_ERR_SDK_DEADLINE_EXCEEDED)));
    slow.enqueue(packet(&state, true, 5));
    let Item::Encoded(recovered) = slow.pull(0).unwrap() else {
        panic!()
    };
    assert_eq!(recovered.abi.seq, 5);
    assert_eq!(state.metrics.resyncs.load(Ordering::Acquire), 1);
    drop(recovered);
    drop(slow);
    drop(fast);
    cleanup(ctx, state);
}
#[test]
fn failed_export_discards_remaining_predictions_before_another_pull() {
    let (ctx, state) = context();
    let sub = subscription(state.clone(), AM_SUB_PULL);
    sub.enqueue(packet(&state, true, 1));
    sub.enqueue(packet(&state, false, 2));
    let context = state
        .ctx
        .data::<am_sdk_support::context::Context>()
        .unwrap();
    let previous = context.limits.lock().unwrap().external_hold_max_bytes;
    context.limits.lock().unwrap().external_hold_max_bytes = 0;
    assert_eq!(sub.pull_export(0), Err(AM_ERR_SDK_RESOURCE_EXHAUSTED));
    context.limits.lock().unwrap().external_hold_max_bytes = previous;
    assert_eq!(sub.pull_export(0), Err(AM_ERR_SDK_DEADLINE_EXCEEDED));
    sub.enqueue(packet(&state, false, 3));
    assert_eq!(sub.pull_export(0), Err(AM_ERR_SDK_DEADLINE_EXCEEDED));
    sub.enqueue(packet(&state, true, 4));
    let key = sub.pull_export(0).unwrap();
    domain().buffers.release(key).unwrap();
    drop(sub);
    cleanup(ctx, state);
}
struct CallbackState {
    gate: Mutex<bool>,
    wake: Condvar,
    entered: AtomicBool,
    retained: AtomicUsize,
    reentry: AtomicI32,
}
unsafe extern "C" fn blocked_callback(
    _sub: *mut amc_encoded_sub_t,
    pkt: *mut am_encoded_packet_t,
    user: *mut c_void,
) {
    let state = unsafe { &*user.cast::<CallbackState>() };
    state.reentry.store(
        am_sdk_support::callbacks::reject_reentry().unwrap_err(),
        Ordering::Release,
    );
    assert_eq!(unsafe { super::super::amc_encoded_packet_retain(pkt) }, 0);
    state.retained.store(pkt as usize, Ordering::Release);
    state.entered.store(true, Ordering::Release);
    state.wake.notify_all();
    let mut gate = state.gate.lock().unwrap();
    while !*gate {
        gate = state.wake.wait(gate).unwrap();
    }
}
#[test]
fn callback_unregister_waits_and_retained_packet_outlives_callback() {
    let (ctx, state) = context();
    let sub = subscription(state.clone(), AM_SUB_PUSH);
    let key = domain().insert(ctx, sub.clone()).unwrap();
    sub.key.store(key, Ordering::Release);
    let callback = Box::new(CallbackState {
        gate: Mutex::new(false),
        wake: Condvar::new(),
        entered: AtomicBool::new(false),
        retained: AtomicUsize::new(0),
        reentry: AtomicI32::new(0),
    });
    sub.set_callback(
        Some(Callback::Encoded(blocked_callback)),
        (&*callback as *const CallbackState) as usize,
    )
    .unwrap();
    sub.enqueue(packet(&state, true, 1));
    let until = Instant::now() + Duration::from_secs(2);
    while !callback.entered.load(Ordering::Acquire) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(callback.entered.load(Ordering::Acquire));
    let done = Arc::new(AtomicBool::new(false));
    let finished = done.clone();
    let moved = sub.clone();
    let unregister = std::thread::spawn(move || {
        moved.set_callback(None, 0).unwrap();
        finished.store(true, Ordering::Release);
    });
    std::thread::sleep(Duration::from_millis(20));
    assert!(!done.load(Ordering::Acquire));
    *callback.gate.lock().unwrap() = true;
    callback.wake.notify_all();
    unregister.join().unwrap();
    assert_eq!(
        callback.reentry.load(Ordering::Acquire),
        AM_ERR_SDK_CALLBACK_CONTRACT_VIOLATION
    );
    assert_eq!(
        domain().close(key, 1000).unwrap(),
        AM_LIFECYCLE_CLOSED as u32
    );
    domain().destroy(key).unwrap();
    let retained = callback.retained.load(Ordering::Acquire);
    assert!(domain().buffers.get(retained).is_ok());
    assert_eq!(
        unsafe { super::super::amc_encoded_packet_release(retained as _) },
        0
    );
    drop(callback);
    drop(sub);
    cleanup(ctx, state);
}
#[test]
fn unsubscribe_wakes_blocked_pull_and_mode_is_exclusive() {
    let (ctx, state) = context();
    let sub = subscription(state.clone(), AM_SUB_PULL);
    assert_eq!(sub.set_callback(None, 0), Err(AM_ERR_CAMERA_MODE_CONFLICT));
    let moved = sub.clone();
    let thread = std::thread::spawn(move || moved.pull(1000));
    let until = Instant::now() + Duration::from_secs(1);
    while lock(&sub.data).unwrap().waiters == 0 && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(1));
    }
    sub.request_close();
    assert!(matches!(
        thread.join().unwrap(),
        Err(AM_ERR_SDK_HANDLE_CLOSED)
    ));
    drop(sub);
    cleanup(ctx, state);
}
