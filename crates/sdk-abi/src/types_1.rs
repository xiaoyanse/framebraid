// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_shared_pool_config_t {
    pub struct_size: u32,
    pub slot_count: u32,
    pub max_frame_bytes: u32,
    pub total_bytes: u64,
    pub max_consumers: u32,
    pub max_outstanding_borrows: u32,
    pub drop_policy: u32,
    pub publish_wait_ms: u32,
    pub reserved: [u32; 4],
}
impl Default for am_shared_pool_config_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            slot_count: Default::default(),
            max_frame_bytes: Default::default(),
            total_bytes: Default::default(),
            max_consumers: Default::default(),
            max_outstanding_borrows: Default::default(),
            drop_policy: Default::default(),
            publish_wait_ms: Default::default(),
            reserved: [Default::default(); 4],
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_shared_pool_attach_desc_t {
    pub struct_size: u32,
    pub protocol_version: u32,
    pub pool_id: u64,
    pub owner_pid: u64,
    pub owner_started_at_100ns: u64,
    pub pool_generation: u32,
    pub identity_tag: [u8; 32],
}
impl Default for am_shared_pool_attach_desc_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            protocol_version: Default::default(),
            pool_id: Default::default(),
            owner_pid: Default::default(),
            owner_started_at_100ns: Default::default(),
            pool_generation: Default::default(),
            identity_tag: [Default::default(); 32],
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_shared_frame_view_t {
    pub struct_size: u32,
    pub desc: am_shared_frame_desc_t,
    pub plane_data: [*const c_void; 4],
    pub borrow_id: u64,
}
impl Default for am_shared_frame_view_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            desc: Default::default(),
            plane_data: [Default::default(); 4],
            borrow_id: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_shared_pool_stats_t {
    pub struct_size: u32,
    pub published: u64,
    pub dropped_stale: u64,
    pub dropped_full: u64,
    pub wait_timeouts: u64,
    pub slots_in_use: u32,
    pub slots_borrowed: u32,
    pub outstanding_borrows: u32,
    pub consumers_attached: u32,
    pub bytes_in_use: u64,
    pub metadata_bytes: u64,
    pub reserved: [u32; 4],
}
impl Default for am_shared_pool_stats_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            published: Default::default(),
            dropped_stale: Default::default(),
            dropped_full: Default::default(),
            wait_timeouts: Default::default(),
            slots_in_use: Default::default(),
            slots_borrowed: Default::default(),
            outstanding_borrows: Default::default(),
            consumers_attached: Default::default(),
            bytes_in_use: Default::default(),
            metadata_bytes: Default::default(),
            reserved: [Default::default(); 4],
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct amc_capability_t {
    pub struct_size: u32,
    pub abi_minor: u32,
    pub feature_bits: u32,
    pub pixel_formats: u32,
    pub max_sources: u32,
    pub max_decoders: u32,
    pub max_subscribers_per_source: u32,
    pub frame_budget_bytes: u64,
    pub shared_pool_state: u32,
}
impl Default for amc_capability_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            abi_minor: Default::default(),
            feature_bits: Default::default(),
            pixel_formats: Default::default(),
            max_sources: Default::default(),
            max_decoders: Default::default(),
            max_subscribers_per_source: Default::default(),
            frame_budget_bytes: Default::default(),
            shared_pool_state: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_source_desc_t {
    pub struct_size: u32,
    pub kind: u32,
    pub url_utf8: *const c_char,
    pub stream_kind_utf8: *const c_char,
    pub creds: *const am_credential_data_t,
    pub provider: *const am_credential_provider_t,
    pub reuse: u32,
    pub reserved: [u32; 4],
}
impl Default for am_source_desc_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            kind: Default::default(),
            url_utf8: Default::default(),
            stream_kind_utf8: Default::default(),
            creds: Default::default(),
            provider: Default::default(),
            reuse: Default::default(),
            reserved: [Default::default(); 4],
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_source_status_t {
    pub struct_size: u32,
    pub state: am_source_state_e,
    pub source_id: u64,
    pub generation: u32,
    pub config_generation: u32,
    pub fault_class: u32,
    pub reconnect_attempts: u32,
    pub last_frame_at_ms: i64,
}
impl Default for am_source_status_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            state: Default::default(),
            source_id: Default::default(),
            generation: Default::default(),
            config_generation: Default::default(),
            fault_class: Default::default(),
            reconnect_attempts: Default::default(),
            last_frame_at_ms: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_reconnect_policy_t {
    pub struct_size: u32,
    pub initial_backoff_ms: u32,
    pub max_backoff_ms: u32,
    pub jitter_ms: u32,
    pub max_attempts: u32,
}
impl Default for am_reconnect_policy_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            initial_backoff_ms: Default::default(),
            max_backoff_ms: Default::default(),
            jitter_ms: Default::default(),
            max_attempts: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_probe_candidate_t {
    pub struct_size: u32,
    pub reference_utf8: *const c_char,
    pub status: am_probe_status_e,
    pub codec_utf8: *const c_char,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub observed_at_ms: i64,
    pub valid_until_ms: i64,
}
impl Default for am_probe_candidate_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            reference_utf8: Default::default(),
            status: Default::default(),
            codec_utf8: Default::default(),
            width: Default::default(),
            height: Default::default(),
            fps: Default::default(),
            observed_at_ms: Default::default(),
            valid_until_ms: Default::default(),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct am_probe_request_t {
    pub struct_size: u32,
    pub candidate_count: u32,
    pub references_utf8: *const *const c_char,
    pub creds: *const am_credential_data_t,
    pub force_refresh: u32,
    pub timeout_ms: u32,
}
impl Default for am_probe_request_t {
    fn default() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            candidate_count: Default::default(),
            references_utf8: Default::default(),
            creds: Default::default(),
            force_refresh: Default::default(),
            timeout_ms: Default::default(),
        }
    }
}
