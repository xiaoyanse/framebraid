//! 审计盲区回归：amc_capture_async 记录满拒绝与spawn失败回收，失败不得残留请求记录。
use super::*;
use crate::sources::{self, SharedSource};

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
        let ctx = unsafe { am_sdk_support::context::initialize_camera(crate::domain(), &params) }
            .unwrap();
        Self {
            ctx,
            sources: Vec::new(),
        }
    }
    fn open_isapi(&mut self) -> usize {
        let desc = am_source_desc_t {
            kind: AM_SRC_ISAPI_URL as u32,
            url_utf8: c"http://127.0.0.1:9/ISAPI/Streaming/channels/101/picture".as_ptr(),
            ..Default::default()
        };
        let mut out = std::ptr::null_mut();
        assert_eq!(
            unsafe { crate::sources::api::amc_source_open(self.ctx as *mut _, &desc, &mut out) },
            AM_OK as i32
        );
        self.sources.push(out as usize);
        out as usize
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for source in self.sources.drain(..) {
            crate::domain().close(source, 2000).unwrap();
            crate::domain().destroy(source).unwrap();
        }
        crate::domain().close(self.ctx, 2000).unwrap();
        crate::domain().destroy(self.ctx).unwrap();
    }
}

fn capture_state(source: usize) -> (Arc<SharedSource>, Arc<state::State>) {
    let entry = sources::get(source).unwrap();
    let shared = entry
        .data::<crate::sources::Source>()
        .unwrap()
        .shared
        .clone();
    drop(entry);
    let state = state::State::get(&shared.state).unwrap();
    (shared, state)
}

fn request() -> am_capture_request_t {
    am_capture_request_t {
        deadline_ms: 1000,
        ..Default::default()
    }
}

#[test]
fn records_full_rejects_queue_full_without_touching_existing_records() {
    let mut f = Fixture::new();
    let source = f.open_isapi();
    let (shared, state) = capture_state(source);
    let mut policy = am_queue_policy_t::default();
    assert_eq!(
        unsafe {
            crate::amc_get_queue_policy(
                f.ctx as *mut amc_context_t,
                AM_Q_RESULTS as u32,
                &mut policy,
            )
        },
        AM_OK as i32
    );
    policy.max_items = 2;
    assert_eq!(
        unsafe {
            crate::amc_set_queue_policy(f.ctx as *mut amc_context_t, AM_Q_RESULTS as u32, &policy)
        },
        AM_OK as i32
    );
    {
        let mut records = state.records.lock().unwrap();
        for _ in 0..2 {
            let id = state.id().unwrap();
            records
                .try_insert(
                    &shared.state,
                    id,
                    state::Record {
                        state: AM_REQ_DONE as u32,
                        code: 0,
                        created: monotonic_ms(),
                        finished: monotonic_ms(),
                        expires: i64::MAX,
                        result: None,
                        _charge: shared.state.control(0, false).unwrap(),
                    },
                    256,
                )
                .unwrap();
        }
    }
    let mut out = 0u64;
    assert_eq!(
        unsafe { requests::amc_capture_async(source as *mut _, &request(), &mut out) },
        AM_ERR_SDK_QUEUE_FULL,
        "记录已满必须以QUEUE_FULL拒绝"
    );
    assert_eq!(out, 0);
    assert_eq!(
        state.records.lock().unwrap().len(),
        2,
        "拒绝路径不得增删既有记录"
    );
}

#[test]
fn spawn_failure_recovers_the_record_without_ghost_pending() {
    let mut f = Fixture::new();
    let source = f.open_isapi();
    let (_shared, state) = capture_state(source);
    assert_eq!(
        unsafe { crate::threads::amc_set_worker_budget(f.ctx as *mut amc_context_t, 0) },
        AM_OK as i32
    );
    for _ in 0..3 {
        let mut out = 0u64;
        assert_eq!(
            unsafe { requests::amc_capture_async(source as *mut _, &request(), &mut out) },
            AM_ERR_SDK_RESOURCE_EXHAUSTED
        );
        assert_eq!(out, 0);
        assert!(
            state.records.lock().unwrap().is_empty(),
            "spawn失败必须回收记录，不得残留幽灵PENDING占队列配额"
        );
    }
}
