// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_version_info_t {
    pub struct_size: u32,
    pub abi_major: u32,
    pub abi_minor: u32,
    pub package_version_utf8: *const c_char,
    pub build_target_utf8: *const c_char,
    pub runtime_notes_utf8: *const c_char,
}
impl Default for am_version_info_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            abi_major: Default::default(),
            abi_minor: Default::default(),
            package_version_utf8: Default::default(),
            build_target_utf8: Default::default(),
            runtime_notes_utf8: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct amc_init_params_t {
    pub struct_size: u32,
    pub abi_version: u32,
    pub runtime_dirs_utf8: *const *const c_char,
    pub runtime_dir_count: u32,
    pub reserved: [u32; 4],
    pub feature_flags: u64,
}
impl Default for amc_init_params_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            abi_version: Default::default(),
            runtime_dirs_utf8: Default::default(),
            runtime_dir_count: Default::default(),
            reserved: [Default::default(); 4],
            feature_flags: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct amv_init_params_t {
    pub struct_size: u32,
    pub abi_version: u32,
    pub runtime_dirs_utf8: *const *const c_char,
    pub runtime_dir_count: u32,
    pub infer_threads: u32,
    pub domain_infer_slots: u64,
    pub total_thread_budget: u32,
    pub reserved: [u32; 3],
    pub feature_flags: u64,
}
impl Default for amv_init_params_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            abi_version: Default::default(),
            runtime_dirs_utf8: Default::default(),
            runtime_dir_count: Default::default(),
            infer_threads: Default::default(),
            domain_infer_slots: Default::default(),
            total_thread_budget: Default::default(),
            reserved: [Default::default(); 3],
            feature_flags: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_error_info_t {
    pub struct_size: u32,
    pub code: am_status_t,
    pub message_utf8: *const c_char,
    pub source_utf8: *const c_char,
    pub retryable: u32,
    pub fault_class: u32,
    pub reserved: [u32; 4],
}
impl Default for am_error_info_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            code: Default::default(),
            message_utf8: Default::default(),
            source_utf8: Default::default(),
            retryable: Default::default(),
            fault_class: Default::default(),
            reserved: [Default::default(); 4],
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_init_error_t {
    pub struct_size: u32,
    pub code: am_status_t,
    pub message: [c_char; 256],
    pub source_utf8: *const c_char,
    pub failed_at_ms: i64,
    pub attempt_seq: u64,
}
impl Default for am_init_error_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            code: Default::default(),
            message: [Default::default(); 256],
            source_utf8: Default::default(),
            failed_at_ms: Default::default(),
            attempt_seq: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_request_snapshot_t {
    pub struct_size: u32,
    pub state: u32,
    pub terminal: u32,
    pub error: *const am_error_info_t,
    pub created_at_ms: i64,
    pub finished_at_ms: i64,
    pub reserved: [u32; 4],
}
impl Default for am_request_snapshot_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            state: Default::default(),
            terminal: Default::default(),
            error: Default::default(),
            created_at_ms: Default::default(),
            finished_at_ms: Default::default(),
            reserved: [Default::default(); 4],
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_queue_policy_t {
    pub struct_size: u32,
    pub max_items: u32,
    pub max_bytes: u64,
    pub max_waiters: u32,
    pub wait_deadline_ms: u32,
    pub ttl_ms: u32,
    pub reserved: [u32; 4],
}
impl Default for am_queue_policy_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            max_items: Default::default(),
            max_bytes: Default::default(),
            max_waiters: Default::default(),
            wait_deadline_ms: Default::default(),
            ttl_ms: Default::default(),
            reserved: [Default::default(); 4],
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_credential_data_t {
    pub struct_size: u32,
    pub username_utf8: *const c_char,
    pub password_utf8: *const c_char,
}
impl Default for am_credential_data_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            username_utf8: Default::default(),
            password_utf8: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_credential_provider_t {
    pub struct_size: u32,
    pub fetch: am_credential_fetch_fn,
    pub on_drop: am_credential_drop_fn,
    pub user_data: *mut c_void,
    pub fetch_timeout_ms: u32,
}
impl Default for am_credential_provider_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            fetch: Default::default(),
            on_drop: Default::default(),
            user_data: Default::default(),
            fetch_timeout_ms: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct am_shared_frame_desc_t {
    pub protocol_version: u32,
    pub pixel_format: u32,
    pub width: u32,
    pub height: u32,
    pub plane_count: u32,
    pub stride_bytes: [u32; 4],
    pub offset_bytes: [u32; 4],
    pub plane_valid_bytes: [u32; 4],
    pub color_matrix: u32,
    pub color_range: u32,
    pub publication_id: u64,
    pub frame_id: u64,
    pub source_id: u64,
    pub source_generation: u32,
    pub config_generation: u32,
    pub assoc_valid: u32,
    pub pool_generation: u32,
    pub time_flags: u32,
    pub pts: i64,
    pub time_base_num: u32,
    pub time_base_den: u32,
    pub captured_at_ms: i64,
    pub received_at_ms: i64,
    pub clock_quality: u32,
    pub reserved: [u32; 4],
}
