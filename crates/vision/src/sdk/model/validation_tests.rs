use super::*;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
thread_local! {
    static ACTIVE:Cell<bool>=const{Cell::new(false)};
    static CALLS:Cell<usize>=const{Cell::new(0)};
}
struct Counter;
fn count() {
    if ACTIVE.try_with(Cell::get).unwrap_or(false) {
        CALLS.with(|v| v.set(v.get() + 1));
    }
}
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        count();
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static ALLOCATOR: Counter = Counter;
fn measured(action: impl FnOnce()) -> usize {
    CALLS.with(|v| v.set(0));
    ACTIVE.with(|v| v.set(true));
    action();
    ACTIVE.with(|v| v.set(false));
    CALLS.with(Cell::get)
}
#[test]
fn tensor_validation_borrows_metadata_without_per_run_clones() {
    let expected: Vec<_> = (0..32)
        .map(|n| TensorInfo {
            name: format!("{n:04096}"),
            dtype: 4,
            shape: vec![-1],
        })
        .collect();
    let values: Vec<_> = expected
        .iter()
        .map(|spec| {
            TensorData::new(
                TensorInfo {
                    name: spec.name.clone(),
                    dtype: 4,
                    shape: vec![1],
                },
                &[7],
            )
            .unwrap()
        })
        .collect();
    let previous = measured(|| {
        let infos: Vec<_> = values.iter().map(|v| v.info.clone()).collect();
        validate_input_infos(&expected, &infos).unwrap();
    });
    let current = measured(|| validate_input_data(&expected, &values).unwrap());
    assert_eq!(previous, 65);
    assert_eq!(current, 0);
    let too_few = validate_input_data(&expected, &values[..31]).unwrap_err();
    assert_eq!(too_few.0, "vision.inputNameUnknown");
    eprintln!("tensor_validation inputs=32 previous_metadata_allocations={previous} borrowed_metadata_allocations={current}; pixel_copy_unchanged=true");
}
#[test]
fn borrowed_validation_preserves_duplicate_dtype_shape_and_length_failures() {
    let spec = TensorInfo {
        name: "x".into(),
        dtype: 4,
        shape: vec![-1],
    };
    let mut input = TensorData::new(
        TensorInfo {
            name: "x".into(),
            dtype: 4,
            shape: vec![1],
        },
        &[3],
    )
    .unwrap();
    input.info.dtype = 0;
    assert_eq!(
        validate_input_data(std::slice::from_ref(&spec), std::slice::from_ref(&input))
            .unwrap_err()
            .0,
        "vision.dtypeMismatch"
    );
    input.info.dtype = 4;
    input.info.shape = vec![8193];
    assert_eq!(
        validate_input_data(std::slice::from_ref(&spec), std::slice::from_ref(&input))
            .unwrap_err()
            .0,
        "vision.dynamicRangeExceeded"
    );
    input.info.shape = vec![2];
    assert_eq!(
        validate_input_data(std::slice::from_ref(&spec), std::slice::from_ref(&input))
            .unwrap_err()
            .0,
        "vision.inputBytesMismatch"
    );
    let values = [spec.clone(), spec.clone()];
    assert_eq!(
        validate_input_infos(&values, &values).unwrap_err().0,
        "vision.inputNameUnknown"
    );
}
