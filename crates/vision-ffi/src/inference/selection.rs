//! 有界选择诊断独立于ORT会话锁；旧完成不能覆盖新context尝试。
use super::{
    lock,
    models::ModelHandle,
    state::{ControlCharge, State},
};
use am_sdk_support::{abi::*, args, context::Context, status, Result};
use am_vision_core::sdk::selection::{
    BackendSelectionPolicy, BackendSelectionReport, BackendTarget,
};
use std::{
    any::TypeId,
    ffi::c_char,
    io::Write,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

const CACHE_BYTES: usize = 8192;
const QUERY_BYTES: usize = 16384;
static NEXT: AtomicU64 = AtomicU64::new(1);
#[derive(Default)]
struct Slot(Mutex<Option<Arc<Cached>>>);
struct Data {
    id: u64,
    state: &'static str,
    outcome: i32,
    observed: i64,
    report: BackendSelectionReport,
}
pub(super) struct Cached {
    data: Mutex<Data>,
    _charge: ControlCharge,
}
pub(super) struct Attempt {
    pub id: u64,
    cache: Arc<Cached>,
}
fn slot(context: &Context) -> Result<Arc<Slot>> {
    let mut services = lock(&context.services)?;
    services
        .get_or_insert_with(TypeId::of::<Slot>(), || Arc::new(Slot::default()))?
        .downcast()
        .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)
}
pub(super) unsafe fn policy(p: *const am_backend_selection_t) -> Result<BackendSelectionPolicy> {
    let p = unsafe { args::read(p, std::mem::size_of::<am_backend_selection_t>()) }?;
    if p.candidate_count == 0 || p.candidate_count > 3 || p.allow_fallback > 1 {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    args::pointer(p.candidates)?;
    let priority = unsafe { std::slice::from_raw_parts(p.candidates, p.candidate_count as usize) }
        .iter()
        .map(|p| BackendTarget {
            backend: p.backend,
            device_id: p.device_id,
        })
        .collect();
    let result = BackendSelectionPolicy {
        priority,
        allow_fallback: p.allow_fallback != 0,
        budget_ms: p.budget_ms,
    };
    result.validate().map_err(super::error)?;
    Ok(result)
}
impl Attempt {
    pub fn begin(ctx: usize, policy: &BackendSelectionPolicy) -> Result<Self> {
        let entry = crate::domain().active(ctx)?;
        let context = entry.data::<Context>()?;
        let state = State::get(ctx)?;
        let charge = state.cached_control(CACHE_BYTES)?;
        let id = NEXT
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        let cache = Arc::new(Cached {
            data: Mutex::new(Data {
                id,
                state: "pending",
                outcome: AM_OK as i32,
                observed: am_sdk_support::clock::monotonic_ms(),
                report: BackendSelectionReport {
                    candidates: policy.priority.clone(),
                    allow_fallback: policy.allow_fallback,
                    budget_ms: Some(policy.budget_ms),
                    ..Default::default()
                },
            }),
            _charge: charge,
        });
        // ID在发布锁内比较，较慢的旧开始也不能把新槽位倒退。
        let slot = slot(context)?;
        let mut latest = lock(&slot.0)?;
        if latest
            .as_ref()
            .is_none_or(|v| lock(&v.data).is_ok_and(|v| v.id < id))
        {
            *latest = Some(cache.clone());
        }
        Ok(Self { id, cache })
    }
    pub fn record(&self, report: &BackendSelectionReport) -> Result<()> {
        lock(&self.cache.data)?.report = report.clone();
        Ok(())
    }
    pub fn finish(&self, outcome: i32) -> Result<()> {
        let mut data = lock(&self.cache.data)?;
        data.outcome = outcome;
        data.state = if outcome == AM_OK as i32 {
            "ready"
        } else {
            "failed"
        };
        data.observed = am_sdk_support::clock::monotonic_ms();
        Ok(())
    }
    pub fn model_cache(&self, state: &State) -> Result<Arc<Cached>> {
        let charge = state.cached_control(CACHE_BYTES)?;
        let data = lock(&self.cache.data)?;
        Ok(Arc::new(Cached {
            data: Mutex::new(Data {
                id: self.id,
                state: "ready",
                outcome: AM_OK as i32,
                observed: am_sdk_support::clock::monotonic_ms(),
                report: data.report.clone(),
            }),
            _charge: charge,
        }))
    }
}
impl Cached {
    pub fn refresh(&self, report: &BackendSelectionReport) {
        if let Ok(mut data) = self.data.lock() {
            let target = &mut data.report;
            // 每次Run只复制数值；不在热路径clone候选、错误字符串或分配新快照。
            target.native_runs = report.native_runs;
            target.delivered_runs = report.delivered_runs;
            target.native_failures = report.native_failures;
            target.last_native_run_succeeded = report.last_native_run_succeeded;
            target.last_native_error_code = report.last_native_error_code;
            target.placement_verified = report.placement_verified;
            data.observed = am_sdk_support::clock::monotonic_ms();
        }
    }
}
struct Bounded(Vec<u8>);
impl Write for Bounded {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > CACHE_BYTES - 1 - self.0.len() {
            return Err(std::io::Error::other("bounded selection report"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[derive(serde::Serialize)]
struct View<'a> {
    schema: &'static str,
    state: &'a str,
    attempt_id: u64,
    outcome_status: i32,
    observed_at_ms: Option<i64>,
    selection: Option<&'a BackendSelectionReport>,
}
unsafe fn query(
    state: &State,
    cache: Option<&Cached>,
    empty: &str,
    buffer: *mut c_char,
    capacity: u64,
    required: *mut u64,
) -> Result<()> {
    if capacity > 65536 {
        return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
    }
    let _charge = state.cached_control(QUERY_BYTES)?;
    let data = cache.map(|c| lock(&c.data)).transpose()?;
    let view = View {
        schema: "framebraid.inference-selection/1",
        state: data.as_ref().map_or(empty, |d| d.state),
        attempt_id: data.as_ref().map_or(0, |d| d.id),
        outcome_status: data.as_ref().map_or(AM_OK as i32, |d| d.outcome),
        observed_at_ms: data.as_ref().map(|d| d.observed),
        selection: data.as_ref().map(|d| &d.report),
    };
    let mut bytes = Bounded(Vec::with_capacity(CACHE_BYTES));
    serde_json::to_writer(&mut bytes, &view).map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
    unsafe { super::hardware::copy_report(&bytes.0, buffer, capacity, required) }
}
unsafe fn inactive(
    lifecycle: u32,
    buffer: *mut c_char,
    capacity: u64,
    required: *mut u64,
) -> Result<()> {
    if capacity > 65536 {
        return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
    }
    let bytes = if lifecycle == AM_LIFECYCLE_CLOSED as u32 {
        br#"{"schema":"framebraid.inference-selection/1","state":"closed","attempt_id":0,"outcome_status":0,"observed_at_ms":null,"selection":null}"#.as_slice()
    } else {
        br#"{"schema":"framebraid.inference-selection/1","state":"closing","attempt_id":0,"outcome_status":0,"observed_at_ms":null,"selection":null}"#.as_slice()
    };
    unsafe { super::hardware::copy_report(bytes, buffer, capacity, required) }
}
#[no_mangle]
pub unsafe extern "C" fn amv_get_model_selection_report(
    model: *const amv_model_t,
    buffer: *mut c_char,
    capacity: u64,
    required: *mut u64,
) -> i32 {
    status(crate::domain(), model as usize, || {
        let entry = crate::domain().get(model as usize)?;
        let model = entry.data::<ModelHandle>()?;
        let read = match entry.readable() {
            Ok(v) => Some(v),
            Err(AM_ERR_SDK_HANDLE_CLOSED) => None,
            Err(code) => return Err(code),
        };
        let lifecycle = entry.state.load(Ordering::Acquire);
        if lifecycle != AM_LIFECYCLE_RUNNING as u32 {
            return unsafe { inactive(lifecycle, buffer, capacity, required) };
        }
        let _read = read.ok_or(AM_ERR_SDK_HANDLE_CLOSED)?;
        let cache = lock(&model.selection)?;
        unsafe {
            query(
                &model.state,
                cache.as_deref(),
                "legacy_model",
                buffer,
                capacity,
                required,
            )
        }
    })
}
#[no_mangle]
pub unsafe extern "C" fn amv_get_selection_attempt_report(
    ctx: *mut amv_context_t,
    id: u64,
    buffer: *mut c_char,
    capacity: u64,
    required: *mut u64,
) -> i32 {
    status(crate::domain(), ctx as usize, || {
        let entry = crate::domain().get(ctx as usize)?;
        let context = entry.data::<Context>()?;
        let read = match entry.readable() {
            Ok(v) => Some(v),
            Err(AM_ERR_SDK_HANDLE_CLOSED) => None,
            Err(code) => return Err(code),
        };
        let lifecycle = entry.state.load(Ordering::Acquire);
        if lifecycle != AM_LIFECYCLE_RUNNING as u32 {
            return unsafe { inactive(lifecycle, buffer, capacity, required) };
        }
        let _read = read.ok_or(AM_ERR_SDK_HANDLE_CLOSED)?;
        let cache = lock(&slot(context)?.0)?
            .clone()
            .ok_or(AM_ERR_SDK_RESULT_EXPIRED)?;
        if lock(&cache.data)?.id != id {
            return Err(AM_ERR_SDK_RESULT_EXPIRED);
        }
        unsafe {
            query(
                State::get(ctx as usize)?.as_ref(),
                Some(&cache),
                "failed",
                buffer,
                capacity,
                required,
            )
        }
    })
}

#[cfg(test)]
#[test]
fn fixed_foundation_layout_snapshot() {
    println!("r3_selection_foundation slot={} cached={} data={} cache_reservation={} query_reservation={} arc_header_bytes=16",std::mem::size_of::<Slot>(),std::mem::size_of::<Cached>(),std::mem::size_of::<Data>(),CACHE_BYTES,QUERY_BYTES);
}
