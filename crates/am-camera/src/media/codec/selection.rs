//! 两种像素解码入口共用初始化策略；不重放视频包，不把初始化当实际执行。
use super::{hardware::Binding, CpuDecoder, DecodeError, HardwareInput, Result, VideoCodec};
use crate::media::{
    ffmpeg_lib::FfmpegApi,
    hardware::{self, Adapter, Backend, ProbeError},
};
use serde::Serialize;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum HardwarePolicy {
    SoftwareOnly,
    PreferHardware,
}
#[derive(Clone, Debug, Serialize)]
pub enum SelectionFailure {
    Inventory(ProbeError),
    DecoderConfigurationUnavailable,
    Decoder(DecodeError),
    NoCompatibleHardware,
}
#[derive(Clone, Debug, Serialize)]
pub struct SelectionAttempt {
    pub backend: Backend,
    pub adapter_index: u32,
    pub adapter_luid: u64,
    pub error: Option<SelectionFailure>,
}
#[derive(Clone, Debug, Serialize)]
pub struct SelectedDevice {
    pub backend: Backend,
    pub adapter: Adapter,
}
#[derive(Clone, Debug, Serialize)]
pub struct BackendSupport {
    pub backend: Backend,
    pub compiled: Option<bool>,
    pub codec_configured: Option<bool>,
}
#[derive(Clone, Debug, Serialize)]
pub struct SelectionReport {
    pub policy: HardwarePolicy,
    pub selected: Option<SelectedDevice>,
    pub attempts: Vec<SelectionAttempt>,
    pub backend_support: [BackendSupport; 2],
    pub fallback: Option<SelectionFailure>,
    pub elapsed_ms: u128,
}
pub struct Selection {
    pub decoder: CpuDecoder,
    pub report: SelectionReport,
}

impl CpuDecoder {
    /// 仅选择初始化设备；实际后端须在输出真实帧后查询hardware_backend。
    pub fn create_with_policy(
        api: Arc<FfmpegApi>,
        codec: VideoCodec,
        extra: &[u8],
        max_pixels: u64,
        policy: HardwarePolicy,
        budget: Duration,
    ) -> Result<Selection> {
        Self::create_with_adapter_source(
            api,
            codec,
            extra,
            max_pixels,
            policy,
            budget,
            hardware::adapters,
        )
    }

    fn create_with_adapter_source(
        api: Arc<FfmpegApi>,
        codec: VideoCodec,
        extra: &[u8],
        max_pixels: u64,
        policy: HardwarePolicy,
        budget: Duration,
        adapters_source: impl FnOnce() -> std::result::Result<Vec<Adapter>, ProbeError>,
    ) -> Result<Selection> {
        let start = Instant::now();
        // 先校验共同参数并准备CPU后备；尚未送包，不在此处分配解码参考帧。
        let cpu = Self::create(api.clone(), codec, extra, max_pixels)?;
        let mut report = SelectionReport {
            policy,
            selected: None,
            attempts: Vec::new(),
            backend_support: [Backend::D3d11va, Backend::Dxva2].map(|backend| BackendSupport {
                backend,
                compiled: None,
                codec_configured: None,
            }),
            fallback: None,
            elapsed_ms: 0,
        };
        if policy == HardwarePolicy::SoftwareOnly {
            report.elapsed_ms = start.elapsed().as_millis();
            return Ok(Selection {
                decoder: cpu,
                report,
            });
        }
        if budget.is_zero() || budget > Duration::from_secs(5) {
            return Err(DecodeError::InvalidInput);
        }
        let admission = hardware::enter_probe(budget);
        let _guard = match admission {
            Ok(guard) => guard,
            Err(error) => {
                return Ok(fallback(
                    cpu,
                    report,
                    SelectionFailure::Inventory(error),
                    start,
                ))
            }
        };
        if start.elapsed() >= budget {
            return Ok(fallback(
                cpu,
                report,
                SelectionFailure::Inventory(ProbeError::BudgetExhausted),
                start,
            ));
        }
        let codec_ptr = unsafe { (api.avcodec_find_decoder_by_name)(codec.name().as_ptr()) };
        for support in &mut report.backend_support {
            if start.elapsed() >= budget {
                return Ok(fallback(
                    cpu,
                    report,
                    SelectionFailure::Inventory(ProbeError::BudgetExhausted),
                    start,
                ));
            }
            support.compiled = Some(hardware::compiled(&api, support.backend));
            if start.elapsed() >= budget {
                return Ok(fallback(
                    cpu,
                    report,
                    SelectionFailure::Inventory(ProbeError::BudgetExhausted),
                    start,
                ));
            }
            support.codec_configured =
                Some(unsafe { Binding::format(&api, codec_ptr, support.backend) }.is_some());
        }
        let adapters = match adapters_source() {
            Ok(adapters) => adapters,
            Err(error) => {
                return Ok(fallback(
                    cpu,
                    report,
                    SelectionFailure::Inventory(error),
                    start,
                ))
            }
        };
        for support in &report.backend_support {
            let backend = support.backend;
            let compiled = support.compiled == Some(true);
            let configured = support.codec_configured == Some(true);
            for adapter in adapters.iter().filter(|a| !a.software) {
                if start.elapsed() >= budget {
                    return Ok(fallback(
                        cpu,
                        report,
                        SelectionFailure::Inventory(ProbeError::BudgetExhausted),
                        start,
                    ));
                }
                let failure = if !compiled {
                    Some(SelectionFailure::Inventory(ProbeError::NotCompiled))
                } else if !configured {
                    Some(SelectionFailure::DecoderConfigurationUnavailable)
                } else if backend == Backend::Dxva2 && adapter.dxva2_index.is_none() {
                    Some(SelectionFailure::Inventory(
                        ProbeError::AdapterMappingUnavailable,
                    ))
                } else {
                    match create_selected_device(
                        api.clone(),
                        codec,
                        extra,
                        max_pixels,
                        backend,
                        adapter,
                    ) {
                        Ok(decoder) => {
                            report.selected = Some(SelectedDevice {
                                backend,
                                adapter: adapter.clone(),
                            });
                            report.attempts.push(attempt(backend, adapter, None));
                            report.elapsed_ms = start.elapsed().as_millis();
                            return Ok(Selection { decoder, report });
                        }
                        Err(error) => Some(error),
                    }
                };
                report.attempts.push(attempt(backend, adapter, failure));
            }
        }
        Ok(fallback(
            cpu,
            report,
            SelectionFailure::NoCompatibleHardware,
            start,
        ))
    }
}

fn create_selected_device(
    api: Arc<FfmpegApi>,
    codec: VideoCodec,
    extra: &[u8],
    max_pixels: u64,
    backend: Backend,
    adapter: &Adapter,
) -> std::result::Result<CpuDecoder, SelectionFailure> {
    // 保留设备初始化与身份校验原因；上下文获取独立引用后设备守卫才释放。
    let device = hardware::Device::create(api.clone(), backend, adapter)
        .map_err(SelectionFailure::Inventory)?;
    CpuDecoder::create_inner(
        api,
        codec,
        extra,
        max_pixels,
        Some(HardwareInput::Existing(backend, device.pointer)),
    )
    .map_err(SelectionFailure::Decoder)
}

fn attempt(
    backend: Backend,
    adapter: &Adapter,
    error: Option<SelectionFailure>,
) -> SelectionAttempt {
    SelectionAttempt {
        backend,
        adapter_index: adapter.index,
        adapter_luid: adapter.luid,
        error,
    }
}
fn fallback(
    decoder: CpuDecoder,
    mut report: SelectionReport,
    reason: SelectionFailure,
    start: Instant,
) -> Selection {
    report.fallback = Some(reason);
    report.elapsed_ms = start.elapsed().as_millis();
    Selection { decoder, report }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "需要显式FFmpeg运行库；模拟没有适配器"]
    fn no_adapter_retains_real_library_capabilities() {
        let path = std::env::var_os("AM_SDK_FFMPEG_DIR").unwrap();
        let api = Arc::new(FfmpegApi::load_from(std::path::Path::new(&path)).unwrap());
        let selection = CpuDecoder::create_with_adapter_source(
            api,
            VideoCodec::H264,
            &[],
            1920 * 1080,
            HardwarePolicy::PreferHardware,
            Duration::from_secs(5),
            || Ok(Vec::new()),
        )
        .unwrap();
        assert!(selection.report.attempts.is_empty());
        assert!(matches!(
            selection.report.fallback,
            Some(SelectionFailure::NoCompatibleHardware)
        ));
        assert!(selection
            .report
            .backend_support
            .iter()
            .all(|support| support.compiled.is_some() && support.codec_configured.is_some()));
        assert!(selection.report.selected.is_none());
    }

    #[test]
    #[ignore = "需要显式FFmpeg运行库与本机D3D11适配器；只使用合成数据"]
    fn adapter_identity_failure_retains_the_inventory_reason() {
        let path = std::env::var_os("AM_SDK_FFMPEG_DIR").unwrap();
        let api = Arc::new(FfmpegApi::load_from(std::path::Path::new(&path)).unwrap());
        let mut adapter = hardware::adapters()
            .unwrap()
            .into_iter()
            .find(|adapter| !adapter.software)
            .expect("需要可用的真实适配器");
        let valid = create_selected_device(
            api.clone(),
            VideoCodec::H264,
            &[],
            1920 * 1080,
            Backend::D3d11va,
            &adapter,
        );
        assert!(valid.is_ok());
        drop(valid);
        adapter.luid ^= 1;
        let mismatched = create_selected_device(
            api,
            VideoCodec::H264,
            &[],
            1920 * 1080,
            Backend::D3d11va,
            &adapter,
        );
        assert!(matches!(
            mismatched,
            Err(SelectionFailure::Inventory(
                ProbeError::AdapterIdentityMismatch
            ))
        ));
    }

    fn intel_adapter() -> Adapter {
        hardware::adapters()
            .expect("枚举适配器")
            .into_iter()
            .find(|adapter| !adapter.software && adapter.vendor_id == 0x8086)
            .expect("本机需要Intel核显（vendor 0x8086）")
    }

    fn intel_forced_leg(
        api: &Arc<FfmpegApi>,
        codec: VideoCodec,
        sample: &std::path::Path,
        max_pixels: u64,
    ) {
        let intel = intel_adapter();
        let selection = CpuDecoder::create_with_adapter_source(
            api.clone(),
            codec,
            &[],
            max_pixels,
            HardwarePolicy::PreferHardware,
            Duration::from_secs(5),
            || Ok(vec![intel.clone()]),
        )
        .expect("Intel 注入适配器源注入选择应成功");
        let selected = selection
            .report
            .selected
            .as_ref()
            .expect("Intel 适配器硬解初始化应成功");
        assert_eq!(selected.backend, Backend::D3d11va);
        assert_eq!(selected.adapter.vendor_id, 0x8086);
        assert_eq!(selected.adapter.luid, intel.luid);
        let mut decoder = selection.decoder;
        let data = std::fs::read(sample).expect("读取样本");
        decoder.send(&data, 90000, 90000).expect("送入码流");
        if decoder.peek().unwrap().is_none() {
            decoder.drain().unwrap();
        }
        let info = decoder.peek().unwrap().unwrap();
        let backend = decoder.hardware_backend().expect("实际后端已确定");
        assert_eq!(backend, Backend::D3d11va);
        let gpu = decoder.gpu_frame_info(info).expect("GPU 帧元数据提取");
        assert!(!decoder.gpu_texture().expect("纹理指针").is_null());
        println!(
            "m3.1 intel {codec:?} {}x{} -> backend={backend:?} fmt={} subresource={} handle={} select_ms={} attempts={}",
            gpu.info.width,
            gpu.info.height,
            gpu.d3d_format,
            gpu.subresource,
            gpu.shared_nt_handle,
            selection.report.elapsed_ms,
            selection.report.attempts.len(),
        );
    }

    #[test]
    #[ignore = "需要AM_SDK_FFMPEG_DIR、AM_SDK_CODEC_SAMPLES与本机Intel核显（vendor 0x8086）"]
    fn intel_igpu_d3d11va_decode_and_fallback_measured() {
        let runtime = std::env::var_os("AM_SDK_FFMPEG_DIR").expect("缺少隔离运行库目录");
        let samples = std::path::PathBuf::from(
            std::env::var_os("AM_SDK_CODEC_SAMPLES").expect("缺少合成样本目录"),
        );
        let api = Arc::new(
            FfmpegApi::load_from(std::path::Path::new(&runtime)).expect("加载隔离FFmpeg运行库"),
        );
        let intel = intel_adapter();
        println!(
            "m3.1 intel adapter: index={} name={} luid={:#x} dedicated_video_bytes={}",
            intel.index, intel.name, intel.luid, intel.dedicated_video_bytes
        );
        for adapter in hardware::adapters().expect("枚举适配器") {
            println!(
                "m3.1 inventory: index={} vendor={:#06x} name={} software={} video_bytes={}",
                adapter.index,
                adapter.vendor_id,
                adapter.name,
                adapter.software,
                adapter.dedicated_video_bytes
            );
        }

        let auto = CpuDecoder::create_with_policy(
            api.clone(),
            VideoCodec::H264,
            &[],
            512 * 288,
            HardwarePolicy::PreferHardware,
            Duration::from_secs(5),
        )
        .expect("默认策略选择");
        let picked = auto
            .report
            .selected
            .as_ref()
            .expect("本机存在可用硬解适配器");
        println!(
            "m3.1 default policy picks: index={} vendor={:#06x} name={} select_ms={} attempts={}",
            picked.adapter.index,
            picked.adapter.vendor_id,
            picked.adapter.name,
            auto.report.elapsed_ms,
            auto.report.attempts.len()
        );

        intel_forced_leg(&api, VideoCodec::H264, &samples.join("one.h264"), 512 * 288);
        intel_forced_leg(
            &api,
            VideoCodec::H264,
            &samples.join("blue-1080.h264"),
            1920 * 1088,
        );
        intel_forced_leg(
            &api,
            VideoCodec::H265,
            &samples.join("one.hevc"),
            1920 * 1088,
        );

        let ghost = Adapter {
            index: hardware::MAX_ADAPTERS,
            name: String::from("intel-ghost"),
            vendor_id: 0x8086,
            device_id: 0,
            luid: intel.luid ^ 1,
            software: false,
            dedicated_video_bytes: 0,
            dxva2_index: None,
            dxva2_mapping_error: None,
        };
        let ghost_selection = CpuDecoder::create_with_adapter_source(
            api,
            VideoCodec::H264,
            &[],
            512 * 288,
            HardwarePolicy::PreferHardware,
            Duration::from_secs(5),
            || Ok(vec![ghost]),
        )
        .expect("故障注入仍应返回CPU解码器");
        assert!(ghost_selection.report.selected.is_none());
        assert!(matches!(
            ghost_selection.report.fallback,
            Some(SelectionFailure::NoCompatibleHardware)
        ));
        assert!(ghost_selection
            .report
            .attempts
            .iter()
            .all(|attempt| attempt.error.is_some()));
        let mut fallback_decoder = ghost_selection.decoder;
        let data = std::fs::read(samples.join("one.h264")).expect("读取样本");
        fallback_decoder
            .send(&data, 90000, 90000)
            .expect("送入码流");
        if fallback_decoder.peek().unwrap().is_none() {
            fallback_decoder.drain().unwrap();
        }
        let info = fallback_decoder.peek().unwrap().unwrap();
        assert!(
            fallback_decoder.hardware_backend().is_none(),
            "故障注入路径应回软解"
        );
        assert!(info.width > 0);
        println!(
            "m3.1 ghost adapter (index={}) -> fallback=NoCompatibleHardware attempts={} errors={} decoded_cpu={}x{}",
            hardware::MAX_ADAPTERS,
            ghost_selection.report.attempts.len(),
            ghost_selection
                .report
                .attempts
                .iter()
                .map(|a| a
                    .error
                    .as_ref()
                    .map_or_else(String::new, |e| format!("{e:?}")))
                .collect::<Vec<_>>()
                .join(" | "),
            info.width,
            info.height,
        );
    }
}
