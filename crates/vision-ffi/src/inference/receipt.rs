use super::{
    lock,
    state::{ControlCharge, State},
};
use am_sdk_support::{abi::*, args, buffers::Buffer, Result};
use std::{
    any::Any,
    mem::size_of,
    sync::{Arc, Mutex},
};

pub struct Data {
    pub ids: Box<[u64]>,
    pub codes: Box<[i32]>,
}
pub struct Receipt {
    pub data: Mutex<Data>,
    _charge: ControlCharge,
}
impl Buffer for Receipt {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
impl Receipt {
    pub fn create(state: &State, count: usize) -> Result<Arc<Self>> {
        let charge = state.control(size_of::<Self>() + count * 12)?;
        Ok(Arc::new(Self {
            data: Mutex::new(Data {
                ids: vec![0; count].into_boxed_slice(),
                codes: vec![0; count].into_boxed_slice(),
            }),
            _charge: charge,
        }))
    }
}
pub unsafe fn release(pointer: *mut am_batch_receipt_t) -> Result<()> {
    let value = unsafe { args::read(pointer, size_of::<am_batch_receipt_t>()) }?;
    let key = value.request_ids as usize;
    let owner = crate::domain().buffers.get(key)?;
    let receipt = owner
        .as_any()
        .downcast_ref::<Receipt>()
        .ok_or(AM_ERR_SDK_HANDLE_INVALID)?;
    let data = lock(&receipt.data)?;
    if value.rejects != data.codes.as_ptr()
        || value.request_ids != data.ids.as_ptr().cast_mut()
        || value.accepted_count as usize != data.ids.iter().filter(|id| **id != 0).count()
    {
        return Err(AM_ERR_SDK_HANDLE_INVALID);
    }
    unsafe { args::write(pointer, am_batch_receipt_t::default()) }?;
    drop(data);
    crate::domain().buffers.release(key)
}
