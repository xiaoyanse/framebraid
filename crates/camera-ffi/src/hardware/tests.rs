use super::*;
use crate::frames::registry_tests::allocations;
use am_camera_core::media::{
    codec::selection::{
        BackendSupport, SelectedDevice, SelectionAttempt, SelectionFailure, SelectionReport,
    },
    hardware::{Adapter, Backend, ProbeError},
};

#[test]
fn fixed_serializer_refuses_before_growth_and_keeps_exact_output_limit() {
    let value = "a".repeat(REPORT_BYTES - 3);
    let (result, count) = allocations(|| serialize(&value));
    let result = result.unwrap();
    assert_eq!(count, 1);
    assert_eq!(result.len(), REPORT_BYTES - 1);
    assert_eq!(result.capacity(), REPORT_BYTES);
    let too_large = "a".repeat(REPORT_BYTES - 2);
    let (result, count) = allocations(|| serialize(&too_large));
    assert_eq!(result, Err(AM_ERR_SDK_RESOURCE_EXHAUSTED));
    // 序列化错误可能分配错误体，但输出Vec绝不扩容。
    assert!(count >= 1);
    let mut bounded = BoundedJson(Vec::with_capacity(REPORT_BYTES));
    std::io::Write::write_all(&mut bounded, &vec![0; REPORT_BYTES - 1]).unwrap();
    let (result, count) = allocations(|| std::io::Write::write_all(&mut bounded, &[1]));
    assert!(result.is_err());
    assert_eq!(bounded.0.len(), REPORT_BYTES - 1);
    assert_eq!(bounded.0.capacity(), REPORT_BYTES);
    assert_eq!(count, 0);
    assert_eq!(serialize(&"retry").unwrap(), b"\"retry\"");
}

#[test]
fn worst_native_selection_fits_and_avoids_json_tree_allocations() {
    let report = SelectionReport {
        policy: HardwarePolicy::PreferHardware,
        selected: Some(SelectedDevice {
            backend: Backend::D3d11va,
            adapter: Adapter {
                index: u32::MAX,
                name: "\u{1}".repeat(128),
                vendor_id: u32::MAX,
                device_id: u32::MAX,
                luid: u64::MAX,
                software: false,
                dedicated_video_bytes: u64::MAX,
                dxva2_index: Some(u32::MAX),
                dxva2_mapping_error: Some(ProbeError::AdapterQuery(i32::MIN)),
            },
        }),
        attempts: (0..16)
            .map(|_| SelectionAttempt {
                backend: Backend::Dxva2,
                adapter_index: u32::MAX,
                adapter_luid: u64::MAX,
                error: Some(SelectionFailure::Inventory(ProbeError::DependencyMissing(
                    "d3d11.dll",
                ))),
            })
            .collect(),
        backend_support: [Backend::D3d11va, Backend::Dxva2].map(|backend| BackendSupport {
            backend,
            compiled: Some(true),
            codec_configured: Some(false),
        }),
        fallback: Some(SelectionFailure::NoCompatibleHardware),
        elapsed_ms: u64::MAX as u128,
    };
    let (old, old_count) = allocations(|| {
        let value = serde_json::to_value(&report).unwrap();
        serde_json::to_vec(&value).unwrap()
    });
    let (new, new_count) = allocations(|| serialize(&report).unwrap());
    assert_eq!(new_count, 1);
    assert!(old_count > new_count);
    assert!(new.len() + 2048 < REPORT_BYTES, "为三个外层报告保留空间");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&old).unwrap(),
        serde_json::from_slice::<serde_json::Value>(&new).unwrap()
    );
    println!("r3_decode_report old_allocations={old_count} new_allocations={new_count} json_bytes={} backing_bytes={REPORT_BYTES} cache_overhead_bytes={}",new.len(),std::mem::size_of::<Cached>()+16);
}

#[test]
fn report_keeps_hardware_failure_categories_and_native_code() {
    let report = SelectionReport {
        policy: HardwarePolicy::PreferHardware,
        selected: None,
        attempts: [
            ProbeError::NotCompiled,
            ProbeError::AdapterIdentityMismatch,
            ProbeError::DeviceInit(-12),
        ]
        .into_iter()
        .map(|error| SelectionAttempt {
            backend: Backend::D3d11va,
            adapter_index: 0,
            adapter_luid: 7,
            error: Some(SelectionFailure::Inventory(error)),
        })
        .collect(),
        backend_support: [Backend::D3d11va, Backend::Dxva2].map(|backend| BackendSupport {
            backend,
            compiled: Some(true),
            codec_configured: Some(false),
        }),
        fallback: Some(SelectionFailure::NoCompatibleHardware),
        elapsed_ms: 1,
    };
    let json: serde_json::Value = serde_json::from_slice(&serialize(&report).unwrap()).unwrap();
    assert_eq!(json["attempts"][0]["error"]["Inventory"], "NotCompiled");
    assert_eq!(
        json["attempts"][1]["error"]["Inventory"],
        "AdapterIdentityMismatch"
    );
    assert_eq!(json["attempts"][2]["error"]["Inventory"]["DeviceInit"], -12);
}
#[test]
fn no_adapter_report_keeps_backend_support_separate_from_device_failure() {
    let report = SelectionReport {
        policy: HardwarePolicy::PreferHardware,
        selected: None,
        attempts: Vec::new(),
        backend_support: [
            BackendSupport {
                backend: Backend::D3d11va,
                compiled: Some(true),
                codec_configured: Some(true),
            },
            BackendSupport {
                backend: Backend::Dxva2,
                compiled: Some(false),
                codec_configured: None,
            },
        ],
        fallback: Some(SelectionFailure::NoCompatibleHardware),
        elapsed_ms: 1,
    };
    let json: serde_json::Value = serde_json::from_slice(&serialize(&report).unwrap()).unwrap();
    assert!(json["attempts"].as_array().unwrap().is_empty());
    assert_eq!(json["backend_support"][0]["compiled"], true);
    assert_eq!(json["backend_support"][1]["compiled"], false);
    assert!(json["backend_support"][1]["codec_configured"].is_null());
    assert!(json["selected"].is_null());
}

#[test]
fn blocking_probe_rejects_callback_before_touching_handles_or_drivers() {
    let _callback = am_sdk_support::callbacks::Scope::enter();
    assert_eq!(
        unsafe { amc_probe_decode_hardware(std::ptr::null_mut(), AM_CODEC_H264, 1) },
        AM_ERR_SDK_CALLBACK_CONTRACT_VIOLATION
    );
}
#[test]
fn json_copy_is_owned_bounded_nul_terminated_and_rejects_overlap() {
    let mut needed = 0;
    assert_eq!(
        unsafe { copy_report(b"{}", std::ptr::null_mut(), 0, &mut needed) },
        Ok(())
    );
    assert_eq!(needed, 3);
    let mut out = [42i8; 4];
    assert_eq!(
        unsafe { copy_report(b"{}", out.as_mut_ptr(), 2, &mut needed) },
        Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE)
    );
    assert_eq!(out, [0, 42, 42, 42]);
    assert_eq!(
        unsafe { copy_report(b"{}", out.as_mut_ptr(), 4, &mut needed) },
        Ok(())
    );
    assert_eq!(out, [123, 125, 0, 42]);
    let mut overlap = 0u64;
    let ptr = &mut overlap as *mut u64;
    assert_eq!(
        unsafe { copy_report(b"{}", ptr.cast(), 8, ptr) },
        Err(AM_ERR_SDK_INVALID_ARGUMENT)
    );
}

#[test]
fn fixed_foundation_layout_snapshot() {
    println!("r3_media_hardware_foundation slot={} cached={} output_backing={} query_reservation={} arc_header_bytes=16",std::mem::size_of::<Slot>(),std::mem::size_of::<Cached>(),REPORT_BYTES,QUERY_RESERVATION);
}
