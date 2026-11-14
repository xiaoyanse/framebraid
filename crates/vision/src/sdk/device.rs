//! 设备定向选择器（ABI 1.6 §3.5b）：描述子串匹配+实例序号消歧，匹配域排除软件适配器。
//! 选择器只在 selection 消费点（模型加载）读取一次；设备枚举按需显式执行、不缓存。
use super::selection::BackendTarget;
use crate::{Error, Result};

/// 已解析的定向选择器：needle 为小写子串，instance 取同子串第 N 个匹配（0 起）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceSelector {
    needle: String,
    instance: u32,
}

impl DeviceSelector {
    pub fn new(needle: &str, instance: u32) -> Self {
        Self {
            needle: needle.to_lowercase(),
            instance,
        }
    }
    pub fn needle(&self) -> &str {
        &self.needle
    }
    pub fn instance(&self) -> u32 {
        self.instance
    }
}

/// 大小写不敏感子串匹配+实例序号消歧：返回第 instance 个名字含子串的条目标识；
/// 零命中或 instance 越界返回 None（调用方按配置错误拒绝，不静默回退其他适配器）。
pub fn select_by_substring<'a>(
    names: impl IntoIterator<Item = (u32, &'a str)>,
    needle: &str,
    instance: u32,
) -> Option<u32> {
    let needle = needle.to_lowercase();
    names
        .into_iter()
        .filter(|(_, name)| name.to_lowercase().contains(&needle))
        .nth(instance as usize)
        .map(|(id, _)| id)
}

/// DXGI 适配器条目（DML/OV 候选匹配域）：index 为原始枚举序（DML device_id 语义），
/// software 条目（WARP/软件渲染器）排除出匹配域。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DxgiAdapter {
    pub index: u32,
    pub description: String,
    pub software: bool,
}

/// selection 消费点的定向解析：selector 按各候选匹配域重写候选——CUDA 命中转为
/// 设备 ordinal（selector 优先于候选 device_id）；DML 命中转为 DXGI 序号写入
/// device_id（该值仅可由 selector 解析产生，外部候选 device_id≠0 已被校验拒绝）；
/// OV EP 候选做 DXGI 存在性校验并切换 GPU 设备串；OV 原生（E1）运行库仅含 CPU
/// 插件，定向按 notSupported 拒绝（含 GPU 插件的运行库接入后开放）。
/// 返回（重写后候选，OV 是否定向 GPU）；零命中/实例越界=sdk.invalidArgument。
pub(crate) fn resolve_targets(
    targets: &[BackendTarget],
    device: Option<&DeviceSelector>,
    cuda_names: &[(u32, String)],
    dxgi: &[DxgiAdapter],
) -> Result<(Vec<BackendTarget>, bool)> {
    let Some(selector) = device else {
        return Ok((targets.to_vec(), false));
    };
    let hardware: Vec<(u32, &str)> = dxgi
        .iter()
        .filter(|adapter| !adapter.software)
        .map(|adapter| (adapter.index, adapter.description.as_str()))
        .collect();
    let mut ov_gpu = false;
    let mut resolved = Vec::with_capacity(targets.len());
    for &target in targets {
        let mut target = target;
        match target.backend {
            0 => {}
            1 => {
                select_by_substring(
                    hardware.iter().copied(),
                    selector.needle(),
                    selector.instance,
                )
                .ok_or_else(|| Error("sdk.invalidArgument".into()))?;
                // OV 设备粒度为单 "GPU"（本机枚举无 GPU.0/1 细分）；DXGI 匹配作存在性校验。
                ov_gpu = true;
            }
            2 => {
                let ordinal = select_by_substring(
                    cuda_names
                        .iter()
                        .map(|(ordinal, name)| (*ordinal, name.as_str())),
                    selector.needle(),
                    selector.instance,
                )
                .ok_or_else(|| Error("sdk.invalidArgument".into()))?;
                target.device_id = ordinal;
            }
            3 => {
                let index = select_by_substring(
                    hardware.iter().copied(),
                    selector.needle(),
                    selector.instance,
                )
                .ok_or_else(|| Error("sdk.invalidArgument".into()))?;
                target.device_id = index;
            }
            _ => return Err(Error("sdk.notSupported".into())),
        }
        resolved.push(target);
    }
    Ok((resolved, ov_gpu))
}

/// 枚举 CUDA 设备名（CUDA 候选匹配域）：加载 nvcuda 并 cuInit 后逐 ordinal 取名；
/// 驱动不可用返回空匹配域（定向 CUDA 时按零命中拒绝）。
pub fn cuda_device_names() -> Vec<(u32, String)> {
    enumerate_cuda().unwrap_or_default()
}

/// 枚举 DXGI 适配器 Description（DML/OV 候选匹配域）。枚举失败视为平台不支持定向
/// （dxgi.dll 属系统组件，常规 Windows 总在）。
pub fn dxgi_adapters() -> Result<Vec<DxgiAdapter>> {
    enumerate_dxgi().ok_or_else(|| Error("sdk.notSupported".into()))
}

#[cfg(all(windows, target_pointer_width = "64"))]
fn enumerate_cuda() -> Option<Vec<(u32, String)>> {
    use std::{
        ffi::{c_char, c_void, CStr},
        ptr,
    };
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetSystemDirectoryW(out: *mut u16, size: u32) -> u32;
        fn LoadLibraryExW(path: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
        fn FreeLibrary(module: *mut c_void) -> i32;
    }
    let mut path = [0u16; 32768];
    let n = unsafe { GetSystemDirectoryW(path.as_mut_ptr(), path.len() as u32) } as usize;
    if n == 0 || n + 12 >= path.len() {
        return None;
    }
    let mut path = path[..n].to_vec();
    path.extend("\\nvcuda.dll\0".encode_utf16());
    let module = unsafe { LoadLibraryExW(path.as_ptr(), ptr::null_mut(), 0x800) };
    if module.is_null() {
        return None;
    }
    struct Library(*mut c_void);
    impl Drop for Library {
        fn drop(&mut self) {
            unsafe {
                FreeLibrary(self.0);
            }
        }
    }
    let library = Library(module);
    let symbol = |name: &CStr| unsafe { GetProcAddress(library.0, name.as_ptr()) };
    // cuInit 幂等：进程已有 CUDA 上下文时为空操作，与显式探测共用驱动安全。
    type Init = unsafe extern "system" fn(u32) -> i32;
    type Count = unsafe extern "system" fn(*mut i32) -> i32;
    type Get = unsafe extern "system" fn(*mut i32, i32) -> i32;
    type Name = unsafe extern "system" fn(*mut c_char, i32, i32) -> i32;
    let init: Init = unsafe { std::mem::transmute(symbol(c"cuInit")) };
    let count: Count = unsafe { std::mem::transmute(symbol(c"cuDeviceGetCount")) };
    let get: Get = unsafe { std::mem::transmute(symbol(c"cuDeviceGet")) };
    let name: Name = unsafe { std::mem::transmute(symbol(c"cuDeviceGetName")) };
    if unsafe { init(0) } != 0 {
        return None;
    }
    let mut total = 0;
    if unsafe { count(&mut total) } != 0 || total <= 0 {
        return None;
    }
    let mut devices = Vec::new();
    for ordinal in 0..total.min(16) {
        let mut device = 0;
        let mut label = [0u8; 256];
        if unsafe { get(&mut device, ordinal) } != 0 {
            return None;
        }
        if unsafe { name(label.as_mut_ptr().cast(), label.len() as i32, device) } != 0 {
            return None;
        }
        let end = label.iter().position(|b| *b == 0).unwrap_or(label.len());
        devices.push((
            ordinal as u32,
            String::from_utf8_lossy(&label[..end]).into_owned(),
        ));
    }
    Some(devices)
}

#[cfg(not(all(windows, target_pointer_width = "64")))]
fn enumerate_cuda() -> Option<Vec<(u32, String)>> {
    None
}

#[cfg(all(windows, target_pointer_width = "64"))]
fn enumerate_dxgi() -> Option<Vec<DxgiAdapter>> {
    use libloading::os::windows as ll;
    use std::{ffi::c_void, mem::transmute, ptr};
    // 布局对照 dxgi.h 的 DXGI_ADAPTER_DESC1（与 camera 侧一致，312 字节）。
    #[repr(C)]
    struct Description {
        name: [u16; 128],
        vendor: u32,
        device: u32,
        subsystem: u32,
        revision: u32,
        dedicated_video: usize,
        dedicated_system: usize,
        shared: usize,
        luid_low: u32,
        luid_high: i32,
        flags: u32,
    }
    const { assert!(std::mem::size_of::<Description>() == 312) };
    #[repr(C)]
    struct Guid {
        a: u32,
        b: u16,
        c: u16,
        d: [u8; 8],
    }
    struct Com(*mut c_void);
    impl Com {
        unsafe fn method(&self, index: usize) -> *const c_void {
            unsafe { *(*self.0.cast::<*const *const c_void>()).add(index) }
        }
    }
    impl Drop for Com {
        fn drop(&mut self) {
            if !self.0.is_null() {
                let release: unsafe extern "system" fn(*mut c_void) -> u32 =
                    unsafe { transmute(self.method(2)) };
                unsafe {
                    release(self.0);
                }
            }
        }
    }
    let library = unsafe { ll::Library::load_with_flags("dxgi.dll", 0x800) }.ok()?;
    let create = unsafe {
        library.get::<unsafe extern "system" fn(*const Guid, *mut *mut c_void) -> i32>(
            b"CreateDXGIFactory1\0",
        )
    }
    .ok()?;
    // IID_IDXGIFactory1 = {770AAE78-F26F-4DBA-A829-253C83D1B387}
    let iid = Guid {
        a: 0x770aae78,
        b: 0xf26f,
        c: 0x4dba,
        d: [0xa8, 0x29, 0x25, 0x3c, 0x83, 0xd1, 0xb3, 0x87],
    };
    let mut factory = Com(ptr::null_mut());
    if unsafe { create(&iid, &mut factory.0) } < 0 || factory.0.is_null() {
        return None;
    }
    // IDXGIFactory1::EnumAdapters1（slot 12）与 IDXGIAdapter1::GetDesc1（slot 10）。
    let enumerate: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> i32 =
        unsafe { transmute(factory.method(12)) };
    let mut adapters = Vec::new();
    for index in 0..8 {
        let mut adapter = Com(ptr::null_mut());
        let rc = unsafe { enumerate(factory.0, index, &mut adapter.0) };
        if rc as u32 == 0x887a0002 {
            break;
        }
        if rc < 0 || adapter.0.is_null() {
            return None;
        }
        let get_desc: unsafe extern "system" fn(*mut c_void, *mut Description) -> i32 =
            unsafe { transmute(adapter.method(10)) };
        let mut desc: Description = unsafe { std::mem::zeroed() };
        if unsafe { get_desc(adapter.0, &mut desc) } < 0 {
            return None;
        }
        let length = desc.name.iter().position(|c| *c == 0).unwrap_or(128);
        adapters.push(DxgiAdapter {
            index,
            description: String::from_utf16_lossy(&desc.name[..length]),
            software: desc.flags & 2 != 0,
        });
    }
    Some(adapters)
}

#[cfg(not(all(windows, target_pointer_width = "64")))]
fn enumerate_dxgi() -> Option<Vec<DxgiAdapter>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cuda_stub() -> Vec<(u32, String)> {
        vec![
            (0, "NVIDIA GeForce RTX 5060 Laptop GPU".to_owned()),
            (1, "NVIDIA GeForce RTX 4090".to_owned()),
        ]
    }

    fn dxgi_stub() -> Vec<DxgiAdapter> {
        vec![
            DxgiAdapter {
                index: 0,
                description: "Intel(R) Arc(TM) 140T GPU (16GB)".to_owned(),
                software: false,
            },
            DxgiAdapter {
                index: 1,
                description: "NVIDIA GeForce RTX 5060 Laptop GPU".to_owned(),
                software: false,
            },
            DxgiAdapter {
                index: 2,
                description: "Microsoft Basic Render Driver".to_owned(),
                software: true,
            },
        ]
    }

    fn targets(backends: &[u32]) -> Vec<BackendTarget> {
        backends
            .iter()
            .map(|&backend| BackendTarget {
                backend,
                device_id: 0,
            })
            .collect()
    }

    #[test]
    fn substring_match_is_case_insensitive_and_instance_disambiguates() {
        let names = [
            (0u32, "Intel Arc 140T"),
            (1u32, "NVIDIA RTX 5060"),
            (2u32, "ARC second"),
        ];
        assert_eq!(select_by_substring(names, "arc 1", 0), Some(0));
        assert_eq!(select_by_substring(names, "ARC", 1), Some(2));
        assert_eq!(select_by_substring(names, "rtx", 0), Some(1));
        assert_eq!(select_by_substring(names, "missing", 0), None);
        assert_eq!(select_by_substring(names, "arc", 5), None);
    }

    #[test]
    fn absent_selector_passes_targets_through_unchanged() {
        let original = targets(&[2, 0]);
        let (resolved, ov_gpu) =
            resolve_targets(&original, None, &cuda_stub(), &dxgi_stub()).unwrap();
        assert_eq!(resolved, original);
        assert!(!ov_gpu);
    }

    #[test]
    fn cuda_selector_rewrites_device_id_and_overrides_candidate_device() {
        let mut original = targets(&[2]);
        original[0].device_id = 1;
        let selector = DeviceSelector::new("4090", 0);
        let (resolved, _) =
            resolve_targets(&original, Some(&selector), &cuda_stub(), &dxgi_stub()).unwrap();
        assert_eq!(resolved[0].device_id, 1);
        let selector = DeviceSelector::new("5060", 0);
        let (resolved, _) =
            resolve_targets(&original, Some(&selector), &cuda_stub(), &dxgi_stub()).unwrap();
        assert_eq!(
            resolved[0].device_id, 0,
            "selector 命中优先于候选 device_id"
        );
    }

    #[test]
    fn zero_cuda_match_is_invalid_argument_not_silent_fallback() {
        let selector = DeviceSelector::new("nonexistent", 0);
        let error = resolve_targets(&targets(&[2]), Some(&selector), &cuda_stub(), &dxgi_stub())
            .unwrap_err();
        assert_eq!(error.0, "sdk.invalidArgument");
    }

    #[test]
    fn dml_selector_resolves_dxgi_ordinal_and_skips_software_adapters() {
        let selector = DeviceSelector::new("140t", 0);
        let (resolved, _) =
            resolve_targets(&targets(&[3]), Some(&selector), &cuda_stub(), &dxgi_stub()).unwrap();
        assert_eq!(resolved[0].device_id, 0);
        // 软件适配器不在匹配域：Basic Render Driver 命中子串也不可选中。
        let selector = DeviceSelector::new("basic render", 0);
        let error = resolve_targets(&targets(&[3]), Some(&selector), &cuda_stub(), &dxgi_stub())
            .unwrap_err();
        assert_eq!(error.0, "sdk.invalidArgument");
    }

    #[test]
    fn openvino_candidate_validates_presence_and_flags_gpu() {
        let selector = DeviceSelector::new("rtx", 0);
        let (resolved, ov_gpu) =
            resolve_targets(&targets(&[1]), Some(&selector), &cuda_stub(), &dxgi_stub()).unwrap();
        assert_eq!(resolved[0].device_id, 0);
        assert!(ov_gpu);
        let selector = DeviceSelector::new("nothing", 0);
        let error = resolve_targets(&targets(&[1]), Some(&selector), &cuda_stub(), &dxgi_stub())
            .unwrap_err();
        assert_eq!(error.0, "sdk.invalidArgument");
    }

    #[test]
    fn cpu_candidates_ignore_selector_and_mixed_chains_keep_fallback_order() {
        let selector = DeviceSelector::new("arc", 0);
        let (resolved, ov_gpu) = resolve_targets(
            &targets(&[3, 0]),
            Some(&selector),
            &cuda_stub(),
            &dxgi_stub(),
        )
        .unwrap();
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[1].backend, 0);
        assert!(!ov_gpu);
    }

    #[test]
    fn openvino_native_with_selector_is_not_supported() {
        let selector = DeviceSelector::new("arc", 0);
        let error = resolve_targets(&targets(&[4]), Some(&selector), &cuda_stub(), &dxgi_stub())
            .unwrap_err();
        assert_eq!(error.0, "sdk.notSupported");
    }

    #[test]
    fn selector_instance_beyond_matches_is_rejected() {
        let selector = DeviceSelector::new("rtx", 3);
        let error = resolve_targets(&targets(&[3]), Some(&selector), &cuda_stub(), &dxgi_stub())
            .unwrap_err();
        assert_eq!(error.0, "sdk.invalidArgument");
    }
}
