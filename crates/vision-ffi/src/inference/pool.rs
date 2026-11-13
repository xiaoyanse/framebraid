#![allow(clippy::missing_safety_doc)]
//! 推理池编排容器（P1，合同 §3.5a/§3.4a）：三导出，提交路径零新增（B 案）。
use super::{
    error, lock,
    models::{self, ModelHandle},
    scheduler,
    state::State,
};
use am_sdk_support::{abi::*, args, callbacks, context::Context, handles::Managed, status, Result};
use am_vision_core::sdk::{batching as core, ModelSource};
use std::{
    any::Any,
    ffi::c_char,
    mem::size_of,
    path::Path,
    sync::{atomic::Ordering, Arc, RwLock},
};

#[cfg(test)]
use std::sync::atomic::AtomicUsize;

pub(super) struct PoolHandle {
    pub(super) lanes: u32,
    pub(super) shards: RwLock<Option<Arc<[usize]>>>,
}
impl Managed for PoolHandle {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn close_resources(&self) {
        // 分片登记项不在此销毁：CLOSED 后仍按 model close 后语义在册（§1.1），
        // 最终释放由调用方经 object_destroy 子先父后完成。
        if let Ok(mut shards) = self.shards.write() {
            *shards = None;
        }
    }
}

enum Source<'a> {
    Path(String),
    Memory(&'a [u8]),
}
struct Parsed<'a> {
    mode: u32,
    profile: *const c_char,
    backend: *const am_backend_policy_t,
    threads: u32,
    source: Source<'a>,
    shards: u32,
    lanes: u32,
    batching: Option<am_batching_options_t>,
}

/// 校验顺序同合同 §3.5a：shards 域→lanes 域→模型源→batching→reserved。
unsafe fn parse_desc(desc: &am_pool_desc_t) -> Result<Parsed<'_>> {
    if desc.shards < 2 || desc.shards > SDK_MAX_CONTEXTS as u32 {
        return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
    }
    if desc.lanes == 0 || desc.lanes < desc.shards || !desc.lanes.is_multiple_of(desc.shards) {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    let batching = if desc.batching.is_null() {
        None
    } else {
        let options = unsafe { args::read(desc.batching, size_of::<am_batching_options_t>()) }?;
        if options.abi_minor > AM_ABI_VERSION & 0xffff {
            return Err(AM_ERR_SDK_ABI_UNSUPPORTED);
        }
        Some(options)
    };
    let (mode, profile, backend, threads, source) =
        match (desc.model.is_null(), desc.model_memory.is_null()) {
            (false, true) => {
                let model = unsafe { args::read(desc.model, size_of::<am_model_desc_t>()) }?;
                if model.path_utf8.is_null() {
                    return Err(AM_ERR_SDK_INVALID_ARGUMENT);
                }
                (
                    model.load_mode,
                    model.profile_id,
                    model.backend,
                    model.intra_threads,
                    Source::Path(unsafe { args::string(model.path_utf8) }?),
                )
            }
            // 内存源逐片复制语义同 amv_model_load_memory：线程数同其 0=SDK 默认。
            (true, false) => {
                let memory =
                    unsafe { args::read(desc.model_memory, size_of::<am_model_memory_desc_t>()) }?;
                if memory.size == 0 || memory.size > SDK_MAX_MEMORY_MODEL_BYTES {
                    return Err(AM_ERR_VISION_MODEL_TOO_LARGE);
                }
                args::pointer(memory.bytes)?;
                (
                    memory.load_mode,
                    memory.profile_id,
                    memory.backend,
                    0,
                    Source::Memory(unsafe {
                        std::slice::from_raw_parts(memory.bytes, memory.size as usize)
                    }),
                )
            }
            _ => return Err(AM_ERR_SDK_INVALID_ARGUMENT),
        };
    if desc.reserved != 0 {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    Ok(Parsed {
        mode,
        profile,
        backend,
        threads,
        source,
        shards: desc.shards,
        lanes: desc.lanes,
        batching,
    })
}

/// 逐片 load→借用标记→逐片 configure（§3.5a 顺序）；loaded 记在途分片，
/// 任一步失败由调用方原子回滚。load 可注入：生产传 load_shard，测试传合成片。
unsafe fn build_shards(
    pool: usize,
    parsed: &Parsed<'_>,
    loaded: &mut Vec<usize>,
    load: &dyn Fn() -> Result<usize>,
) -> Result<()> {
    for _ in 0..parsed.shards {
        let key = load()?;
        loaded.push(key);
        let entry = crate::domain().get(key)?;
        let handle = entry.data::<ModelHandle>()?;
        handle.pool.store(pool, Ordering::Release);
        let config = match parsed.batching {
            None => None,
            Some(options) => core::validate(
                handle.info.native_batch,
                options.max_batch_items,
                options.max_wait_ms,
                options.flags,
            )
            .map_err(error)?,
        };
        scheduler::batching_configure(key, &entry, config)?;
    }
    Ok(())
}

/// 失败原子回滚（§3.5a）：不留半池——分片按 model close 语义排空后销毁，
/// 池登记项随册回收；个别回收失败不掩盖原始错误。
fn rollback(pool: usize, loaded: &[usize]) {
    let domain = crate::domain();
    for &key in loaded {
        let _ = domain.close(key, 0);
        let _ = domain.destroy(key);
    }
    let _ = domain.close(pool, 0);
    let _ = domain.destroy(pool);
}

#[no_mangle]
pub unsafe extern "C" fn amv_pool_create(
    ctx: *mut amv_context_t,
    desc: *const am_pool_desc_t,
    out_pool: *mut *mut amv_pool_t,
) -> i32 {
    status(crate::domain(), ctx as usize, || unsafe {
        callbacks::reject_reentry()?;
        args::pointer(out_pool)?;
        out_pool.write(std::ptr::null_mut());
        if ctx.is_null() {
            return Err(AM_ERR_SDK_NULL_ARGUMENT);
        }
        let read = args::read(desc, size_of::<am_pool_desc_t>())?;
        let parsed = parse_desc(&read)?;
        let active = crate::domain().active(ctx as usize)?;
        let context = active.data::<Context>()?;
        let limits = *lock(&context.limits)?;
        let state = State::get(ctx as usize)?;
        if state.models.load(Ordering::Acquire) as u64 + parsed.shards as u64
            > limits.max_models as u64
        {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        let pool = crate::domain().insert(
            ctx as usize,
            Arc::new(PoolHandle {
                lanes: parsed.lanes,
                shards: RwLock::new(None),
            }),
        )?;
        let mut loaded = Vec::new();
        let ctx_key = ctx as usize;
        let load = || match &parsed.source {
            Source::Path(path) => models::load_shard(
                ctx_key,
                pool,
                parsed.mode,
                parsed.profile,
                parsed.backend,
                parsed.threads,
                ModelSource::Path(Path::new(path)),
            ),
            Source::Memory(bytes) => models::load_shard(
                ctx_key,
                pool,
                parsed.mode,
                parsed.profile,
                parsed.backend,
                parsed.threads,
                ModelSource::Memory(bytes),
            ),
        };
        if let Err(code) = build_shards(pool, &parsed, &mut loaded, &load) {
            rollback(pool, &loaded);
            return Err(code);
        }
        if let Ok(entry) = crate::domain().get(pool) {
            if let Ok(handle) = entry.data::<PoolHandle>() {
                if let Ok(mut shards) = handle.shards.write() {
                    *shards = Some(loaded.into());
                }
            }
        }
        out_pool.write(pool as _);
        Ok(())
    })
}

/// lane→shard 借用句柄：⌊i×k/lanes⌋ 连续分组；只读查询任意线程并发（对齐
/// get_info），池 CLOSED 后借出句柄业务调用返回 handleClosed。
#[no_mangle]
pub unsafe extern "C" fn amv_pool_lane_model(
    pool: *mut amv_pool_t,
    lane: u32,
    out_model: *mut *mut amv_model_t,
) -> i32 {
    status(crate::domain(), pool as usize, || unsafe {
        args::pointer(out_model)?;
        out_model.write(std::ptr::null_mut());
        if pool.is_null() {
            return Err(AM_ERR_SDK_NULL_ARGUMENT);
        }
        let entry = crate::domain().get(pool as usize)?;
        let handle = entry.data::<PoolHandle>()?;
        if entry.state.load(Ordering::Acquire) == AM_LIFECYCLE_CLOSED as u32 {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        if lane >= handle.lanes {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        let guard = handle
            .shards
            .read()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        let shards = guard.as_ref().ok_or(AM_ERR_SDK_INTERNAL_ERROR)?;
        let index = lane as u64 * shards.len() as u64 / handle.lanes as u64;
        out_model.write(shards[index as usize] as *mut amv_model_t);
        Ok(())
    })
}

/// 逐片排空关闭（复用 model close 语义，级联子片）；超时 CLOSING 轮询，
/// CLOSED 幂等（§1.1 两阶段总则，登记项经 object_destroy 子先父后回收）。
#[no_mangle]
pub unsafe extern "C" fn amv_pool_close(
    pool: *mut amv_pool_t,
    timeout_ms: u32,
    out_state: *mut am_lifecycle_state_t,
) -> i32 {
    status(crate::domain(), pool as usize, || unsafe {
        callbacks::reject_reentry()?;
        args::pointer(out_state)?;
        if pool.is_null() {
            return Err(AM_ERR_SDK_NULL_ARGUMENT);
        }
        let entry = crate::domain().get(pool as usize)?;
        entry.data::<PoolHandle>()?;
        out_state.write(crate::domain().close(pool as usize, timeout_ms)?);
        Ok(())
    })
}

#[cfg(test)]
pub(super) fn stub_shard(ctx: usize, pool: usize, native_batch: u32) -> Result<usize> {
    use std::sync::{atomic::AtomicBool, Mutex as SyncMutex};
    let state = State::get(ctx)?;
    let mut info = super::models::Info::stub();
    info.native_batch = native_batch;
    let handle = Arc::new(ModelHandle {
        session: SyncMutex::new(None),
        selection: SyncMutex::new(None),
        info,
        state: state.clone(),
        inflight: AtomicUsize::new(0),
        stopping: AtomicBool::new(false),
        busy: AtomicBool::new(false),
        policy: SyncMutex::new(am_realtime_policy_t::default()),
        permit: SyncMutex::new(None),
        notify: Arc::default(),
        vocab: RwLock::new(Arc::new(None)),
        pool: AtomicUsize::new(0),
    });
    let key = crate::domain().insert(pool, handle.clone())?;
    handle.info.key.store(key, Ordering::Release);
    state.models.fetch_add(1, Ordering::AcqRel);
    Ok(key)
}

#[cfg(test)]
pub(super) fn test_pool(
    ctx: usize,
    shards: u32,
    lanes: u32,
    native_batch: &dyn Fn(u32) -> u32,
    batching: Option<am_batching_options_t>,
) -> Result<usize> {
    let parsed = Parsed {
        mode: AM_LOAD_RAW,
        profile: std::ptr::null(),
        backend: std::ptr::null(),
        threads: 0,
        source: Source::Path("pool-stub.onnx".to_owned()),
        shards,
        lanes,
        batching,
    };
    let pool = crate::domain().insert(
        ctx,
        Arc::new(PoolHandle {
            lanes,
            shards: RwLock::new(None),
        }),
    )?;
    let mut loaded = Vec::new();
    let index = AtomicUsize::new(0);
    let load = || {
        let i = index.fetch_add(1, Ordering::AcqRel) as u32;
        stub_shard(ctx, pool, native_batch(i))
    };
    if let Err(code) = unsafe { build_shards(pool, &parsed, &mut loaded, &load) } {
        rollback(pool, &loaded);
        return Err(code);
    }
    if let Ok(entry) = crate::domain().get(pool) {
        if let Ok(handle) = entry.data::<PoolHandle>() {
            if let Ok(mut shards) = handle.shards.write() {
                *shards = Some(loaded.into());
            }
        }
    }
    Ok(pool)
}
