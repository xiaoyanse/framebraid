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
pub struct SelectionReport {
    pub policy: HardwarePolicy,
    pub selected: Option<SelectedDevice>,
    pub attempts: Vec<SelectionAttempt>,
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
        let start = Instant::now();
        // 先校验共同参数并准备CPU后备；尚未送包，不在此处分配解码参考帧。
        let cpu = Self::create(api.clone(), codec, extra, max_pixels)?;
        let mut report = SelectionReport {
            policy,
            selected: None,
            attempts: Vec::new(),
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
        let adapters = match hardware::adapters() {
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
        let codec_ptr = unsafe { (api.avcodec_find_decoder_by_name)(codec.name().as_ptr()) };
        for backend in [Backend::D3d11va, Backend::Dxva2] {
            let compiled = hardware::compiled(&api, backend);
            let configured = unsafe { Binding::format(&api, codec_ptr, backend) }.is_some();
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
}
