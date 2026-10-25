use am_sdk_support::{abi::*, common::version};
use std::{ffi::CStr, mem::size_of};

#[test]
fn version_matches_cargo_and_strings_outlive_the_call() {
    let mut first = am_version_info_t {
        struct_size: size_of::<am_version_info_t>() as u32,
        ..Default::default()
    };
    unsafe { version(&mut first).unwrap() };
    let package = unsafe { CStr::from_ptr(first.package_version_utf8) };
    assert_eq!(package.to_str().unwrap(), env!("CARGO_PKG_VERSION"));
    assert_eq!((first.abi_major, first.abi_minor), (1, 0));
    let target = unsafe { CStr::from_ptr(first.build_target_utf8) }
        .to_str()
        .unwrap();
    assert!(!target.is_empty());
    let notes = unsafe { CStr::from_ptr(first.runtime_notes_utf8) }
        .to_str()
        .unwrap();
    assert!(notes.contains("source_commit="));
    assert!(notes.contains("profile="));
    assert!(notes.contains("runtimes=on-demand"));
    let mut second = am_version_info_t {
        struct_size: first.struct_size,
        ..Default::default()
    };
    unsafe { version(&mut second).unwrap() };
    assert_eq!(first.package_version_utf8, second.package_version_utf8);
    assert_eq!(first.runtime_notes_utf8, second.runtime_notes_utf8);
    assert_eq!(package.to_str().unwrap(), env!("CARGO_PKG_VERSION"));
}

#[test]
fn version_keeps_existing_pointer_and_short_buffer_contracts() {
    assert_eq!(
        unsafe { version(std::ptr::null_mut()) },
        Err(AM_ERR_SDK_NULL_ARGUMENT)
    );
    let mut short = am_version_info_t {
        struct_size: 4,
        ..Default::default()
    };
    assert_eq!(unsafe { version(&mut short) }, Ok(()));
    assert_eq!(short.struct_size, 4);
    assert!(short.package_version_utf8.is_null());
    short.struct_size = 0;
    assert_eq!(
        unsafe { version(&mut short) },
        Err(AM_ERR_SDK_STRUCT_SIZE_INVALID)
    );
}
