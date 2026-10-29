//! M5 槽位原子热切换导出。
use super::{
    lock,
    models::{self, ModelHandle},
    state::State,
};
use am_sdk_support::{abi::*, args, status, Result};
use am_vision_core::sdk::ModelSource;
use std::{mem::size_of, path::Path};

fn swap_flags_valid(swap: &am_model_swap_desc_t) -> bool {
    swap.flags & !AM_SWAP_DETACH == 0 && swap.reserved == 0
}

pub(super) fn swap_registry(state: &State, old_key: usize, new_key: usize) -> Result<bool> {
    let mut slots = lock(&state.swap_slots)?;
    if slots.get(&0) != Some(&old_key) {
        return Ok(false);
    }
    slots.insert(0, new_key);
    Ok(true)
}

pub(super) fn swap_registry_current(state: &State) -> Result<usize> {
    lock(&state.swap_slots)?
        .get(&0)
        .copied()
        .ok_or(AM_ERR_SDK_HANDLE_INVALID)
}

#[no_mangle]
pub unsafe extern "C" fn amv_model_swap(
    ctx: *mut amv_context_t,
    swap_desc: *const am_model_swap_desc_t,
    new_desc: *const am_model_desc_t,
    out_model: *mut *mut amv_model_t,
    out_old_model_id: *mut u64,
) -> i32 {
    status(crate::domain(), ctx as usize, || unsafe {
        args::pointer(out_model)?;
        out_model.write(std::ptr::null_mut());
        args::pointer(out_old_model_id)?;
        out_old_model_id.write(0);
        am_sdk_support::callbacks::reject_reentry()?;
        let swap = args::read(swap_desc, size_of::<am_model_swap_desc_t>())?;
        if !swap_flags_valid(&swap) {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        if swap.slot_key != 0 {
            return Err(AM_ERR_SDK_HANDLE_INVALID);
        }
        let state = State::get(ctx as usize)?;
        let old_key = swap_registry_current(&state)?;
        let old = crate::domain().active(old_key)?;
        old.data::<ModelHandle>()?;
        let desc = args::read(new_desc, size_of::<am_model_desc_t>())?;
        let path = args::string(desc.path_utf8)?;
        let new_key = models::load_for_swap(
            ctx as usize,
            desc.load_mode,
            desc.profile_id,
            desc.backend,
            desc.intra_threads,
            ModelSource::Path(Path::new(&path)),
        )?;
        if !swap_registry(&state, old_key, new_key)? {
            if crate::domain().close(new_key, 1000).is_ok() {
                let _ = crate::domain().destroy(new_key);
            }
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        drop(old);
        out_model.write(new_key as _);
        out_old_model_id.write(old_key as u64);
        if swap.flags & AM_SWAP_DETACH != 0 {
            let _ = crate::domain().close(old_key, 0);
        }
        Ok(())
    })
}
