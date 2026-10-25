#![allow(clippy::missing_safety_doc)]
use super::{exports::submit_sync, models::ModelHandle, scheduler, submission::Plan, tensors};
use am_sdk_support::{abi::*, args, status};

#[no_mangle]
pub unsafe extern "C" fn amv_infer_tensors_async(
    model: *mut amv_model_t,
    count: u32,
    inputs: *const am_tensor_in_t,
    params: *const am_infer_params_t,
    out_id: *mut u64,
) -> i32 {
    status(crate::domain(), model as usize, || unsafe {
        am_sdk_support::callbacks::reject_reentry()?;
        args::pointer(out_id)?;
        out_id.write(0);
        let entry = crate::domain().active(model as usize)?;
        let plan = tensors::plan(entry.data::<ModelHandle>()?, count, inputs, params)?;
        out_id.write(scheduler::submit(entry.0.clone(), Plan::Tensors(plan))?);
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amv_infer_tensors_sync(
    model: *mut amv_model_t,
    count: u32,
    inputs: *const am_tensor_in_t,
    params: *const am_infer_params_t,
    out: *mut *mut amv_result_t,
    out_id: *mut u64,
) -> i32 {
    status(crate::domain(), model as usize, || unsafe {
        am_sdk_support::callbacks::reject_reentry()?;
        args::pointer(out)?;
        out.write(std::ptr::null_mut());
        if !out_id.is_null() {
            args::pointer(out_id)?;
            out_id.write(0);
        }
        let entry = crate::domain().active(model as usize)?;
        let plan = tensors::plan(entry.data::<ModelHandle>()?, count, inputs, params)?;
        submit_sync(entry.0.clone(), Plan::Tensors(plan), out, out_id)
    })
}
