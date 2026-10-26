use super::{api::*, *};
use std::time::{Duration, Instant};

#[test]
fn source_decode_policy_is_shared_frozen_on_push_subscription_and_closed_report_is_idle() {
    let mut f = Fixture::new();
    let desc = am_source_desc_t {
        kind: AM_SRC_RTSP_URL as u32,
        url_utf8: c"rtsp://127.0.0.1:9/isolated".as_ptr(),
        reuse: 1,
        ..Default::default()
    };
    let mut first = std::ptr::null_mut();
    assert_eq!(unsafe { amc_source_open(f.ctx as _, &desc, &mut first) }, 0);
    f.sources.push(first as usize);
    let set = hardware::amc_source_set_decode_policy;
    assert_eq!(unsafe { set(first, AM_DHW_SOFTWARE_ONLY as u32) }, 0);
    let mut second = std::ptr::null_mut();
    assert_eq!(
        unsafe { amc_source_open(f.ctx as _, &desc, &mut second) },
        0
    );
    f.sources.push(second as usize);
    assert_eq!(
        unsafe { set(first, AM_DHW_PREFER_HW as u32) },
        AM_ERR_CAMERA_MODE_CONFLICT
    );
    assert_eq!(unsafe { set(second, AM_DHW_SOFTWARE_ONLY as u32) }, 0);
    let mut lifecycle = 0;
    assert_eq!(unsafe { amc_source_close(second, 100, &mut lifecycle) }, 0);
    assert_eq!(unsafe { set(first, AM_DHW_PREFER_HW as u32) }, 0);
    let subscription = am_frame_sub_desc_t {
        pixel_format: AM_PF_RGBA8 as u32,
        mode: AM_SUB_PUSH,
        ..Default::default()
    };
    let mut sub = std::ptr::null_mut();
    assert_eq!(
        unsafe { crate::subscriptions::amc_subscribe_frames(first, &subscription, &mut sub) },
        0
    );
    assert_eq!(
        unsafe { set(first, AM_DHW_SOFTWARE_ONLY as u32) },
        AM_ERR_CAMERA_MODE_CONFLICT,
        "尚无回调的push订阅也必须冻结策略"
    );
    let report = hardware::report(first as usize).unwrap();
    assert_eq!(report["policy_locked"], true);
    assert_eq!(report["actual_backend"], serde_json::Value::Null);
    let closed = hardware::report(second as usize).unwrap();
    assert_eq!(closed["state"], "closed");
    assert_eq!(closed["selection"], serde_json::Value::Null);
    assert_eq!(closed["decoder_active"], false);
    assert_eq!(
        unsafe { crate::subscriptions::amc_frame_unsubscribe(sub, 100, &mut lifecycle) },
        0
    );
    domain().destroy(sub as usize).unwrap();
    assert_eq!(
        unsafe { set(first, AM_DHW_SOFTWARE_ONLY as u32) },
        AM_ERR_CAMERA_MODE_CONFLICT,
        "撤销订阅后不隐式解锁"
    );
    let isapi = f.open(std::ptr::null(), std::ptr::null(), 0);
    assert_eq!(
        unsafe { set(isapi as _, AM_DHW_PREFER_HW as u32) },
        AM_ERR_CAMERA_MODE_CONFLICT
    );
    get(first as usize)
        .unwrap()
        .data::<Source>()
        .unwrap()
        .request_close();
    let closing = hardware::report(first as usize).unwrap();
    assert_eq!(closing["state"], "closing", "排空前不能报告closed");
    assert_eq!(closing["decoder_active"], false);
    let _callback = am_sdk_support::callbacks::Scope::enter();
    assert_eq!(
        unsafe { set(first, AM_DHW_PREFER_HW as u32) },
        AM_ERR_SDK_CALLBACK_CONTRACT_VIOLATION
    );
}

struct Fixture {
    ctx: usize,
    sources: Vec<usize>,
}
impl Fixture {
    fn new() -> Self {
        let params = amc_init_params_t {
            abi_version: AM_ABI_VERSION,
            ..Default::default()
        };
        let ctx = unsafe { am_sdk_support::context::initialize_camera(domain(), &params) }.unwrap();
        Self {
            ctx,
            sources: Vec::new(),
        }
    }
    fn open(
        &mut self,
        creds: *const am_credential_data_t,
        provider: *const am_credential_provider_t,
        reuse: u32,
    ) -> usize {
        let desc = am_source_desc_t {
            kind: AM_SRC_ISAPI_URL as u32,
            url_utf8: c"http://127.0.0.1:9/ISAPI/Streaming/channels/101/picture".as_ptr(),
            creds,
            provider,
            reuse,
            ..Default::default()
        };
        let mut out = std::ptr::null_mut();
        assert_eq!(
            unsafe { amc_source_open(self.ctx as *mut _, &desc, &mut out) },
            AM_OK as i32
        );
        self.sources.push(out as usize);
        out as usize
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for source in self.sources.drain(..) {
            domain().close(source, 2000).unwrap();
            domain().destroy(source).unwrap();
        }
        domain().close(self.ctx, 2000).unwrap();
        domain().destroy(self.ctx).unwrap();
    }
}
fn status_of(source: usize) -> am_source_status_t {
    let mut out = am_source_status_t::default();
    assert_eq!(
        unsafe { amc_source_get_status(source as *mut _, &mut out) },
        AM_OK as i32
    );
    out
}
fn wait(mut check: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(2);
    while !check() {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn source_waiting_reuse_rotation_and_close_keep_consumers_independent() {
    let mut f = Fixture::new();
    let a = f.open(std::ptr::null(), std::ptr::null(), 1);
    assert_eq!(status_of(a).state, AM_SRC_WAITING_CREDENTIALS);
    let b = f.open(std::ptr::null(), std::ptr::null(), 1);
    assert_eq!(status_of(a).source_id, status_of(b).source_id);
    let creds = am_credential_data_t {
        username_utf8: c"test".as_ptr(),
        password_utf8: c"only-isolated-test".as_ptr(),
        ..Default::default()
    };
    assert_eq!(
        unsafe { amc_credentials_set(a as *mut _, &creds) },
        AM_OK as i32
    );
    assert_eq!(status_of(b).generation, 2);
    let c = f.open(std::ptr::null(), std::ptr::null(), 1);
    assert_ne!(status_of(c).source_id, status_of(b).source_id);
    let d = f.open(&creds, std::ptr::null(), 1);
    assert_eq!(status_of(d).source_id, status_of(b).source_id);
    let mut lifecycle = 99;
    assert_eq!(
        unsafe { amc_source_close(a as *mut _, 0, &mut lifecycle) },
        AM_OK as i32
    );
    assert_eq!(lifecycle, AM_LIFECYCLE_CLOSED as u32);
    assert_eq!(status_of(a).state, AM_SRC_CLOSED);
    assert_eq!(status_of(b).state, AM_SRC_CONNECTING);
    let invalid = am_reconnect_policy_t {
        initial_backoff_ms: 1,
        max_backoff_ms: 60001,
        ..Default::default()
    };
    assert_eq!(
        unsafe { amc_source_set_reconnect_policy(b as *mut _, &invalid) },
        AM_ERR_SDK_LIMIT_OUT_OF_RANGE
    );
}

struct ProviderGate {
    entered: AtomicBool,
    release: AtomicBool,
    dropped: AtomicUsize,
}
unsafe extern "C" fn fetch(
    user: *mut c_void,
    known: u32,
    out: *mut am_credential_data_t,
    version: *mut u32,
) -> i32 {
    let gate = unsafe { &*(user as *const ProviderGate) };
    gate.entered.store(true, Ordering::Release);
    while !gate.release.load(Ordering::Acquire) {
        std::thread::sleep(Duration::from_millis(1));
    }
    unsafe {
        out.write(am_credential_data_t {
            username_utf8: c"test".as_ptr(),
            password_utf8: c"test".as_ptr(),
            ..Default::default()
        });
        version.write(known + 1);
    }
    AM_OK as i32
}
unsafe extern "C" fn dropped(user: *mut c_void) {
    unsafe { &*(user as *const ProviderGate) }
        .dropped
        .fetch_add(1, Ordering::AcqRel);
}
#[test]
fn provider_timeout_does_not_spawn_replacements_or_outlive_close_barrier() {
    let mut f = Fixture::new();
    let gate = Box::new(ProviderGate {
        entered: AtomicBool::new(false),
        release: AtomicBool::new(false),
        dropped: AtomicUsize::new(0),
    });
    let provider = am_credential_provider_t {
        fetch: Some(fetch),
        on_drop: Some(dropped),
        user_data: (&*gate as *const ProviderGate).cast_mut().cast(),
        fetch_timeout_ms: 5,
        ..Default::default()
    };
    let source = f.open(std::ptr::null(), &provider, 0);
    wait(|| gate.entered.load(Ordering::Acquire));
    std::thread::sleep(Duration::from_millis(10));
    assert_eq!(status_of(source).state, AM_SRC_AUTH_WAITING);
    assert_eq!(
        get(source)
            .unwrap()
            .data::<Source>()
            .unwrap()
            .shared
            .state_code
            .load(Ordering::Acquire),
        AM_SRC_CONNECTING as u32
    );
    assert_eq!(gate.dropped.load(Ordering::Acquire), 0);
    let mut lifecycle = 99;
    assert_eq!(
        unsafe { amc_source_close(source as *mut _, 0, &mut lifecycle) },
        AM_OK as i32
    );
    assert_eq!(lifecycle, AM_LIFECYCLE_CLOSING as u32);
    gate.release.store(true, Ordering::Release);
    wait(|| domain().state(source).unwrap() == AM_LIFECYCLE_CLOSED as u32);
    assert_eq!(gate.dropped.load(Ordering::Acquire), 1);
}

#[test]
fn shared_claim_never_resurrects_last_closed_owner() {
    let credentials = Credentials::default();
    let owners = AtomicUsize::new(1);
    let stop = AtomicBool::new(false);
    assert!(credentials.claim(None, None, &owners, &stop));
    assert_eq!(owners.load(Ordering::Acquire), 2);
    owners.store(0, Ordering::Release);
    assert!(!credentials.claim(None, None, &owners, &stop));
    assert_eq!(owners.load(Ordering::Acquire), 0);
}

#[test]
fn final_auth_rejection_invalidates_static_credentials_until_explicit_update() {
    let mut f = Fixture::new();
    let creds = am_credential_data_t {
        username_utf8: c"test".as_ptr(),
        password_utf8: c"test".as_ptr(),
        ..Default::default()
    };
    let source = f.open(&creds, std::ptr::null(), 0);
    let entry = get(source).unwrap();
    let shared = &entry.data::<Source>().unwrap().shared;
    assert!(shared.secret().is_ok());
    shared.report(
        AM_SRC_AUTH_WAITING as u32,
        AM_FAULT_AUTH as u32,
        c"camera.authenticationRejected",
    );
    assert!(matches!(
        shared.secret(),
        Err(AM_ERR_CAMERA_AUTH_FINAL_FAILED)
    ));
    assert_eq!(shared.generation.load(Ordering::Acquire), 2);
    assert_eq!(status_of(source).state, AM_SRC_AUTH_WAITING);
    drop(entry);
    assert_eq!(
        unsafe { amc_credentials_set(source as *mut _, &creds) },
        AM_OK as i32
    );
    assert!(get(source)
        .unwrap()
        .data::<Source>()
        .unwrap()
        .shared
        .secret()
        .is_ok());
}

struct EventGate {
    entered: AtomicBool,
    release: AtomicBool,
    calls: AtomicUsize,
    reentry: AtomicI64,
}
unsafe extern "C" fn event(
    src: *mut amc_source_t,
    _: i32,
    _: *const am_event_detail_t,
    user: *mut c_void,
) {
    let gate = unsafe { &*(user as *const EventGate) };
    gate.calls.fetch_add(1, Ordering::AcqRel);
    let mut state = 0;
    gate.reentry.store(
        unsafe { amc_source_close(src, 0, &mut state) } as i64,
        Ordering::Release,
    );
    let mut snapshot = am_source_status_t::default();
    assert_eq!(
        unsafe { amc_source_get_status(src, &mut snapshot) },
        AM_OK as i32
    );
    gate.entered.store(true, Ordering::Release);
    while !gate.release.load(Ordering::Acquire) {
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn source_event_unregister_waits_for_callback_and_rejects_blocking_reentry() {
    let mut f = Fixture::new();
    let source = f.open(std::ptr::null(), std::ptr::null(), 0);
    let gate = Arc::new(EventGate {
        entered: AtomicBool::new(false),
        release: AtomicBool::new(false),
        calls: AtomicUsize::new(0),
        reentry: AtomicI64::new(0),
    });
    assert_eq!(
        unsafe {
            amc_source_set_event_callback(
                source as *mut _,
                Some(event),
                Arc::as_ptr(&gate).cast_mut().cast(),
            )
        },
        AM_OK as i32
    );
    let entry = get(source).unwrap();
    entry.data::<Source>().unwrap().shared.event(
        AM_EVENT_STATE_CHANGED,
        AM_EVENT_SCOPE_SOURCE,
        0,
        AM_FAULT_NONE as u32,
        c"camera.test",
    );
    drop(entry);
    wait(|| gate.entered.load(Ordering::Acquire));
    assert_eq!(
        gate.reentry.load(Ordering::Acquire),
        AM_ERR_SDK_CALLBACK_CONTRACT_VIOLATION as i64
    );
    let completed = Arc::new(AtomicBool::new(false));
    let completed_worker = completed.clone();
    let unregister = std::thread::spawn(move || {
        assert_eq!(
            unsafe { amc_source_set_event_callback(source as *mut _, None, std::ptr::null_mut()) },
            AM_OK as i32
        );
        completed_worker.store(true, Ordering::Release);
    });
    std::thread::sleep(Duration::from_millis(10));
    assert!(!completed.load(Ordering::Acquire));
    gate.release.store(true, Ordering::Release);
    unregister.join().unwrap();
    assert!(completed.load(Ordering::Acquire));
    assert_eq!(gate.calls.load(Ordering::Acquire), 1);
}

#[test]
fn reused_source_registry_reclaims_dead_aliases_without_events() {
    let mut f = Fixture::new();
    let anchor = f.open(std::ptr::null(), std::ptr::null(), 1);
    let shared = get(anchor)
        .unwrap()
        .data::<Source>()
        .unwrap()
        .shared
        .clone();
    let mut peak = 0;
    for _ in 0..256 {
        let alias = f.open(std::ptr::null(), std::ptr::null(), 1);
        assert_eq!(status_of(alias).source_id, shared.source_id);
        peak = peak.max(shared.sources.lock().unwrap().len());
        assert_eq!(f.sources.pop(), Some(alias));
        domain().close(alias, 100).unwrap();
        domain().destroy(alias).unwrap();
    }
    eprintln!("alias_registry_peak={peak}; cycles=256");
    assert_eq!(peak, 2, "无事件的共享源不能累积已销毁句柄的Weak和Arc分配体");
    assert_eq!(status_of(anchor).state, AM_SRC_WAITING_CREDENTIALS);
}
