//! 原生会话装配与错误分层；不将编译支持冒充provider已加载。
use super::{
    model::ModelSource,
    selection::{AttemptError, BackendTarget},
};
use ort::{ep::ExecutionProvider, session::Session};

pub(super) fn create(
    source: ModelSource<'_>,
    bytes: &[u8],
    target: BackendTarget,
    threads: usize,
    cuda_fp32: bool,
    ov_gpu: bool,
) -> std::result::Result<Session, AttemptError> {
    if !super::runtime::initialized() {
        return Err(AttemptError::key("vision.runtimeMissing", "runtime"));
    }
    let supported = match target.backend {
        0 => ort::ep::CPU::default().is_available(),
        1 => ort::ep::OpenVINO::default().is_available(),
        3 => ort::ep::DirectML::default().is_available(),
        _ => ort::ep::CUDA::default().is_available(),
    }
    .map_err(|e| AttemptError::native(e, "vision.backendStrictUnavailable", "compiled_provider"))?;
    if !supported {
        return Err(AttemptError::key(
            "vision.backendStrictUnavailable",
            "compiled_provider",
        ));
    }
    let mut builder = Session::builder()
        .and_then(|b| Ok(b.with_intra_threads(threads)?))
        .and_then(|b| Ok(b.with_inter_threads(1)?))
        .and_then(|b| Ok(b.with_parallel_execution(false)?))
        .and_then(|b| Ok(b.with_independent_thread_pool()?))
        .and_then(|b| Ok(b.with_no_environment_execution_providers()?))
        .map_err(|e| AttemptError::native(e, "vision.modelLoadFailed", "session_options"))?;
    let provider = match target.backend {
        0 => ort::ep::CPU::default().build().error_on_failure(),
        1 => ort::ep::OpenVINO::default()
            // 定向（§3.5b）：OV 设备粒度为单 "GPU"；默认保持 CPU（既有行为零变化）。
            .with_device_type(if ov_gpu { "GPU" } else { "CPU" })
            .with_num_threads(threads)
            .build()
            .error_on_failure(),
        // 定向（§3.5b）：device_id 由 selector 解析为 DXGI 序号（外部候选恒 0）；
        // 0 保持 EP 默认选卡（既有行为零变化）。
        3 => {
            let directml = if target.device_id != 0 {
                ort::ep::DirectML::default().with_device_id(target.device_id as i32)
            } else {
                ort::ep::DirectML::default()
            };
            // 与 CPU/OV/CUDA 分支同构（F01）：注册失败必须报错。ort rc.13 默认
            // fail_silently 会放行 EP 注册失败、回落 CPU 仍建出会话，导致选中
            // 记录与实际执行后端失真（strict/actual/设备定向全部失真）；是否
            // 回退由 selection::choose 按候选策略统一裁决（回退可达/严格即停）。
            directml.build().error_on_failure()
        }
        _ => {
            let cuda = ort::ep::CUDA::default().with_device_id(target.device_id as i32);
            let cuda = if cuda_fp32 {
                cuda.with_tf32(false)
            } else {
                cuda
            };
            cuda.build().error_on_failure()
        }
    };
    super::runtime::claim_backend(target.backend).map_err(|e| AttemptError {
        error: e,
        stage: "backend_domain",
        native_code: None,
    })?;
    builder = builder.with_execution_providers([provider]).map_err(|e| {
        AttemptError::native(
            e,
            "vision.backendStrictUnavailable",
            "provider_registration",
        )
    })?;
    let session = match source {
        ModelSource::Path(path) => builder.commit_from_file(path),
        ModelSource::Memory(_) => builder.commit_from_memory(bytes),
    }
    .map_err(|e| {
        let permanent = target.backend == 0
            || matches!(
                e.code(),
                ort::ErrorCode::InvalidArgument
                    | ort::ErrorCode::NoSuchFile
                    | ort::ErrorCode::NoModel
                    | ort::ErrorCode::InvalidProtobuf
                    | ort::ErrorCode::InvalidGraph
            );
        AttemptError::native(
            e,
            if permanent {
                "vision.modelLoadFailed"
            } else {
                "vision.backendStrictUnavailable"
            },
            "model_session",
        )
    })?;
    super::runtime::loaded(target.backend);
    Ok(session)
}
