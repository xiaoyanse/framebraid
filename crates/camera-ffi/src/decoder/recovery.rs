//! 运行期恢复只切一次CPU；等待真实随机访问点，启动单画面例外有界保留。
use super::*;
use am_camera_core::media::hardware::Backend;
impl Inner {
    pub(super) fn hardware_active(&self) -> bool {
        self.selection.selected.is_some() && !self.hardware_failed
    }
    pub(super) fn update_actual(&self, decoder: &Decoder) -> Result<()> {
        let mut value = decoder
            .codec_info
            .lock()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        let actual = self.cpu.hardware_backend();
        value.executed = true;
        value.failed = false;
        value.hw_accelerated = u32::from(actual.is_some());
        value.backend_name_utf8 = match actual {
            Some(Backend::D3d11va) => c"ffmpeg-d3d11va".as_ptr(),
            Some(Backend::Dxva2) => c"ffmpeg-dxva2".as_ptr(),
            None => c"ffmpeg-cpu".as_ptr(),
        };
        Ok(())
    }
    pub(super) fn recover(
        &mut self,
        decoder: &Decoder,
        dimensions: Option<(u32, u32)>,
        cause: DecodeError,
    ) -> Result<()> {
        if !self.hardware_active() {
            return Err(AM_ERR_CAMERA_CODEC_UNSUPPORTED);
        }
        {
            let mut snapshot = decoder
                .codec_info
                .lock()
                .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
            snapshot.last_failure = Some(cause);
            snapshot.failed = true;
        }
        let generation = decoder
            .generation
            .load(Ordering::Acquire)
            .checked_add(1)
            .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        let config_generation = self.config_generation_checked(decoder)?;
        let (width, height) = dimensions
            .or_else(|| self.cpu.dimensions_hint())
            .ok_or(AM_ERR_CAMERA_CODEC_UNSUPPORTED)?;
        let output =
            super::super::frames::scale::bytes(width, height, self.config.abi.output_pixel_format)?;
        let scratch = if self.config.abi.output_pixel_format == AM_PF_NV12 as u32 {
            width as u64 * height as u64 * 4
        } else {
            0
        };
        // 已有回读缓存先出账；输出预算不足时保留原硬件帧，供明确重试。
        self.cpu.clear_cache();
        let reservation = decoder.ctx.charge(output as u64 + scratch, false)?;
        let replacement = self
            .cpu
            .software_reconfigure(
                self.config.codec,
                &self.gate.extra(),
                (decoder.ctx.limits()?.frame_budget_bytes / 4)
                    .min(SDK_MAX_DYNAMIC_AXIS * SDK_MAX_DYNAMIC_AXIS),
            )
            .map_err(convert)?;
        let mut value = decoder
            .codec_info
            .lock()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        let replayable = decoder.sequence.load(Ordering::Acquire) == 0
            && self.timings.len() == 1
            && self.startup.as_ref().is_some_and(|packet| {
                self.timings
                    .front()
                    .is_some_and(|timing| timing.tag == packet.tag && !timing.delivered)
            });
        self.replay = if replayable {
            self.startup.take()
        } else {
            None
        };
        self.startup.take();
        self.cpu = replacement.into();
        self.recovery_charge = Some(reservation);
        self.hardware_failed = true;
        self.timings.clear();
        self.gate.restart();
        self.flushed = false;
        decoder.generation.store(generation, Ordering::Release);
        decoder
            .config_generation
            .store(config_generation, Ordering::Release);
        value.abi = info(&self.config, config_generation, false);
        value.recovered = true;
        value.executed = false;
        value.failed = false;
        self.metrics.resyncs.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }
    pub(super) fn output_charges(
        &mut self,
        decoder: &Decoder,
        bytes: u64,
        scratch: u64,
    ) -> Result<(Charge, Option<Charge>)> {
        let required = bytes
            .checked_add(scratch)
            .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        if self
            .recovery_charge
            .as_ref()
            .is_some_and(|charge| charge.bytes() < required)
        {
            self.recovery_charge.take();
        }
        if let Some(reservation) = &mut self.recovery_charge {
            let external = reservation.take_external(bytes)?;
            let remainder = self.recovery_charge.take();
            return Ok((
                external,
                if scratch > 0 {
                    remainder
                } else {
                    drop(remainder);
                    None
                },
            ));
        }
        let external = decoder.ctx.charge(bytes, true)?;
        let internal = if scratch > 0 {
            Some(decoder.ctx.charge(scratch, false)?)
        } else {
            None
        };
        Ok((external, internal))
    }
}
pub(super) fn record_fallback(key: usize) {
    // 成功调用保留本轮硬件失败；等待RAP返回TRY_AGAIN时也保留对应解释。
    let detail = am_sdk_support::diagnostics::Detail {
            message: c"Native hardware decode failed; admitted CPU recovery and waiting for a random-access frame. Reset explicitly to retry hardware.",
            source: c"camera.decoder.hardwareFallback",
            fault_class: AM_FAULT_RESOURCE as u32,
        };
    domain().record_error_detail(key, AM_ERR_CAMERA_CODEC_UNSUPPORTED, Some(detail));
    am_sdk_support::diagnostics::record(domain(), key, AM_ERR_SDK_TRY_AGAIN, detail);
}
