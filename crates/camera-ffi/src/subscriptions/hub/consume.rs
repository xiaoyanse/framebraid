use super::*;

impl Hub {
    pub(super) fn consume(
        &self,
        source: &Arc<SharedSource>,
        reader: &mut Reader,
        stamp: SessionStamp,
    ) -> std::result::Result<(), IoError> {
        let _activity = super::native::Activity(Arc::downgrade(source));
        let old = reader.info();
        let _info_charge = source
            .state
            .charge(old.extradata.len() as u64, false)
            .map_err(|_| IoError::Resource)?;
        let info = StreamInfo {
            codec: old.codec,
            width: old.width,
            height: old.height,
            fps: old.fps,
            time_base_num: old.time_base_num,
            time_base_den: old.time_base_den,
            extradata: old.extradata.clone(),
        };
        let codec = match info.codec {
            AM_CODEC_H264 => Some(VideoCodec::H264),
            AM_CODEC_H265 => Some(VideoCodec::H265),
            AM_CODEC_MJPEG => None,
            _ => return Err(IoError::Unsupported),
        };
        if codec.is_none() {
            let mut state = lock(&source.decode).map_err(|_| IoError::Resource)?;
            state.selection.take();
            state.report_charge.take();
            state.key = None;
            state.failure = None;
            state.last_status = None;
            state.recovered = false;
            state.effective = AM_DHW_SOFTWARE_ONLY as u32;
        }
        let _initial_charge = source
            .state
            .charge(info.extradata.len() as u64, false)
            .map_err(|_| IoError::Resource)?;
        let mut cache_bytes = info.extradata.len() as u64;
        let mut cache_charge = source
            .state
            .charge(cache_bytes, false)
            .map_err(|_| IoError::Resource)?;
        let mut gate = if let Some(codec) = codec {
            Some(Gate::new(&configuration(codec, &info)).map_err(|_| IoError::Invalid)?)
        } else {
            None
        };
        if let Some(gate) = &mut gate {
            gate.dirty = false;
        }
        drop(_initial_charge);
        let mut decoder: Option<super::native::NativeDecoder> = None;
        let mut reconfigure_pending = false;
        let mut times = VecDeque::new();
        let _times_charge = source
            .state
            .charge(128 * 16, false)
            .map_err(|_| IoError::Resource)?;
        while self.current(source, stamp) {
            let Some(packet) = reader.read_packet()? else {
                return Err(IoError::Network);
            };
            if !self.current(source, stamp) {
                return Err(IoError::Closed);
            }
            let received = am_sdk_support::clock::monotonic_ms();
            let packet_sequence = sequence(&self.packet_sequence)?;
            let charge = source
                .state
                .charge(packet.data.len() as u64, false)
                .map_err(|_| IoError::Resource)?;
            let mut ready = true;
            let mut random = packet.keyframe;
            if let (Some(gate), Some(codec)) = (&mut gate, codec) {
                let _temporary = source
                    .state
                    .charge(packet.data.len() as u64, false)
                    .map_err(|_| IoError::Resource)?;
                ready = gate
                    .observe(codec, &packet.data)
                    .map_err(|_| IoError::Invalid)?;
                random = is_random(codec, &packet.data);
                if gate.dirty {
                    let _copy_charge = source
                        .state
                        .charge(cache_bytes.saturating_add(packet.data.len() as u64), false)
                        .map_err(|_| IoError::Resource)?;
                    let extra = gate.extra();
                    let new_charge = source
                        .state
                        .charge(extra.len() as u64, false)
                        .map_err(|_| IoError::Resource)?;
                    cache_charge = new_charge;
                    cache_bytes = extra.len() as u64;
                    source.config_generation.fetch_add(1, Ordering::AcqRel);
                    gate.dirty = false;
                    reconfigure_pending = decoder.is_some();
                    if reconfigure_pending {
                        let mut state = lock(&source.decode).map_err(|_| IoError::Resource)?;
                        state.actual = None;
                        state.active = false;
                        state.phase = "waiting_keyframe";
                    }
                    times.clear();
                    self.reset_subscribers();
                    source.event(
                        AM_EVENT_SPATIAL_CHANGED,
                        AM_EVENT_SCOPE_ALL_CONSUMERS,
                        0,
                        AM_FAULT_KEYFRAME_WAIT as u32,
                        c"camera.codecConfigurationChanged",
                    );
                }
                if ready {
                    gate.started = true;
                }
            }
            let _ = &cache_charge;
            if !ready {
                continue;
            }
            let generation = stamp.generation;
            let config_generation = source.config_generation.load(Ordering::Acquire);
            let has_sets = has_parameter_sets(info.codec, &packet.data);
            let data = Arc::new(queue::Payload {
                abi: am_encoded_packet_t {
                    codec: info.codec,
                    bitstream_format: AM_BSF_ANNEXB,
                    flags: AM_PKT_AU_COMPLETE
                        | if random { AM_PKT_KEYFRAME } else { 0 }
                        | if has_sets { AM_PKT_PARAMSETS } else { 0 },
                    config_generation,
                    source_generation: generation,
                    source_id: source.source_id,
                    seq: packet_sequence,
                    pts: packet.pts,
                    dts: packet.dts,
                    time_base_num: info.time_base_num,
                    time_base_den: info.time_base_den,
                    size: packet.data.len() as u64,
                    ..Default::default()
                },
                bytes: packet.data,
                _charge: charge,
            });
            let subscribers = self.list();
            if !self.current(source, stamp) {
                return Err(IoError::Closed);
            }
            for sub in &subscribers {
                if matches!(sub.kind, Kind::Encoded { .. }) {
                    sub.enqueue(Item::Encoded(data.clone()));
                }
            }
            let frames = subscribers.iter().any(|s| s.wants_frames());
            if !frames {
                decoder.take();
                reconfigure_pending = false;
                times.clear();
                if let Ok(mut state) = source.decode.lock() {
                    state.idle();
                }
                source.received(received);
                continue;
            }
            if let Some(codec) = codec {
                if reconfigure_pending {
                    if !random {
                        continue;
                    }
                    let _extra_charge = match source.state.charge(cache_bytes, false) {
                        Ok(charge) => charge,
                        Err(_) => {
                            source.report(
                                AM_SRC_LIVE as u32,
                                AM_FAULT_RESOURCE as u32,
                                c"camera.pixelOutputResourceWait",
                            );
                            continue;
                        }
                    };
                    if !self.reconfigure_pixels(
                        source,
                        &mut decoder,
                        codec,
                        &info,
                        &gate.as_ref().map_or_else(Vec::new, Gate::extra),
                        random,
                    )? {
                        // 故障已清空旧decoder时，下一随机访问点走既有create/CPU恢复。
                        reconfigure_pending = decoder.is_some();
                        continue;
                    }
                    reconfigure_pending = false;
                }
                if decoder.is_none() {
                    if !random {
                        if source.fault.load(Ordering::Acquire) != AM_FAULT_RESOURCE as u32 {
                            source.report(
                                AM_SRC_LIVE as u32,
                                AM_FAULT_KEYFRAME_WAIT as u32,
                                c"camera.waitingKeyframe",
                            );
                        }
                        continue;
                    }
                    let extra = gate.as_ref().map_or_else(Vec::new, Gate::extra);
                    let _extra_charge = match source.state.charge(extra.len() as u64, false) {
                        Ok(charge) => charge,
                        Err(_) => continue,
                    };
                    match super::native::NativeDecoder::create(
                        source,
                        codec,
                        &info,
                        &extra,
                        self.now(),
                    ) {
                        Ok(Some(value)) => decoder = Some(value),
                        Ok(None) => continue,
                        Err(code) => {
                            if let Ok(mut state) = source.decode.lock() {
                                state.last_status = Some(code);
                                state.retry_after = Some(self.now() + Duration::from_secs(1));
                                state.actual = None;
                                state.active = false;
                                state.phase = if code == AM_ERR_SDK_RESOURCE_EXHAUSTED {
                                    "waiting_resource"
                                } else {
                                    "failed"
                                };
                            }
                            source.report(
                                AM_SRC_LIVE as u32,
                                AM_FAULT_RESOURCE as u32,
                                c"camera.pixelDecoderUnavailable",
                            );
                            continue;
                        }
                    }
                }
                times.push_back((packet.pts, received));
                while times.len() > 128 {
                    times.pop_front();
                }
                if let Some(native) = &mut decoder {
                    let cpu = &mut native.raw;
                    let decoded = (|| {
                        let mut sent = cpu
                            .send(&data.bytes, packet.pts, packet.dts)
                            .map_err(|_| IoError::Invalid)?;
                        self.emit_decoded::<true>(source, cpu, &info, &subscribers, &times, stamp)?;
                        if !sent {
                            sent = cpu
                                .send(&data.bytes, packet.pts, packet.dts)
                                .map_err(|_| IoError::Invalid)?;
                            self.emit_decoded::<true>(
                                source,
                                cpu,
                                &info,
                                &subscribers,
                                &times,
                                stamp,
                            )?;
                        }
                        if sent {
                            Ok(())
                        } else {
                            Err(IoError::Resource)
                        }
                    })();
                    match decoded {
                        Ok(()) => {}
                        Err(IoError::Closed) => return Err(IoError::Closed),
                        Err(IoError::Resource) => {
                            source.report(
                                AM_SRC_LIVE as u32,
                                AM_FAULT_RESOURCE as u32,
                                c"camera.pixelOutputResourceWait",
                            );
                        }
                        Err(_) => self.pixel_failure(
                            source,
                            &mut decoder,
                            &mut times,
                            am_camera_core::media::codec::DecodeError::InvalidInput,
                        )?,
                    }
                }
            } else {
                let _encoded_charge = source
                    .state
                    .charge(data.bytes.len() as u64, false)
                    .map_err(|_| IoError::Resource)?;
                let max_pixels = (source
                    .state
                    .limits()
                    .map_err(|_| IoError::Resource)?
                    .frame_budget_bytes
                    / 4)
                .min(8192 * 8192);
                let header = am_camera_core::media::sdk_io::decode_image(
                    data.bytes.clone(),
                    max_pixels,
                    false,
                )?;
                self.observe_dimensions(source, header.width, header.height);
                let frame_sequence = sequence(&self.frame_sequence)?;
                if !subscribers
                    .iter()
                    .any(|sub| sub.needs_pixels(Instant::now()))
                {
                    source.received(received);
                    continue;
                }
                let _raw_charge = match source
                    .state
                    .charge(header.width as u64 * header.height as u64 * 4, false)
                {
                    Ok(charge) => charge,
                    Err(_) => {
                        source
                            .state
                            .metrics
                            .frames_dropped
                            .fetch_add(1, Ordering::AcqRel);
                        source.report(
                            AM_SRC_LIVE as u32,
                            AM_FAULT_RESOURCE as u32,
                            c"camera.outputFrameBudgetExceeded",
                        );
                        continue;
                    }
                };
                let image =
                    am_camera_core::media::sdk_io::decode_image(header.encoded, max_pixels, true)?;
                if let Some(rgba) = image.rgba {
                    {
                        let mut state = lock(&source.decode).map_err(|_| IoError::Resource)?;
                        state.actual = Some("image-cpu");
                        state.effective = AM_DHW_SOFTWARE_ONLY as u32;
                        state.active = true;
                        state.phase = "verified";
                    }
                    source.state.metrics.decoded.fetch_add(1, Ordering::AcqRel);
                    self.observe_dimensions(source, image.width, image.height);
                    if !self.current(source, stamp) {
                        return Err(IoError::Closed);
                    }
                    pixels::fanout(
                        source,
                        &subscribers,
                        rgba,
                        image.width,
                        image.height,
                        packet.pts,
                        &info,
                        received,
                        frame_sequence,
                        stamp.generation,
                        source.config_generation.load(Ordering::Acquire),
                        self.now(),
                    )
                    .map_err(|_| IoError::Resource)?;
                }
            }
        }
        Err(IoError::Closed)
    }
}
