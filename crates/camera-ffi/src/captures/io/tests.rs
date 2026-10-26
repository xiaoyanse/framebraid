use super::*;

#[test]
fn cache_storage_failure_completes_and_removes_the_coalesced_flight() {
    with_state(|state, camera| {
        let key = (99, 1000);
        state
            .consumers
            .lock()
            .unwrap()
            .try_insert(camera, key, 1, 4)
            .unwrap();
        let flight = Arc::new(Flight {
            result: Mutex::default(),
            changed: Condvar::new(),
            _charge: camera.metadata(std::mem::size_of::<Flight>()).unwrap(),
        });
        state
            .flights
            .lock()
            .unwrap()
            .try_insert(camera, 99, flight.clone(), 16)
            .unwrap();
        let ctx = camera
            .ctx
            .data::<am_sdk_support::context::Context>()
            .unwrap();
        ctx.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_bytes = 0;
        let image = raw(camera, 42);
        let follower = flight.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = follower.result.lock().unwrap();
            ready_tx.send(()).unwrap();
            let (result, wait) = follower
                .changed
                .wait_timeout_while(result, Duration::from_secs(2), |r| r.is_none())
                .unwrap();
            done_tx
                .send((
                    !wait.timed_out(),
                    matches!(result.as_ref(), Some(Err(AM_ERR_SDK_RESOURCE_EXHAUSTED))),
                ))
                .unwrap();
        });
        ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let outcome = cache_recent(state, camera, key, &image).map(|()| image.clone());
        assert!(matches!(
            finish(state, 99, &flight, outcome),
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
        ));
        assert_eq!(
            done_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            (true, true)
        );
        worker.join().unwrap();
        assert!(state.flights.lock().unwrap().is_empty());
        assert!(state.recent.lock().unwrap().is_empty());
        drop(image);
        drop(flight);
    });
}

fn with_state(test: impl FnOnce(&Arc<state::State>, &Arc<CameraState>)) {
    let (domain, camera) = crate::frames::isolated_test_state();
    let key = camera.ctx_key;
    let state = state::State::get(&camera).unwrap();
    test(&state, &camera);
    state.request_close();
    assert_eq!(camera.usage_snapshot().unwrap(), (0, 0, 0));
    drop((state, camera));
    assert_eq!(domain.close(key, 100).unwrap(), AM_LIFECYCLE_CLOSED as u32);
    domain.destroy(key).unwrap();
}

fn raw(camera: &Arc<CameraState>, id: u64) -> Arc<Raw> {
    Arc::new(Raw {
        encoded: vec![id as u8; 64],
        width: 4,
        height: 4,
        meta: Box::new(am_frame_meta_t {
            frame_id: id,
            ..Default::default()
        }),
        _charge: camera.charge(64, false).unwrap(),
    })
}

#[test]
fn retiring_cadences_releases_history_while_another_cadence_stays_live() {
    with_state(|state, camera| {
        let steady = (11, 60000);
        state
            .consumers
            .lock()
            .unwrap()
            .try_insert(camera, steady, 1, 4)
            .unwrap();
        cache_recent(state, camera, steady, &raw(camera, 1)).unwrap();
        for index in 0..100 {
            let changing = (11, 900 - index);
            state
                .consumers
                .lock()
                .unwrap()
                .try_insert(camera, changing, 1, 4)
                .unwrap();
            cache_recent(state, camera, changing, &raw(camera, index as u64 + 2)).unwrap();
            assert_eq!(state.recent.lock().unwrap().len(), 2);
            state.remove_consumer(changing);
            assert_eq!(state.recent.lock().unwrap().len(), 1);
            assert_eq!(camera.usage_snapshot().unwrap().0, 64);
        }
        state.remove_consumer(steady);
        assert!(state.recent.lock().unwrap().is_empty());
    });
}

#[test]
fn shared_cadence_retires_only_after_last_consumer_without_revoking_a_reader() {
    with_state(|state, camera| {
        let key = (12, 100);
        state
            .consumers
            .lock()
            .unwrap()
            .try_insert(camera, key, 2, 4)
            .unwrap();
        let held = raw(camera, 7);
        cache_recent(state, camera, key, &held).unwrap();
        state.remove_consumer(key);
        assert!(Arc::ptr_eq(&state.recent.lock().unwrap()[&key], &held));
        state.remove_consumer(key);
        assert!(state.recent.lock().unwrap().is_empty());
        assert_eq!(held.encoded, vec![7; 64]);
        assert_eq!(camera.usage_snapshot().unwrap().0, 64);
        cache_recent(state, camera, key, &held).unwrap();
        assert!(state.recent.lock().unwrap().is_empty());
        drop(held);
        assert_eq!(camera.usage_snapshot().unwrap().0, 0);
    });
}

#[test]
fn late_follower_cannot_replace_newer_image_or_revive_a_closed_cache() {
    with_state(|state, camera| {
        let key = (13, 100);
        state
            .consumers
            .lock()
            .unwrap()
            .try_insert(camera, key, 1, 4)
            .unwrap();
        cache_recent(state, camera, key, &raw(camera, 2)).unwrap();
        cache_recent(state, camera, key, &raw(camera, 1)).unwrap();
        assert_eq!(state.recent.lock().unwrap()[&key].meta.frame_id, 2);
        state.request_close();
        cache_recent(state, camera, key, &raw(camera, 3)).unwrap();
        assert!(state.recent.lock().unwrap().is_empty());
    });
}

#[test]
fn retiring_last_consumer_serializes_with_inflight_cache_submission() {
    with_state(|state, camera| {
        for index in 0..100 {
            let key = (14, index);
            state
                .consumers
                .lock()
                .unwrap()
                .try_insert(camera, key, 1, 4)
                .unwrap();
            let state2 = state.clone();
            let camera2 = camera.clone();
            let frame = raw(camera, index as u64 + 1);
            let gate = Arc::new(std::sync::Barrier::new(2));
            let gate2 = gate.clone();
            let worker = std::thread::spawn(move || {
                gate2.wait();
                cache_recent(&state2, &camera2, key, &frame).unwrap();
            });
            gate.wait();
            state.remove_consumer(key);
            worker.join().unwrap();
            assert!(state.recent.lock().unwrap().is_empty());
            assert_eq!(camera.usage_snapshot().unwrap().0, 0);
        }
    });
}
