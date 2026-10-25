use crate::{
    abi::*,
    args,
    handles::{Domain, Managed},
    Result,
};
use std::{
    any::Any,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThreadBudget {
    pub parallel: u32,
    pub total: u32,
    pub slots: u64,
}
/// 只保留弱引用，context关闭先清业务无关缓存，不制造服务与拥有者环。
pub struct CloseHooks(pub Mutex<Vec<std::sync::Weak<dyn Managed>>>);
pub struct Context {
    pub limits: Mutex<am_limits_t>,
    pub queues: Mutex<[am_queue_policy_t; 6]>,
    pub flags: u64,
    pub runtime_dirs: Vec<String>,
    pub buffers: AtomicUsize,
    pub thread_budget: Option<ThreadBudget>,
    pub services: Mutex<std::collections::HashMap<std::any::TypeId, Arc<dyn Any + Send + Sync>>>,
    #[cfg(all(windows, target_arch = "x86_64", feature = "shared-frames"))]
    pub shared: Mutex<crate::shared::Ledger>,
    #[cfg(all(windows, target_arch = "x86_64", feature = "shared-frames"))]
    pub shared_waiters: Arc<AtomicUsize>,
}
impl Managed for Context {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn is_context(&self) -> bool {
        true
    }
    fn request_close(&self) {
        let hooks = self
            .services
            .lock()
            .ok()
            .and_then(|s| s.get(&std::any::TypeId::of::<CloseHooks>()).cloned())
            .and_then(|s| s.downcast::<CloseHooks>().ok());
        if let Some(hooks) = hooks {
            let items = hooks
                .0
                .lock()
                .map(|h| {
                    h.iter()
                        .filter_map(std::sync::Weak::upgrade)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            for hook in items {
                hook.request_close();
            }
        }
    }
    fn drained(&self) -> bool {
        self.buffers.load(Ordering::Acquire) == 0
    }
}
pub fn shared_state(domain: &Domain, flags: u64) -> u32 {
    if !cfg!(all(windows, target_arch = "x86_64")) {
        AM_SFP_PLATFORM_UNSUPPORTED as u32
    } else if !domain.shared_compiled
        || !cfg!(feature = "shared-frames")
        || !crate::SHARED_IMPLEMENTED
    {
        AM_SFP_NOT_COMPILED as u32
    } else if flags & AM_INIT_SHARED_FRAMES as u64 != 0 {
        AM_SFP_ENABLED as u32
    } else {
        AM_SFP_DISABLED as u32
    }
}
fn flags_valid(domain: &Domain, flags: u64) -> Result<()> {
    if flags & !(AM_INIT_SHARED_FRAMES as u64) != 0 {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    if flags != 0 && shared_state(domain, flags) != AM_SFP_ENABLED as u32 {
        return Err(AM_ERR_SDK_NOT_SUPPORTED);
    }
    Ok(())
}
fn version_valid(version: u32) -> Result<()> {
    if version != AM_ABI_VERSION {
        Err(AM_ERR_SDK_ABI_UNSUPPORTED)
    } else {
        Ok(())
    }
}

/// # Safety
/// params须遵循公开init结构及size约定，所有输入只借用到返回。
pub unsafe fn initialize_camera(
    domain: &Domain,
    params: *const amc_init_params_t,
) -> Result<usize> {
    let prefix = std::mem::offset_of!(amc_init_params_t, feature_flags);
    let mut p = unsafe { args::read(params, prefix) }?;
    if (p.struct_size as usize) < prefix + 8 {
        p.feature_flags = 0;
    }
    version_valid(p.abi_version)?;
    flags_valid(domain, p.feature_flags)?;
    if p.reserved.iter().any(|v| *v != 0) {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    let dirs = unsafe { args::runtime_dirs(p.runtime_dirs_utf8, p.runtime_dir_count) }?;
    domain.insert(
        0,
        Arc::new(Context {
            limits: Mutex::new(default_limits(false)),
            queues: Mutex::new(crate::queues::defaults()),
            flags: p.feature_flags,
            runtime_dirs: dirs,
            buffers: AtomicUsize::new(0),
            thread_budget: None,
            services: Mutex::default(),
            #[cfg(all(windows, target_arch = "x86_64", feature = "shared-frames"))]
            shared: Mutex::default(),
            #[cfg(all(windows, target_arch = "x86_64", feature = "shared-frames"))]
            shared_waiters: Arc::new(AtomicUsize::new(0)),
        }),
    )
}

/// # Safety
/// params须遵循公开init结构及size约定，指针数组保持到返回。
pub unsafe fn initialize_vision(
    domain: &Domain,
    params: *const amv_init_params_t,
) -> Result<usize> {
    let prefix = std::mem::offset_of!(amv_init_params_t, feature_flags);
    let mut p = unsafe { args::read(params, prefix) }?;
    if (p.struct_size as usize) < prefix + 8 {
        p.feature_flags = 0;
    }
    version_valid(p.abi_version)?;
    flags_valid(domain, p.feature_flags)?;
    if p.reserved.iter().any(|v| *v != 0) {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    let dirs = unsafe { args::runtime_dirs(p.runtime_dirs_utf8, p.runtime_dir_count) }?;
    let mut existing = domain
        .thread_config
        .lock()
        .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
    let machine = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1);
    let total = if p.total_thread_budget == 0 {
        existing.map(|b| b.total).unwrap_or(machine)
    } else {
        p.total_thread_budget
    };
    let parallel = if p.infer_threads == 0 {
        existing.map(|b| b.parallel).unwrap_or(total.min(4))
    } else {
        p.infer_threads
    };
    let slots = if p.domain_infer_slots == 0 {
        existing.map(|b| b.slots).unwrap_or(SDK_DOMAIN_INFER_SLOTS)
    } else {
        p.domain_infer_slots
    };
    if total == 0
        || total > machine
        || parallel == 0
        || parallel > total.min(4)
        || slots == 0
        || slots > SDK_DOMAIN_INFER_SLOTS
    {
        return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
    }
    let budget = ThreadBudget {
        parallel,
        total,
        slots,
    };
    if existing.is_some_and(|value| value != budget) {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    let mut limits = default_limits(true);
    limits.infer_parallelism = parallel;
    let key = domain.insert(
        0,
        Arc::new(Context {
            limits: Mutex::new(limits),
            queues: Mutex::new(crate::queues::defaults()),
            flags: p.feature_flags,
            runtime_dirs: dirs,
            buffers: AtomicUsize::new(0),
            thread_budget: Some(budget),
            services: Mutex::default(),
            #[cfg(all(windows, target_arch = "x86_64", feature = "shared-frames"))]
            shared: Mutex::default(),
            #[cfg(all(windows, target_arch = "x86_64", feature = "shared-frames"))]
            shared_waiters: Arc::new(AtomicUsize::new(0)),
        }),
    )?;
    *existing = Some(budget);
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_or_partial_feature_field_is_ignored_and_contexts_are_bounded() {
        let domain = Domain::new(false);
        let params = amc_init_params_t {
            struct_size: (std::mem::offset_of!(amc_init_params_t, feature_flags) + 4) as u32,
            abi_version: AM_ABI_VERSION,
            feature_flags: 1,
            ..Default::default()
        };
        let mut keys = Vec::new();
        for _ in 0..SDK_MAX_CONTEXTS {
            keys.push(unsafe { initialize_camera(&domain, &params) }.unwrap());
        }
        assert_eq!(
            unsafe { initialize_camera(&domain, &params) }.unwrap_err(),
            AM_ERR_SDK_RESOURCE_EXHAUSTED
        );
        for key in keys {
            domain.close(key, 0).unwrap();
            domain.destroy(key).unwrap();
        }
        let full = amc_init_params_t {
            abi_version: AM_ABI_VERSION,
            feature_flags: 2,
            ..Default::default()
        };
        assert_eq!(
            unsafe { initialize_camera(&domain, &full) }.unwrap_err(),
            AM_ERR_SDK_INVALID_ARGUMENT
        );
    }
    #[test]
    fn vision_configuration_is_singleton_and_must_not_silently_change() {
        let domain = Domain::new(true);
        let first = amv_init_params_t {
            abi_version: AM_ABI_VERSION,
            domain_infer_slots: 2,
            ..Default::default()
        };
        let key = unsafe { initialize_vision(&domain, &first) }.unwrap();
        let changed = amv_init_params_t {
            domain_infer_slots: 3,
            ..first
        };
        assert_eq!(
            unsafe { initialize_vision(&domain, &changed) }.unwrap_err(),
            AM_ERR_SDK_INVALID_ARGUMENT
        );
        let inherited = amv_init_params_t {
            domain_infer_slots: 0,
            ..first
        };
        let second = unsafe { initialize_vision(&domain, &inherited) }.unwrap();
        domain.close(second, 0).unwrap();
        domain.destroy(second).unwrap();
        domain.close(key, 0).unwrap();
        domain.destroy(key).unwrap();
    }
}
