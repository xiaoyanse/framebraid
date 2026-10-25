use crate::{abi::AM_ERR_SDK_CALLBACK_CONTRACT_VIOLATION, Result};
use std::cell::Cell;

thread_local! { static ACTIVE: Cell<bool> = const { Cell::new(false) }; }
pub struct Scope(bool);
impl Scope {
    pub fn enter() -> Self {
        Self(ACTIVE.with(|active| active.replace(true)))
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        ACTIVE.with(|active| active.set(self.0));
    }
}
pub fn reject_reentry() -> Result<()> {
    if ACTIVE.with(Cell::get) {
        Err(AM_ERR_SDK_CALLBACK_CONTRACT_VIOLATION)
    } else {
        Ok(())
    }
}
