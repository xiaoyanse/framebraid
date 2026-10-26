//! 参数变化只重建codec上下文；旧设备与decoder额度继续持有。
use super::*;

impl NativeDecoder {
    pub(in crate::subscriptions::hub) fn reconfigure(
        &mut self,
        source: &Arc<SharedSource>,
        codec: VideoCodec,
        info: &StreamInfo,
        extra: &[u8],
    ) -> Result<()> {
        let generation = lock(&source.decode)?
            .generation
            .checked_add(1)
            .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        let max_pixels = (source.state.limits()?.frame_budget_bytes / 4).min(8192 * 8192);
        let raw = self
            .raw
            .reconfigure(codec, extra, max_pixels)
            .map_err(config::convert)?;
        let reservation = if self.hardware {
            None
        } else {
            let dimensions = raw
                .dimensions_hint()
                .or_else(|| {
                    (info.width > 0 && info.height > 0).then_some((info.width, info.height))
                })
                .ok_or(AM_ERR_CAMERA_CODEC_UNSUPPORTED)?;
            let bytes =
                crate::frames::scale::bytes(dimensions.0, dimensions.1, AM_PF_RGBA8 as u32)?;
            Some(source.state.charge(bytes as u64, false)?)
        };
        // 原生构造/析构不持状态锁；所有构造失败均保留旧上下文。
        self.raw = raw.into();
        self._cpu_output = reservation;
        let mut state = lock(&source.decode)?;
        state.key = Some(configuration_key(info, extra));
        state.generation = generation;
        state.actual = None;
        state.active = true;
        state.phase = "initialized";
        state.last_status = None;
        state.retry_after = None;
        Ok(())
    }
}

impl Hub {
    pub(in crate::subscriptions::hub) fn reconfigure_pixels(
        &self,
        source: &Arc<SharedSource>,
        decoder: &mut Option<NativeDecoder>,
        codec: VideoCodec,
        info: &StreamInfo,
        extra: &[u8],
        random: bool,
    ) -> std::result::Result<bool, IoError> {
        if !random {
            return Ok(false);
        }
        let Some(native) = decoder.as_mut() else {
            return Ok(true);
        };
        match native.reconfigure(source, codec, info, extra) {
            Ok(()) => Ok(true),
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED) => {
                let mut state = lock(&source.decode).map_err(|_| IoError::Resource)?;
                state.last_status = Some(AM_ERR_SDK_RESOURCE_EXHAUSTED);
                state.actual = None;
                state.active = false;
                state.phase = "waiting_resource";
                drop(state);
                source.report(
                    AM_SRC_LIVE as u32,
                    AM_FAULT_RESOURCE as u32,
                    c"camera.pixelOutputResourceWait",
                );
                Ok(false)
            }
            Err(_) => {
                self.pixel_failure(
                    source,
                    decoder,
                    &mut VecDeque::new(),
                    DecodeError::InvalidInput,
                )?;
                // 不在同一个随机访问包上重放CPU，沿已有恢复流程等待下一包。
                Ok(false)
            }
        }
    }
}

#[cfg(test)]
mod tests;
