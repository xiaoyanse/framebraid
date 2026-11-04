// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_stats_t {
    pub struct_size: u32,
    pub scope_utf8: *const c_char,
    pub queue_items: u64,
    pub queue_bytes: u64,
    pub external_held_items: u64,
    pub external_held_bytes: u64,
    pub accepted: u64,
    pub rejected_backpressure: u64,
    pub replaced: u64,
    pub expired: u64,
    pub dropped: u64,
    pub frames_decoded: u64,
    pub frames_dropped: u64,
    pub newest_frame_age_ms: i64,
    pub reconnect_count: u64,
    pub resync_count: u64,
    pub infer_executed: u64,
    pub infer_failed: u64,
    pub infer_queue_wait_avg_ms: i64,
    pub infer_exec_avg_ms: i64,
    pub actual_capture_fps: f64,
    pub native_rss_estimate_bytes: u64,
    pub threads_active: u32,
    pub threads_peak: u32,
    pub shared_mapped_bytes: u64,
    pub shared_metadata_bytes: u64,
    pub shared_pool_count: u32,
    pub shared_outstanding_borrows: u32,
    pub reserved: [u32; 6],
    pub gpu_inputs: u64,
    pub gpu_copies_per_frame: u64,
}
impl Default for am_stats_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            scope_utf8: Default::default(),
            queue_items: Default::default(),
            queue_bytes: Default::default(),
            external_held_items: Default::default(),
            external_held_bytes: Default::default(),
            accepted: Default::default(),
            rejected_backpressure: Default::default(),
            replaced: Default::default(),
            expired: Default::default(),
            dropped: Default::default(),
            frames_decoded: Default::default(),
            frames_dropped: Default::default(),
            newest_frame_age_ms: Default::default(),
            reconnect_count: Default::default(),
            resync_count: Default::default(),
            infer_executed: Default::default(),
            infer_failed: Default::default(),
            infer_queue_wait_avg_ms: Default::default(),
            infer_exec_avg_ms: Default::default(),
            actual_capture_fps: Default::default(),
            native_rss_estimate_bytes: Default::default(),
            threads_active: Default::default(),
            threads_peak: Default::default(),
            shared_mapped_bytes: Default::default(),
            shared_metadata_bytes: Default::default(),
            shared_pool_count: Default::default(),
            shared_outstanding_borrows: Default::default(),
            reserved: [Default::default(); 6],
            gpu_inputs: Default::default(),
            gpu_copies_per_frame: Default::default(),
        }
    }
}
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
