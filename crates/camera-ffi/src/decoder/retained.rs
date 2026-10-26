//! 独立解码器的配置/参数集持有；所有SDK字节先准入再复制。
use super::*;
use std::ops::Deref;

pub(super) struct OwnedConfig {
    config: Config,
    _charge: Charge,
}
// ABI借用指针已清除；构造后只经不可变引用读取配置和Vec。
unsafe impl Sync for OwnedConfig {}
impl Deref for OwnedConfig {
    type Target = Config;
    fn deref(&self) -> &Config {
        &self.config
    }
}
impl OwnedConfig {
    pub(super) unsafe fn read(
        state: &Arc<CameraState>,
        pointer: *const am_decoder_config_t,
    ) -> Result<Arc<Self>> {
        let mut charge = None;
        let config = unsafe {
            Config::read(pointer, |bytes| {
                charge = Some(state.charge(bytes as u64, false)?);
                Ok(())
            })
        }?;
        let charge = charge.ok_or(AM_ERR_SDK_INTERNAL_ERROR)?;
        if config.extra.capacity() as u64 > charge.bytes() {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        Ok(Arc::new(Self {
            config,
            _charge: charge,
        }))
    }
}

pub(super) struct BudgetedGate {
    gate: Gate,
    state: Arc<CameraState>,
    _charge: Charge,
}
impl Deref for BudgetedGate {
    type Target = Gate;
    fn deref(&self) -> &Gate {
        &self.gate
    }
}
impl BudgetedGate {
    pub(super) fn restart(&mut self) {
        self.gate.restart();
    }
    pub(super) fn mark_started(&mut self) {
        self.gate.started = true;
    }
    pub(super) fn clear_dirty(&mut self) {
        self.gate.dirty = false;
    }
    pub(super) fn new(state: &Arc<CameraState>, config: &Config) -> Result<Self> {
        let mut value = Self {
            gate: Gate::default(),
            state: state.clone(),
            _charge: state.charge(0, false)?,
        };
        if !config.extra.is_empty() {
            value.observe(config.codec, &config.extra)?;
        }
        Ok(value)
    }
    pub(super) fn observe(&mut self, codec: VideoCodec, bytes: &[u8]) -> Result<bool> {
        let mut replacement = None;
        let allowed = self.gate.observe_reserved(codec, bytes, |bytes| {
            replacement = Some(self.state.charge(bytes as u64, false)?);
            Ok(())
        })?;
        if let Some(charge) = replacement {
            self._charge = charge;
        }
        Ok(allowed)
    }
    pub(super) fn extra(&self) -> Result<Extra> {
        let charge = self.state.charge(self.gate.extra_bytes() as u64, false)?;
        let bytes = self.gate.try_extra()?;
        if bytes.capacity() as u64 > charge.bytes() {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        Ok(Extra {
            bytes,
            _charge: charge,
        })
    }
}
pub(super) struct Extra {
    bytes: Vec<u8>,
    _charge: Charge,
}
impl Deref for Extra {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.bytes
    }
}

#[cfg(test)]
mod tests;
