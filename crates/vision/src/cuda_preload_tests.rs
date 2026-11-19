use super::*;

#[test]
fn cuda_dependency_match_covers_families_and_rejects_others() {
    for name in [
        "cudart64_13.dll",
        "cudart64_12.dll",
        "cublas64_13.dll",
        "cublasLt64_13.dll",
        "cufft64_12.dll",
        "nvblas64_13.dll",
        "nvrtc64_130_0.dll",
        "cudnn64_9.dll",
        "cudnn_graph64_9.dll",
        "cudnn_heuristic64_9.dll",
        "cudnn_ops64_9.dll",
        "cudnn_cnn64_9.dll",
        "cudnn_adv64_9.dll",
        "cudnn_engines_precompiled64_9.dll",
        "cudnn_engines_runtime_compiled64_9.dll",
        "cudnn_engines_tensor_ir64_9.dll",
        "cudnn_ext64_9.dll",
    ] {
        assert!(is_cuda_dependency(name), "{name} 应命中预载族");
    }
    for name in [
        "onnxruntime.dll",
        "onnxruntime_providers_cuda.dll",
        "onnxruntime_providers_shared.dll",
        "DirectML.dll",
        "cufftw64_12.dll",
        "nvrtc-builtins64_134.dll",
        "cudart64_13",
        "cudart64_13.dll.bak",
        "cudnn_ops64_9.pdb",
    ] {
        assert!(!is_cuda_dependency(name), "{name} 不应命中预载族");
    }
}

#[cfg(windows)]
#[test]
fn preload_from_dirs_tolerates_missing_directory() {
    preload_from_dirs(&[std::path::PathBuf::from("Z:/no-such-cuda-dir")]);
    preload_from_dirs(&[]);
}

#[cfg(windows)]
#[test]
fn preload_module_is_idempotent_and_silent_on_failure() {
    // 系统必载模块名命中 GetModuleHandleW，走幂等跳过路径。
    preload_module(Path::new("C:/Windows/System32/kernel32.dll"));
    // 缺席文件加载失败静默返回，不 panic 不阻断会话创建。
    preload_module(Path::new("Z:/missing/cudart64_99.dll"));
}
