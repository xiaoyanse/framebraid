//! SDK 推理值类型；无模型运行库、宿主服务或业务副作用。
use serde::{Deserialize, Serialize};

/// 推理后端（P09）：CPU / OpenVINO 执行提供器 / CUDA（GPU 包按需安装）/ DirectML（E2）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum InferenceBackend {
    #[default]
    Cpu,
    OpenVINO,
    Cuda,
    DirectML,
}

/// 实际执行路径：请求后端、实际设备与回退原因（试推理展示用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ExecutionInfo {
    pub backend: InferenceBackend,
    /// 提供器设备名：CPU 为 "CPU"；OpenVINO 固定 CPU（R1）；CUDA/TensorRT 为 "CUDA:0"/"TensorRT:0"。
    pub device: String,
    /// 请求非 CPU 后端但注册/构建失败回退 CPU 时记录原因。
    #[serde(default)]
    pub fallback_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct PoseModelInfo {
    pub decoder: String,
    pub input_name: String,
    pub input_shape: Vec<i64>,
    pub output_shape: Vec<i64>,
    pub keypoint_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Keypoint {
    /// Normalized original-image coordinates, not model letterbox coordinates.
    pub x: f32,
    pub y: f32,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct PoseDetection {
    /// Normalized xyxy coordinates in the original image.
    pub bounds: [f32; 4],
    pub confidence: f32,
    pub keypoints: Vec<Keypoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct PoseFrame {
    pub width: u32,
    pub height: u32,
    pub inference_ms: u64,
    pub detections: Vec<PoseDetection>,
}

/// NVIDIA GPU 硬件探测结果（P09 R2）：驱动侧信息，进程级缓存。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct GpuStatus {
    pub detected: bool,
    /// nvidia-smi 报告的显卡名。
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub driver_version: Option<String>,
    /// 驱动支持的 CUDA 上限版本；探测失败时缺省。
    #[serde(default)]
    pub cuda_version: Option<String>,
}
