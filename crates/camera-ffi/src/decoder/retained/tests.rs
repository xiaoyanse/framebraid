use super::*;

fn config(bytes: &[u8]) -> am_decoder_config_t {
    am_decoder_config_t {
        codec: AM_CODEC_H264,
        bitstream_format: AM_BSF_ANNEXB,
        hw_policy: AM_DHW_SOFTWARE_ONLY as u32,
        extradata: bytes.as_ptr(),
        extradata_size: bytes.len() as u64,
        ..Default::default()
    }
}

#[test]
fn configuration_admission_and_shared_ownership_follow_actual_storage() {
    let (_domain, state) = crate::frames::isolated_test_state();
    let mut data = vec![0xff; 1024];
    data[..4].copy_from_slice(&[0, 0, 1, 0x0c]);
    let ctx = state.ctx.data::<Context>().unwrap();
    ctx.limits.lock().unwrap().frame_budget_bytes = 1023;
    assert!(matches!(
        unsafe { OwnedConfig::read(&state, &config(&data)) },
        Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
    ));
    assert_eq!(state.usage_snapshot().unwrap().0, 0);
    // 仅改变隔离测试额度，不提高公开set_limits上限。
    ctx.limits.lock().unwrap().frame_budget_bytes = 1024;
    let original = unsafe { OwnedConfig::read(&state, &config(&data)) }.unwrap();
    let mut references = Vec::new();
    for _ in 0..100 {
        references.push(original.clone());
    }
    assert!(references.iter().all(|p| Arc::ptr_eq(p, &original)));
    assert_eq!(state.usage_snapshot().unwrap().0, 1024);
    assert!(original.abi.extradata.is_null());
    drop(original);
    assert_eq!(state.usage_snapshot().unwrap().0, 1024);
    drop(references);
    assert_eq!(state.usage_snapshot().unwrap().0, 0);
}

#[test]
fn parameter_rejection_is_atomic_and_serialization_keeps_its_own_credit() {
    let (_domain, state) = crate::frames::isolated_test_state();
    let raw = [0, 0, 1, 0x67, 1, 0, 0, 1, 0x68, 2];
    let cfg = Config {
        abi: config(&raw),
        codec: VideoCodec::H264,
        extra: raw.to_vec(),
    };
    let ctx = state.ctx.data::<Context>().unwrap();
    ctx.limits.lock().unwrap().frame_budget_bytes = 10;
    let mut gate = BudgetedGate::new(&state, &cfg).unwrap();
    gate.mark_started();
    gate.clear_dirty();
    let before = gate.gate.extra();
    let changed = [0, 0, 1, 0x67, 9, 9, 9, 9, 9, 9, 9];
    assert!(matches!(
        gate.observe(VideoCodec::H264, &changed),
        Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
    ));
    assert_eq!(gate.gate.extra(), before);
    assert!(gate.started && !gate.dirty);
    assert_eq!(state.usage_snapshot().unwrap().0, 4);
    ctx.limits.lock().unwrap().frame_budget_bytes = 14;
    assert!(!gate.observe(VideoCodec::H264, &changed).unwrap());
    assert!(gate.dirty && !gate.started);
    assert_eq!(state.usage_snapshot().unwrap().0, 10);
    assert!(matches!(gate.extra(), Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)));
    ctx.limits.lock().unwrap().frame_budget_bytes = 28;
    let serialized = gate.extra().unwrap();
    assert_eq!(serialized.len(), 18);
    assert_eq!(state.usage_snapshot().unwrap().0, 28);
    drop(serialized);
    assert_eq!(state.usage_snapshot().unwrap().0, 10);
    drop(gate);
    assert_eq!(state.usage_snapshot().unwrap().0, 0);
}

#[test]
fn malformed_tail_never_commits_the_earlier_parameter_candidate() {
    let mut gate = Gate::default();
    gate.observe(VideoCodec::H264, &[0, 0, 1, 0x67, 1]).unwrap();
    gate.dirty = false;
    gate.started = true;
    let old = gate.extra();
    assert!(gate
        .observe(VideoCodec::H264, &[0, 0, 1, 0x67, 2, 0, 0, 1, 0xff])
        .is_err());
    assert_eq!(gate.extra(), old);
    assert!(!gate.dirty && gate.started);
    // 同包先变后还原仍要等待IDR，保留原先保守的参考链语义。
    gate.observe(VideoCodec::H264, &[0, 0, 1, 0x67, 2, 0, 0, 1, 0x67, 1])
        .unwrap();
    assert_eq!(gate.extra(), old);
    assert!(gate.dirty && !gate.started);
}
