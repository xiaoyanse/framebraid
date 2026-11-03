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
            let mut gate = Gate::new(&configuration(codec, &info)).map_err(|_| IoError::Invalid)?;
            if source.file {
                // FILE 源（P1-8）：文件起点是受控随机访问起点（参数集由 Framing 在首包
                // 前置），不同于网络乱流恢复——非 IDR 的 I 帧（x264 open-GOP 常态）照常
                // 放行输出；IDR 门槛建留流源。
                gate.started = true;
            }
            Some(gate)
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
        let mut file_first_packet = source.file;
        let mut exact_until: Option<i64> = None;
        let mut pending_landing = false;
        let mut pace_base: Option<(f64, Instant)> = None;
        let mut last_rate_epoch = 0u64;
        std::thread::scope(|scope| {
            let mut execution = pipeline::Execution::new(scope, self, source, stamp);
            let mut pixel_config: Option<Arc<pipeline::Configuration>> = None;
            while self.current(source, stamp) {
                let Some(owned) = reader.read_packet()? else {
                    // FILE 源读到文件尾为预期终态（P1-8）：正常返回，由调用侧置 AM_SRC_EOF；
                    // 流源维持断流语义触发外层重连。
                    if source.file {
                        if pending_landing {
                            self.complete_seek(Ok(self.file_duration() as f64));
                            pending_landing = false;
                        }
                        if self.file_loop.load(Ordering::Acquire) && reader.seek(0, true).is_ok() {
                            self.seek_resync(source, c"camera.fileLoop");
                            exact_until = None;
                            pace_base = None;
                            file_first_packet = true;
                            continue;
                        }
                        return Ok(());
                    }
                    return Err(IoError::Network);
                };
                // 局部变量逆序析构，早退也先释放包缓冲再退额度。
                let (charge, packet) = (owned.1, owned.0);
                if source.file {
                    if let Some(cmd) = self.take_seek() {
                        let target = self.seek_target(cmd.target_ms);
                        match reader.seek(target as i64, cmd.mode != 1) {
                            Ok(()) => {
                                self.seek_resync(source, c"camera.seek");
                                exact_until = (cmd.mode == 2).then_some(
                                    (target * info.time_base_den as f64
                                        / (info.time_base_num as f64 * 1000.0))
                                        as i64,
                                );
                                pending_landing = cmd.mode != 2;
                                if cmd.mode == 2 {
                                    self.complete_seek(Ok(target));
                                }
                                pace_base = None;
                                file_first_packet = true;
                                // 成功：当前包属旧时间轴，丢弃重读落点。
                                continue;
                            }
                            Err(error) => {
                                // 失败：时间轴未变，当前包仍属顺序读流——照常投递
                                //（FILE 背压不丢帧合同；失败仅见于损坏容器）。
                                self.complete_seek(Err(error));
                            }
                        }
                    }
                    let packet_pts_ms = packet.pts as f64 * 1000.0 * info.time_base_num as f64
                        / info.time_base_den as f64;
                    if pending_landing {
                        self.complete_seek(Ok(packet_pts_ms));
                        pending_landing = false;
                    }
                    let epoch = self.rate_epoch.load(Ordering::Acquire);
                    let rate = self.file_rate.lock().map_or(f64::INFINITY, |rate| *rate);
                    if epoch != last_rate_epoch {
                        pace_base = None;
                        last_rate_epoch = epoch;
                    }
                    if rate.is_finite() && rate > 0.0 {
                        match pace_base {
                            None => pace_base = Some((packet_pts_ms, Instant::now())),
                            Some((t0, w0)) => {
                                if let Some(due) = Duration::try_from_secs_f64(
                                    ((packet_pts_ms - t0).max(0.0) / 1000.0) / rate,
                                )
                                .ok()
                                .and_then(|d| w0.checked_add(d))
                                {
                                    while Instant::now() < due {
                                        if source.stop.load(Ordering::Acquire)
                                            || self.has_pending_seek()
                                            || self.rate_epoch.load(Ordering::Acquire) != epoch
                                        {
                                            break;
                                        }
                                        std::thread::sleep(
                                            due.saturating_duration_since(Instant::now())
                                                .min(Duration::from_millis(20)),
                                        );
                                    }
                                }
                            }
                        }
                    } else {
                        pace_base = None;
                    }
                    if self.has_pending_seek() {
                        continue;
                    }
                }
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
                    if source.file && file_first_packet {
                        // FILE 起点（P1-8）：首包按随机访问点输出（与 Gate started、
                        // Framing 首包 keyframe 同一语义链），非 IDR I 帧不因缺
                        // KEYFRAME 标志被订阅队列 waiting_key 全量丢弃。
                        random = true;
                        file_first_packet = false;
                    }
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
                let encoded_drop = exact_until.is_some_and(|u| data.abi.pts < u);
                if !self.current(source, stamp) {
                    return Err(IoError::Closed);
                }
                for sub in subscribers.iter() {
                    if !encoded_drop && matches!(sub.kind, Kind::Encoded { .. }) {
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
                        drop_before_pts: exact_until,
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
