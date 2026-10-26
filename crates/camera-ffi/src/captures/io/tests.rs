use super::*;

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
        state.consumers.lock().unwrap().insert(steady, 1);
        cache_recent(state, steady, &raw(camera, 1)).unwrap();
        for index in 0..100 {
            let changing = (11, 900 - index);
            state.consumers.lock().unwrap().insert(changing, 1);
            cache_recent(state, changing, &raw(camera, index as u64 + 2)).unwrap();
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
        state.consumers.lock().unwrap().insert(key, 2);
        let held = raw(camera, 7);
        cache_recent(state, key, &held).unwrap();
        state.remove_consumer(key);
        assert!(Arc::ptr_eq(&state.recent.lock().unwrap()[&key], &held));
        state.remove_consumer(key);
        assert!(state.recent.lock().unwrap().is_empty());
        assert_eq!(held.encoded, vec![7; 64]);
        assert_eq!(camera.usage_snapshot().unwrap().0, 64);
        cache_recent(state, key, &held).unwrap();
        assert!(state.recent.lock().unwrap().is_empty());
        drop(held);
        assert_eq!(camera.usage_snapshot().unwrap().0, 0);
    });
}

#[test]
fn late_follower_cannot_replace_newer_image_or_revive_a_closed_cache() {
    with_state(|state, camera| {
        let key = (13, 100);
        state.consumers.lock().unwrap().insert(key, 1);
        cache_recent(state, key, &raw(camera, 2)).unwrap();
        cache_recent(state, key, &raw(camera, 1)).unwrap();
        assert_eq!(state.recent.lock().unwrap()[&key].meta.frame_id, 2);
        state.request_close();
        cache_recent(state, key, &raw(camera, 3)).unwrap();
        assert!(state.recent.lock().unwrap().is_empty());
    });
}

#[test]
fn retiring_last_consumer_serializes_with_inflight_cache_submission() {
    with_state(|state, camera| {
        for index in 0..100 {
            let key = (14, index);
            state.consumers.lock().unwrap().insert(key, 1);
            let state2 = state.clone();
            let frame = raw(camera, index as u64 + 1);
            let gate = Arc::new(std::sync::Barrier::new(2));
            let gate2 = gate.clone();
            let worker = std::thread::spawn(move || {
                gate2.wait();
                cache_recent(&state2, key, &frame).unwrap();
            });
            gate.wait();
            state.remove_consumer(key);
            worker.join().unwrap();
            assert!(state.recent.lock().unwrap().is_empty());
            assert_eq!(camera.usage_snapshot().unwrap().0, 0);
        }
    });
}
