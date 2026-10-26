use super::*;

pub(super) fn receive(
    key: usize,
    decoder: &Decoder,
    inner: &mut Inner,
) -> Result<Option<Arc<Frame>>> {
    loop {
        match inner.cpu.peek() {
            Ok(Some(f)) => {
                // 帧内保存内部唯一包号，再恢复原PTS；重复/未知PTS不能串到别帧。
                let timing = inner
                    .buffers
                    .timings
                    .iter()
                    .find(|t| t.tag == f.pts)
                    .ok_or(AM_ERR_SDK_INTERNAL_ERROR)?;
                let (pts, received, config) = (timing.pts, timing.received, timing.config);
                let format = inner.config.abi.output_pixel_format;
                let bytes = super::frames::scale::bytes(f.width, f.height, format)?;
                // NV12经有预算的RGB中间帧规范化为601 limited，不冒充源流色彩元数据。
                let (charge, scratch) = inner.output_charges(
                    decoder,
                    bytes as u64,
                    if format == AM_PF_NV12 as u32 {
                        f.rgba_bytes as u64
                    } else {
                        0
                    },
                )?;
                let decoded = budget::copy_pixels(
                    &decoder.ctx,
                    &mut inner.cpu,
                    if scratch.is_some() {
                        f.rgba_bytes
                    } else {
                        bytes
                    },
                    if scratch.is_some() {
                        AM_PF_RGBA8 as u32
                    } else {
                        format
                    },
                );
                let decoded = match decoded {
                    Ok(Some(decoded)) => decoded,
                    Ok(None) => return Err(AM_ERR_SDK_INTERNAL_ERROR),
                    Err(error)
                        if inner.hardware_active() && error == AM_ERR_CAMERA_CODEC_UNSUPPORTED =>
                    {
                        drop((charge, scratch));
                        let cause = inner
                            .cpu
                            .take_last_error()
                            .unwrap_or(DecodeError::InvalidInput);
                        inner.recover(decoder, Some((f.width, f.height)), cause)?;
                        recovery::record_fallback(key);
                        continue;
                    }
                    Err(error) => return Err(error),
                };
                let data = if scratch.is_some() {
                    am_camera_core::media::pixel_ops::rgba_to_nv12(&decoded.data, f.width, f.height)
                        .map_err(|_| AM_ERR_CAMERA_PIXEL_FORMAT_UNSUPPORTED)?
                } else {
                    decoded.data
                };
                drop(scratch);
                inner.update_actual(decoder)?;
                let c = &inner.config.abi;
                let meta = am_frame_meta_t {
                    source_id: decoder.source,
                    frame_id: crate::frames::next_frame_id()?,
                    seq: next(&decoder.sequence)?,
                    source_generation: decoder.generation.load(Ordering::Acquire),
                    config_generation: config,
                    pts,
                    time_base_num: if pts != i64::MIN { c.time_base_num } else { 0 },
                    time_base_den: if pts != i64::MIN { c.time_base_den } else { 0 },
                    captured_at_ms: i64::MIN,
                    received_at_ms: received,
                    clock_quality: AM_CQ_LOCAL_RECEIPT as u32,
                    clock_mask: AM_CLK_RECEIVED | if pts != i64::MIN { AM_CLK_PTS } else { 0 },
                    transform: am_spatial_transform_t {
                        scale_x: 1.,
                        scale_y: 1.,
                        valid: 1,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let frame = Frame::with_format(data, f.width, f.height, format, meta, charge)?;
                if let Some(timing) = inner
                    .buffers
                    .timings
                    .iter_mut()
                    .find(|timing| timing.tag == f.pts)
                {
                    timing.delivered = true;
                }
                inner.startup.take();
                return Ok(Some(frame));
            }
            Err(DecodeError::Drained) => {
                decoder
                    .state
                    .store(AM_DEC_DRAINED as u32, Ordering::Release);
                return Err(AM_ERR_SDK_DRAINED);
            }
            Err(e) => {
                if inner.hardware_active()
                    && matches!(e, DecodeError::Runtime(_) | DecodeError::InvalidInput)
                {
                    inner.recover(decoder, None, e)?;
                    recovery::record_fallback(key);
                    continue;
                }
                return Err(convert(e));
            }
            Ok(None) => {}
        }
        if let Some(packet) = inner.front_packet() {
            // 先复制引用范围信息，再允许Gate独立更新；输入所有权仍在有界槽内。
            let codec = inner.config.codec;
            let bytes = &packet.bytes[..packet.valid];
            let mut random = false;
            visit_access_units(codec, bytes, |nal| {
                random |= if codec == VideoCodec::H264 {
                    nal[0] & 31 == 5
                } else {
                    (16..=21).contains(&((nal[0] >> 1) & 63))
                }
            })
            .map_err(convert)?;
            // 分开借用字段，避免通过front_packet借用整个Inner跨越可变Gate操作。
            let packet = inner
                .replay
                .as_ref()
                .or_else(|| inner.buffers.queue.front())
                .ok_or(AM_ERR_SDK_INTERNAL_ERROR)?;
            let allowed = inner
                .gate
                .observe(inner.config.codec, &packet.bytes[..packet.valid])?;
            if !allowed {
                inner.pop_packet()?;
                continue;
            }
            if inner.gate.dirty {
                let config = inner.config_generation_checked(decoder)?;
                let cpu = inner.cpu.reconfigure(
                    inner.config.codec,
                    &inner.gate.extra()?,
                    (decoder.ctx.limits()?.frame_budget_bytes / 4)
                        .min(SDK_MAX_DYNAMIC_AXIS * SDK_MAX_DYNAMIC_AXIS),
                );
                let cpu = match cpu {
                    Ok(cpu) => cpu,
                    Err(error) if inner.hardware_active() => {
                        inner.recover(decoder, None, error)?;
                        recovery::record_fallback(key);
                        continue;
                    }
                    Err(error) => return Err(convert(error)),
                };
                inner.cpu = cpu.into();
                inner.buffers.timings.clear();
                inner.startup.take();
                inner.recovery_charge.take();
                inner.gate.clear_dirty();
                decoder.config_generation.store(config, Ordering::Release);
                let mut snapshot = decoder
                    .codec_info
                    .lock()
                    .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
                snapshot.abi = info(&inner.config, config, inner.hardware_active());
                snapshot.executed = false;
            }
            let packet = inner
                .replay
                .as_ref()
                .or_else(|| inner.buffers.queue.front())
                .ok_or(AM_ERR_SDK_INTERNAL_ERROR)?;
            let mut picture = false;
            visit_access_units(inner.config.codec, &packet.bytes[..packet.valid], |nal| {
                picture |= if inner.config.codec == VideoCodec::H264 {
                    (1..=5).contains(&(nal[0] & 31))
                } else {
                    (nal[0] >> 1) & 63 <= 31
                }
            })
            .map_err(convert)?;
            if picture && inner.buffers.timings.len() >= buffers::TIMING_ITEMS {
                if inner.buffers.timings.front().is_some_and(|t| t.delivered) {
                    inner.buffers.timings.pop_front();
                } else {
                    return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
                }
            }
            let sent = inner
                .cpu
                .send_padded(&packet.bytes, packet.valid, packet.tag, packet.tag);
            let sent = match sent {
                Ok(sent) => sent,
                Err(error) if inner.hardware_active() => {
                    inner.recover(decoder, None, error)?;
                    recovery::record_fallback(key);
                    continue;
                }
                Err(error) => return Err(convert(error)),
            };
            if !sent {
                return Err(AM_ERR_SDK_TRY_AGAIN);
            }
            if picture {
                inner.buffers.timings.push_back(Timing {
                    tag: packet.tag,
                    pts: packet.pts,
                    received: packet.received,
                    config: decoder.config_generation.load(Ordering::Acquire),
                    delivered: false,
                });
            }
            inner.gate.mark_started();
            let consumed = inner.pop_packet()?;
            if picture
                && random
                && inner.hardware_active()
                && decoder.sequence.load(Ordering::Acquire) == 0
                && inner.buffers.timings.len() == 1
            {
                inner.startup = Some(consumed);
            } else if picture {
                inner.startup.take();
            }
            continue;
        }
        if decoder.state.load(Ordering::Acquire) == AM_DEC_DRAINING as u32 && !inner.flushed {
            match inner.cpu.drain() {
                Ok(true) => inner.flushed = true,
                Ok(false) => {}
                Err(error) if inner.hardware_active() => {
                    inner.recover(decoder, None, error)?;
                    recovery::record_fallback(key);
                }
                Err(error) => return Err(convert(error)),
            }
            continue;
        }
        return Ok(None);
    }
}
