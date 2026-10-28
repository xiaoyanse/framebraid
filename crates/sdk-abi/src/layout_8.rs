// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use crate::*;
#[test]
fn am_pixel_view_t_matches_c() {
    assert_eq!(core::mem::size_of::<am_pixel_view_t>(), 48);
    assert_eq!(core::mem::align_of::<am_pixel_view_t>(), 8);
    assert_eq!(
        core::mem::offset_of!(am_pixel_view_t, struct_size),
        0,
        "am_pixel_view_t.struct_size"
    );
    assert_eq!(
        core::mem::offset_of!(am_pixel_view_t, magic),
        4,
        "am_pixel_view_t.magic"
    );
    assert_eq!(
        core::mem::offset_of!(am_pixel_view_t, owner_handle),
        8,
        "am_pixel_view_t.owner_handle"
    );
    assert_eq!(
        core::mem::offset_of!(am_pixel_view_t, retain_owner),
        16,
        "am_pixel_view_t.retain_owner"
    );
    assert_eq!(
        core::mem::offset_of!(am_pixel_view_t, release_owner),
        24,
        "am_pixel_view_t.release_owner"
    );
    assert_eq!(
        core::mem::offset_of!(am_pixel_view_t, view),
        32,
        "am_pixel_view_t.view"
    );
    assert_eq!(
        core::mem::offset_of!(am_pixel_view_t, assoc),
        40,
        "am_pixel_view_t.assoc"
    );
}
