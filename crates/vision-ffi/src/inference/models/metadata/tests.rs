use super::*;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static FAIL: Cell<Option<usize>> = const { Cell::new(None) };
    static CALLS: Cell<usize> = const { Cell::new(0) };
    static LIVE: Cell<isize> = const { Cell::new(0) };
}
struct Counter;
fn reject() -> bool {
    if !ACTIVE.try_with(Cell::get).unwrap_or(false) {
        return false;
    }
    CALLS.with(|v| v.set(v.get() + 1));
    FAIL.with(|v| match v.get() {
        Some(0) => {
            v.set(None);
            true
        }
        Some(n) => {
            v.set(Some(n - 1));
            false
        }
        None => false,
    })
}
fn bytes(amount: isize) {
    if ACTIVE.try_with(Cell::get).unwrap_or(false) {
        LIVE.with(|v| v.set(v.get() + amount));
    }
}
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if reject() {
            return std::ptr::null_mut();
        }
        let value = unsafe { System.alloc(layout) };
        if !value.is_null() {
            bytes(layout.size() as isize);
        }
        value
    }
    unsafe fn dealloc(&self, value: *mut u8, layout: Layout) {
        bytes(-(layout.size() as isize));
        unsafe { System.dealloc(value, layout) };
    }
    unsafe fn realloc(&self, value: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if reject() {
            return std::ptr::null_mut();
        }
        let next = unsafe { System.realloc(value, layout, size) };
        if !next.is_null() {
            bytes(size as isize - layout.size() as isize);
        }
        next
    }
}
#[global_allocator]
static ALLOCATOR: Counter = Counter;
fn measured<T>(failure: Option<usize>, run: impl FnOnce() -> T) -> (T, usize, isize) {
    CALLS.with(|v| v.set(0));
    LIVE.with(|v| v.set(0));
    FAIL.with(|v| v.set(failure));
    ACTIVE.with(|v| v.set(true));
    let result = run();
    ACTIVE.with(|v| v.set(false));
    FAIL.with(|v| v.set(None));
    (result, CALLS.with(Cell::get), LIVE.with(Cell::get))
}
fn specs(count: usize) -> Vec<TensorInfo> {
    (0..count)
        .map(|_| TensorInfo {
            name: "x".repeat(4096),
            dtype: 0,
            shape: vec![-1; 8],
        })
        .collect()
}
#[test]
fn maximal_metadata_is_exact_owned_and_dynamic_contract_is_preserved() {
    let specs = specs(32);
    let (owners, views) = ios(&specs, 1).unwrap();
    let copies = clone_specs(&specs).unwrap();
    assert_eq!(
        (owners.capacity(), views.capacity(), copies.capacity()),
        (32, 32, 32)
    );
    for ((owner, view), copied) in owners.iter().zip(&views).zip(&copies) {
        assert_eq!(owner.name.as_bytes_with_nul().len(), 4097);
        assert_eq!(
            (
                owner.shape.capacity(),
                owner.min.capacity(),
                owner.max.capacity()
            ),
            (8, 8, 8)
        );
        assert_eq!(owner.min, vec![1; 8]);
        assert_eq!(owner.max, [64, 8192, 8192, 8192, 8192, 8192, 8192, 8192]);
        assert_eq!(view.name, owner.name.as_ptr());
        assert_eq!(view.shape, owner.shape.as_ptr());
        assert_eq!(view.dyn_min, owner.min.as_ptr());
        assert_eq!(view.dyn_max, owner.max.as_ptr());
        assert_eq!(copied.name.capacity(), 4096);
        assert_eq!(copied.shape.capacity(), 8);
    }
    let io_bytes = owners.capacity() * std::mem::size_of::<Io>()
        + views.capacity() * std::mem::size_of::<am_io_spec_t>()
        + owners
            .iter()
            .map(|o| {
                o.name.as_bytes_with_nul().len()
                    + (o.shape.capacity() + o.min.capacity() + o.max.capacity()) * 8
            })
            .sum::<usize>();
    let copy_bytes = copies.capacity() * std::mem::size_of::<TensorInfo>()
        + copies
            .iter()
            .map(|s| s.name.capacity() + s.shape.capacity() * 8)
            .sum::<usize>();
    eprintln!("model_metadata io_items=32 name_bytes=4096 rank=8 one_direction_owned_bytes={io_bytes} copied_input_bytes={copy_bytes} total_io_pair_and_input_copy={}",2*io_bytes+copy_bytes);
    let (limit_owners, limits) = ios(&specs, 2).unwrap();
    assert_eq!(limit_owners[0].max[0], 8192);
    assert_eq!(limits[0].dyn_max, limit_owners[0].max.as_ptr());
}
#[test]
fn every_metadata_allocation_can_fail_without_retained_partial_storage() {
    let specs = specs(32);
    for copied in [false, true] {
        let (_, calls, live) = measured(None, || {
            if copied {
                drop(clone_specs(&specs).unwrap());
            } else {
                drop(ios(&specs, 1).unwrap());
            }
        });
        assert!(calls > 0);
        assert_eq!(live, 0);
        for at in 0..calls {
            let (code, _, retained) = measured(Some(at), || {
                if copied {
                    clone_specs(&specs).err()
                } else {
                    ios(&specs, 1).err()
                }
            });
            assert_eq!(
                code,
                Some(AM_ERR_SDK_RESOURCE_EXHAUSTED),
                "copied={copied} at={at}"
            );
            assert_eq!(retained, 0, "copied={copied} at={at}");
        }
        eprintln!("model_metadata copied_specs={copied} fallible_allocation_points={calls} rejected_partial_live_bytes=0");
    }
    assert!(ios(&specs, 1).is_ok());
    assert!(clone_specs(&specs).is_ok());
}
#[test]
fn unsupported_metadata_is_rejected_before_allocating() {
    let mut invalid = specs(33);
    for mode in 0..4 {
        match mode {
            0 => (),
            1 => {
                invalid.truncate(32);
                invalid[0].shape.push(1);
            }
            2 => {
                invalid[0].shape.truncate(8);
                invalid[0].name.push('x');
            }
            _ => invalid[0].name = "bad\0name".into(),
        }
        let (code, calls, live) = measured(None, || ios(&invalid, 1).err());
        assert_eq!(code, Some(AM_ERR_VISION_IO_UNSUPPORTED));
        assert_eq!((calls, live), (0, 0));
    }
    let empty: Vec<TensorInfo> = vec![];
    assert_eq!(
        clone_specs(&empty).err(),
        Some(AM_ERR_VISION_IO_UNSUPPORTED)
    );
}

fn legacy_ios(specs: &[TensorInfo], batch: u32) -> Result<(Vec<Io>, Vec<am_io_spec_t>)> {
    let owners = specs
        .iter()
        .map(|s| {
            Ok(Io {
                name: CString::new(s.name.clone()).map_err(|_| AM_ERR_VISION_IO_UNSUPPORTED)?,
                shape: s.shape.clone(),
                min: s
                    .shape
                    .iter()
                    .map(|v| if *v < 0 { 1 } else { *v })
                    .collect(),
                max: s
                    .shape
                    .iter()
                    .enumerate()
                    .map(|(axis, v)| {
                        if *v >= 0 {
                            *v
                        } else if axis == 0 && batch == 1 {
                            64
                        } else {
                            8192
                        }
                    })
                    .collect(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let views = owners
        .iter()
        .zip(specs)
        .map(|(o, s)| am_io_spec_t {
            name: o.name.as_ptr(),
            dtype: s.dtype,
            shape: o.shape.as_ptr(),
            shape_len: o.shape.len() as u32,
            dyn_min: o.min.as_ptr(),
            dyn_max: o.max.as_ptr(),
            ..Default::default()
        })
        .collect();
    Ok((owners, views))
}

#[test]
fn equivalent_metadata_copy_allocation_work_is_compared_without_speed_claim() {
    let specs = specs(32);
    let (_, old_calls, old_live) = measured(None, || drop(legacy_ios(&specs, 1).unwrap()));
    let (_, new_calls, new_live) = measured(None, || drop(ios(&specs, 1).unwrap()));
    assert_eq!((old_live, new_live), (0, 0));
    assert!(new_calls <= old_calls);
    eprintln!("model_metadata allocation_comparison equivalent_io_items=32 previous_calls={old_calls} exact_fallible_calls={new_calls}; no_latency_claim=true");
}
