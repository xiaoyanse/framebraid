#![allow(clippy::missing_safety_doc)]
//! 自适应攒批兼容扩展（R5-2.1，合同 §3.5 末）：仅两个新导出，不改既有结构与语义。
use super::{error, models::ModelHandle, scheduler};
use am_sdk_support::{abi::*, args, status};
use am_vision_core::sdk::batching as core;
use std::mem::size_of;

#[no_mangle]
pub unsafe extern "C" fn amv_model_configure_batching(
    model: *mut amv_model_t,
    options: *const am_batching_options_t,
) -> i32 {
    status(crate::domain(), model as usize, || unsafe {
        am_sdk_support::callbacks::reject_reentry()?;
        let entry = crate::domain().active(model as usize)?;
        let handle = entry.data::<ModelHandle>()?;
        // NULL=关闭：维持现行逐次执行语义，旧调用方零影响。
        let config = if options.is_null() {
            None
        } else {
            let options = args::read(options, size_of::<am_batching_options_t>())?;
            // 版本协商同全局规则：接受旧次版本，拒绝未来次版本声明。
            if options.abi_minor > AM_ABI_VERSION & 0xffff {
                return Err(AM_ERR_SDK_ABI_UNSUPPORTED);
            }
            // 仅动态批量轴模型（native_batch=1 且动态上限≥2）接受启用；
            // 固定轴/无批量能力返回 vision.batchUnsupported。
            core::validate(
                handle.info.native_batch,
                options.max_batch_items,
                options.max_wait_ms,
                options.flags,
            )
            .map_err(error)?
        };
        scheduler::batching_configure(
            handle.info.key.load(std::sync::atomic::Ordering::Acquire),
            &entry.0,
            config,
        )?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn amv_model_get_batching(
    model: *mut amv_model_t,
    out: *mut am_batching_options_t,
    stats: *mut am_batching_stats_t,
) -> i32 {
    status(crate::domain(), model as usize, || unsafe {
        am_sdk_support::callbacks::reject_reentry()?;
        args::validate_output(out, size_of::<am_batching_options_t>())?;
        args::validate_output(stats, size_of::<am_batching_stats_t>())?;
        let entry = crate::domain().active(model as usize)?;
        let handle = entry.data::<ModelHandle>()?;
        // 能力查询与实际支持一致：无动态批量能力的模型返回 flags=0；
        // 关闭状态返回默认选项与全零计数。
        let (config, counters) = scheduler::batching_snapshot(
            handle.info.key.load(std::sync::atomic::Ordering::Acquire),
        );
        let effective = config.unwrap_or(core::Config {
            max_batch_items: 0,
            max_wait_ms: 0,
        });
        args::write(
            out,
            am_batching_options_t {
                struct_size: size_of::<am_batching_options_t>() as u32,
                abi_minor: AM_ABI_VERSION & 0xffff,
                max_batch_items: effective.max_batch_items,
                max_wait_ms: effective.max_wait_ms,
                flags: if config.is_some() { core::ENABLE } else { 0 },
            },
        )?;
        args::write(
            stats,
            am_batching_stats_t {
                struct_size: size_of::<am_batching_stats_t>() as u32,
                abi_minor: AM_ABI_VERSION & 0xffff,
                formed_batches: counters.formed_batches,
                batched_items: counters.batched_items,
                timeout_batches: counters.timeout_batches,
                rejected_items: counters.rejected_items,
            },
        )?;
        Ok(())
    })
}
