//! 策略属于共享源，查询只读；失败缓存只保留当前配置的一项。
use super::*;
use am_camera_core::media::codec::{selection::SelectionReport, DecodeError};
use std::time::{Duration, Instant};

pub(crate) struct DecodeState {
    pub policy: u32,
    pub effective: u32,
    pub locked: bool,
    pub selection: Option<Arc<SelectionReport>>,
    pub report_charge: Option<crate::frames::ControlCharge>,
    pub actual: Option<&'static str>,
    pub phase: &'static str,
    pub failure: Option<DecodeError>,
    pub last_status: Option<i32>,
    pub active: bool,
    pub recovered: bool,
    pub key: Option<u64>,
    pub generation: u32,
    pub retry_after: Option<Instant>,
    pub hardware_after: Option<Instant>,
}
impl Default for DecodeState {
    fn default() -> Self {
        Self {
            policy: AM_DHW_PREFER_HW as u32,
            effective: AM_DHW_PREFER_HW as u32,
            locked: false,
            selection: None,
            report_charge: None,
            actual: None,
            phase: "idle",
            failure: None,
            last_status: None,
            active: false,
            recovered: false,
            key: None,
            generation: 1,
            retry_after: None,
            hardware_after: None,
        }
    }
}
impl DecodeState {
    pub fn prepare(&mut self, key: u64, now: Instant) -> Option<u32> {
        if self.key != Some(key) {
            self.key = Some(key);
            self.failure = None;
            self.last_status = None;
            self.hardware_after = None;
            self.retry_after = None;
            self.recovered = false;
        }
        if self.retry_after.is_some_and(|at| now < at) {
            return None;
        }
        self.actual = None;
        self.active = false;
        self.phase = "initializing";
        self.effective = if self.hardware_after.is_some_and(|at| now < at) {
            AM_DHW_SOFTWARE_ONLY as u32
        } else {
            self.policy
        };
        Some(self.effective)
    }
    pub fn failed(&mut self, error: DecodeError, hardware: bool, now: Instant) {
        self.failure = Some(error);
        self.actual = None;
        self.active = false;
        self.phase = "waiting_keyframe";
        self.retry_after = Some(now + Duration::from_secs(1));
        if hardware {
            self.hardware_after = Some(now + Duration::from_secs(30));
            self.recovered = true;
            // 正常下一随机访问点即可恢复CPU，不额外等待初始化失败的退避。
            self.retry_after = None;
        }
    }
    pub fn idle(&mut self) {
        self.actual = None;
        self.active = false;
        self.phase = "idle";
    }
}

#[no_mangle]
pub unsafe extern "C" fn amc_source_set_decode_policy(src: *mut amc_source_t, policy: u32) -> i32 {
    status(domain(), src as usize, || {
        am_sdk_support::callbacks::reject_reentry()?;
        let _active = domain().active(src as usize)?;
        if policy != AM_DHW_PREFER_HW as u32 && policy != AM_DHW_SOFTWARE_ONLY as u32 {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        // 与reuse领取所有权同锁，不能在检查单拥有者后又插入共享拥有者。
        let _registry = registry().lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        let entry = get(src as usize)?;
        let source = entry.data::<Source>()?;
        if source.shared.isapi {
            return Err(AM_ERR_CAMERA_MODE_CONFLICT);
        }
        let mut state = source
            .shared
            .decode
            .lock()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        if state.policy == policy {
            return Ok(());
        }
        if state.locked || source.shared.owners.load(Ordering::Acquire) != 1 {
            return Err(AM_ERR_CAMERA_MODE_CONFLICT);
        }
        state.policy = policy;
        state.effective = policy;
        Ok(())
    })
}

pub(crate) fn owner(key: usize) -> Result<Arc<CameraState>> {
    Ok(get(key)?.data::<Source>()?.shared.state.clone())
}
pub(crate) fn report(key: usize) -> Result<serde_json::Value> {
    let entry = get(key)?;
    let source = entry.data::<Source>()?;
    let state = source
        .shared
        .decode
        .lock()
        .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
    // 关闭的一个拥有者不能观察其他拥有者后续的活动状态。
    let closed = source.closed.load(Ordering::Acquire);
    let hidden = closed || source.closing.load(Ordering::Acquire);
    Ok(serde_json::json!({
        "schema":"framebraid.decode-hardware/1", "kind":"source",
        "state":if closed {"closed"} else if hidden {"closing"} else {state.phase},
        "selection":if hidden {None} else {state.selection.as_deref()},
        "actual_backend":if hidden {None} else {state.actual},
        "last_failure":if hidden {None} else {state.failure},
        "last_status":if hidden {None} else {state.last_status},
        "stream_verified":!hidden && state.active && state.actual.is_some(),
        "requested_policy":state.policy,
        "effective_policy":if hidden {state.policy} else {state.effective},
        "policy_locked":state.locked, "decoder_active":!hidden && state.active,
        "source_id":source.shared.source_id,
        "decode_generation":state.generation,
        "source_generation":source.shared.generation.load(Ordering::Acquire),
        "config_generation":source.shared.config_generation.load(Ordering::Acquire)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_failure_cache_is_bounded_by_time_and_configuration() {
        let mut state = DecodeState::default();
        let now = Instant::now();
        assert_eq!(state.prepare(1, now), Some(AM_DHW_PREFER_HW as u32));
        state.failed(DecodeError::Runtime(-5), true, now);
        assert_eq!(state.prepare(1, now), Some(AM_DHW_SOFTWARE_ONLY as u32));
        assert_eq!(
            state.prepare(1, now + Duration::from_secs(31)),
            Some(AM_DHW_PREFER_HW as u32)
        );
        state.failed(DecodeError::Runtime(-5), true, now);
        assert_eq!(state.prepare(2, now), Some(AM_DHW_PREFER_HW as u32));
        state.failed(DecodeError::AllocationFailed, false, now);
        assert_eq!(state.prepare(2, now), None);
        assert!(state.prepare(2, now + Duration::from_secs(2)).is_some());
    }
}
