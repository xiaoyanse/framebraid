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

/// 仅读取调用方提供的可见性快照，不调用原生硬件驱动。
pub fn query_snapshot(dirs: &[String], cuda: CudaSnapshot) -> Result<Vec<BackendStatus>> {
    let state = observations()?;
    let ready = initialized();
    Ok((0..4)
        .map(|backend| {
            let dependencies = dirs.iter().any(|d| {
                let d = Path::new(d);
                if !d.join(library_name()).is_file() {
                    return false;
                }
                match backend {
                    0 => true,
                    3 => {
                        // DirectML（E2）：运行库目录自含 DirectML.dll 或系统可解析。
                        d.join(if cfg!(windows) { "DirectML.dll" } else { "libdirectml.so" })
                            .is_file()
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
            let conflict = backend != 0 && state.accelerator_domain.is_some_and(|b| b != backend);
            let loaded =
                !conflict && ready && dependencies && state.loaded_mask & (1 << backend) != 0;
            BackendStatus {
                backend,
                dependencies,
                hardware: match backend {
                    0 => true,
                    1 => cfg!(target_arch = "x86_64"),
                    _ => cuda == CudaSnapshot::Present,
                },
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
