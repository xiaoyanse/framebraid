use crate::{abi::*, args, context::Context, handles::Domain, Result};

pub fn defaults() -> [am_queue_policy_t; 6] {
    let limits = default_limits(true);
    let items = [
        SDK_FRAME_PULL_ITEMS,
        SDK_ENCODED_QUEUE_ITEMS,
        limits.realtime_max_inflight as u64,
        SDK_MAX_NATIVE_BATCH,
        limits.result_max_items as u64,
        limits.control_max_items as u64,
    ];
    let bytes = [
        SDK_FRAME_QUEUE_BYTES,
        SDK_ENCODED_QUEUE_BYTES,
        limits.preprocess_budget_bytes,
        limits.preprocess_budget_bytes,
        limits.result_budget_bytes,
        SDK_CONTROL_BYTES,
    ];
    std::array::from_fn(|i| am_queue_policy_t {
        max_items: items[i] as u32,
        max_bytes: bytes[i],
        max_waiters: if i == 5 {
            0
        } else {
            SDK_QUEUE_MAX_WAITERS as u32
        },
        wait_deadline_ms: if i == 5 { 0 } else { SDK_QUEUE_WAIT_MS as u32 },
        ttl_ms: if i == 4 { SDK_RESULT_TTL_MS as u32 } else { 0 },
        ..Default::default()
    })
}
fn index(domain: &Domain, class: u32) -> Result<usize> {
    if class > 5 {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    if (domain.vision && class < 2) || (!domain.vision && (class == 2 || class == 3)) {
        return Err(AM_ERR_SDK_NOT_SUPPORTED);
    }
    Ok(class as usize)
}
/// # Safety
/// out须是已初始化size的可写结构。
pub unsafe fn get(
    domain: &Domain,
    key: usize,
    class: u32,
    out: *mut am_queue_policy_t,
) -> Result<()> {
    let i = index(domain, class)?;
    let entry = domain.get(key)?;
    let ctx = entry.data::<Context>()?;
    let value = ctx.queues.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?[i];
    unsafe { args::write(out, value) }
}
/// # Safety
/// policy指向完整只读结构，新值只影响后续准入。
pub unsafe fn set(
    domain: &Domain,
    key: usize,
    class: u32,
    policy: *const am_queue_policy_t,
) -> Result<()> {
    let i = index(domain, class)?;
    crate::callbacks::reject_reentry()?;
    let entry = domain.active(key)?;
    let ctx = entry.data::<Context>()?;
    let p = unsafe { args::read(policy, std::mem::size_of::<am_queue_policy_t>()) }?;
    let mut queues = ctx.queues.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
    let old = queues[i];
    if p.reserved.iter().any(|v| *v != 0)
        || p.max_items > old.max_items
        || p.max_bytes > old.max_bytes
        || p.max_waiters > old.max_waiters
        || p.wait_deadline_ms > old.wait_deadline_ms
        || p.ttl_ms > old.ttl_ms
    {
        return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
    }
    queues[i] = p;
    Ok(())
}

#[macro_export]
macro_rules! queue_exports {
    ($context:ty,$get:ident,$set:ident) => {
        #[no_mangle]
        pub unsafe extern "C" fn $get(
            ctx: *mut $context,
            class: u32,
            out: *mut $crate::abi::am_queue_policy_t,
        ) -> i32 {
            $crate::status(domain(), ctx as usize, || unsafe {
                $crate::queues::get(domain(), ctx as usize, class, out)
            })
        }
        #[no_mangle]
        pub unsafe extern "C" fn $set(
            ctx: *mut $context,
            class: u32,
            policy: *const $crate::abi::am_queue_policy_t,
        ) -> i32 {
            $crate::status(domain(), ctx as usize, || unsafe {
                $crate::queues::set(domain(), ctx as usize, class, policy)
            })
        }
    };
}
