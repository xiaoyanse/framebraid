//! 推理后端抽象：枚举、执行路径定位与可用性探测（P09 R1/R2）。
use super::Error;
use am_vision_types::{GpuStatus, InferenceBackend};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// 后端字符串协议值：跨 worker 参数与设置存储共用。
pub fn backend_key(backend: InferenceBackend) -> &'static str {
    match backend {
        InferenceBackend::Cpu => "cpu",
        InferenceBackend::OpenVINO => "openvino",
        InferenceBackend::Cuda => "cuda",
        InferenceBackend::DirectML => "directml",
    }
}
/// 解析后端字符串；未知值报错，禁止静默映射到任一后端。
pub fn parse_backend(value: &str) -> Result<InferenceBackend, Error> {
    match value {
        "cpu" => Ok(InferenceBackend::Cpu),
        "openvino" => Ok(InferenceBackend::OpenVINO),
        "cuda" => Ok(InferenceBackend::Cuda),
        "directml" => Ok(InferenceBackend::DirectML),
        _ => Err(Error("vision.backendUnsupported".into())),
    }
}

fn library_name() -> &'static str {
    if cfg!(windows) {
        "onnxruntime.dll"
    } else if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else {
        "libonnxruntime.so"
    }
}
fn vendor_subdir() -> &'static str {
    if cfg!(windows) {
        "win-x64"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux-x64"
    }
}

/// CPU/OpenVINO 线搜索列表：环境变量 → exe 旁 → vendor/onnxruntime 向上五级。
/// CUDA 用独立 GPU 包目录（vendor/onnxruntime-gpu），两线 DLL 同名不能混放。
/// 环境变量与 exe 旁候选既可能是 DLL 文件全路径，也可能是目录（DEL-15）。
fn candidate_dirs(gpu: bool) -> Vec<PathBuf> {
    let mut list = Vec::new();
    let env_key = if gpu {
        "AM_RT_ORT_GPU_LIBRARY"
    } else {
        "AM_RT_ORT_LIBRARY"
    };
    if let Some(from_env) = std::env::var_os(env_key) {
        list.push(PathBuf::from(from_env));
    }
    if let Ok(exe) = std::env::current_exe() {
        let exe_dir = exe.with_file_name("");
        if gpu {
            list.push(exe_dir.join("onnxruntime-gpu"));
        } else {
            list.push(exe.with_file_name(library_name()));
        }
        let vendor = if gpu {
            "onnxruntime-gpu"
        } else {
            "onnxruntime"
        };
        for ancestor in exe.ancestors().skip(1).take(5) {
            list.push(ancestor.join("vendor").join(vendor).join(vendor_subdir()));
        }
    }
    list
}
/// 候选路径解析：候选本身是 DLL 文件直接用，是目录才拼库名。
/// 修复对文件路径再 join 生成 "onnxruntime.dll/onnxruntime.dll" 的语义回归（DEL-15）。
fn resolve_runtime(candidates: Vec<PathBuf>, name: &str) -> Option<PathBuf> {
    candidates.into_iter().find_map(|candidate| {
        if candidate.is_file() {
            Some(candidate)
        } else {
            let joined = candidate.join(name);
            joined.is_file().then_some(joined)
        }
    })
}
/// 定位 onnxruntime 核心 DLL（只做文件探测，不加载）。
pub fn locate_runtime() -> Option<PathBuf> {
    resolve_runtime(candidate_dirs(false), library_name())
}
/// 定位 GPU 包核心 DLL；目录独立于 CPU 线，缺包返回 None。
pub fn locate_gpu_runtime() -> Option<PathBuf> {
    resolve_runtime(candidate_dirs(true), library_name())
}

/// OpenVINO 提供器伴随文件名（与核心 DLL 同目录）。
fn provider_files() -> &'static [&'static str] {
    if cfg!(windows) {
        &["onnxruntime_providers_openvino.dll", "openvino.dll"]
    } else if cfg!(target_os = "linux") {
        &[
            "libonnxruntime_providers_openvino.so",
            "libopenvino.so",
            "libopenvino.so.2510",
        ]
    } else {
        // macOS 无官方 OpenVINO EP 分发
        &[]
    }
}

/// OpenVINO 可用性文件级探测：核心 DLL 旁提供器与 openvino 运行库齐全才可选。
/// 真正的运行时校验发生在会话构建（注册失败自动回退并标注），探测只决定 UI 是否放行。
pub fn openvino_available() -> bool {
    if provider_files().is_empty() {
        return false;
    }
    let Some(dir) = locate_runtime().and_then(|p| p.parent().map(Path::to_path_buf)) else {
        return false;
    };
    provider_files().iter().all(|name| dir.join(name).is_file())
}

/// GPU 包伴随文件（与 GPU 核心 DLL 同目录）；cudnn 前缀匹配 cuDNN 9 线任一小版本。
fn cuda_package_files(dir: &Path) -> (bool, bool) {
    let has_cuda_provider = dir
        .join(if cfg!(windows) {
            "onnxruntime_providers_cuda.dll"
        } else {
            "libonnxruntime_providers_cuda.so"
        })
        .is_file();
    // cuDNN 9.x 运行库文件名（cudnn64_9.dll / libcudnn.so.9）
    let has_cudnn = std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .contains(if cfg!(windows) {
                    "cudnn64_9"
                } else {
                    "libcudnn.so.9"
                })
        })
    });
    (has_cuda_provider, has_cudnn)
}

/// CUDA 可选 = NVIDIA GPU 检测到 + GPU 包核心/提供器/cuDNN 文件齐全。
/// 同 OpenVINO：探测只决定 UI 放行，运行时注册失败自动回退并标注。
pub fn cuda_available() -> bool {
    if !probe_gpu().detected {
        return false;
    }
    let Some(dir) = locate_gpu_runtime().and_then(|p| p.parent().map(Path::to_path_buf)) else {
        return false;
    };
    let (provider, cudnn) = cuda_package_files(&dir);
    provider && cudnn
}

/// TensorRT 可选 EP 的文件级探测：GPU 目录旁需有 nvinfer 运行库（GPU 包不含，缺则显式禁用）。
pub fn trt_available() -> bool {
    if !cfg!(windows) {
        return false;
    }
    locate_gpu_runtime()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .is_some_and(|dir| dir.join("nvinfer64_12.dll").is_file())
}

/// NVIDIA GPU 硬件探测（进程级缓存）：驱动文件存在性兜底 + nvidia-smi 补充详情。
/// 缓存原因：子进程探测有百毫秒级开销，且硬件在进程生命周期内不变。
pub fn probe_gpu() -> GpuStatus {
    static CACHED: OnceLock<GpuStatus> = OnceLock::new();
    CACHED.get_or_init(detect_gpu).clone()
}

fn detect_gpu() -> GpuStatus {
    if !driver_library_present() {
        return GpuStatus {
            detected: false,
            name: None,
            driver_version: None,
            cuda_version: None,
        };
    }
    let query = query_smi("--query-gpu=name,driver_version --format=csv,noheader");
    // CSV 行：name, driver_version；含逗号的显卡名不带引号（NVIDIA 命名无逗号）
    let (name, driver_version) = query
        .and_then(|output| {
            let mut parts = output.lines().next()?.split(',');
            Some((
                parts.next()?.trim().to_owned(),
                parts.next()?.trim().to_owned(),
            ))
        })
        .map(|(name, driver)| (Some(name), Some(driver)))
        .unwrap_or((None, None));
    let cuda_version = parse_cuda_version();
    GpuStatus {
        detected: true,
        name,
        driver_version,
        cuda_version,
    }
}

/// 驱动存在性：Windows 看 System32\nvcuda.dll，Linux 看 libcuda.so.1。
fn driver_library_present() -> bool {
    if cfg!(windows) {
        std::env::var_os("SystemRoot")
            .map(|root| PathBuf::from(root).join(r"System32\nvcuda.dll").is_file())
            .unwrap_or(false)
    } else if cfg!(target_os = "linux") {
        Path::new("/usr/lib/x86_64-linux-gnu/libcuda.so.1").is_file()
            || Path::new("/lib/x86_64-linux-gnu/libcuda.so.1").is_file()
    } else {
        false
    }
}

/// nvidia-smi 子进程输出（3 秒超时轮询：std Command 无原生超时）。
fn query_smi(args: &str) -> Option<String> {
    let mut child = std::process::Command::new("nvidia-smi")
        .args(args.split_whitespace())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                use std::io::Read;
                let mut text = String::new();
                child.stdout?.read_to_string(&mut text).ok()?;
                return Some(text);
            }
            Ok(Some(_)) | Err(_) => return None,
            Ok(None) if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    }
}

/// 默认输出首屏含 "CUDA Version : 12.x"；驱动不支持时字段缺失返回 None。
fn parse_cuda_version() -> Option<String> {
    let output = query_smi("")?;
    let marker = "CUDA Version";
    let rest = output.split(marker).nth(1)?;
    let after_colon = rest.split(':').nth(1)?;
    let token = after_colon.split_whitespace().next()?;
    token
        .chars()
        .all(|c| c.is_ascii_digit() || c == '.')
        .then(|| token.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// 进程内唯一临时目录（无外部测试依赖）。
    fn temp_dir(tag: &str) -> PathBuf {
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "am-vision-ort-{tag}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
    /// 在目录内落一个占位文件并返回其路径。
    fn touch(dir: &Path, name: &str) -> PathBuf {
        let file = dir.join(name);
        std::fs::write(&file, b"stub").unwrap();
        file
    }

    #[test]
    fn probe_never_panics_without_driver() {
        // 探测只做文件/子进程检查；无 GPU 机器返回 false 而非报错。
        let _ = probe_gpu();
        let _ = cuda_available();
        let _ = trt_available();
    }

    // 以下四类覆盖 DEL-15：文件路径 / 目录路径 / exe 相邻 DLL / 缺库。

    #[test]
    fn file_candidate_used_directly_without_rejoin() {
        let dir = temp_dir("file");
        let dll = touch(&dir, library_name());
        // 环境变量传 DLL 文件全路径：原样命中，不得再拼一层库名
        let resolved = resolve_runtime(vec![dll.clone()], library_name()).unwrap();
        assert_eq!(resolved, dll);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn directory_candidate_joins_library_name() {
        let dir = temp_dir("dir");
        let dll = touch(&dir, library_name());
        // 目录候选：目录 + 库名命中
        assert_eq!(
            resolve_runtime(vec![dir.clone()], library_name()).as_deref(),
            Some(dll.as_path())
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn exe_adjacent_dll_file_resolves() {
        let dir = temp_dir("exe");
        // exe 旁候选 = exe.with_file_name(库名)，本身是文件路径（DEL-15 语义回归点）
        let exe = dir.join("am-server.exe");
        let adjacent = touch(&dir, library_name());
        let candidate = exe.with_file_name(library_name());
        assert_eq!(
            resolve_runtime(vec![candidate], library_name()).as_deref(),
            Some(adjacent.as_path())
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_library_candidates_return_none() {
        let dir = temp_dir("missing");
        // 文件不存在 + 空目录都不命中 → None
        let absent_file = dir.join(library_name());
        assert!(resolve_runtime(vec![absent_file, dir.clone()], library_name()).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn directml_key_round_trip() {
    assert_eq!(backend_key(InferenceBackend::DirectML), "directml");
    assert!(matches!(
        parse_backend("directml"),
        Ok(InferenceBackend::DirectML)
    ));
}
