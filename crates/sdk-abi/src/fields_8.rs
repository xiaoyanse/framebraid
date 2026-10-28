// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
impl AbiLayout for am_pixel_view_t {
    const FIELDS: &'static [(usize, usize)] = &[
        (
            core::mem::offset_of!(am_pixel_view_t, struct_size),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_pixel_view_t, magic),
            core::mem::size_of::<u32>(),
        ),
        (
            core::mem::offset_of!(am_pixel_view_t, owner_handle),
            core::mem::size_of::<*mut c_void>(),
        ),
        (
            core::mem::offset_of!(am_pixel_view_t, retain_owner),
            core::mem::size_of::<Option<unsafe extern "C" fn(*mut c_void)>>(),
        ),
        (
            core::mem::offset_of!(am_pixel_view_t, release_owner),
            core::mem::size_of::<Option<unsafe extern "C" fn(*mut c_void)>>(),
        ),
        (
            core::mem::offset_of!(am_pixel_view_t, view),
            core::mem::size_of::<*const am_frame_view_t>(),
        ),
        (
            core::mem::offset_of!(am_pixel_view_t, assoc),
            core::mem::size_of::<*const am_frame_assoc_t>(),
        ),
    ];
}
