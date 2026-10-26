use am_camera::media::codec::{access_units, visit_access_units, DecodeError, VideoCodec};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
type Result<T> = std::result::Result<T, DecodeError>;
thread_local! { static TRACK: Cell<(bool, usize, usize)> = const { Cell::new((false, 0, 0)) }; }
struct Counter;
fn record(bytes: usize) {
    let _ = TRACK.try_with(|state| {
        let (on, n, total) = state.get();
        if on {
            state.set((on, n + 1, total + bytes));
        }
    });
}
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(size);
        unsafe { System.realloc(p, layout, size) }
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Counter = Counter;
fn measured<T>(f: impl FnOnce() -> T) -> (T, usize, usize) {
    TRACK.with(|s| s.set((true, 0, 0)));
    let value = f();
    let (_, n, bytes) = TRACK.with(|s| s.replace((false, 0, 0)));
    (value, n, bytes)
}
/// 对完整Annex B包做边界检查，不把keyframe标志当真实随机访问点。
fn reference(codec: VideoCodec, data: &[u8]) -> Result<Vec<&[u8]>> {
    if data.is_empty() || data.len() > 16 * 1024 * 1024 {
        return Err(DecodeError::InvalidInput);
    }
    let start = |at: usize| -> usize {
        if data.get(at..at + 4) == Some(&[0, 0, 0, 1]) {
            4
        } else if data.get(at..at + 3) == Some(&[0, 0, 1]) {
            3
        } else {
            0
        }
    };
    let first = start(0);
    if first == 0 {
        return Err(DecodeError::InvalidInput);
    }
    let mut units = Vec::new();
    let mut begin = first;
    let mut at = begin;
    while at < data.len() {
        let n = start(at);
        if n != 0 {
            if at == begin {
                return Err(DecodeError::InvalidInput);
            }
            units.push(&data[begin..at]);
            if units.len() > 4096 {
                return Err(DecodeError::InvalidInput);
            }
            begin = at + n;
            at = begin;
        } else {
            at += 1;
        }
    }
    if begin == data.len() {
        return Err(DecodeError::InvalidInput);
    }
    units.push(&data[begin..]);
    for nal in &units {
        match codec {
            VideoCodec::H264 => {
                if nal[0] & 128 != 0 || nal[0] & 31 == 0 || nal[0] & 31 > 23 {
                    return Err(DecodeError::InvalidInput);
                }
            }
            VideoCodec::H265 => {
                if nal.len() < 2 || nal[0] & 128 != 0 || nal[1] & 7 == 0 {
                    return Err(DecodeError::InvalidInput);
                }
            }
        }
    }
    Ok(units)
}

#[test]
fn visitor_matches_frozen_parser_for_boundaries_and_deterministic_inputs() {
    for codec in [VideoCodec::H264, VideoCodec::H265] {
        let mut seed = 77u64;
        for index in 0..4096 {
            let mut data = Vec::new();
            if index % 3 != 0 {
                data.extend_from_slice(&[0, 0, 1, 0x26, 1]);
            }
            for _ in 0..index % 129 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                data.push((seed >> 32) as u8);
            }
            if index % 4 == 0 {
                data.extend_from_slice(&[0, 0, 0, 1, 0x65]);
            }
            assert_eq!(access_units(codec, &data), reference(codec, &data));
        }
        for count in [4096, 4097, 4098] {
            let nal = if codec == VideoCodec::H264 {
                &b"\0\0\x01\x65"[..]
            } else {
                &b"\0\0\x01\x26\x01"[..]
            };
            let data = nal.repeat(count);
            assert_eq!(access_units(codec, &data), reference(codec, &data));
        }
    }
    let mut visits = 0;
    assert!(
        visit_access_units(VideoCodec::H264, &[0, 0, 1, 0x65, 3, 0, 0, 1, 0xff], |_| {
            visits += 1
        })
        .is_err()
    );
    assert_eq!(visits, 1);
}
#[test]
fn visitor_removes_only_the_nal_list_allocation_and_keeps_borrowed_bytes() {
    for count in [1, 4, 64, 4097] {
        let data = b"\0\0\x01\x65".repeat(count);
        let (old, allocs, bytes) = measured(|| access_units(VideoCodec::H264, &data).unwrap());
        let mut seen = 0;
        let ((), new_allocs, new_bytes) = measured(|| {
            visit_access_units(VideoCodec::H264, &data, |nal| {
                assert_eq!(nal, old[seen]);
                seen += 1;
            })
            .unwrap()
        });
        assert_eq!(seen, count);
        assert!(allocs > 0);
        assert_eq!((new_allocs, new_bytes), (0, 0));
        println!("nal-allocation {{\"nals\":{count},\"vec_allocations\":{allocs},\"vec_requested_bytes\":{bytes},\"visitor_allocations\":{new_allocs},\"visitor_requested_bytes\":{new_bytes}}}");
    }
}
