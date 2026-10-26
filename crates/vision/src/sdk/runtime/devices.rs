//! 显式设备探测；CUDA ordinal独立于视频DXGI索引，不读取宿主配置。
use serde::Serialize;
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
static PROBE: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub enum DeviceProbeFailure {
    Busy,
    InvalidBudget,
    UnsupportedPlatform,
    BudgetExhausted,
    SystemLibrary(u32),
    MissingSymbol(&'static str),
    Driver { operation: &'static str, code: i32 },
}
#[derive(Clone, Debug, Serialize)]
pub struct CudaDevice {
    pub ordinal: u32,
    pub name: String,
    pub uuid: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct DeviceProbeReport {
    pub schema: &'static str,
    pub cpu_logical_parallelism: usize,
    pub cuda_driver_present: bool,
    pub cuda_initialized: bool,
    pub cuda_driver_version: Option<i32>,
    pub cuda_visible_count: Option<u32>,
    pub cuda_devices: Vec<CudaDevice>,
    pub limit_reached: bool,
    pub stopped: Option<DeviceProbeFailure>,
    pub elapsed_ms: u64,
    pub soft_budget_exceeded: bool,
}
/// 最多16个设备，预算在原生调用之间检查，不强杀驱动；不创建ORT模型或GPU上下文。
pub fn probe(budget: Duration) -> std::result::Result<DeviceProbeReport, DeviceProbeFailure> {
    if budget.is_zero() || budget > Duration::from_secs(5) {
        return Err(DeviceProbeFailure::InvalidBudget);
    }
    let _guard = PROBE.try_lock().map_err(|_| DeviceProbeFailure::Busy)?;
    let start = Instant::now();
    let mut report = DeviceProbeReport {
        schema: "framebraid.inference-devices/1",
        cpu_logical_parallelism: std::thread::available_parallelism().map_or(1, |v| v.get()),
        cuda_driver_present: false,
        cuda_initialized: false,
        cuda_driver_version: None,
        cuda_visible_count: None,
        cuda_devices: Vec::new(),
        limit_reached: false,
        stopped: None,
        elapsed_ms: 0,
        soft_budget_exceeded: false,
    };
    report.stopped = walk(&mut report, start, budget).err();
    report.elapsed_ms = start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    report.soft_budget_exceeded = start.elapsed() > budget;
    Ok(report)
}
#[cfg(not(all(windows, target_pointer_width = "64")))]
fn walk(
    _: &mut DeviceProbeReport,
    _: Instant,
    _: Duration,
) -> std::result::Result<(), DeviceProbeFailure> {
    Err(DeviceProbeFailure::UnsupportedPlatform)
}

#[cfg(all(windows, target_pointer_width = "64"))]
fn walk(
    report: &mut DeviceProbeReport,
    start: Instant,
    budget: Duration,
) -> std::result::Result<(), DeviceProbeFailure> {
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
        fn GetLastError() -> u32;
    }
    struct Library(*mut c_void);
    impl Drop for Library {
        fn drop(&mut self) {
            unsafe {
                FreeLibrary(self.0);
            }
        }
    }
    let check = || {
        if start.elapsed() >= budget {
            Err(DeviceProbeFailure::BudgetExhausted)
        } else {
            Ok(())
        }
    };
    check()?;
    let mut path = [0u16; 32768];
    let n = unsafe { GetSystemDirectoryW(path.as_mut_ptr(), path.len() as u32) } as usize;
    if n == 0 || n + 12 >= path.len() {
        return Err(DeviceProbeFailure::SystemLibrary(unsafe { GetLastError() }));
    }
    let mut path = path[..n].to_vec();
    path.extend("\\nvcuda.dll\0".encode_utf16());
    check()?;
    let module = unsafe { LoadLibraryExW(path.as_ptr(), ptr::null_mut(), 0x800) };
    if module.is_null() {
        return Err(DeviceProbeFailure::SystemLibrary(unsafe { GetLastError() }));
    }
    let library = Library(module);
    report.cuda_driver_present = true;
    let symbol = |name: &'static CStr, label| {
        let value = unsafe { GetProcAddress(library.0, name.as_ptr()) };
        if value.is_null() {
            Err(DeviceProbeFailure::MissingSymbol(label))
        } else {
            Ok(value)
        }
    };
    let init: unsafe extern "system" fn(u32) -> i32 =
        unsafe { std::mem::transmute(symbol(c"cuInit", "cuInit")?) };
    let count: unsafe extern "system" fn(*mut i32) -> i32 =
        unsafe { std::mem::transmute(symbol(c"cuDeviceGetCount", "cuDeviceGetCount")?) };
    let get: unsafe extern "system" fn(*mut i32, i32) -> i32 =
        unsafe { std::mem::transmute(symbol(c"cuDeviceGet", "cuDeviceGet")?) };
    let name: unsafe extern "system" fn(*mut c_char, i32, i32) -> i32 =
        unsafe { std::mem::transmute(symbol(c"cuDeviceGetName", "cuDeviceGetName")?) };
    let version: unsafe extern "system" fn(*mut i32) -> i32 =
        unsafe { std::mem::transmute(symbol(c"cuDriverGetVersion", "cuDriverGetVersion")?) };
    let status = |operation, code| {
        if code == 0 {
            Ok(())
        } else {
            Err(DeviceProbeFailure::Driver { operation, code })
        }
    };
    check()?;
    status("cuInit", unsafe { init(0) })?;
    report.cuda_initialized = true;
    let mut value = 0;
    check()?;
    status("cuDriverGetVersion", unsafe { version(&mut value) })?;
    report.cuda_driver_version = Some(value);
    check()?;
    status("cuDeviceGetCount", unsafe { count(&mut value) })?;
    let total = u32::try_from(value).map_err(|_| DeviceProbeFailure::Driver {
        operation: "cuDeviceGetCount.invalidCount",
        code: value,
    })?;
    report.cuda_visible_count = Some(total);
    report.limit_reached = total > 16;
    let uuid = unsafe { GetProcAddress(library.0, c"cuDeviceGetUuid_v2".as_ptr()) };
    for ordinal in 0..total.min(16) {
        let mut device = 0;
        let mut label = [0u8; 256];
        check()?;
        status("cuDeviceGet", unsafe { get(&mut device, ordinal as i32) })?;
        check()?;
        status("cuDeviceGetName", unsafe {
            name(label.as_mut_ptr().cast(), label.len() as i32, device)
        })?;
        let end = label.iter().position(|b| *b == 0).unwrap_or(label.len());
        let mut row = CudaDevice {
            ordinal,
            name: String::from_utf8_lossy(&label[..end]).into_owned(),
            uuid: None,
        };
        if !uuid.is_null() {
            let get_uuid: unsafe extern "system" fn(*mut u8, i32) -> i32 =
                unsafe { std::mem::transmute(uuid) };
            let mut bytes = [0u8; 16];
            check()?;
            if unsafe { get_uuid(bytes.as_mut_ptr(), device) } == 0 {
                row.uuid = Some(bytes.iter().map(|b| format!("{b:02x}")).collect());
            }
        }
        report.cuda_devices.push(row);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn probe_budget_and_singleflight_precede_driver_calls() {
        assert!(matches!(
            probe(Duration::ZERO),
            Err(DeviceProbeFailure::InvalidBudget)
        ));
        let _busy = PROBE.lock().unwrap();
        assert!(matches!(
            probe(Duration::from_secs(1)),
            Err(DeviceProbeFailure::Busy)
        ));
    }
    #[test]
    #[ignore = "显式本机驱动探测，不冒充ORT/CUDA模型运行验收"]
    fn native_device_inventory_is_bounded_and_owned() {
        let report = probe(Duration::from_secs(2)).unwrap();
        assert!(report.cpu_logical_parallelism > 0);
        assert!(report.cuda_devices.len() <= 16);
        assert!(report.cuda_devices.iter().all(|d| d.name.len() <= 768));
        println!(
            "inference-device-probe {}",
            serde_json::to_string(&report).unwrap()
        );
    }
}
