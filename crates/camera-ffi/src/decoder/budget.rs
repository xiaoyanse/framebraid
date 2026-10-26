//! 回读暂存先占资源域字节，copy的所有退出路径释放原生CPU缓冲后才能归还许可。
use super::*;
use am_camera_core::media::codec::DecodedRgba;
pub(crate) struct BudgetedDecoder {
    // 字段按声明顺序析构：原生CPU缓存先释放，随后才归还资源域许可。
    raw: CpuDecoder,
    reservation: Option<Charge>,
    bytes: usize,
}
impl From<CpuDecoder> for BudgetedDecoder {
    fn from(raw: CpuDecoder) -> Self {
        Self {
            raw,
            reservation: None,
            bytes: 0,
        }
    }
}
impl std::ops::Deref for BudgetedDecoder {
    type Target = CpuDecoder;
    fn deref(&self) -> &CpuDecoder {
        &self.raw
    }
}
impl std::ops::DerefMut for BudgetedDecoder {
    fn deref_mut(&mut self) -> &mut CpuDecoder {
        &mut self.raw
    }
}
impl BudgetedDecoder {
    fn clear_cache(&mut self) {
        self.raw.release_readback_cache();
        self.reservation.take();
        self.bytes = 0;
    }
    pub(crate) fn discard_frame(&mut self) -> std::result::Result<(), DecodeError> {
        let result = self.raw.discard_frame();
        self.clear_cache();
        result
    }
}

pub(crate) fn copy_pixels(
    state: &Arc<CameraState>,
    decoder: &mut BudgetedDecoder,
    output_bytes: usize,
    format: u32,
) -> Result<Option<DecodedRgba>> {
    let bytes = match decoder.hardware_readback_bytes() {
        Ok(bytes) => bytes,
        Err(error) => {
            decoder.clear_cache();
            return Err(convert(error));
        }
    };
    if bytes != decoder.bytes {
        decoder.clear_cache();
        if bytes > 0 {
            decoder.reservation = Some(state.charge(bytes as u64, false)?);
        }
        decoder.bytes = bytes;
    }
    let result = decoder
        .copy_pixels_reusing_readback(output_bytes, format, bytes)
        .map_err(convert);
    if result.is_err() {
        decoder.clear_cache();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use am_camera_core::media::codec::selection::HardwarePolicy;

    #[test]
    #[ignore = "显式设置AM_RT_FFMPEG_DIR、AM_SDK_H264_SAMPLE和AM_SDK_H264_ALTERNATE，在原生硬解本机运行"]
    fn readback_domain_budget_rejects_then_recovers_same_hardware_frame() {
        let runtime_dir = std::env::var("AM_RT_FFMPEG_DIR").unwrap();
        let sample = std::fs::read(std::env::var("AM_SDK_H264_SAMPLE").unwrap()).unwrap();
        let alternate = std::fs::read(std::env::var("AM_SDK_H264_ALTERNATE").unwrap()).unwrap();
        let path = std::ffi::CString::new(runtime_dir).unwrap();
        let paths = [path.as_ptr()];
        let params = amc_init_params_t {
            abi_version: AM_ABI_VERSION,
            runtime_dirs_utf8: paths.as_ptr(),
            runtime_dir_count: 1,
            ..Default::default()
        };
        let mut ctx = std::ptr::null_mut();
        assert_eq!(unsafe { crate::amc_init(&params, &mut ctx) }, 0);
        let state = camera_state(ctx as usize).unwrap();
        let selected = CpuDecoder::create_with_policy(
            runtime(&state).unwrap(),
            VideoCodec::H264,
            &[],
            1920 * 1080,
            HardwarePolicy::PreferHardware,
            Duration::from_secs(5),
        )
        .unwrap();
        assert!(
            selected.report.selected.is_some(),
            "本测试需要真实硬件，不将CPU当硬件验收"
        );
        let mut cpu: BudgetedDecoder = selected.decoder.into();
        assert!(cpu.send(&sample, 37, 37).unwrap());
        let frame = cpu.peek().unwrap().unwrap();
        assert!(cpu.hardware_backend().is_some());
        let required = cpu.hardware_readback_bytes().unwrap() as u64;
        assert!(required > 0);
        let occupied = state
            .charge(
                state.limits().unwrap().frame_budget_bytes - required + 1,
                false,
            )
            .unwrap();
        let before = state.usage_snapshot().unwrap();
        assert!(matches!(
            copy_pixels(&state, &mut cpu, frame.rgba_bytes, 0),
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
        ));
        assert_eq!(state.usage_snapshot().unwrap(), before);
        assert_eq!(cpu.peek().unwrap().unwrap().pts, 37);
        drop(occupied);
        let output = copy_pixels(&state, &mut cpu, frame.rgba_bytes, 0)
            .unwrap()
            .unwrap();
        assert_eq!((output.info.pts, output.data.len()), (37, frame.rgba_bytes));
        assert_eq!(state.usage_snapshot().unwrap(), (required, 0, 0));
        let original = output.data.clone();
        assert!(cpu.send(&alternate, 38, 38).unwrap());
        let second = copy_pixels(&state, &mut cpu, frame.rgba_bytes, 0)
            .unwrap()
            .unwrap();
        assert_eq!(second.info.pts, 38);
        assert_ne!(
            second.data, original,
            "验证必须使用不同画面，不能用相同像素掩盖覆写"
        );
        assert_eq!(output.data, original, "缓存复用不得覆写已输出Vec");
        assert_eq!(state.usage_snapshot().unwrap(), (required, 0, 0));
        // 已输出完当前帧也要能主动清缓存，不能先归还许可却留下缓存。
        cpu.discard_frame().unwrap();
        assert_eq!(state.usage_snapshot().unwrap(), (0, 0, 0));
        drop((output, cpu));
        assert_eq!(state.usage_snapshot().unwrap(), (0, 0, 0));
        drop(state);
        let mut lifecycle = 0;
        assert_eq!(unsafe { crate::amc_shutdown(ctx, 100, &mut lifecycle) }, 0);
        assert_eq!(crate::amc_object_destroy(ctx.cast()), 0);
    }
}
