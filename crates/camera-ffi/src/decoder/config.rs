use super::*;
use am_camera_core::media::{
    codec::{selection::DeviceSelector, visit_access_units},
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
    /// 设备定向选择器（§3.5b）：None=默认策略零变化；解析后的匹配在 decoder
    /// create/reset 的设备选择点执行（零命中=sdk.invalidArgument 同步返回）。
    pub device: Option<DeviceSelector>,
}
unsafe impl Send for Config {}
/// 解析 am_decoder_config_t.device（§3.5b）：NULL=不定向；结构/保留位/终结符
/// 错误在此同步返回；零命中与实例越界在设备选择点（make_inner）拒绝。
unsafe fn device_selector(pointer: *const am_device_selector_t) -> Result<Option<DeviceSelector>> {
    if pointer.is_null() {
        return Ok(None);
    }
    let raw = unsafe { args::read(pointer, size_of::<am_device_selector_t>()) }?;
    if raw.reserved != 0 {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    if raw.match_utf8.is_null() || raw.match_bytes == 0 {
        return Ok(None);
    }
    if raw.match_bytes as usize > SDK_MAX_STRING_BYTES as usize {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    // match_bytes 含 NUL 终结：末字节必须为 0、前缀无内嵌 NUL 且为合法 UTF-8。
    let bytes = unsafe {
        std::slice::from_raw_parts(raw.match_utf8.cast::<u8>(), raw.match_bytes as usize)
    };
    if bytes.last() != Some(&0) || bytes[..bytes.len() - 1].contains(&0) {
        return Err(AM_ERR_SDK_UTF8_INVALID);
    }
    let needle =
        std::str::from_utf8(&bytes[..bytes.len() - 1]).map_err(|_| AM_ERR_SDK_UTF8_INVALID)?;
    Ok(Some(DeviceSelector::new(needle, raw.instance)))
}
impl Config {
    pub unsafe fn read(
        pointer: *const am_decoder_config_t,
        mut reserve: impl FnMut(usize) -> Result<()>,
    ) -> Result<Self> {
        let mut abi = unsafe { args::read(pointer, size_of::<am_decoder_config_t>()) }?;
        let device = unsafe { device_selector(abi.device) }?;
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
        reserve(abi.extradata_size as usize)?;
        let mut extra = Vec::new();
        extra
            .try_reserve_exact(abi.extradata_size as usize)
            .map_err(|_| AM_ERR_SDK_OUT_OF_MEMORY)?;
        if abi.extradata_size > 0 {
            extra.extend_from_slice(unsafe {
                std::slice::from_raw_parts(abi.extradata, abi.extradata_size as usize)
            });
        }
        if !extra.is_empty() {
            visit_access_units(codec, &extra, |_| {}).map_err(convert)?;
        }
        if abi.output_pixel_format == AM_PF_ANY {
            abi.output_pixel_format = AM_PF_RGBA8 as u32;
        }
        abi.extradata = std::ptr::null();
        Ok(Self {
            abi,
            extra,
            codec,
            device,
        })
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
                version: None,
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
        self.observe_reserved(codec, bytes, |_| Ok(()))
    }
    pub(super) fn observe_reserved(
        &mut self,
        codec: VideoCodec,
        bytes: &[u8],
        mut reserve: impl FnMut(usize) -> Result<()>,
    ) -> Result<bool> {
        let mut replacements = [None; 3];
        let mut saw_change = false;
        let mut oversized = false;
        let mut random = false;
        let mut rasl = false;
        visit_access_units(codec, bytes, |nal| {
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
                let previous = replacements[i].or(self.sets[i].as_deref());
                if previous != Some(nal) {
                    saw_change = true;
                    let other: usize = replacements
                        .iter()
                        .enumerate()
                        .filter(|(j, _)| *j != i)
                        .map(|(j, next)| {
                            next.map_or_else(
                                || self.sets[j].as_ref().map_or(0, Vec::len),
                                <[u8]>::len,
                            )
                        })
                        .sum();
                    oversized |= other.saturating_add(nal.len()).saturating_add(12)
                        > SDK_GOP_CACHE_BYTES as usize;
                }
                replacements[i] = Some(nal);
            }
            random |= codec == VideoCodec::H264 && kind == 5
                || codec == VideoCodec::H265 && (16..=21).contains(&kind);
            rasl |= codec == VideoCodec::H265 && (kind == 8 || kind == 9);
        })
        .map_err(convert)?;
        if oversized {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        let changed = replacements
            .iter()
            .enumerate()
            .any(|(i, next)| next.is_some_and(|value| self.sets[i].as_deref() != Some(value)));
        if changed {
            let total: usize = replacements
                .iter()
                .enumerate()
                .map(|(i, next)| {
                    next.map_or_else(|| self.sets[i].as_ref().map_or(0, Vec::len), <[u8]>::len)
                })
                .sum();
            if total.saturating_add(12) > SDK_GOP_CACHE_BYTES as usize {
                return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
            }
            reserve(total)?;
            let mut owned: [Option<Vec<u8>>; 3] = std::array::from_fn(|_| None);
            for (i, next) in replacements.iter().enumerate() {
                if let Some(next) = next.filter(|next| self.sets[i].as_deref() != Some(*next)) {
                    let mut value = Vec::new();
                    value
                        .try_reserve_exact(next.len())
                        .map_err(|_| AM_ERR_SDK_OUT_OF_MEMORY)?;
                    if value.capacity() > next.len() {
                        return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
                    }
                    value.extend_from_slice(next);
                    owned[i] = Some(value);
                }
            }
            for (i, next) in owned.into_iter().enumerate() {
                if let Some(next) = next {
                    self.sets[i] = Some(next);
                }
            }
        }
        if saw_change {
            self.dirty = true;
            self.started = false;
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
        let mut result = Vec::with_capacity(self.extra_bytes());
        self.append_extra(&mut result);
        result
    }
    pub(super) fn try_extra(&self) -> Result<Vec<u8>> {
        let mut result = Vec::new();
        result
            .try_reserve_exact(self.extra_bytes())
            .map_err(|_| AM_ERR_SDK_OUT_OF_MEMORY)?;
        self.append_extra(&mut result);
        Ok(result)
    }
    fn append_extra(&self, result: &mut Vec<u8>) {
        for nal in self.sets.iter().flatten() {
            result.extend_from_slice(&[0, 0, 0, 1]);
            result.extend_from_slice(nal);
        }
    }
    pub(super) fn extra_bytes(&self) -> usize {
        self.sets.iter().flatten().map(|v| v.len() + 4).sum()
    }
}
pub fn convert(e: DecodeError) -> i32 {
    match e {
        DecodeError::AllocationFailed => AM_ERR_SDK_OUT_OF_MEMORY,
        DecodeError::FrameTooLarge => AM_ERR_SDK_RESOURCE_EXHAUSTED,
        DecodeError::Drained => AM_ERR_SDK_DRAINED,
        // 定向零命中/实例越界（§3.5b）：配置错误同步拒绝。
        DecodeError::InvalidDeviceSelector => AM_ERR_SDK_INVALID_ARGUMENT,
        _ => AM_ERR_CAMERA_CODEC_UNSUPPORTED,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn device_selector_contract_is_validated_on_config_read() {
        let needle = b"Arc 140T\0";
        let make = |match_bytes: u32, reserved: u32, struct_size: u32| am_device_selector_t {
            struct_size,
            match_utf8: needle.as_ptr().cast(),
            match_bytes,
            instance: 1,
            reserved,
        };
        let mut abi = am_decoder_config_t {
            struct_size: size_of::<am_decoder_config_t>() as u32,
            codec: AM_CODEC_H264,
            bitstream_format: AM_BSF_ANNEXB,
            output_pixel_format: AM_PF_RGBA8 as u32,
            hw_policy: 0,
            ..Default::default()
        };
        let mut read = |selector: *const am_device_selector_t| {
            abi.device = selector;
            unsafe { Config::read(&abi, |_| Ok(())) }
        };
        // NULL 与空 match_bytes 均为不定向：默认策略零变化（§3.5b）。
        let empty = make(0, 0, size_of::<am_device_selector_t>() as u32);
        assert!(read(std::ptr::null()).unwrap().device.is_none());
        assert!(read(&empty).unwrap().device.is_none());
        // 合法定向：needle 小写化+instance 透传。
        let valid = make(
            needle.len() as u32,
            0,
            size_of::<am_device_selector_t>() as u32,
        );
        let parsed = read(&valid).unwrap().device.unwrap();
        assert_eq!(parsed.needle(), "arc 140t");
        assert_eq!(parsed.instance(), 1);
        // reserved≠0 = sdk.invalidArgument。
        let reserved = make(
            needle.len() as u32,
            1,
            size_of::<am_device_selector_t>() as u32,
        );
        assert_eq!(
            read(&reserved).map(|_| ()).unwrap_err(),
            AM_ERR_SDK_INVALID_ARGUMENT
        );
        // 无 NUL 终结 = sdk.utf8Invalid。
        let unterminated = make(
            needle.len() as u32 - 1,
            0,
            size_of::<am_device_selector_t>() as u32,
        );
        assert_eq!(
            read(&unterminated).map(|_| ()).unwrap_err(),
            AM_ERR_SDK_UTF8_INVALID
        );
        // struct_size 非法 = sdk.structSizeInvalid。
        let short = make(needle.len() as u32, 0, 8);
        assert_eq!(
            read(&short).map(|_| ()).unwrap_err(),
            AM_ERR_SDK_STRUCT_SIZE_INVALID
        );
    }
    #[test]
    fn h264_combined_sets_and_idr_in_one_packet_is_ready() {
        // FILE 源 framing 输出形态：SPS+PPS 前置与 IDR 同包（Annex-B）。
        let mut g = Gate::default();
        let combined = [
            &[0u8, 0, 0, 1, 0x67, 100, 0, 30, 172, 217][..],
            &[0, 0, 0, 1, 0x68, 100, 0][..],
            &[0, 0, 0, 1, 0x65, 158, 66, 120, 133][..],
        ]
        .concat();
        assert!(
            g.observe(VideoCodec::H264, &combined).unwrap(),
            "SPS+PPS+IDR in one packet must be ready"
        );
    }
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
