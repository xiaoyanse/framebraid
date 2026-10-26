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
) -> std::result::Result<Session, AttemptError> {
    if !super::runtime::initialized() {
        return Err(AttemptError::key("vision.runtimeMissing", "runtime"));
    }
    let supported = match target.backend {
        0 => ort::ep::CPU::default().is_available(),
        1 => ort::ep::OpenVINO::default().is_available(),
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
            .with_device_type("CPU")
            .with_num_threads(threads)
            .build()
            .error_on_failure(),
        _ => ort::ep::CUDA::default()
            .with_device_id(target.device_id as i32)
            .build()
            .error_on_failure(),
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
