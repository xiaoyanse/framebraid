use super::*;
#[test]
fn borrowed_strings_validate_without_owning_or_scanning_past_limit() {
    let good = c"unsupported:test";
    let actual = unsafe { planning::borrowed(good.as_ptr()) }.unwrap();
    assert_eq!(actual.as_ptr(), good.as_ptr().cast::<u8>());
    assert!(matches!(
        unsafe { planning::borrowed(std::ptr::null()) },
        Err(AM_ERR_SDK_NULL_ARGUMENT)
    ));
    let invalid = [0xffu8, 0];
    assert!(matches!(
        unsafe { planning::borrowed(invalid.as_ptr().cast()) },
        Err(AM_ERR_SDK_UTF8_INVALID)
    ));
    let unterminated = [b'x'; SDK_MAX_STRING_BYTES as usize];
    assert!(matches!(
        unsafe { planning::borrowed(unterminated.as_ptr().cast()) },
        Err(AM_ERR_SDK_INVALID_ARGUMENT)
    ));
}
#[test]
fn invalid_context_is_rejected_before_request_string_access() {
    let mut output = 1usize as *mut am_probe_result_t;
    let code = unsafe { amc_probe_streams(std::ptr::null_mut(), std::ptr::null(), &mut output) };
    assert_eq!(code, AM_ERR_SDK_HANDLE_INVALID);
    assert!(output.is_null());
}
#[test]
fn result_reservation_contains_every_owned_array_and_survives_until_release() {
    let (domain, state) = crate::frames::isolated_test_state();
    let names = [c"unsupported:a".as_ptr(), c"unsupported:b".as_ptr()];
    let request = am_probe_request_t {
        candidate_count: 2,
        references_utf8: names.as_ptr(),
        ..Default::default()
    };
    let plan = unsafe { planning::Plan::read(&request) }.unwrap();
    let expected = size_of::<ProbeResult>()
        + 2 * size_of::<usize>()
        + size_of::<am_probe_result_t>()
        + 2 * (size_of::<CString>() + size_of::<am_probe_candidate_t>())
        + 2 * "unsupported:a".len()
        + 2;
    assert_eq!(plan.result_bytes(), expected);
    let context = state.ctx.data::<Context>().unwrap();
    context.queues.lock().unwrap()[5].max_bytes = expected as u64 - 1;
    assert!(matches!(
        state.control(plan.reservation(), true),
        Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
    ));
    assert_eq!(state.control_usage_snapshot().unwrap(), (0, 0));
    context.queues.lock().unwrap()[5].max_bytes = expected as u64;
    let charge = state.control(plan.reservation(), true).unwrap();
    let references = plan.references().unwrap();
    let rows = planning::exact_vec::<am_probe_candidate_t>(2).unwrap();
    let body = Arc::new(ProbeResult {
        abi: Box::default(),
        _rows: rows,
        _references: references,
        _charge: charge,
    });
    let key = (&*body.abi as *const am_probe_result_t) as usize;
    domain.buffers.insert(key, body).unwrap();
    assert_eq!(state.control_usage_snapshot().unwrap(), (1, expected));
    assert_eq!(context.buffers.load(Ordering::Acquire), 1);
    domain.buffers.release(key).unwrap();
    assert_eq!(state.control_usage_snapshot().unwrap(), (0, 0));
    assert_eq!(context.buffers.load(Ordering::Acquire), 0);
}
#[test]
fn cache_storage_is_really_released_to_prioritize_result() {
    let (_domain, state) = crate::frames::isolated_test_state();
    let cache = Cache::new();
    cache
        .put(
            &state,
            0,
            "unsupported:test",
            [0; 2],
            Observation::failed(AM_PROBE_UNSUPPORTED),
        )
        .unwrap();
    let cached_bytes = state.control_usage_snapshot().unwrap().1;
    assert!(cached_bytes > 0);
    state.ctx.data::<Context>().unwrap().queues.lock().unwrap()[5].max_bytes = cached_bytes as u64;
    let charge = planning::reserve(&state, &cache, cached_bytes).unwrap();
    assert!(cache.get("unsupported:test", [0; 2]).unwrap().is_none());
    assert_eq!(state.control_usage_snapshot().unwrap(), (1, cached_bytes));
    drop(charge);
    assert_eq!(state.control_usage_snapshot().unwrap(), (0, 0));
}
