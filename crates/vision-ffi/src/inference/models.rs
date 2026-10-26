use super::{error, lock, scheduler, state::State};
use am_sdk_support::{abi::*, args, context::Context, handles::Managed, Result};
use am_vision_core::sdk::{Model, ModelSource, TensorInfo};
use std::{
    any::Any,
    ffi::CString,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

static MODEL_ID: AtomicU64 = AtomicU64::new(1);
pub struct ModelHandle {
    pub session: Mutex<Option<Model>>,
    pub selection: Mutex<Option<Arc<super::selection::Cached>>>,
    pub info: Info,
    pub state: Arc<State>,
    pub inflight: AtomicUsize,
    pub stopping: AtomicBool,
    pub busy: AtomicBool,
    pub policy: Mutex<am_realtime_policy_t>,
    pub permit: Mutex<Option<scheduler::ModelPermit>>,
    pub notify: Arc<super::notify::Notify>,
}
impl Managed for ModelHandle {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn request_close(&self) {
        self.stopping.store(true, Ordering::Release);
        self.notify.stop();
        if let Ok(mut records) = self.state.records.lock() {
            for r in records.values_mut() {
                if r.model == self.info.key.load(Ordering::Acquire) {
                    r.canceled = true;
                    r.result = None;
                }
            }
        }
        scheduler::wake();
    }
    fn drained(&self) -> bool {
        self.inflight.load(Ordering::Acquire) == 0 && self.notify.drained()
    }
    fn close_resources(&self) {
        self.notify.join();
        if let Ok(mut cache) = self.selection.lock() {
            cache.take();
        }
        if let Ok(mut session) = self.session.lock() {
            session.take();
        }
        // 先释放ORT会话，再归还线程配额并停止最后一组工作线程。
        if let Ok(mut permit) = self.permit.lock() {
            permit.take();
        }
    }
}
impl Drop for ModelHandle {
    fn drop(&mut self) {
        self.state.models.fetch_sub(1, Ordering::AcqRel);
    }
}
pub struct Io {
    name: CString,
    shape: Vec<i64>,
    min: Vec<i64>,
    max: Vec<i64>,
}
pub struct Info {
    pub key: AtomicUsize,
    pub uid: u64,
    pub profile: CString,
    pub mode: u32,
    pub actual: u32,
    pub device: CString,
    pub fallback_reason: Option<CString>,
    pub native_batch: u32,
    input_owners: Vec<Io>,
    output_owners: Vec<Io>,
    pub inputs: Vec<am_io_spec_t>,
    pub input_specs: Vec<TensorInfo>,
    pub outputs: Vec<am_io_spec_t>,
}
// 所有指针只指向本拥有者的堆缓冲；构造后不移动缓冲、不改元数据。
unsafe impl Send for Info {}
unsafe impl Sync for Info {}
fn ios(specs: &[TensorInfo], batch: u32) -> Result<(Vec<Io>, Vec<am_io_spec_t>)> {
    let owners = specs
        .iter()
        .map(|s| {
            Ok(Io {
                name: CString::new(s.name.clone()).map_err(|_| AM_ERR_VISION_IO_UNSUPPORTED)?,
                shape: s.shape.clone(),
                min: s
                    .shape
                    .iter()
                    .map(|v| if *v < 0 { 1 } else { *v })
                    .collect(),
                max: s
                    .shape
                    .iter()
                    .enumerate()
                    .map(|(axis, v)| {
                        if *v >= 0 {
                            *v
                        } else if axis == 0 && batch == 1 {
                            64
                        } else {
                            8192
                        }
                    })
                    .collect(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let views = owners
        .iter()
        .zip(specs)
        .map(|(o, s)| am_io_spec_t {
            name: o.name.as_ptr(),
            dtype: s.dtype,
            shape: o.shape.as_ptr(),
            shape_len: o.shape.len() as u32,
            dyn_min: o.min.as_ptr(),
            dyn_max: o.max.as_ptr(),
            ..Default::default()
        })
        .collect();
    Ok((owners, views))
}
impl Info {
    pub fn value(&self) -> am_model_info_t {
        let _ = (&self.input_owners, &self.output_owners);
        am_model_info_t {
            identity: am_model_identity_t {
                model_uid: self.uid,
                model_version: 1,
                profile_id: self.profile.as_ptr(),
            },
            load_mode: self.mode,
            input_count: self.inputs.len() as u32,
            output_count: self.outputs.len() as u32,
            inputs: self.inputs.as_ptr(),
            outputs: self.outputs.as_ptr(),
            native_batch: self.native_batch,
            execution: self.execution(),
            ..Default::default()
        }
    }
}
impl Info {
    pub fn execution(&self) -> am_backend_actual_t {
        am_backend_actual_t {
            actual: self.actual,
            device_utf8: self.device.as_ptr(),
            subgraph_cpu_nodes: if self.actual == 0 { 0 } else { -1 },
            fallback_reason_utf8: self
                .fallback_reason
                .as_ref()
                .map_or(std::ptr::null(), |s| s.as_ptr()),
        }
    }
}
impl ModelHandle {
    pub fn refresh_selection(&self, model: &Model) {
        if let Ok(cache) = self.selection.lock() {
            if let Some(cache) = cache.as_ref() {
                cache.refresh(model.selection_report());
            }
        }
    }
}
pub unsafe fn load(
    ctx: usize,
    mode: u32,
    profile: *const std::ffi::c_char,
    backend: *const am_backend_policy_t,
    threads: u32,
    source: ModelSource<'_>,
) -> Result<usize> {
    unsafe { load_inner(ctx, mode, profile, backend, threads, source, None) }
}
pub unsafe fn load_selected(
    ctx: usize,
    mode: u32,
    profile: *const std::ffi::c_char,
    threads: u32,
    source: ModelSource<'_>,
    policy: &am_vision_core::sdk::selection::BackendSelectionPolicy,
    attempt: &super::selection::Attempt,
) -> Result<usize> {
    unsafe {
        load_inner(
            ctx,
            mode,
            profile,
            std::ptr::null(),
            threads,
            source,
            Some((policy, attempt)),
        )
    }
}
unsafe fn load_inner(
    ctx: usize,
    mode: u32,
    profile: *const std::ffi::c_char,
    backend: *const am_backend_policy_t,
    threads: u32,
    source: ModelSource<'_>,
    selected: Option<(
        &am_vision_core::sdk::selection::BackendSelectionPolicy,
        &super::selection::Attempt,
    )>,
) -> Result<usize> {
    am_sdk_support::callbacks::reject_reentry()?;
    let active = crate::domain().active(ctx)?;
    let context = active.data::<Context>()?;
    if mode > AM_LOAD_RAW {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    let profile = if mode == AM_LOAD_PROFILE {
        if profile.is_null() {
            return Err(AM_ERR_VISION_PROFILE_REQUIRED);
        }
        Some(unsafe { args::string(profile) }?)
    } else {
        None
    };
    let policy = if backend.is_null() {
        am_backend_policy_t::default()
    } else {
        unsafe { args::read(backend, std::mem::size_of::<am_backend_policy_t>()) }?
    };
    if policy.requested > 2 || policy.strict > 1 {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    let runtime = context
        .runtime_dirs
        .iter()
        .map(|p| Path::new(p).join(am_vision_core::sdk::runtime::library_name()))
        .find(|p| p.is_file())
        .ok_or(AM_ERR_VISION_RUNTIME_MISSING)?;
    am_vision_core::sdk::initialize(&runtime).map_err(error)?;
    let state = State::get(ctx)?;
    let limits = *lock(&context.limits)?;
    let budget = context.thread_budget.ok_or(AM_ERR_SDK_INTERNAL_ERROR)?;
    let threads = if threads == 0 {
        2.min(budget.total.saturating_sub(budget.parallel) + 1)
    } else {
        threads
    };
    if threads == 0 || threads > budget.total {
        return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
    }
    let old = state.models.fetch_add(1, Ordering::AcqRel);
    if old >= limits.max_models as usize {
        state.models.fetch_sub(1, Ordering::AcqRel);
        return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
    }
    let extra = threads as usize - 1
        + if policy.requested == AM_BACKEND_OPENVINO as u32
            || selected.is_some_and(|(p, _)| {
                p.priority
                    .iter()
                    .take(if p.allow_fallback {
                        p.priority.len()
                    } else {
                        1
                    })
                    .any(|v| v.backend == AM_BACKEND_OPENVINO as u32)
            })
        {
            threads as usize
        } else {
            0
        };
    let permit = match scheduler::reserve_model(budget, extra) {
        Ok(p) => p,
        Err(e) => {
            state.models.fetch_sub(1, Ordering::AcqRel);
            return Err(e);
        }
    };
    let model = if let Some((selected_policy, attempt)) = selected {
        match Model::load_selected(
            source,
            profile.as_deref(),
            selected_policy,
            threads as usize,
        ) {
            Ok(model) => attempt.record(model.selection_report()).map(|()| model),
            Err(failure) => attempt
                .record(&failure.report)
                .and(Err(error(failure.error))),
        }
    } else {
        Model::load(
            source,
            profile.as_deref(),
            policy.requested,
            policy.strict != 0,
            threads as usize,
        )
        .map_err(error)
    };
    let model = match model {
        Ok(m) => m,
        Err(e) => {
            state.models.fetch_sub(1, Ordering::AcqRel);
            drop(permit);
            return Err(e);
        }
    };
    let info_result = (|| {
        let selection = selected.map(|(_, a)| a.model_cache(&state)).transpose()?;
        let (input_owners, inputs) = ios(&model.inputs, model.native_batch())?;
        let (output_owners, outputs) = ios(&model.outputs, model.native_batch())?;
        Ok((
            Info {
                key: AtomicUsize::new(0),
                uid: MODEL_ID.fetch_add(1, Ordering::Relaxed),
                profile: CString::new(profile.unwrap_or_default())
                    .map_err(|_| AM_ERR_SDK_INVALID_ARGUMENT)?,
                mode,
                actual: model.actual_backend,
                device: CString::new(match model.actual_backend {
                    1 => "OpenVINO:CPU".to_owned(),
                    2 => format!(
                        "CUDA:{}",
                        model.selection_report().selected.map_or(0, |v| v.device_id)
                    ),
                    _ => "CPU".to_owned(),
                })
                .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?,
                fallback_reason: if model.fallback {
                    Some(
                        CString::new(if selected.is_some() {
                            model
                                .selection_report()
                                .attempts
                                .iter()
                                .find_map(|v| v.error.as_deref())
                                .unwrap_or("vision.backendStrictUnavailable")
                        } else {
                            "vision.backendStrictUnavailable"
                        })
                        .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?,
                    )
                } else {
                    None
                },
                native_batch: model.native_batch(),
                input_owners,
                output_owners,
                inputs,
                input_specs: model.inputs.clone(),
                outputs,
            },
            selection,
        ))
    })();
    let (info, selection) = match info_result {
        Ok(v) => v,
        Err(e) => {
            state.models.fetch_sub(1, Ordering::AcqRel);
            drop(permit);
            return Err(e);
        }
    };
    let handle = Arc::new(ModelHandle {
        session: Mutex::new(Some(model)),
        selection: Mutex::new(selection),
        info,
        state,
        inflight: AtomicUsize::new(0),
        stopping: AtomicBool::new(false),
        busy: AtomicBool::new(false),
        policy: Mutex::new(am_realtime_policy_t {
            max_inflight: limits.realtime_max_inflight,
            replace_oldest: 1,
            slot_count: 1,
            fair_round_robin: 1,
            ..Default::default()
        }),
        permit: Mutex::new(Some(permit)),
        notify: Arc::default(),
    });
    let key = crate::domain().insert(ctx, handle.clone())?;
    handle.info.key.store(key, Ordering::Release);
    Ok(key)
}
