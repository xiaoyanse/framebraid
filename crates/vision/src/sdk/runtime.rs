use crate::{Error, Result};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
pub mod devices;
mod windows;

static RUNTIME: Mutex<Option<PathBuf>> = Mutex::new(None);
static INITIALIZED: AtomicBool = AtomicBool::new(false);
/// DLL终止前仅清自己的路径堆；不释放ORT环境、运行库或执行提供器。
pub fn sdk_unload_ready() -> bool {
    RUNTIME.try_lock().is_ok()
}
/// # Safety
/// 仅供已排空的外发DLL终止钩子；宿主静态链接运行中不得调用。
pub unsafe fn sdk_unload_static() {
    if let Ok(mut path) = RUNTIME.try_lock() {
        path.take();
        INITIALIZED.store(false, Ordering::Release);
    }
}
#[derive(Default)]
struct Backends {
    chosen: Option<u32>,
    loaded: u32,
    executed: u32,
}
static BACKENDS: Mutex<Backends> = Mutex::new(Backends {
    chosen: None,
    loaded: 0,
    executed: 0,
});

pub fn library_name() -> &'static str {
    if cfg!(windows) {
        "onnxruntime.dll"
    } else if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else {
        "libonnxruntime.so"
    }
}
pub fn initialized() -> bool {
    INITIALIZED.load(Ordering::Acquire)
}
pub fn initialize(path: &Path) -> Result<()> {
    let canonical = path
        .canonicalize()
        .map_err(|_| Error("vision.runtimeMissing".into()))?;
    let mut current = RUNTIME
        .lock()
        .map_err(|_| Error("sdk.internalError".into()))?;
    if let Some(existing) = &*current {
        return if existing == &canonical {
            Ok(())
        } else {
            Err(Error("vision.backendConflict".into()))
        };
    }
    windows::check_loaded(&canonical)?;
    let _preflight = windows::preflight(&canonical)?;
    ort::init_from(&canonical)
        .map_err(|_| Error("vision.runtimeLoadFailed".into()))?
        .commit();
    *current = Some(canonical);
    INITIALIZED.store(true, Ordering::Release);
    Ok(())
}
pub(super) fn claim_backend(backend: u32) -> Result<()> {
    let mut state = BACKENDS
        .lock()
        .map_err(|_| Error("sdk.internalError".into()))?;
    state.claim(backend)
}
impl Backends {
    fn claim(&mut self, backend: u32) -> Result<()> {
        if backend > 3 {
            return Err(Error("sdk.invalidArgument".into()));
        }
        if backend != 0 {
            if self.chosen.is_some_and(|b| b != backend) {
                return Err(Error("vision.backendConflict".into()));
            }
            // 注册失败仍可能留下已加载的provider；只允许同后端重试或CPU。
            self.chosen = Some(backend);
        }
        Ok(())
    }
}
pub(super) fn loaded(backend: u32) {
    if let Ok(mut state) = BACKENDS.lock() {
        state.loaded |= 1 << backend;
    }
}
pub(super) fn executed(backend: u32) {
    if let Ok(mut state) = BACKENDS.lock() {
        state.executed |= 1 << backend;
    }
}
pub struct BackendStatus {
    pub backend: u32,
    pub hardware: bool,
    pub dependencies: bool,
    pub loaded: bool,
    pub executed: bool,
    pub note: &'static std::ffi::CStr,
}
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct BackendObservations {
    pub accelerator_domain: Option<u32>,
    pub loaded_mask: u32,
    pub executed_mask: u32,
}
/// 只复制进程历史观察，不探测驱动/文件，不代表当前模型健康状态。
pub fn observations() -> Result<BackendObservations> {
    let state = BACKENDS
        .lock()
        .map_err(|_| Error("sdk.internalError".into()))?;
    Ok(BackendObservations {
        accelerator_domain: state.chosen,
        loaded_mask: state.loaded,
        executed_mask: state.executed,
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CudaSnapshot {
    Unprobed,
    Failed,
    Absent,
    Present,
}

impl CudaSnapshot {
    /// 仅设备数明确取得时报告存在或不存在；其他探测终态为未知。
    pub fn from_probe(report: &devices::DeviceProbeReport) -> Self {
        if !report.cuda_driver_present || !report.cuda_initialized {
            return Self::Failed;
        }
        match report.cuda_visible_count {
            Some(0) => Self::Absent,
            Some(_) => Self::Present,
            None => Self::Failed,
        }
    }
}

/// 不进行驱动探测；如需设备判定，显式 probe 后使用 query_snapshot。
pub fn query(dirs: &[String]) -> Result<Vec<BackendStatus>> {
    query_snapshot(dirs, CudaSnapshot::Unprobed)
}

/// 系统级 DirectML 运行库可见性（E2 合同"或系统可解析"路径）：仅查 System32
/// 文件存在，不在矩阵查询路径加载系统库。
fn system_directml_available() -> bool {
    if !cfg!(windows) {
        return false;
    }
    std::env::var_os("SystemRoot")
        .is_some_and(|root| Path::new(&root).join(r"System32\DirectML.dll").is_file())
}

/// 仅读取调用方提供的可见性快照，不调用原生硬件驱动。
pub fn query_snapshot(dirs: &[String], cuda: CudaSnapshot) -> Result<Vec<BackendStatus>> {
    let state = observations()?;
    let ready = initialized();
    Ok((0..5)
        .map(|backend| {
            let mut dependencies = dirs.iter().any(|d| {
                let d = Path::new(d);
                if !d.join(library_name()).is_file() {
                    return false;
                }
                match backend {
                    0 => true,
                    3 => {
                        // DirectML（E2）：运行库目录自含 DirectML.dll 或系统可解析
                        //（onnxruntime 本体仍须目录提供，系统回退只覆盖 DML 运行时）。
                        d.join(if cfg!(windows) { "DirectML.dll" } else { "libdirectml.so" })
                            .is_file()
                            || system_directml_available()
                    }
                    1 => {
                        d.join(if cfg!(windows) {
                            "onnxruntime_providers_openvino.dll"
                        } else {
                            "libonnxruntime_providers_openvino.so"
                        })
                        .is_file()
                            && ["openvino.dll", "libopenvino.so", "libopenvino.so.2510"]
                                .iter()
                                .any(|n| d.join(n).is_file())
                    }
                    _ => d
                        .join(if cfg!(windows) {
                            "onnxruntime_providers_cuda.dll"
                        } else {
                            "libonnxruntime_providers_cuda.so"
                        })
                        .is_file(),
                }
            });
            if backend == 4 {
                // E1（合同 §3.1 引擎段）：deps=CPU 设备始终可用；
                // hw=openvino_c 运行库在 runtime_dirs 可加载（存在性，不在查询时加载）。
                dependencies = true;
            }
            let hardware = match backend {
                0 => true,
                1 => cfg!(target_arch = "x86_64"),
                4 => dirs.iter().any(|d| {
                    Path::new(d)
                        .join(crate::engine::ov_sys::library_name())
                        .is_file()
                }),
                _ => cuda == CudaSnapshot::Present,
            };
            // E1 走 OpenVINO C API 直连（无 ORT 参与），不占加速域也不要求 ORT 就绪。
            let conflict = backend != 0
                && backend != 4
                && state.accelerator_domain.is_some_and(|b| b != backend);
            let loaded = !conflict
                && (backend == 4 || ready)
                && dependencies
                && state.loaded_mask & (1 << backend) != 0;
            BackendStatus {
                backend,
                dependencies,
                hardware,
                loaded,
                executed: loaded && state.executed_mask & (1 << backend) != 0,
                note: if backend == 2 && conflict && cuda == CudaSnapshot::Unprobed {
                    c"vision.backendConflict; cuda-hardware-unknown; explicit-probe-required"
                } else if backend == 2 && conflict && cuda == CudaSnapshot::Failed {
                    c"vision.backendConflict; cuda-hardware-unknown; probe-failed"
                } else if backend == 2 && conflict && cuda == CudaSnapshot::Absent {
                    c"vision.backendConflict; cuda-no-device"
                } else if backend == 2 && cuda == CudaSnapshot::Unprobed {
                    c"cuda-hardware-unknown; explicit-probe-required; check-dependency-and-provider-fields"
                } else if backend == 2 && cuda == CudaSnapshot::Failed {
                    c"cuda-hardware-unknown; probe-failed; check-detailed-report"
                } else if backend == 2 && cuda == CudaSnapshot::Absent {
                    c"cuda-no-device; explicit-probe-observed-zero"
                } else if backend == 4 && !hardware {
                    c"openvino-native-runtime-not-in-dirs; optional-candidate; cpu-device-always-available"
                } else if conflict {
                    c"vision.backendConflict"
                } else if !dependencies {
                    c"vision.runtimeMissing"
                } else if !loaded {
                    c"provider-not-yet-observed; model-load-is-authoritative"
                } else if state.executed_mask & (1 << backend) == 0 {
                    c"provider-loaded; real-inference-not-yet-observed"
                } else {
                    c"real-inference-observed-in-this-process; package-matrix-separate"
                },
            }
        })
        .collect())
}

/// E1 原生引擎运行库登记：在 runtime_dirs 中定位 openvino_c 运行库目录并登记
/// （首次登记优先）。定位不到不视为错误——OpenVINO 原生是可选候选，
/// 由引擎装载期按候选不可用回退；ABI 合同（dual-dll-abi.md E1）约定
/// OpenVINO 运行库随 vision runtime_dirs 提供，AM_SDK_OPENVINO_DIR 为显式覆盖。
pub fn note_openvino_dirs(dirs: &[String]) {
    for dir in dirs {
        if Path::new(dir)
            .join(crate::engine::ov_sys::library_name())
            .is_file()
        {
            crate::engine::ov_sys::register_runtime_dir(Path::new(dir));
            return;
        }
    }
}

/// 最近一次 OpenVINO 原生引擎失败详情（检测版本/期望线）；模型加载失败时进
/// amv_get_last_error 详情，读取前由 FFI 层在每次模型加载前清零保证新鲜度。
pub fn openvino_load_failure() -> Option<String> {
    crate::engine::ov_sys::load_failure()
}

pub fn clear_openvino_load_failure() {
    crate::engine::ov_sys::clear_load_failure()
}

#[cfg(test)]
mod backend_tests {
    use super::*;
    #[test]
    fn rust_query_requires_explicit_probe_and_converts_report() {
        let rows = query(&[]).unwrap();
        assert!(!rows[2].hardware);
        assert!(rows[2]
            .note
            .to_str()
            .unwrap()
            .contains("explicit-probe-required"));
        let mut report = devices::DeviceProbeReport {
            schema: "test",
            cpu_logical_parallelism: 1,
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
        assert_eq!(CudaSnapshot::from_probe(&report), CudaSnapshot::Failed);
        report.cuda_visible_count = Some(2);
        assert_eq!(CudaSnapshot::from_probe(&report), CudaSnapshot::Failed);
        report.cuda_driver_present = true;
        report.cuda_initialized = true;
        report.cuda_visible_count = Some(0);
        assert_eq!(CudaSnapshot::from_probe(&report), CudaSnapshot::Absent);
        report.cuda_visible_count = Some(2);
        assert_eq!(CudaSnapshot::from_probe(&report), CudaSnapshot::Present);
        assert!(query_snapshot(&[], CudaSnapshot::from_probe(&report)).unwrap()[2].hardware);
    }
    #[test]
    fn cached_cuda_unknown_and_absent_do_not_claim_hardware() {
        for (cuda, reason) in [
            (CudaSnapshot::Unprobed, "explicit-probe-required"),
            (CudaSnapshot::Failed, "probe-failed"),
            (CudaSnapshot::Absent, "no-device"),
        ] {
            let rows = query_snapshot(&[], cuda).unwrap();
            assert!(!rows[2].hardware);
            assert!(rows[2].note.to_str().unwrap().contains(reason));
        }
        assert!(query_snapshot(&[], CudaSnapshot::Present).unwrap()[2].hardware);
    }
    #[test]
    fn openvino_native_row_follows_the_fifth_row_contract() {
        // E1（合同 §3.1 引擎段）：deps=CPU 设备始终可用；hw=openvino_c 运行库在
        // runtime_dirs 可加载；loaded/executed=本进程真实编译/推理观察位。
        let rows = query_snapshot(&[], CudaSnapshot::Unprobed).unwrap();
        assert_eq!(rows.len(), 5, "矩阵须含 backend=0..4 五行");
        let e1 = &rows[4];
        assert_eq!(e1.backend, 4);
        assert!(e1.dependencies);
        assert!(!e1.hardware, "空 runtime_dirs：openvino_c 不可加载");
        assert!(!e1.loaded && !e1.executed);
        assert!(e1
            .note
            .to_str()
            .unwrap()
            .contains("openvino-native-runtime-not-in-dirs"));
        // 提供 openvino_c 运行库目录时 hw 置位（存在性检查，不实际加载）。
        let dir = std::env::temp_dir().join("amv-v08-e1-matrix-probe");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("openvino_c.dll"), b"").unwrap();
        let rows = query_snapshot(
            &[dir.to_string_lossy().into_owned()],
            CudaSnapshot::Unprobed,
        )
        .unwrap();
        assert!(rows[4].hardware, "目录含 openvino_c.dll：hw 置位");
        assert!(!rows[4].loaded, "观察位须由真实模型编译登记，不由查询置位");
        std::fs::remove_file(dir.join("openvino_c.dll")).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }
    #[test]
    fn dml_dependency_accepts_system_directml_fallback() {
        // E2 合同（§3.1 E2 段）：deps=目录自含 DirectML.dll 或系统可解析——
        // 本机 System32 含 DirectML.dll（sdk-v08 侦察 §4.1），目录仅提供
        // onnxruntime.dll 时 deps 亦成立。
        let dir = std::env::temp_dir().join("amv-v08-dml-deps-probe");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("onnxruntime.dll"), b"").unwrap();
        let dirs = [dir.to_string_lossy().into_owned()];
        let without_system = !system_directml_available();
        let rows = query_snapshot(&dirs, CudaSnapshot::Unprobed).unwrap();
        let dml = &rows[3];
        if without_system {
            assert!(!dml.dependencies, "无系统 DirectML 且目录缺失：deps 不成立");
        } else {
            assert!(
                dml.dependencies,
                "系统 DirectML 可解析：deps 成立（合同回退路径）"
            );
        }
        std::fs::remove_file(dir.join("onnxruntime.dll")).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }
    #[test]
    fn readiness_snapshot_does_not_wait_for_native_initialization_lock() {
        let guard = RUNTIME.lock().unwrap();
        let (send, receive) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || send.send(initialized()).unwrap());
        let result = receive.recv_timeout(std::time::Duration::from_secs(1));
        drop(guard);
        thread.join().unwrap();
        assert!(result.is_ok(), "查询不应等待原生初始化路径锁");
    }
    #[test]
    fn partial_registration_keeps_domain_but_does_not_cache_failure_for_same_backend() {
        let mut state = Backends::default();
        state.claim(2).unwrap();
        assert_eq!(state.loaded, 0);
        assert_eq!(state.executed, 0);
        assert_eq!(state.claim(1).unwrap_err().0, "vision.backendConflict");
        state.claim(0).unwrap();
        state.claim(2).unwrap();
        assert_eq!(state.chosen, Some(2));
    }
}
