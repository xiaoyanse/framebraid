use super::*;
use am_camera_core::media::{
    codec::access_units,
    ffmpeg_lib::{probe_dirs, FfmpegApi, FfmpegLoadError},
};
use std::path::PathBuf;
type RuntimeBinding = Mutex<Option<(PathBuf, Arc<FfmpegApi>)>>;
static RUNTIME: am_sdk_support::unload::UnloadCell<RuntimeBinding> =
    am_sdk_support::unload::UnloadCell::new();
#[cfg(all(windows, not(test)))]
pub fn unload_ready() -> bool {
    RUNTIME.get().is_none_or(|runtime| {
        runtime.try_lock().is_ok_and(|value| {
            value
                .as_ref()
                .is_none_or(|(_, api)| Arc::strong_count(api) == 1)
        })
    })
}
#[cfg(all(windows, not(test)))]
pub unsafe fn unload_static() {
    // 符号表只含裸函数指针；释放它的Arc不会卸载FFmpeg或调用原生API。
    let _ = unsafe {
        RUNTIME.clear_if(|runtime| {
            runtime.try_lock().is_ok_and(|value| {
                value
                    .as_ref()
                    .is_none_or(|(_, api)| Arc::strong_count(api) == 1)
            })
        })
    };
}

#[derive(Clone)]
pub struct Config {
    pub abi: am_decoder_config_t,
    pub extra: Vec<u8>,
    pub codec: VideoCodec,
}
unsafe impl Send for Config {}
impl Config {
    pub unsafe fn read(pointer: *const am_decoder_config_t) -> Result<Self> {
        let mut abi = unsafe { args::read(pointer, size_of::<am_decoder_config_t>()) }?;
        let codec = match abi.codec {
            AM_CODEC_H264 => VideoCodec::H264,
            AM_CODEC_H265 => VideoCodec::H265,
            _ => return Err(AM_ERR_CAMERA_CODEC_UNSUPPORTED),
        };
        if abi.bitstream_format != AM_BSF_ANNEXB {
            return Err(AM_ERR_CAMERA_CODEC_UNSUPPORTED);
        }
        if abi.output_pixel_format != AM_PF_ANY && abi.output_pixel_format > AM_PF_NV12 as u32 {
            return Err(AM_ERR_CAMERA_PIXEL_FORMAT_UNSUPPORTED);
        }
        if abi.hw_policy > 1
            || abi.reserved.iter().any(|x| *x != 0)
            || (abi.time_base_num == 0) != (abi.time_base_den == 0)
        {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        if abi.queue_max_items == 0 {
            abi.queue_max_items = SDK_DECODER_INPUT_ITEMS as u32;
        }
        if abi.queue_max_bytes == 0 {
            abi.queue_max_bytes = SDK_DECODER_INPUT_BYTES;
        }
        if abi.queue_max_items > SDK_DECODER_INPUT_ITEMS as u32
            || abi.queue_max_bytes > SDK_DECODER_INPUT_BYTES
            || abi.extradata_size > SDK_GOP_CACHE_BYTES
        {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        if abi.extradata_size > 0 && abi.extradata.is_null() {
            return Err(AM_ERR_SDK_NULL_ARGUMENT);
        }
        let extra = if abi.extradata_size == 0 {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(abi.extradata, abi.extradata_size as usize) }
                .to_vec()
        };
        if !extra.is_empty() {
            access_units(codec, &extra).map_err(convert)?;
        }
        if abi.output_pixel_format == AM_PF_ANY {
            abi.output_pixel_format = AM_PF_RGBA8 as u32;
        }
        abi.extradata = std::ptr::null();
        Ok(Self { abi, extra, codec })
    }
}
pub fn runtime(state: &CameraState) -> Result<Arc<FfmpegApi>> {
    let failure = |key, component, code| {
        record_failure(
            state,
            FfmpegLoadError {
                key,
                component,
                os_code: None,
            },
            code,
        )
    };
    let ctx = state.ctx.data::<Context>()?;
    let candidates = if ctx.runtime_dirs.is_empty() {
        probe_dirs(
            std::env::var_os("AM_RT_FFMPEG_DIR"),
            std::env::current_exe().ok(),
        )
    } else {
        ctx.runtime_dirs.iter().map(PathBuf::from).collect()
    };
    let chosen = candidates
        .into_iter()
        .find(|p| p.join("avutil-61.dll").is_file())
        .ok_or_else(|| {
            failure(
                "media.ffmpegDirMissing",
                "avutil-61.dll",
                AM_ERR_SDK_NOT_SUPPORTED,
            )
        })?;
    let chosen = chosen
        .canonicalize()
        .map_err(|_| failure("media.ffmpegDirInvalid", "ffmpeg", AM_ERR_SDK_NOT_SUPPORTED))?;
    let mut guard = RUNTIME
        .get_or_init(Mutex::default)
        .lock()
        .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
    if let Some((path, api)) = &*guard {
        if path != &chosen {
            return Err(failure(
                "media.ffmpegDomainConflict",
                "ffmpeg",
                AM_ERR_SDK_DOMAIN_CONFLICT,
            ));
        }
        return Ok(api.clone());
    }
    let api = Arc::new(
        FfmpegApi::load_from_detailed(&chosen)
            .map_err(|error| record_failure(state, error, AM_ERR_SDK_NOT_SUPPORTED))?,
    );
    *guard = Some((chosen, api.clone()));
    Ok(api)
}
fn record_failure(state: &CameraState, error: FfmpegLoadError, code: i32) -> i32 {
    am_sdk_support::diagnostics::record(
        domain(),
        state.ctx_key,
        code,
        super::diagnostics::detail(error),
    );
    code
}
#[derive(Default)]
pub struct Gate {
    sets: [Option<Vec<u8>>; 3],
    pub dirty: bool,
    pub started: bool,
    rasl: bool,
}
impl Gate {
    pub fn restart(&mut self) {
        self.started = false;
        self.dirty = false;
        self.rasl = false;
    }
    pub fn new(cfg: &Config) -> Result<Self> {
        let mut g = Self::default();
        if !cfg.extra.is_empty() {
            g.observe(cfg.codec, &cfg.extra)?;
        }
        Ok(g)
    }
    pub fn observe(&mut self, codec: VideoCodec, bytes: &[u8]) -> Result<bool> {
        let units = access_units(codec, bytes).map_err(convert)?;
        let mut random = false;
        let mut rasl = false;
        for nal in units {
            let kind = if codec == VideoCodec::H264 {
                nal[0] & 31
            } else {
                (nal[0] >> 1) & 63
            };
            let index = match (codec, kind) {
                (VideoCodec::H264, 7) => Some(1),
                (VideoCodec::H264, 8) => Some(2),
                (VideoCodec::H265, 32) => Some(0),
                (VideoCodec::H265, 33) => Some(1),
                (VideoCodec::H265, 34) => Some(2),
                _ => None,
            };
            if let Some(i) = index {
                if self.sets[i].as_deref() != Some(nal) {
                    if nal.len() > SDK_GOP_CACHE_BYTES as usize {
                        return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
                    }
                    let total: usize = self
                        .sets
                        .iter()
                        .enumerate()
                        .filter(|(j, _)| *j != i)
                        .filter_map(|(_, p)| p.as_ref())
                        .map(Vec::len)
                        .sum();
                    if total + nal.len() + 12 > SDK_GOP_CACHE_BYTES as usize {
                        return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
                    }
                    self.sets[i] = Some(nal.to_vec());
                    self.dirty = true;
                    self.started = false;
                }
            }
            random |= codec == VideoCodec::H264 && kind == 5
                || codec == VideoCodec::H265 && (16..=21).contains(&kind);
            rasl |= codec == VideoCodec::H265 && (kind == 8 || kind == 9);
        }
        if codec == VideoCodec::H265 && random {
            self.rasl = true;
        }
        if self.rasl && rasl {
            return Ok(false);
        }
        let complete = self.sets[1].is_some()
            && self.sets[2].is_some()
            && (codec == VideoCodec::H264 || self.sets[0].is_some());
        Ok(complete && (self.started || random))
    }
    pub fn extra(&self) -> Vec<u8> {
        let mut result = Vec::new();
        for nal in self.sets.iter().flatten() {
            result.extend_from_slice(&[0, 0, 0, 1]);
            result.extend_from_slice(nal);
        }
        result
    }
}
pub fn convert(e: DecodeError) -> i32 {
    match e {
        DecodeError::AllocationFailed => AM_ERR_SDK_OUT_OF_MEMORY,
        DecodeError::FrameTooLarge => AM_ERR_SDK_RESOURCE_EXHAUSTED,
        DecodeError::Drained => AM_ERR_SDK_DRAINED,
        _ => AM_ERR_CAMERA_CODEC_UNSUPPORTED,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn h264_parameter_change_requires_real_idr_again() {
        let mut g = Gate::default();
        assert!(!g
            .observe(VideoCodec::H264, &[0, 0, 1, 0x67, 1, 0, 0, 1, 0x68, 2])
            .unwrap());
        assert!(!g.observe(VideoCodec::H264, &[0, 0, 1, 0x61, 3]).unwrap());
        assert!(g.observe(VideoCodec::H264, &[0, 0, 1, 0x65, 3]).unwrap());
        g.started = true;
        g.dirty = false;
        assert!(g.observe(VideoCodec::H264, &[0, 0, 1, 0x61, 3]).unwrap());
        assert!(!g.observe(VideoCodec::H264, &[0, 0, 1, 0x67, 9]).unwrap());
        assert!(g.dirty && !g.started);
        assert!(!g.observe(VideoCodec::H264, &[0, 0, 1, 0x61, 3]).unwrap());
    }
    #[test]
    fn hevc_needs_all_sets_and_suppresses_cra_leading_rasl() {
        let mut g = Gate::default();
        assert!(!g
            .observe(VideoCodec::H265, &[0, 0, 1, 0x40, 1, 0, 0, 1, 0x42, 1])
            .unwrap());
        assert!(!g.observe(VideoCodec::H265, &[0, 0, 1, 0x26, 1]).unwrap());
        assert!(!g.observe(VideoCodec::H265, &[0, 0, 1, 0x44, 1]).unwrap());
        assert!(g.observe(VideoCodec::H265, &[0, 0, 1, 0x2a, 1]).unwrap());
        g.started = true;
        assert!(!g.observe(VideoCodec::H265, &[0, 0, 1, 0x10, 1]).unwrap());
        assert!(g.observe(VideoCodec::H265, &[0, 0, 1, 0x02, 1]).unwrap());
    }
}
