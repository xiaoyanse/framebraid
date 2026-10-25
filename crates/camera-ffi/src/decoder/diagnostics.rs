use am_camera_core::media::ffmpeg_lib::FfmpegLoadError;
use am_sdk_support::{abi::AM_FAULT_RESOURCE, diagnostics::Detail};

pub fn detail(error: FfmpegLoadError) -> Detail {
    let source = match error.component {
        "avutil-61.dll" => c"camera.ffmpeg.avutil-61.dll",
        "swresample-7.dll" => c"camera.ffmpeg.swresample-7.dll",
        "swscale-10.dll" => c"camera.ffmpeg.swscale-10.dll",
        "avcodec-63.dll" => c"camera.ffmpeg.avcodec-63.dll",
        "avformat-63.dll" => c"camera.ffmpeg.avformat-63.dll",
        _ => c"camera.ffmpeg",
    };
    let message = match error.key {
        "media.ffmpegIncomplete" | "media.ffmpegDirMissing" => match error.component {
            "swresample-7.dll" => c"FFmpeg missing swresample-7.dll; deploy the complete matched x64 runtime and pass its directory through runtime_dirs_utf8.",
            "swscale-10.dll" => c"FFmpeg missing swscale-10.dll; deploy the complete matched x64 runtime and pass its directory through runtime_dirs_utf8.",
            "avcodec-63.dll" => c"FFmpeg missing avcodec-63.dll; deploy the complete matched x64 runtime and pass its directory through runtime_dirs_utf8.",
            "avformat-63.dll" => c"FFmpeg missing avformat-63.dll; deploy the complete matched x64 runtime and pass its directory through runtime_dirs_utf8.",
            _ => c"FFmpeg missing avutil-61.dll; deploy the complete matched x64 runtime and pass its directory through runtime_dirs_utf8.",
        },
        "media.ffmpegSymbolMissing" => c"FFmpeg required exported symbol missing; use the complete matched runtime (avutil 61, swresample 7, swscale 10, avcodec 63, avformat 63), not renamed DLLs.",
        "media.ffmpegVersionMismatch" => c"FFmpeg ABI major version mismatch; required avutil 61, swresample 7, swscale 10, avcodec 63, avformat 63. Deploy a matched runtime through runtime_dirs_utf8.",
        "media.ffmpegDirInvalid" => c"FFmpeg runtime directory cannot be resolved; pass an existing accessible directory through runtime_dirs_utf8.",
        "media.ffmpegDomainConflict" => c"FFmpeg already bound to a different runtime directory in this DLL; use the same explicit runtime directory for all contexts.",
        _ => match error.os_code {
            Some(126) => c"FFmpeg LoadLibrary failed (Windows error 126): the named module or one of its dependent DLLs was not found. Deploy the matched runtime and its dependencies through runtime_dirs_utf8.",
            Some(193) => c"FFmpeg LoadLibrary failed (Windows error 193): invalid native image or architecture mismatch. This SDK requires valid Windows x64 DLLs.",
            Some(127) => c"FFmpeg LoadLibrary failed (Windows error 127): a dependent exported procedure was not found. Deploy a complete matching runtime; do not mix DLL versions.",
            Some(1114) => c"FFmpeg LoadLibrary failed (Windows error 1114): native DLL initialization failed. Verify the runtime and its native dependencies before retrying.",
            Some(5) => c"FFmpeg LoadLibrary failed (Windows error 5): access denied. Check runtime file access and system security policy.",
            _ => c"FFmpeg native runtime could not be loaded; source identifies the failing module. Verify matching x64 DLLs, native dependencies and runtime_dirs_utf8. No VC++ or GPU failure is inferred.",
        },
    };
    Detail {
        message,
        source,
        fault_class: AM_FAULT_RESOURCE as u32,
    }
}
