//! 离线前处理基准；只计生产prepare函数，摘要和输出在计时区外。
use am_vision::sdk::{pixels::Pixels, prepare, Preprocess};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Instant,
};

struct CountingAllocator;
static COUNTING: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;
// 仅独立基准进程单线程启用；统计Rust请求字节，不声称是RSS或原生堆分配。
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() && COUNTING.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let samples: usize = args.get(1).map_or(Ok(100), |s| s.parse())?;
    let warmup: usize = args.get(2).map_or(Ok(20), |s| s.parse())?;
    if !(1..=100_000).contains(&samples) || warmup > 10_000 {
        return Err("sample/warmup budget exceeded".into());
    }
    let mut rgba = vec![0u8; 1920 * 1080 * 4];
    for (i, value) in rgba.iter_mut().enumerate() {
        *value = ((i.wrapping_mul(17) + i / 1920) % 251) as u8;
    }
    let pixels = Pixels {
        width: 1920,
        height: 1080,
        format: 0,
        planes: vec![&rgba],
        strides: vec![1920 * 4],
        color_matrix: 0,
        color_range: 0,
    };
    let config = Preprocess::profile(640, false);
    let clock_average_ns = (0..1000)
        .map(|_| {
            let start = Instant::now();
            start.elapsed().as_nanos()
        })
        .sum::<u128>()
        / 1000;
    COUNTING.store(true, Ordering::Relaxed);
    let counted = prepare(&pixels, "images", config, 8 * 1024 * 1024);
    COUNTING.store(false, Ordering::Relaxed);
    let counted = counted?;
    drop(counted);
    let mut durations = Vec::with_capacity(samples);
    let mut expected = None;
    let mut checksum_ns = 0u128;
    for index in 0..(warmup + samples) {
        let start = Instant::now();
        let (tensor, transform) = prepare(black_box(&pixels), "images", config, 8 * 1024 * 1024)?;
        let elapsed = start.elapsed().as_nanos();
        let checksum_start = Instant::now();
        let checksum = tensor.bytes().iter().fold(0xcbf29ce484222325u64, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
        });
        checksum_ns += checksum_start.elapsed().as_nanos();
        if expected.is_some_and(|v| v != checksum) {
            return Err("non-deterministic tensor output".into());
        }
        expected = Some(checksum);
        if tensor.bytes().len() != 640 * 640 * 12 || transform.top != 140 {
            return Err("unexpected output layout/letterbox transform".into());
        }
        if index >= warmup {
            durations.push(elapsed);
        }
        black_box(&tensor);
    }
    println!(
        "{{\"workload\":\"rgba1920x1080-letterbox640-nchw-f32\",\"warmup\":{warmup},\"samples_ns\":{durations:?},\"checksum_fnv1a64\":\"{:016x}\",\"rust_allocations_per_prepare\":{},\"rust_allocated_bytes_per_prepare\":{},\"clock_average_ns\":{clock_average_ns},\"checksum_total_ns_outside_timing\":{checksum_ns},\"boundary\":\"prepare only; excludes input generation, output hash and output drop\"}}",
        expected.ok_or("no output")?, ALLOCS.load(Ordering::Relaxed), BYTES.load(Ordering::Relaxed)
    );
    Ok(())
}
