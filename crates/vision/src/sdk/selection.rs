//! 有序允许列表只在加载时选择；不测速、不重放运行中的请求。
use crate::{Error, Result};
use serde::Serialize;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct BackendTarget {
    pub backend: u32,
    pub device_id: u32,
}
#[derive(Clone, Debug)]
pub struct BackendSelectionPolicy {
    pub priority: Vec<BackendTarget>,
    pub allow_fallback: bool,
    pub budget_ms: u32,
}
impl BackendSelectionPolicy {
    pub fn validate(&self) -> Result<()> {
        if self.priority.is_empty()
            || self.priority.len() > 3
            || self.budget_ms == 0
            || self.budget_ms > 5000
        {
            return Err(Error("sdk.invalidArgument".into()));
        }
        for (i, target) in self.priority.iter().enumerate() {
            if target.backend > 2
                || target.device_id > i32::MAX as u32
                || target.backend != 2 && target.device_id != 0
                || self.priority[..i].contains(target)
            {
                return Err(Error("sdk.invalidArgument".into()));
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct BackendAttempt {
    pub target: BackendTarget,
    pub stage: &'static str,
    pub error: Option<String>,
    pub native_code: Option<i32>,
    pub elapsed_us: u64,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct BackendSelectionReport {
    pub candidates: Vec<BackendTarget>,
    pub allow_fallback: bool,
    pub budget_ms: Option<u32>,
    pub selected: Option<BackendTarget>,
    pub attempts: Vec<BackendAttempt>,
    pub elapsed_ms: u64,
    pub soft_budget_exceeded: bool,
    pub model_accepted: bool,
    pub native_runs: u64,
    pub delivered_runs: u64,
    pub native_failures: u64,
    pub last_native_run_succeeded: Option<bool>,
    pub last_native_error_code: Option<i32>,
    pub placement_verified: bool,
}
#[derive(Debug, thiserror::Error)]
#[error("{error}")]
pub struct BackendSelectionError {
    pub error: Error,
    pub report: Box<BackendSelectionReport>,
}
pub type SelectionResult<T> = std::result::Result<T, BackendSelectionError>;
pub(super) struct AttemptError {
    pub error: Error,
    pub stage: &'static str,
    pub native_code: Option<i32>,
}
impl AttemptError {
    pub fn key(key: &'static str, stage: &'static str) -> Self {
        Self {
            error: Error(key.into()),
            stage,
            native_code: None,
        }
    }
    pub fn native<R>(error: ort::Error<R>, key: &'static str, stage: &'static str) -> Self {
        let code: ort::sys::OrtErrorCode = error.code().into();
        // 原生消息可能含模型路径；诊断只保存稳定键和代码。
        Self {
            error: Error(key.into()),
            stage,
            native_code: Some(code as i32),
        }
    }
}
pub(super) fn report(policy: &BackendSelectionPolicy, legacy: bool) -> BackendSelectionReport {
    BackendSelectionReport {
        candidates: policy.priority.clone(),
        allow_fallback: policy.allow_fallback,
        budget_ms: (!legacy).then_some(policy.budget_ms),
        ..Default::default()
    }
}
pub(super) fn choose<T>(
    policy: &BackendSelectionPolicy,
    legacy: bool,
    report: &mut BackendSelectionReport,
    mut create: impl FnMut(BackendTarget) -> std::result::Result<T, AttemptError>,
) -> Result<T> {
    let start = Instant::now();
    let budget = Duration::from_millis(u64::from(policy.budget_ms));
    for target in &policy.priority {
        if !legacy && start.elapsed() >= budget {
            report.soft_budget_exceeded = true;
            return Err(Error("sdk.deadlineExceeded".into()));
        }
        let at = Instant::now();
        let result = create(*target);
        report.elapsed_ms = start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        report.soft_budget_exceeded = !legacy && start.elapsed() > budget;
        let elapsed_us = at.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
        match result {
            Ok(value) => {
                report.selected = Some(*target);
                report.attempts.push(BackendAttempt {
                    target: *target,
                    stage: "session_created",
                    error: None,
                    native_code: None,
                    elapsed_us,
                });
                return Ok(value);
            }
            Err(failure) => {
                let stop = !policy.allow_fallback
                    || failure.error.0 != "vision.backendStrictUnavailable"
                        && !(failure.error.0 == "vision.backendConflict" && !legacy);
                report.attempts.push(BackendAttempt {
                    target: *target,
                    stage: failure.stage,
                    error: Some(failure.error.0.clone()),
                    native_code: failure.native_code,
                    elapsed_us,
                });
                if stop || report.attempts.len() == policy.priority.len() {
                    return Err(failure.error);
                }
            }
        }
    }
    Err(Error("sdk.invalidArgument".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy() -> BackendSelectionPolicy {
        BackendSelectionPolicy {
            priority: vec![
                BackendTarget {
                    backend: 2,
                    device_id: 0,
                },
                BackendTarget {
                    backend: 0,
                    device_id: 0,
                },
            ],
            allow_fallback: true,
            budget_ms: 1000,
        }
    }
    #[test]
    fn allowlist_strict_errors_and_legacy_conflicts_are_distinct() {
        let mut p = policy();
        let mut r = report(&p, false);
        let value = choose(&p, false, &mut r, |t| {
            if t.backend == 0 {
                Ok(42)
            } else {
                Err(AttemptError::key("vision.backendConflict", "domain"))
            }
        })
        .unwrap();
        assert_eq!(value, 42);
        assert_eq!(r.attempts.len(), 2);
        let mut r = report(&p, true);
        assert_eq!(
            choose::<()>(&p, true, &mut r, |_| Err(AttemptError::key(
                "vision.backendConflict",
                "domain"
            )))
            .unwrap_err()
            .0,
            "vision.backendConflict"
        );
        p.allow_fallback = false;
        let mut r = report(&p, false);
        assert!(choose::<()>(&p, false, &mut r, |_| Err(AttemptError::key(
            "vision.backendStrictUnavailable",
            "provider"
        )))
        .is_err());
        assert_eq!(r.attempts.len(), 1);
        p.allow_fallback = true;
        let mut r = report(&p, false);
        assert!(choose::<()>(&p, false, &mut r, |_| Err(AttemptError::key(
            "vision.modelLoadFailed",
            "model"
        )))
        .is_err());
        assert_eq!(r.attempts.len(), 1);
        p.priority.truncate(1);
        let mut r = report(&p, false);
        assert!(choose::<()>(&p, false, &mut r, |_| Err(AttemptError::key(
            "vision.backendStrictUnavailable",
            "provider"
        )))
        .is_err());
        assert!(r.attempts.iter().all(|a| a.target.backend != 0));
    }
    #[test]
    fn invalid_and_duplicate_targets_fail_before_native_work() {
        let mut p = policy();
        p.priority.push(p.priority[0]);
        assert!(p.validate().is_err());
        p = policy();
        p.priority[1].device_id = 1;
        assert!(p.validate().is_err());
        p = policy();
        p.budget_ms = 0;
        assert!(p.validate().is_err());
        p = policy();
        p.priority.clear();
        assert!(p.validate().is_err());
    }
    #[test]
    fn soft_budget_stops_before_another_candidate_and_never_claims_hard_timeout() {
        let mut p = policy();
        p.budget_ms = 1;
        let mut r = report(&p, false);
        let result = choose::<()>(&p, false, &mut r, |_| {
            std::thread::sleep(Duration::from_millis(10));
            Err(AttemptError::key(
                "vision.backendStrictUnavailable",
                "provider_registration",
            ))
        });
        assert_eq!(result.unwrap_err().0, "sdk.deadlineExceeded");
        assert_eq!(r.attempts.len(), 1);
        assert!(r.soft_budget_exceeded);
        assert!(r.selected.is_none());
    }
}
