//! 显式原生硬件探测；设备初始化和真实码流验证分开，不在状态查询中启动驱动。
use super::{ffmpeg_lib::FfmpegApi, ffmpeg_sys::*};
use serde::Serialize;
use std::{
    ffi::CString,
    ptr,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};
mod windows;

pub const MAX_ADAPTERS: u32 = 8;
static PROBE: Mutex<()> = Mutex::new(());
static INVENTORY: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    D3d11va,
    Dxva2,
}
impl Backend {
    pub(crate) fn device_type(self) -> i32 {
        match self {
            Self::D3d11va => AV_HWDEVICE_TYPE_D3D11VA,
            Self::Dxva2 => AV_HWDEVICE_TYPE_DXVA2,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Adapter {
    pub index: u32,
    pub name: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub luid: u64,
    pub software: bool,
    pub dedicated_video_bytes: u64,
    pub dxva2_index: Option<u32>,
    pub dxva2_mapping_error: Option<ProbeError>,
}

#[derive(Debug, Clone, Serialize, thiserror::Error)]
pub enum ProbeError {
    #[error("hardware.platformUnsupported")]
    UnsupportedPlatform,
    #[error("hardware.probeBusy")]
    Busy,
    #[error("hardware.invalidBudget")]
    InvalidBudget,
    #[error("hardware.probeBudgetExhausted")]
    BudgetExhausted,
    #[error("hardware.adapterQueryFailed: {0}")]
    AdapterQuery(i32),
    #[error("hardware.deviceInitFailed: {0}")]
    DeviceInit(i32),
    #[error("hardware.backendNotCompiled")]
    NotCompiled,
    #[error("hardware.systemDependencyMissing: {0}")]
    DependencyMissing(&'static str),
    #[error("hardware.adapterMappingUnavailable")]
    AdapterMappingUnavailable,
    #[error("hardware.adapterIdentityMismatch")]
    AdapterIdentityMismatch,
}

#[derive(Debug, Serialize)]
pub struct Attempt {
    pub requested_adapter_index: u32,
    pub backend: Backend,
    pub compiled: bool,
    pub device_initialized: bool,
    pub stream_verified: bool,
    pub error: Option<ProbeError>,
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub adapters: Vec<Adapter>,
    pub attempts: Vec<Attempt>,
    pub elapsed_ms: u128,
    pub stopped: Option<ProbeError>,
    pub adapter_limit_reached: bool,
}

/// 枚举实际适配器；索引只在本次拓扑快照中有效，不以显卡名称推导可解码格式。
pub fn adapters() -> Result<Vec<Adapter>, ProbeError> {
    let _guard = INVENTORY.try_lock().map_err(|_| ProbeError::Busy)?;
    windows::adapters()
}

#[cfg(any(all(windows, target_arch = "x86_64"), test))]
fn assign_dxva2_index(adapters: &mut [Adapter], luid: u64, index: u32) {
    if let Some(adapter) = adapters.iter_mut().find(|a| a.luid == luid) {
        adapter.dxva2_index = Some(index);
    }
}

pub(crate) fn enter_probe(budget: Duration) -> Result<MutexGuard<'static, ()>, ProbeError> {
    if budget.is_zero() || budget > Duration::from_secs(5) {
        return Err(ProbeError::InvalidBudget);
    }
    PROBE.try_lock().map_err(|_| ProbeError::Busy)
}

pub(crate) struct Device {
    pub pointer: *mut AVBufferRef,
    api: Arc<FfmpegApi>,
}
impl Device {
    pub(crate) fn create(
        api: Arc<FfmpegApi>,
        backend: Backend,
        adapter: &Adapter,
    ) -> Result<Self, ProbeError> {
        // DXVA2使用D3D9序号，不能假定与DXGI序号相同；映射须由LUID实证。
        let index = match backend {
            Backend::D3d11va => adapter.index,
            Backend::Dxva2 => adapter
                .dxva2_index
                .ok_or(ProbeError::AdapterMappingUnavailable)?,
        };
        let selector = CString::new(index.to_string()).map_err(|_| ProbeError::AdapterQuery(-1))?;
        let mut pointer = ptr::null_mut();
        let rc = unsafe {
            (api.av_hwdevice_ctx_create)(
                &mut pointer,
                backend.device_type(),
                selector.as_ptr(),
                ptr::null_mut(),
                0,
            )
        };
        // 即使原生失败时留下引用，也由同一运行库归还。
        let device = Self { pointer, api };
        if rc < 0 || device.pointer.is_null() {
            return Err(ProbeError::DeviceInit(if rc < 0 { rc } else { -12 }));
        }
        if backend == Backend::D3d11va {
            // FFmpeg可能在拓扑变化后回退默认设备，不能把请求序号冒充实际适配器。
            let actual = unsafe { windows::d3d11_luid(device.pointer) }?;
            if actual != adapter.luid {
                return Err(ProbeError::AdapterIdentityMismatch);
            }
        }
        Ok(device)
    }
}
impl Drop for Device {
    fn drop(&mut self) {
        if !self.pointer.is_null() {
            unsafe { (self.api.av_buffer_unref)(&mut self.pointer) };
        }
    }
}

pub(crate) fn compiled(api: &FfmpegApi, backend: Backend) -> bool {
    let mut previous = AV_HWDEVICE_TYPE_NONE;
    for _ in 0..64 {
        previous = unsafe { (api.av_hwdevice_iterate_types)(previous) };
        if previous == backend.device_type() {
            return true;
        }
        if previous == AV_HWDEVICE_TYPE_NONE {
            break;
        }
    }
    false
}

/// 最多一轮探测同时运行；软预算在调用之间检查，不能中止已进入驱动的原生调用。
pub fn probe(api: Arc<FfmpegApi>, budget: Duration) -> Result<Report, ProbeError> {
    let _guard = enter_probe(budget)?;
    let start = Instant::now();
    let adapters = adapters()?;
    let mut attempts = Vec::with_capacity(MAX_ADAPTERS as usize * 2);
    let mut stopped = None;
    'methods: for backend in [Backend::D3d11va, Backend::Dxva2] {
        let compiled = compiled(&api, backend);
        for adapter in adapters.iter().filter(|a| !a.software) {
            if start.elapsed() >= budget {
                stopped = Some(ProbeError::BudgetExhausted);
                break 'methods;
            }
            let result = if compiled {
                Device::create(api.clone(), backend, adapter).map(drop)
            } else {
                Err(ProbeError::NotCompiled)
            };
            attempts.push(Attempt {
                requested_adapter_index: adapter.index,
                backend,
                compiled,
                device_initialized: result.is_ok(),
                stream_verified: false,
                error: result.err(),
            });
        }
    }
    if stopped.is_none() && start.elapsed() >= budget {
        stopped = Some(ProbeError::BudgetExhausted);
    }
    let adapter_limit_reached = adapters.len() == MAX_ADAPTERS as usize;
    Ok(Report {
        adapters,
        attempts,
        elapsed_ms: start.elapsed().as_millis(),
        stopped,
        adapter_limit_reached,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_description_layout_matches_dxgi_header() {
        #[cfg(all(windows, target_arch = "x86_64"))]
        windows::verify_layout();
    }
    #[test]
    fn probe_admission_is_nonblocking() {
        let _guard = enter_probe(Duration::from_secs(1)).unwrap();
        assert!(matches!(
            enter_probe(Duration::from_secs(1)),
            Err(ProbeError::Busy)
        ));
        assert!(matches!(
            enter_probe(Duration::ZERO),
            Err(ProbeError::InvalidBudget)
        ));
        assert!(matches!(
            enter_probe(Duration::from_secs(6)),
            Err(ProbeError::InvalidBudget)
        ));
    }

    #[test]
    fn dxva2_mapping_uses_luid_instead_of_dxgi_ordinal() {
        let mut adapters = vec![Adapter {
            index: 1,
            name: String::from("fixture"),
            vendor_id: 1,
            device_id: 2,
            luid: 42,
            software: false,
            dedicated_video_bytes: 0,
            dxva2_index: None,
            dxva2_mapping_error: None,
        }];
        assign_dxva2_index(&mut adapters, 99, 1);
        assert!(adapters[0].dxva2_index.is_none());
        assign_dxva2_index(&mut adapters, 42, 0);
        assert_eq!(adapters[0].index, 1);
        assert_eq!(adapters[0].dxva2_index, Some(0));
    }
}
