//! 视觉推理与后处理核心；无设备、数据库、任务或报警副作用。
mod backend;
mod decode;
mod engine;
pub mod gpu;
mod preprocess;
/// 源码SDK的五类档案和后处理；宿主原有PoseModel入口保持兼容。
pub mod profiles;

/// 有界公平执行队列（6.2 队列下移）：宿主与 DLL 共享的排队/预算核心。
pub mod executor;
pub mod sdk;
pub use executor::{AcquireError, ExecutorConfig, FairExecutor, FairSlot, RequestClass};

pub use am_vision_types::{
    ExecutionInfo, InferenceBackend, Keypoint, PoseDetection, PoseFrame, PoseModelInfo,
};
pub use backend::{
    backend_key, cuda_available, openvino_available, parse_backend, probe_gpu, trt_available,
};
use ort::{
    session::Session,
    value::{Tensor, TensorElementType, ValueType},
};
use std::{path::Path, sync::OnceLock, time::Instant};

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Error(pub String);
pub type Result<T> = std::result::Result<T, Error>;

/// ort 运行库初始化按后端分线：CPU/OpenVINO 与 CUDA 各自独立目录同名 DLL，
/// worker 进程单后端存活，两条 OnceLock 在同进程内互不覆盖（池槽位互斥保证不混线）。
fn runtime_for(backend: InferenceBackend) -> Result<()> {
    static CPU: OnceLock<std::result::Result<(), String>> = OnceLock::new();
    static GPU: OnceLock<std::result::Result<(), String>> = OnceLock::new();
    // DirectML 用 CPU 线：directml wheel 是单目录包（onnxruntime.dll+DirectML.dll），
    // 无独立 GPU 目录语义，与 CPU 包互斥同线加载。
    let cell = match backend {
        InferenceBackend::Cuda => &GPU,
        _ => &CPU,
    };
    cell.get_or_init(|| {
        // CUDA 线只认 GPU 包目录；其余沿用环境变量 → exe 旁 → vendor 探测。
        let path = match backend {
            InferenceBackend::Cuda => backend::locate_gpu_runtime(),
            _ => backend::locate_runtime(),
        };
        let Some(path) = path else {
            return Err(if matches!(backend, InferenceBackend::Cuda) {
                "vision.gpuRuntimeMissing".into()
            } else {
                "vision.runtimeMissing".into()
            });
        };
        ort::init_from(path)
            .map_err(|_| "vision.runtimeLoadFailed".to_string())?
            .commit();
        Ok(())
    })
    .clone()
    .map_err(Error)
}

pub struct PoseModel {
    session: Session,
    pub info: PoseModelInfo,
    /// 会话实际执行路径：请求的后端注册失败时回退 CPU 并记录原因。
    pub execution: ExecutionInfo,
}

impl PoseModel {
    /// 按后端加载模型：非 CPU 后端注册或会话构建失败自动回退 CPU 并标注；
    /// 回退后仍失败则返回 CPU 侧错误，保持既有合法性判定语义。
    pub fn load(path: &Path, backend: InferenceBackend) -> Result<Self> {
        runtime_for(backend)?;
        match backend {
            InferenceBackend::Cpu => Self::load_with(path, cpu_execution(), None),
            InferenceBackend::OpenVINO => {
                Self::load_or_fallback(path, "OpenVINO", openvino_execution(), openvino_ep())
            }
            InferenceBackend::DirectML => {
                Self::load_or_fallback(path, "DirectML", directml_execution(), directml_ep())
            }
            InferenceBackend::Cuda => {
                // 真逐级 TRT→CUDA→CPU：TRT 失败必须先试 CUDA，禁止跳级落 CPU（DEL-16）；
                // 缺 nvinfer 时探测直接跳过 TRT 级。各级按需懒构建，成功前不预付下级成本。
                let mut attempts: Vec<FallbackAttempt<'_, Self>> = Vec::new();
                if backend::trt_available() {
                    attempts.push((
                        "TensorRT",
                        Box::new(|| Self::load_with(path, trt_execution(), Some(trt_ep()))),
                    ));
                }
                attempts.push((
                    "CUDA",
                    Box::new(|| Self::load_with(path, cuda_execution(), Some(cuda_ep()))),
                ));
                let (mut model, reason) = gpu_fallback_chain(
                    attempts,
                    Box::new(|| Self::load_with(path, cpu_execution(), None)),
                )?;
                model.execution.fallback_reason = reason;
                Ok(model)
            }
        }
    }
    /// 单一非 CPU 后端构建：成功返回，失败回退 CPU 并记录原因。
    fn load_or_fallback(
        path: &Path,
        label: &str,
        execution: ExecutionInfo,
        provider: ort::ep::ExecutionProviderDispatch,
    ) -> Result<Self> {
        Self::load_with(path, execution, Some(provider)).or_else(|error| {
            let reason = format!("{label}: {error}");
            Self::load_with(path, cpu_execution(), None).map(|mut model| {
                model.execution.fallback_reason = Some(reason);
                model
            })
        })
    }
    fn load_with(
        path: &Path,
        execution: ExecutionInfo,
        provider: Option<ort::ep::ExecutionProviderDispatch>,
    ) -> Result<Self> {
        let mut builder = Session::builder()
            .and_then(|builder| Ok(builder.with_intra_threads(2)?))
            .map_err(|e| Error(format!("vision.modelLoadFailed|{e}")))?;
        if let Some(provider) = provider {
            builder = builder
                .with_execution_providers([provider])
                .map_err(|e| Error(format!("vision.modelLoadFailed|{e}")))?;
        }
        let session = builder
            .commit_from_file(path)
            .map_err(|e| Error(format!("vision.modelLoadFailed|{e}")))?;
        if session.inputs().len() != 1 || session.outputs().len() != 1 {
            return Err(Error("vision.ioUnsupported".into()));
        }
        let input = tensor_shape(session.inputs()[0].dtype())?;
        let output = tensor_shape(session.outputs()[0].dtype())?;
        let meta = session.metadata().map_err(|e| Error(e.to_string()))?;
        if input != [1, 3, 640, 640]
            || output != [1, 300, 57]
            || meta.custom("task").as_deref() != Some("pose")
            || meta.custom("end2end").as_deref() != Some("True")
            || meta.custom("kpt_shape").as_deref() != Some("[17, 3]")
        {
            return Err(Error("vision.formatIncompatible".into()));
        }
        let names = decode::KEYPOINT_NAMES
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>();
        let actual_names = meta
            .custom("kpt_names")
            .ok_or_else(|| Error("vision.keypointMetaMissing".into()))?;
        // The supplied Ultralytics model stores a Python-style dictionary in metadata.
        // Validate its canonical COCO keypoint sequence without executing metadata as code.
        let expected = format!(
            "{{0: [{}]}}",
            names
                .iter()
                .map(|name| format!("'{name}'"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        if actual_names != expected {
            return Err(Error("vision.keypointNamesMismatch".into()));
        }
        let info = PoseModelInfo {
            decoder: "yolo26-pose-end2end-coco17-v1".into(),
            input_name: session.inputs()[0].name().into(),
            input_shape: input,
            output_shape: output,
            keypoint_names: names,
        };
        drop(meta);
        Ok(Self {
            session,
            info,
            execution,
        })
    }

    pub fn infer(&mut self, image: &[u8], confidence: f32) -> Result<PoseFrame> {
        if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
            return Err(Error("vision.confidenceRange".into()));
        }
        let prepared = preprocess::prepare(image)?;
        let tensor =
            Tensor::from_array(([1usize, 3, 640, 640], prepared.tensor.into_boxed_slice()))
                .map_err(|e| Error(e.to_string()))?;
        let start = Instant::now();
        let output = self
            .session
            .run(ort::inputs![tensor])
            .map_err(|e| Error(format!("vision.inferenceFailed|{e}")))?;
        let (shape, values) = output[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| Error(e.to_string()))?;
        if shape.as_ref() != [1, 300, 57] {
            return Err(Error("vision.outputShapeMismatch".into()));
        }
        let detections = decode::decode(values, &prepared.mapping, confidence)?;
        Ok(PoseFrame {
            width: prepared.mapping.width,
            height: prepared.mapping.height,
            inference_ms: start.elapsed().as_millis() as u64,
            detections,
        })
    }
}

/// GPU 降级链核心：按序懒尝试各级，成功即用并带上此前失败原因；
/// 全败回退 CPU 并聚合原因。独立纯函数便于无 TRT/GPU 环境注入测试三分支（DEL-16）。
type FallbackAttempt<'a, T> = (&'static str, Box<dyn FnOnce() -> Result<T> + 'a>);
fn gpu_fallback_chain<'a, T>(
    attempts: Vec<FallbackAttempt<'a, T>>,
    cpu: Box<dyn FnOnce() -> Result<T> + 'a>,
) -> Result<(T, Option<String>)> {
    let mut failures: Vec<String> = Vec::new();
    for (label, attempt) in attempts {
        match attempt() {
            Ok(value) => {
                let reason = (!failures.is_empty()).then(|| failures.join("; "));
                return Ok((value, reason));
            }
            Err(error) => failures.push(format!("{label}: {error}")),
        }
    }
    let joined = failures.join("; ");
    match cpu() {
        Ok(value) => Ok((value, Some(joined))),
        // CPU 也失败：返回 CPU 错误并附 GPU 各级失败原因，保住诊断线索
        Err(error) => Err(Error(format!("{error}|gpuTried:{joined}"))),
    }
}

fn cpu_execution() -> ExecutionInfo {
    ExecutionInfo {
        backend: InferenceBackend::Cpu,
        device: "CPU".into(),
        fallback_reason: None,
    }
}

fn tensor_shape(value: &ValueType) -> Result<Vec<i64>> {
    match value {
        ValueType::Tensor {
            ty: TensorElementType::Float32,
            shape,
            ..
        } => Ok(shape.to_vec()),
        _ => Err(Error("vision.tensorTypeUnsupported".into())),
    }
}

/// R1 固定 OpenVINO CPU 设备：确定性优先，GPU/NPU 设备选择留给 R2 硬件探测。
fn openvino_ep() -> ort::ep::ExecutionProviderDispatch {
    ort::ep::OpenVINO::default()
        .with_device_type("CPU")
        .build()
        .error_on_failure()
}

/// CUDA EP：设备 0（多卡分派随 3.3 异构矩阵裁决）；注册失败进入回退链。
fn cuda_ep() -> ort::ep::ExecutionProviderDispatch {
    ort::ep::CUDA::default()
        .with_device_id(0)
        .build()
        .error_on_failure()
}

/// TensorRT EP：引擎缓存目录由 worker 环境变量传入（首编译自动，失效由 ORT 按引擎身份重建）。
fn trt_ep() -> ort::ep::ExecutionProviderDispatch {
    let mut trt = ort::ep::TensorRT::default().with_device_id(0);
    if let Ok(dir) = std::env::var("AM_RT_TRT_CACHE_DIR") {
        if !dir.trim().is_empty() {
            trt = trt.with_engine_cache(true).with_engine_cache_path(dir);
        }
    }
    trt.build().error_on_failure()
}

fn openvino_execution() -> ExecutionInfo {
    ExecutionInfo {
        backend: InferenceBackend::OpenVINO,
        device: "CPU".into(),
        fallback_reason: None,
    }
}
/// DirectML EP（E2）：运行库目录自含 DirectML.dll；注册失败按策略回退。
fn directml_ep() -> ort::ep::ExecutionProviderDispatch {
    ort::ep::DirectML::default().build()
}

fn directml_execution() -> ExecutionInfo {
    ExecutionInfo {
        backend: InferenceBackend::DirectML,
        device: "DirectML".into(),
        fallback_reason: None,
    }
}

fn cuda_execution() -> ExecutionInfo {
    ExecutionInfo {
        backend: InferenceBackend::Cuda,
        device: "CUDA:0".into(),
        fallback_reason: None,
    }
}
fn trt_execution() -> ExecutionInfo {
    ExecutionInfo {
        backend: InferenceBackend::Cuda,
        device: "TensorRT:0".into(),
        fallback_reason: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_keys_roundtrip_and_reject_unknown() {
        for backend in [
            InferenceBackend::Cpu,
            InferenceBackend::OpenVINO,
            InferenceBackend::Cuda,
        ] {
            assert_eq!(parse_backend(backend_key(backend)).unwrap(), backend);
        }
        for value in ["", "auto", "OpenVINO", "Cuda", "gpu"] {
            assert!(parse_backend(value).is_err());
        }
    }

    #[test]
    fn openvino_probe_never_panics_without_runtime() {
        // 探测只做文件存在性检查；无运行库的机器返回 false 而非报错。
        let _ = openvino_available();
    }

    // 以下三分支覆盖 DEL-16：无 TRT/GPU 环境以注入验证降级链逻辑。

    #[test]
    fn cascade_uses_trt_directly_when_it_succeeds() {
        // TRT 成功：直接采用，不再构建 CUDA/CPU（闭包按引用捕获计数验证）
        let mut cpu_called = false;
        let (value, reason) = gpu_fallback_chain(
            vec![
                ("TensorRT", Box::new(|| Ok::<i32, Error>(1))),
                (
                    "CUDA",
                    Box::new(|| Err::<i32, Error>(Error("cudaBoom".into()))),
                ),
            ],
            Box::new(|| {
                cpu_called = true;
                Ok(2)
            }),
        )
        .unwrap();
        assert_eq!(value, 1);
        assert!(reason.is_none());
        assert!(!cpu_called);
    }

    #[test]
    fn cascade_falls_to_cuda_when_trt_fails() {
        // TRT 失败但 CUDA 可用：落 CUDA 不触发 CPU，回退原因记录 TRT 失败
        let mut cpu_called = false;
        let (value, reason) = gpu_fallback_chain(
            vec![
                (
                    "TensorRT",
                    Box::new(|| Err::<i32, Error>(Error("trtBoom".into()))),
                ),
                ("CUDA", Box::new(|| Ok::<i32, Error>(2))),
            ],
            Box::new(|| {
                cpu_called = true;
                Ok(3)
            }),
        )
        .unwrap();
        assert_eq!(value, 2);
        assert!(reason.unwrap().contains("trtBoom"));
        assert!(!cpu_called);
    }

    #[test]
    fn cascade_falls_to_cpu_when_trt_and_cuda_fail() {
        // TRT/CUDA 均失败：回退 CPU 并聚合两级失败原因
        let (value, reason) = gpu_fallback_chain(
            vec![
                (
                    "TensorRT",
                    Box::new(|| Err::<i32, Error>(Error("trtBoom".into()))),
                ),
                (
                    "CUDA",
                    Box::new(|| Err::<i32, Error>(Error("cudaBoom".into()))),
                ),
            ],
            Box::new(|| Ok::<i32, Error>(3)),
        )
        .unwrap();
        assert_eq!(value, 3);
        let reason = reason.unwrap();
        assert!(reason.contains("trtBoom") && reason.contains("cudaBoom"));
        // CPU 也失败：错误带 CPU 原因与 GPU 两级尝试记录
        let error = gpu_fallback_chain::<i32>(
            vec![("CUDA", Box::new(|| Err(Error("cudaBoom".into()))))],
            Box::new(|| Err(Error("cpuBoom".into()))),
        )
        .unwrap_err();
        assert!(
            error.0.contains("cpuBoom")
                && error.0.contains("gpuTried")
                && error.0.contains("cudaBoom")
        );
    }
}
