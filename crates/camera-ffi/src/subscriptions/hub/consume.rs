use super::*;

impl Hub {
    pub(super) fn consume(
        &self,
        source: &Arc<SharedSource>,
        reader: &mut BudgetedReader,
        stamp: SessionStamp,
    ) -> std::result::Result<(), IoError> {
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
        let stream = Arc::new(pipeline::Stream {
            info,
            _charge: _info_charge,
        });
        let info = &stream.info;
        std::thread::scope(|scope| {
            let mut execution = pipeline::Execution::new(scope, self, source, stamp);
            let mut pixel_config: Option<Arc<pipeline::Configuration>> = None;
            while self.current(source, stamp) {
                let Some(owned) = reader.read_packet()? else {
                    return Err(IoError::Network);
                };
                // 局部变量逆序析构，早退也先释放包缓冲再退额度。
                let (charge, packet) = (owned.1, owned.0);
                if !self.current(source, stamp) {
                    return Err(IoError::Closed);
                }
                let received = am_sdk_support::clock::monotonic_ms();
                let packet_sequence = sequence(&self.packet_sequence)?;
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
                        let _configuration =
                            lock(&self.configuration).map_err(|_| IoError::Resource)?;
                        source.config_generation.fetch_add(1, Ordering::AcqRel);
                        {
                            let mut decode = lock(&source.decode).map_err(|_| IoError::Resource)?;
                            decode.actual = None;
                            decode.active = false;
                            decode.phase = "waiting_keyframe";
                        }
                        gate.dirty = false;
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
                for sub in subscribers.iter() {
                    if matches!(sub.kind, Kind::Encoded { .. }) {
                        sub.enqueue(Item::Encoded(data.clone()));
                    }
                }
                source.received(received);
                if !subscribers.iter().any(|s| s.wants_frames()) {
                    execution.stop();
                    pixel_config.take();
                    continue;
                }
                let generation = source.config_generation.load(Ordering::Acquire);
                if pixel_config
                    .as_ref()
                    .is_none_or(|c| c.generation != generation)
                {
                    let charge = match source.state.charge(
                        cache_bytes + std::mem::size_of::<pipeline::Configuration>() as u64,
                        false,
                    ) {
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
                    pixel_config = Some(Arc::new(pipeline::Configuration {
                        stream: stream.clone(),
                        extra: gate.as_ref().map_or_else(Vec::new, Gate::extra),
                        generation,
                        _charge: charge,
                    }));
                }
                if let Some(config) = pixel_config.as_ref() {
                    if let Err(code) = execution.submit(pipeline::Job {
                        packet: data,
                        config: config.clone(),
                        received,
                        epoch: 0,
                    }) {
                        if let Ok(mut state) = source.decode.lock() {
                            state.last_status = Some(code);
                        }
                        source.report(
                            AM_SRC_LIVE as u32,
                            AM_FAULT_RESOURCE as u32,
                            c"camera.pixelOutputResourceWait",
                        );
                    }
                }
            }
            Err(IoError::Closed)
        })
    }
}
