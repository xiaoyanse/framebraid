use super::*;

#[test]
fn internal_workspace_splits_without_recharge_and_shrinks_after_real_release() {
    let (domain, state) = isolated_test_state();
    let key = state.ctx_key;
    let mut workspace = state.charge(4096, false).unwrap();
    let retained = workspace.split_internal(128).unwrap();
    let packet = workspace.split_internal(1024).unwrap();
    assert_eq!(state.usage_snapshot().unwrap(), (4096, 0, 0));
    assert!(matches!(
        workspace.split_internal(4096),
        Err(AM_ERR_SDK_INVALID_ARGUMENT)
    ));
    assert_eq!(workspace.bytes(), 2944);
    workspace.shrink_to(0).unwrap();
    assert_eq!(state.usage_snapshot().unwrap(), (1152, 0, 0));
    drop(packet);
    assert_eq!(state.usage_snapshot().unwrap(), (128, 0, 0));
    drop((retained, workspace));
    assert_eq!(state.usage_snapshot().unwrap(), (0, 0, 0));
    let mut external = state.charge(64, true).unwrap();
    assert!(external.split_internal(1).is_err());
    assert!(external.shrink_to(0).is_err());
    drop(external);
    drop(state);
    assert_eq!(domain.close(key, 100).unwrap(), AM_LIFECYCLE_CLOSED as u32);
    domain.destroy(key).unwrap();
}

#[test]
fn recovery_reservation_transfer_enforces_external_count_and_releases_total_once() {
    let params = amc_init_params_t {
        abi_version: AM_ABI_VERSION,
        ..Default::default()
    };
    // 独立域避免预算单测挤占其他并行API测试的进程域context上限。
    let local = am_sdk_support::handles::Domain::new(false);
    let key = unsafe { am_sdk_support::context::initialize_camera(&local, &params) }.unwrap();
    let state = Arc::new(CameraState {
        ctx_key: key,
        ctx: local.get(key).unwrap(),
        metrics: Arc::default(),
        usage: Arc::default(),
    });
    let context = state.ctx.data::<Context>().unwrap();
    context.limits.lock().unwrap().external_hold_max_frames = 1;
    let mut reservation = state.charge(4096, false).unwrap();
    let first = reservation.take_external(1024).unwrap();
    assert_eq!(state.usage_snapshot().unwrap(), (4096, 1024, 1));
    assert!(matches!(
        reservation.take_external(1024),
        Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
    ));
    assert_eq!(reservation.bytes(), 3072);
    assert_eq!(context.buffers.load(Ordering::Acquire), 1);
    drop(first);
    let second = reservation.take_external(2048).unwrap();
    drop(reservation);
    assert_eq!(state.usage_snapshot().unwrap(), (2048, 2048, 1));
    drop(second);
    assert_eq!(state.usage_snapshot().unwrap(), (0, 0, 0));
    assert_eq!(context.buffers.load(Ordering::Acquire), 0);
    drop(state);
    assert_eq!(local.close(key, 100).unwrap(), AM_LIFECYCLE_CLOSED as u32);
    assert_eq!(local.destroy(key), Ok(()));
}
