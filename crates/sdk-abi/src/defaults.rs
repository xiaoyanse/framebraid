// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。
use super::*;
pub const SDK_MAX_CONTEXTS: u64 = 8;
pub const SDK_MAX_REGISTERED_HANDLES: u64 = 4096;
pub const SDK_HANDLE_QUARANTINE: u64 = 4096;
pub const SDK_MAX_STRING_BYTES: u64 = 4096;
pub const SDK_MAX_RUNTIME_DIRECTORIES: u64 = 16;
pub const SDK_MAX_TENSOR_RANK: u64 = 8;
pub const SDK_MAX_NAMED_TENSORS: u64 = 32;
pub const SDK_MAX_DYNAMIC_AXIS: u64 = 8192;
pub const SDK_MAX_NATIVE_BATCH: u64 = 64;
pub const SDK_DOMAIN_INFER_SLOTS: u64 = 64;
pub const SDK_MAX_MEMORY_MODEL_BYTES: u64 = 268435456;
pub const SDK_ENCODED_QUEUE_ITEMS: u64 = 36;
pub const SDK_ENCODED_QUEUE_BYTES: u64 = 8388608;
pub const SDK_GOP_CACHE_BYTES: u64 = 4194304;
pub const SDK_FRAME_PUSH_ITEMS: u64 = 2;
pub const SDK_FRAME_PULL_ITEMS: u64 = 8;
pub const SDK_FRAME_QUEUE_BYTES: u64 = 33554432;
pub const SDK_MAX_SCALED_FRAMES: u64 = 64;
pub const SDK_DECODER_INPUT_ITEMS: u64 = 64;
pub const SDK_DECODER_INPUT_BYTES: u64 = 16777216;
pub const SDK_QUEUE_MAX_WAITERS: u64 = 16;
pub const SDK_QUEUE_WAIT_MS: u64 = 5000;
pub const SDK_RESULT_TTL_MS: u64 = 60000;
pub const SDK_CONTROL_BYTES: u64 = 65536;
pub const SDK_SHARED_METADATA_PER_POOL: u64 = 65536;
pub const SDK_SHARED_WAIT_MAX_MS: u64 = 5000;
pub const SDK_SHARED_LOCK_WAIT_MS: u64 = 5000;
pub const SDK_SHUTDOWN_TIMEOUT_MS: u64 = 5000;
pub const SDK_INFER_TIMEOUT_MS: u64 = 30000;
pub const SDK_CAPTURE_TIMEOUT_MS: u64 = 10000;
pub const SDK_RECEIVE_TIMEOUT_MS: u64 = 1000;
pub const SDK_PROBE_TTL_MS: u64 = 300000;
pub const SDK_RECONNECT_INITIAL_MS: u64 = 1000;
pub const SDK_RECONNECT_MAX_MS: u64 = 30000;
pub const SDK_RECONNECT_ENVELOPE_MS: u64 = 60000;
pub const SDK_RECONNECT_JITTER_MS: u64 = 500;
pub fn default_limits(vision: bool) -> am_limits_t {
    let mut limits = am_limits_t {
        max_sources: 16,
        max_subscribers_per_source: 64,
        max_decoders: 8,
        max_capture_continuous: 4,
        // 128MiB：2160p RGBA 帧(33MiB)+转换峰值(58MiB)+编码输入副本的
        // 稳定余量（96MiB 时 4K 压线，2026-10-29 下游实测反馈调整）。
        frame_budget_bytes: 134217728,
        external_hold_max_frames: 256,
        external_hold_max_bytes: 134217728,
        max_models: 8,
        infer_parallelism: 4,
        realtime_max_inflight: 16,
        result_budget_bytes: 67108864,
        result_max_items: 256,
        control_max_items: 32,
        preprocess_budget_bytes: 67108864,
        shared_pool_max_slots: 4,
        shared_pool_max_frame_bytes: 16777216,
        shared_pool_max_total_bytes: 67108864,
        shared_pool_max_consumers: 8,
        shared_pool_max_outstanding_borrows: 32,
        shared_pool_max_pools: 4,
        shared_pool_metadata_bytes: 262144,
        ..Default::default()
    };
    if vision {
        limits.max_sources = 0;
        limits.max_subscribers_per_source = 0;
        limits.max_decoders = 0;
        limits.max_capture_continuous = 0;
        limits.frame_budget_bytes = 0;
        limits.external_hold_max_frames = 0;
        limits.external_hold_max_bytes = limits.result_budget_bytes;
    } else {
        limits.max_models = 0;
        limits.infer_parallelism = 0;
        limits.realtime_max_inflight = 0;
        limits.preprocess_budget_bytes = 0;
    }
    limits
}
pub fn limits_within(candidate: &am_limits_t, ceiling: &am_limits_t) -> bool {
    candidate.reserved.iter().all(|v| *v == 0)
        && candidate.max_sources <= ceiling.max_sources
        && candidate.max_subscribers_per_source <= ceiling.max_subscribers_per_source
        && candidate.max_decoders <= ceiling.max_decoders
        && candidate.max_capture_continuous <= ceiling.max_capture_continuous
        && candidate.frame_budget_bytes <= ceiling.frame_budget_bytes
        && candidate.external_hold_max_frames <= ceiling.external_hold_max_frames
        && candidate.external_hold_max_bytes <= ceiling.external_hold_max_bytes
        && candidate.max_models <= ceiling.max_models
        && candidate.infer_parallelism <= ceiling.infer_parallelism
        && candidate.realtime_max_inflight <= ceiling.realtime_max_inflight
        && candidate.result_budget_bytes <= ceiling.result_budget_bytes
        && candidate.result_max_items <= ceiling.result_max_items
        && candidate.control_max_items <= ceiling.control_max_items
        && candidate.preprocess_budget_bytes <= ceiling.preprocess_budget_bytes
        && candidate.shared_pool_max_slots <= ceiling.shared_pool_max_slots
        && candidate.shared_pool_max_frame_bytes <= ceiling.shared_pool_max_frame_bytes
        && candidate.shared_pool_max_total_bytes <= ceiling.shared_pool_max_total_bytes
        && candidate.shared_pool_max_consumers <= ceiling.shared_pool_max_consumers
        && candidate.shared_pool_max_outstanding_borrows
            <= ceiling.shared_pool_max_outstanding_borrows
        && candidate.shared_pool_max_pools <= ceiling.shared_pool_max_pools
        && candidate.shared_pool_metadata_bytes <= ceiling.shared_pool_metadata_bytes
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn domains_expose_only_their_limits() {
        let c = default_limits(false);
        let v = default_limits(true);
        assert_eq!(c.max_models, 0);
        assert_eq!(v.max_sources, 0);
        assert_eq!(c.shared_pool_max_total_bytes, 64 * 1024 * 1024);
        assert_eq!(v.external_hold_max_bytes, v.result_budget_bytes);
    }
    #[test]
    fn tightening_allows_disable_but_not_reexpansion() {
        let cap = default_limits(false);
        let mut next = cap;
        next.max_sources = 0;
        assert!(limits_within(&next, &cap));
        assert!(!limits_within(&cap, &next));
        next = cap;
        next.frame_budget_bytes += 1;
        assert!(!limits_within(&next, &cap));
        next = cap;
        next.reserved[0] = 1;
        assert!(!limits_within(&next, &cap));
    }
}
