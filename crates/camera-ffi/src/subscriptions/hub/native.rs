//! 像素解码的故障不结束共享取流；压缩消费者保持独立。
use super::*;
use crate::decoder::budget::BudgetedDecoder;
use am_camera_core::media::codec::{selection::HardwarePolicy, DecodeError};
use std::hash::{Hash, Hasher};
mod reconfigure;

fn configuration_key(info: &StreamInfo, extra: &[u8]) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    (info.codec, info.width, info.height, extra).hash(&mut hash);
    hash.finish()
}

pub(super) struct NativeDecoder {
    pub raw: BudgetedDecoder,
    _permit: Arc<crate::frames::DecoderPermit>,
    _cpu_output: Option<Charge>,
    hardware: bool,
    source: Weak<SharedSource>,
}
pub(super) struct Activity(pub Weak<SharedSource>);
impl Drop for Activity {
    fn drop(&mut self) {
        if let Some(source) = self.0.upgrade() {
            if let Ok(mut state) = source.decode.lock() {
                state.idle();
            }
        }
    }
}
impl Drop for NativeDecoder {
    fn drop(&mut self) {
        if let Some(source) = self.source.upgrade() {
            if let Ok(mut state) = source.decode.lock() {
                state.idle();
            }
        }
    }
}
impl NativeDecoder {
    #[cfg(test)]
    pub(super) fn create(
        source: &Arc<SharedSource>,
        codec: VideoCodec,
        info: &StreamInfo,
        extra: &[u8],
        now: Instant,
    ) -> Result<Option<Self>> {
        Self::create_with_permit(
            source,
            codec,
            info,
            extra,
            now,
            Arc::new(source.state.decoder()?),
        )
    }
    pub(in crate::subscriptions::hub) fn create_with_permit(
        source: &Arc<SharedSource>,
        codec: VideoCodec,
        info: &StreamInfo,
        extra: &[u8],
        now: Instant,
        permit: Arc<crate::frames::DecoderPermit>,
    ) -> Result<Option<Self>> {
        let key = configuration_key(info, extra);
        let policy = {
            let mut state = lock(&source.decode)?;
            let Some(policy) = state.prepare(key, now) else {
                return Ok(None);
            };
            // 新报告分配前释放旧快照额度；不在驱动调用期间持状态锁。
            if policy == state.policy {
                state.selection.take();
                state.report_charge.take();
                state.recovered = false;
            }
            policy
        };
        let report_charge = if lock(&source.decode)?.selection.is_none() {
            Some(source.state.control(8192, false)?)
        } else {
            None
        };
        let selection = CpuDecoder::create_with_policy(
            config::runtime(&source.state)?,
            codec,
            extra,
            (source.state.limits()?.frame_budget_bytes / 4).min(8192 * 8192),
            if policy == AM_DHW_SOFTWARE_ONLY as u32 {
                HardwarePolicy::SoftwareOnly
            } else {
                HardwarePolicy::PreferHardware
            },
            Duration::from_millis(1500),
        )
        .map_err(|error| {
            if let Ok(mut state) = source.decode.lock() {
                state.failure = Some(error);
            }
            config::convert(error)
        })?;
        let hardware = selection.report.selected.is_some();
        let reservation = if hardware {
            None
        } else {
            let dimensions = selection
                .decoder
                .dimensions_hint()
                .or_else(|| {
                    (info.width > 0 && info.height > 0).then_some((info.width, info.height))
                })
                .ok_or(AM_ERR_CAMERA_CODEC_UNSUPPORTED)?;
            let bytes =
                crate::frames::scale::bytes(dimensions.0, dimensions.1, AM_PF_RGBA8 as u32)?;
            Some(source.state.charge(bytes as u64, false)?)
        };
        {
            let mut state = lock(&source.decode)?;
            if let Some(charge) = report_charge {
                state.selection = Some(Arc::new(selection.report));
                state.report_charge = Some(charge);
            }
            state.phase = "initialized";
            state.active = true;
            state.actual = None;
            state.retry_after = None;
            state.last_status = None;
            if !hardware && policy == AM_DHW_PREFER_HW as u32 {
                state.hardware_after = Some(now + Duration::from_secs(30));
                state.effective = AM_DHW_SOFTWARE_ONLY as u32;
            }
        }
        Ok(Some(Self {
            raw: selection.decoder.into(),
            _permit: permit,
            _cpu_output: reservation,
            hardware,
            source: Arc::downgrade(source),
        }))
    }
}

impl Hub {
    pub(super) fn pixel_failure(
        &self,
        source: &Arc<SharedSource>,
        decoder: &mut Option<NativeDecoder>,
        times: &mut VecDeque<(i64, i64)>,
        fallback: DecodeError,
    ) -> std::result::Result<(), IoError> {
        let (error, hardware) = decoder.as_mut().map_or((fallback, false), |d| {
            (d.raw.take_last_error().unwrap_or(fallback), d.hardware)
        });
        // 原生帧与回读许可先释放，再发布失败状态和新代次。
        decoder.take();
        times.clear();
        let mut state = lock(&source.decode).map_err(|_| IoError::Resource)?;
        state.generation = state.generation.checked_add(1).ok_or(IoError::Resource)?;
        state.failed(error, hardware, self.now());
        drop(state);
        for sub in self
            .list()
            .into_iter()
            .filter(|sub| matches!(sub.kind, Kind::Frames { .. }))
        {
            sub.reset();
            source.event(
                AM_EVENT_STATE_CHANGED,
                AM_EVENT_SCOPE_SINGLE_SUB,
                sub.key.load(Ordering::Acquire),
                AM_FAULT_KEYFRAME_WAIT as u32,
                c"camera.pixelDecoderWaitingKeyframe",
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "显式提供FFmpeg和合成H264/HEVC Rext；实测硬件失败及CPU恢复"]
    fn stream_hardware_failure_only_resets_pixel_consumers_and_preserves_epochs() {
        use am_sdk_support::handles::Managed;
        let (domain, source, hub, _) = super::super::tests::fixture();
        let pixel = super::super::tests::sub(&source, &hub, 0, AM_SUB_PULL);
        let encoded = Subscription::new(
            source.state.clone(),
            &hub,
            Kind::Encoded { mask: 7 },
            AM_SUB_PULL,
            0,
            0,
        )
        .unwrap();
        lock(&hub.subscribers)
            .unwrap()
            .extend([Arc::downgrade(&pixel), Arc::downgrade(&encoded)]);
        let bytes = std::fs::read(std::env::var("AM_SDK_HEVC_REXT_SAMPLE").unwrap()).unwrap();
        let info = StreamInfo {
            codec: AM_CODEC_H265,
            width: 1920,
            height: 1080,
            fps: 1.0,
            time_base_num: 1,
            time_base_den: 1000,
            extradata: vec![],
        };
        let mut native = NativeDecoder::create(&source, VideoCodec::H265, &info, &[], hub.now())
            .unwrap()
            .unwrap();
        assert!(native.hardware, "需真实硬件，不把CPU回退冒充硬解测试");
        // 先入压缩队列，模拟真实consume先直传再解码的顺序。
        encoded.enqueue(Item::Encoded(Arc::new(queue::Payload {
            abi: am_encoded_packet_t {
                codec: AM_CODEC_H265,
                flags: AM_PKT_KEYFRAME | AM_PKT_PARAMSETS | AM_PKT_AU_COMPLETE,
                source_generation: 1,
                config_generation: 1,
                size: bytes.len() as u64,
                ..Default::default()
            },
            _charge: source.state.charge(bytes.len() as u64, false).unwrap(),
            bytes: bytes.clone(),
        })));
        let failure = native
            .raw
            .send(&bytes, 1, 1)
            .err()
            .or_else(|| native.raw.peek().err());
        assert!(failure.is_some(), "合成Rext须真正触发原生硬件错误");
        let original_generation = source.generation.load(Ordering::Acquire);
        let original_revision = hub.revision.load(Ordering::Acquire);
        let mut decoder = Some(native);
        let mut times = VecDeque::from([(1, 1)]);
        hub.pixel_failure(&source, &mut decoder, &mut times, failure.unwrap())
            .unwrap();
        assert!(decoder.is_none());
        assert!(times.is_empty());
        assert_eq!(
            source.generation.load(Ordering::Acquire),
            original_generation
        );
        assert_eq!(hub.revision.load(Ordering::Acquire), original_revision);
        assert_eq!(
            source.config_generation.load(Ordering::Acquire),
            1,
            "像素后端故障不能伪装为压缩码流配置变化"
        );
        assert_eq!(lock(&source.decode).unwrap().generation, 2);
        assert_eq!(encoded.queue_usage().0, 1, "压缩队列不能被像素故障清空");
        let mut cpu = NativeDecoder::create(&source, VideoCodec::H265, &info, &[], hub.now())
            .unwrap()
            .unwrap();
        assert!(!cpu.hardware);
        assert!(cpu.raw.send(&bytes, 2, 2).unwrap());
        let frame = cpu.raw.peek().unwrap().unwrap();
        let stamp = SessionStamp {
            revision: 0,
            generation: 1,
            credentials: source.credential_version(),
        };
        hub.emit_decoded::<true>(
            &source,
            &mut cpu.raw,
            &info,
            std::slice::from_ref(&pixel),
            &VecDeque::from([(2, 2)]),
            stamp,
        )
        .unwrap();
        assert_eq!(frame.pts, 2);
        let report = lock(&source.decode).unwrap();
        assert_eq!(report.actual, Some("ffmpeg-cpu"));
        assert_eq!(report.phase, "recovered_cpu");
        assert_eq!(report.failure, failure);
        assert!(
            report.selection.as_ref().unwrap().selected.is_some(),
            "保留失败前硬件选择证据"
        );
        drop(report);
        drop(cpu);
        assert_eq!(lock(&source.decode).unwrap().actual, None);
        pixel.request_close();
        encoded.request_close();
        drop((pixel, encoded, hub));
        let key = source.state.ctx_key;
        drop(source);
        domain.close(key, 100).unwrap();
        domain.destroy(key).unwrap();
    }
}
