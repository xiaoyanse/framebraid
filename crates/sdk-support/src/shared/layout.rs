//! 协议1只存小端定宽值，不把C结构填充、Rust指针或锁布局写进共享段。
use crate::{abi::*, Result};

pub const CONTROL_BYTES: usize = 65536;
pub const HEADER_BYTES: usize = 512;
pub const SLOT_BASE: usize = 512;
pub const SLOT_BYTES: usize = 512;
pub const CONSUMER_BASE: usize = 4096;
pub const CONSUMER_BYTES: usize = 64;
pub const LEASE_BASE: usize = 8192;
pub const LEASE_BYTES: usize = 64;
pub const RELEASE_OFFSET: usize = 24;
pub const MAGIC: &[u8; 8] = b"AMRTSFP1";
pub const DESC_BYTES: usize = 172;

pub fn configuration(l: &am_limits_t) -> am_shared_pool_config_t {
    am_shared_pool_config_t {
        slot_count: l.shared_pool_max_slots,
        max_frame_bytes: l.shared_pool_max_frame_bytes,
        total_bytes: l.shared_pool_max_total_bytes,
        max_consumers: l.shared_pool_max_consumers,
        max_outstanding_borrows: l.shared_pool_max_outstanding_borrows,
        ..Default::default()
    }
}
pub fn validate_config(c: &am_shared_pool_config_t, l: &am_limits_t) -> Result<()> {
    if c.slot_count == 0
        || c.slot_count > l.shared_pool_max_slots.min(4)
        || c.max_frame_bytes == 0
        || c.max_frame_bytes > l.shared_pool_max_frame_bytes
        || c.total_bytes != u64::from(c.slot_count) * u64::from(c.max_frame_bytes)
        || c.total_bytes > l.shared_pool_max_total_bytes
        || c.max_consumers == 0
        || c.max_consumers > l.shared_pool_max_consumers.min(8)
        || c.max_outstanding_borrows == 0
        || c.max_outstanding_borrows > l.shared_pool_max_outstanding_borrows.min(32)
        || c.drop_policy > AM_SFP_DROP_WAIT
        || c.publish_wait_ms > SDK_SHARED_WAIT_MAX_MS as u32
        || c.reserved.iter().any(|x| *x != 0)
        || (c.drop_policy == AM_SFP_DROP_OLDEST && c.publish_wait_ms != 0)
    {
        return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
    }
    Ok(())
}

pub fn encode(d: &am_shared_frame_desc_t) -> [u8; DESC_BYTES] {
    let mut bytes = [0; DESC_BYTES];
    let mut at = 0;
    macro_rules! field {
        ($v:expr) => {{
            let b = $v.to_le_bytes();
            bytes[at..at + b.len()].copy_from_slice(&b);
            at += b.len();
        }};
    }
    field!(d.protocol_version);
    field!(d.pixel_format);
    field!(d.width);
    field!(d.height);
    field!(d.plane_count);
    for x in d.stride_bytes {
        field!(x);
    }
    for x in d.offset_bytes {
        field!(x);
    }
    for x in d.plane_valid_bytes {
        field!(x);
    }
    field!(d.color_matrix);
    field!(d.color_range);
    field!(d.publication_id);
    field!(d.frame_id);
    field!(d.source_id);
    field!(d.source_generation);
    field!(d.config_generation);
    field!(d.assoc_valid);
    field!(d.pool_generation);
    field!(d.time_flags);
    field!(d.pts);
    field!(d.time_base_num);
    field!(d.time_base_den);
    field!(d.captured_at_ms);
    field!(d.received_at_ms);
    field!(d.clock_quality);
    for x in d.reserved {
        field!(x);
    }
    debug_assert_eq!(at, DESC_BYTES);
    bytes
}
pub fn decode(b: &[u8; DESC_BYTES]) -> am_shared_frame_desc_t {
    let mut at = 0;
    macro_rules! take {
        ($t:ty) => {{
            let mut x = [0u8; std::mem::size_of::<$t>()];
            let end = at + x.len();
            x.copy_from_slice(&b[at..end]);
            at = end;
            <$t>::from_le_bytes(x)
        }};
    }
    let mut d = am_shared_frame_desc_t {
        protocol_version: take!(u32),
        pixel_format: take!(u32),
        width: take!(u32),
        height: take!(u32),
        plane_count: take!(u32),
        ..Default::default()
    };
    for x in &mut d.stride_bytes {
        *x = take!(u32);
    }
    for x in &mut d.offset_bytes {
        *x = take!(u32);
    }
    for x in &mut d.plane_valid_bytes {
        *x = take!(u32);
    }
    d.color_matrix = take!(u32);
    d.color_range = take!(u32);
    d.publication_id = take!(u64);
    d.frame_id = take!(u64);
    d.source_id = take!(u64);
    d.source_generation = take!(u32);
    d.config_generation = take!(u32);
    d.assoc_valid = take!(u32);
    d.pool_generation = take!(u32);
    d.time_flags = take!(u32);
    d.pts = take!(i64);
    d.time_base_num = take!(u32);
    d.time_base_den = take!(u32);
    d.captured_at_ms = take!(i64);
    d.received_at_ms = take!(i64);
    d.clock_quality = take!(u32);
    for x in &mut d.reserved {
        *x = take!(u32);
    }
    debug_assert_eq!(at, DESC_BYTES);
    d
}

pub fn plane_shape(format: u32, width: u32, height: u32) -> Result<Vec<(u64, u64)>> {
    if width == 0
        || height == 0
        || width > SDK_MAX_DYNAMIC_AXIS as u32
        || height > SDK_MAX_DYNAMIC_AXIS as u32
    {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    let w = u64::from(width);
    let h = u64::from(height);
    match format as i32 {
        AM_PF_RGBA8 => Ok(vec![(w * 4, h)]),
        AM_PF_RGB8 | AM_PF_BGR8 => Ok(vec![(w * 3, h)]),
        AM_PF_NV12 if width.is_multiple_of(2) && height.is_multiple_of(2) => {
            Ok(vec![(w, h), (w, h / 2)])
        }
        _ => Err(AM_ERR_SDK_INVALID_ARGUMENT),
    }
}
pub fn validate_desc(d: &am_shared_frame_desc_t, capacity: u64) -> Result<usize> {
    let shape = plane_shape(d.pixel_format, d.width, d.height)?;
    let required = AM_ASSOC_SOURCE | AM_ASSOC_FRAME | AM_ASSOC_GENERATION;
    if d.protocol_version != AM_SHARED_PROTOCOL_VERSION
        || d.plane_count as usize != shape.len()
        || d.assoc_valid & required != required
        || d.assoc_valid & !31 != 0
        || d.time_flags & !7 != 0
        || d.color_matrix > 2
        || d.color_range > 1
        || d.clock_quality > 3
        || d.reserved.iter().any(|x| *x != 0)
        || (d.time_flags & AM_CLK_PTS != 0
            && (d.pts == i64::MIN || d.time_base_num == 0 || d.time_base_den == 0))
        || (d.time_flags & AM_CLK_PTS == 0
            && (d.pts != i64::MIN || d.time_base_num != 0 || d.time_base_den != 0))
        || ((d.assoc_valid & AM_ASSOC_PTS != 0) != (d.time_flags & AM_CLK_PTS != 0))
        || ((d.assoc_valid & AM_ASSOC_CAPTURED != 0) != (d.time_flags & AM_CLK_CAPTURED != 0))
        || (d.time_flags & AM_CLK_CAPTURED == 0 && d.captured_at_ms != i64::MIN)
        || (d.time_flags & AM_CLK_RECEIVED == 0 && d.received_at_ms != i64::MIN)
        || (d.time_flags & AM_CLK_CAPTURED != 0 && d.captured_at_ms == i64::MIN)
        || (d.time_flags & AM_CLK_RECEIVED != 0 && d.received_at_ms == i64::MIN)
    {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    let mut end = 0u64;
    for (i, (row, rows)) in shape.into_iter().enumerate() {
        let stride = u64::from(d.stride_bytes[i]);
        let valid = u64::from(d.plane_valid_bytes[i]);
        let offset = u64::from(d.offset_bytes[i]);
        if stride < row
            || valid < stride * (rows - 1) + row
            || offset < end
            || offset + valid > capacity
        {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        end = offset + valid;
    }
    for i in d.plane_count as usize..4 {
        if d.stride_bytes[i] != 0 || d.offset_bytes[i] != 0 || d.plane_valid_bytes[i] != 0 {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
    }
    Ok(end as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocol_has_fixed_offsets_and_validates_planes() {
        let d = am_shared_frame_desc_t {
            protocol_version: 1,
            pixel_format: AM_PF_NV12 as u32,
            width: 8,
            height: 6,
            plane_count: 2,
            stride_bytes: [8, 8, 0, 0],
            offset_bytes: [0, 48, 0, 0],
            plane_valid_bytes: [48, 24, 0, 0],
            assoc_valid: 7,
            source_id: 0x1122334455667788,
            frame_id: u64::MAX,
            pts: i64::MIN,
            captured_at_ms: i64::MIN,
            received_at_ms: i64::MIN,
            ..Default::default()
        };
        let encoded = encode(&d);
        assert_eq!(encoded.len(), 172);
        assert_eq!(encode(&decode(&encoded)), encoded);
        assert_eq!(validate_desc(&d, 72), Ok(72));
        let mut bad = d;
        bad.offset_bytes[1] = 47;
        assert!(validate_desc(&bad, 72).is_err());
        bad = d;
        bad.stride_bytes[0] = u32::MAX;
        assert!(validate_desc(&bad, 72).is_err());
        bad = d;
        bad.time_flags = AM_CLK_PTS;
        assert!(validate_desc(&bad, 72).is_err());
        bad = d;
        bad.width = 7;
        assert!(validate_desc(&bad, 72).is_err());
        assert_eq!(LEASE_BASE % 8, 0);
        assert_eq!(RELEASE_OFFSET % 8, 0);
        const {
            assert!(LEASE_BASE + LEASE_BYTES * 32 < CONTROL_BYTES);
        }
    }
}
