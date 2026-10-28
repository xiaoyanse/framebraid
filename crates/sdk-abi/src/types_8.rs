// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_pixel_view_t {
    pub struct_size: u32,
    pub magic: u32,
    pub owner_handle: *mut c_void,
    pub retain_owner: Option<unsafe extern "C" fn(*mut c_void)>,
    pub release_owner: Option<unsafe extern "C" fn(*mut c_void)>,
    pub view: *const am_frame_view_t,
    pub assoc: *const am_frame_assoc_t,
}
impl Default for am_pixel_view_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            magic: Default::default(),
            owner_handle: Default::default(),
            retain_owner: Default::default(),
            release_owner: Default::default(),
            view: Default::default(),
            assoc: Default::default(),
        }
    }
}
