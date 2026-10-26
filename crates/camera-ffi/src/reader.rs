//! Reader先取得转换许可，成功拆分载荷/配置；错误时先销毁原缓冲再归还。
use crate::frames::{CameraState, Charge};
use am_camera_core::{
    credentials::Secret,
    media::{
        ffmpeg_lib::FfmpegApi,
        sdk_io::{IoError, Packet, Reader, StreamInfo},
    },
};
use std::{
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
#[cfg(test)]
mod tests;

pub(crate) struct BudgetedReader {
    // 字段按声明顺序析构，Reader缓冲不能晚于许可释放。
    reader: Reader,
    retained: Charge,
    pending: Option<Charge>,
    state: Arc<CameraState>,
    failed: bool,
}
impl BudgetedReader {
    pub fn open(
        state: Arc<CameraState>,
        api: Arc<FfmpegApi>,
        url: &str,
        secret: &Secret,
        stop: Arc<AtomicBool>,
        timeout: Duration,
    ) -> Result<Self, IoError> {
        let mut pending = None;
        let reader = Reader::open_with_budget(api, url, secret, stop, timeout, |bytes| {
            pending = Some(
                state
                    .charge(bytes as u64, false)
                    .map_err(|_| IoError::Resource)?,
            );
            Ok(())
        })?;
        let mut owned = Self {
            reader,
            retained: pending.take().ok_or(IoError::Resource)?,
            pending: None,
            state,
            failed: false,
        };
        owned
            .retained
            .shrink_to(owned.reader.retained_buffer_bytes() as u64)
            .map_err(|_| IoError::Resource)?;
        Ok(owned)
    }
    pub fn info(&self) -> &StreamInfo {
        self.reader.info()
    }
    pub fn read_packet(&mut self) -> Result<Option<(Packet, Charge)>, IoError> {
        if self.failed {
            return Err(IoError::Closed);
        }
        let state = &self.state;
        let pending = &mut self.pending;
        let result = self.reader.read_packet_with_budget(|bytes| {
            *pending = Some(
                state
                    .charge(bytes as u64, false)
                    .map_err(|_| IoError::Resource)?,
            );
            Ok(())
        });
        match result {
            Ok(Some(packet)) => {
                let retained_bytes = self.reader.retained_buffer_bytes() as u64;
                let payload_bytes = packet.data.capacity() as u64;
                let held = self.pending.as_mut().ok_or(IoError::Resource)?;
                if retained_bytes.saturating_add(payload_bytes) > held.bytes() {
                    self.failed = true;
                    return Err(IoError::Resource);
                }
                let cache = held
                    .split_internal(retained_bytes)
                    .map_err(|_| IoError::Resource)?;
                let output = held
                    .split_internal(payload_bytes)
                    .map_err(|_| IoError::Resource)?;
                self.retained = cache;
                self.pending.take();
                Ok(Some((packet, output)))
            }
            Err(error) => {
                self.failed = true;
                Err(error)
            }
            Ok(None) => Ok(None),
        }
    }
}
