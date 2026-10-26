use super::super::native::NativeDecoder;
use super::*;

pub(super) struct Worker<'a> {
    hub: &'a Hub,
    source: &'a Arc<SharedSource>,
    stamp: SessionStamp,
    permit: Arc<DecoderPermit>,
    decoder: Option<NativeDecoder>,
    previous: Option<(u64, u32)>,
    times: VecDeque<(i64, i64)>,
    _times_charge: Charge,
}
impl<'a> Worker<'a> {
    pub fn new(
        hub: &'a Hub,
        source: &'a Arc<SharedSource>,
        stamp: SessionStamp,
        permit: Arc<DecoderPermit>,
    ) -> Result<Self> {
        let times_charge = source.state.charge(128 * 16, false)?;
        Ok(Self {
            hub,
            source,
            stamp,
            permit,
            decoder: None,
            previous: None,
            times: VecDeque::new(),
            _times_charge: times_charge,
        })
    }
    pub fn run(mut self, pipe: &Pipeline) {
        let _activity = super::super::native::Activity(Arc::downgrade(self.source));
        while let Ok(Some(job)) = pipe.pop() {
            let mut delivery = Delivery {
                pipe,
                epoch: job.epoch,
                config: job.config.generation,
            };
            if !self.hub.current(self.source, self.stamp) || !delivery.current(self.source) {
                continue;
            }
            #[cfg(test)]
            if let Some(block) = &self.hub.pixel_block {
                block.wait();
            }
            if !self.hub.current(self.source, self.stamp) || !delivery.current(self.source) {
                continue;
            }
            let result = self.process(&job, &mut delivery);
            if !self.hub.current(self.source, self.stamp) || !delivery.current(self.source) {
                continue;
            }
            match result {
                Ok(()) | Err(IoError::Closed) => {}
                Err(IoError::Resource) => {
                    let _guard = match self.hub.configuration.lock() {
                        Ok(g) => g,
                        Err(_) => break,
                    };
                    if delivery.current(self.source) {
                        if pipe.resync().is_err() {
                            break;
                        }
                        for sub in self
                            .hub
                            .list()
                            .iter()
                            .filter(|s| matches!(s.kind, Kind::Frames { .. }))
                        {
                            sub.reset();
                        }
                        self.source.report(
                            AM_SRC_LIVE as u32,
                            AM_FAULT_RESOURCE as u32,
                            c"camera.pixelOutputResourceWait",
                        );
                    }
                }
                Err(_) => {
                    let _guard = match self.hub.configuration.lock() {
                        Ok(g) => g,
                        Err(_) => break,
                    };
                    if delivery.current(self.source) {
                        let _ = self.hub.pixel_failure(
                            self.source,
                            &mut self.decoder,
                            &mut self.times,
                            am_camera_core::media::codec::DecodeError::InvalidInput,
                        );
                    }
                }
            }
        }
    }
    fn process(
        &mut self,
        job: &Job,
        delivery: &mut Delivery<'_>,
    ) -> std::result::Result<(), IoError> {
        let source = self.source;
        let hub = self.hub;
        let info = &job.config.stream.info;
        let subscribers = hub.list();
        if !subscribers.iter().any(|s| s.wants_frames()) {
            self.decoder.take();
            self.times.clear();
            return Ok(());
        }
        let codec = match info.codec {
            AM_CODEC_H264 => Some(VideoCodec::H264),
            AM_CODEC_H265 => Some(VideoCodec::H265),
            AM_CODEC_MJPEG => None,
            _ => return Err(IoError::Unsupported),
        };
        let Some(codec) = codec else {
            return self.jpeg(job, &subscribers, delivery);
        };
        let random = job.packet.abi.flags & AM_PKT_KEYFRAME != 0;
        let identity = (job.epoch, job.config.generation);
        if self.previous != Some(identity) {
            self.times.clear();
            if !random {
                return Ok(());
            }
            if !hub.reconfigure_pixels(
                source,
                &mut self.decoder,
                codec,
                info,
                &job.config.extra,
                true,
            )? {
                return Ok(());
            }
            self.previous = Some(identity);
        }
        if self.decoder.is_none() {
            if !random {
                return Ok(());
            }
            match NativeDecoder::create_with_permit(
                source,
                codec,
                info,
                &job.config.extra,
                hub.now(),
                self.permit.clone(),
            ) {
                Ok(Some(native)) => self.decoder = Some(native),
                Ok(None) => return Ok(()),
                Err(code) => {
                    let _guard = lock(&hub.configuration).map_err(|_| IoError::Resource)?;
                    if delivery.current(source) {
                        let mut state = lock(&source.decode).map_err(|_| IoError::Resource)?;
                        state.last_status = Some(code);
                        state.retry_after = Some(hub.now() + Duration::from_secs(1));
                        state.actual = None;
                        state.active = false;
                        state.phase = if code == AM_ERR_SDK_RESOURCE_EXHAUSTED {
                            "waiting_resource"
                        } else {
                            "failed"
                        };
                    }
                    return Err(IoError::Resource);
                }
            }
        }
        if !hub.current(source, self.stamp) || !delivery.current(source) {
            return Err(IoError::Closed);
        }
        let native = self.decoder.as_mut().ok_or(IoError::Invalid)?;
        self.times.push_back((job.packet.abi.pts, job.received));
        while self.times.len() > 128 {
            self.times.pop_front();
        }
        let cpu = &mut native.raw;
        let mut sent = cpu
            .send(&job.packet.bytes, job.packet.abi.pts, job.packet.abi.dts)
            .map_err(|_| IoError::Invalid)?;
        hub.emit_decoded_checked::<true>(
            source,
            cpu,
            info,
            &subscribers,
            &self.times,
            self.stamp,
            Some(delivery),
        )?;
        if !sent {
            sent = cpu
                .send(&job.packet.bytes, job.packet.abi.pts, job.packet.abi.dts)
                .map_err(|_| IoError::Invalid)?;
            hub.emit_decoded_checked::<true>(
                source,
                cpu,
                info,
                &subscribers,
                &self.times,
                self.stamp,
                Some(delivery),
            )?;
        }
        if sent {
            Ok(())
        } else {
            Err(IoError::Resource)
        }
    }
    fn jpeg(
        &self,
        job: &Job,
        subscribers: &[Arc<Subscription>],
        delivery: &mut Delivery<'_>,
    ) -> std::result::Result<(), IoError> {
        let source = self.source;
        let _encoded_charge = source
            .state
            .charge(job.packet.bytes.len() as u64, false)
            .map_err(|_| IoError::Resource)?;
        let max_pixels = (source
            .state
            .limits()
            .map_err(|_| IoError::Resource)?
            .frame_budget_bytes
            / 4)
        .min(8192 * 8192);
        let header = am_camera_core::media::sdk_io::decode_image(
            job.packet.bytes.clone(),
            max_pixels,
            false,
        )?;
        {
            let _guard = lock(&self.hub.configuration).map_err(|_| IoError::Resource)?;
            if !self.hub.current(source, self.stamp) || !delivery.current(source) {
                return Err(IoError::Closed);
            }
            self.hub
                .observe_dimensions(source, header.width, header.height);
            delivery.config = source.config_generation.load(Ordering::Acquire);
        }
        let seq = sequence(&self.hub.frame_sequence)?;
        if !subscribers.iter().any(|s| s.needs_pixels(Instant::now())) {
            return Ok(());
        }
        let _rgba = source
            .state
            .charge(header.width as u64 * header.height as u64 * 4, false)
            .map_err(|_| IoError::Resource)?;
        let image = am_camera_core::media::sdk_io::decode_image(header.encoded, max_pixels, true)?;
        if let Some(rgba) = image.rgba {
            let _guard = lock(&self.hub.configuration).map_err(|_| IoError::Resource)?;
            if !self.hub.current(source, self.stamp) || !delivery.current(source) {
                return Err(IoError::Closed);
            }
            {
                let mut state = lock(&source.decode).map_err(|_| IoError::Resource)?;
                state.actual = Some("image-cpu");
                state.effective = AM_DHW_SOFTWARE_ONLY as u32;
                state.active = true;
                state.phase = "verified";
            }
            source.state.metrics.decoded.fetch_add(1, Ordering::AcqRel);
            pixels::fanout(
                source,
                subscribers,
                rgba,
                image.width,
                image.height,
                job.packet.abi.pts,
                &job.config.stream.info,
                job.received,
                seq,
                self.stamp.generation,
                delivery.config,
                self.hub.now(),
            )
            .map_err(|_| IoError::Resource)?;
        }
        Ok(())
    }
}
