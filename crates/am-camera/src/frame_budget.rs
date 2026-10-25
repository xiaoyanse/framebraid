use crate::{Error, Result};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
// 单元测试并行线程共享进程级水位，翻倍避免普通用例被并行大占用挤兑（抖动根治）；
// 生产维持 32MiB。随 crate 迁出后 app-core 测试构建经 testkit 特性获得同样的放大，
// 大字节压力用例仍按 ignore 语义独占进程执行。
const LIMIT: usize = if cfg!(any(test, feature = "testkit")) {
    64 * 1024 * 1024
} else {
    32 * 1024 * 1024
};
static USED: AtomicUsize = AtomicUsize::new(0);
// 帧字节预算越限累计（资源账本）
static REJECTS: AtomicU64 = AtomicU64::new(0);
#[derive(Debug)]
pub struct EncodedBudget(usize);
impl EncodedBudget {
    pub fn reserve(bytes: usize) -> Result<Self> {
        USED.fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
            used.checked_add(bytes).filter(|sum| *sum <= LIMIT)
        })
        .map(|_| Self(bytes))
        .map_err(|_| {
            REJECTS.fetch_add(1, Ordering::AcqRel);
            Error::Invalid("model.encodedCapacityExceeded".into())
        })
    }
    pub fn shrink(&mut self, bytes: usize) {
        if bytes < self.0 {
            USED.fetch_sub(self.0 - bytes, Ordering::AcqRel);
            self.0 = bytes;
        }
    }
}
impl Drop for EncodedBudget {
    fn drop(&mut self) {
        USED.fetch_sub(self.0, Ordering::AcqRel);
    }
}

/// 字节和预算共享同一所有权，不能导出不带预算的 Arc。
#[derive(Debug)]
pub struct EncodedImage {
    bytes: Box<[u8]>,
    _budget: EncodedBudget,
    sha256: std::sync::OnceLock<String>,
}
pub fn used() -> usize {
    USED.load(Ordering::Acquire)
}
/// 帧字节预算上限（资源账本）。
pub fn limit() -> usize {
    LIMIT
}
/// 帧字节预算越限累计（资源账本）。
pub fn rejects() -> u64 {
    REJECTS.load(Ordering::Acquire)
}
impl EncodedImage {
    pub fn sha256(&self) -> &str {
        use sha2::{Digest, Sha256};
        self.sha256
            .get_or_init(|| format!("{:x}", Sha256::digest(&self.bytes)))
    }

    pub fn with_budget(bytes: Vec<u8>, mut budget: EncodedBudget) -> Self {
        budget.shrink(bytes.len());
        Self {
            bytes: bytes.into_boxed_slice(),
            _budget: budget,
            sha256: std::sync::OnceLock::new(),
        }
    }
    pub fn copy(bytes: &[u8]) -> Result<std::sync::Arc<Self>> {
        let budget = EncodedBudget::reserve(bytes.len())?;
        Ok(std::sync::Arc::new(Self::with_budget(
            bytes.to_vec(),
            budget,
        )))
    }
}
impl std::ops::Deref for EncodedImage {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.bytes
    }
}
impl AsRef<[u8]> for EncodedImage {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn payload_clone_cannot_escape_budget_after_frame_owner_drops() {
        let payload = EncodedImage::copy(&[7; 16]).unwrap();
        let guard = payload.clone();
        drop(payload);
        assert_eq!(guard.len(), 16);
        assert_eq!(guard._budget.0, 16);
        assert_eq!(std::sync::Arc::strong_count(&guard), 1);
        assert!(EncodedBudget::reserve(LIMIT).is_err());
        drop(guard);
    }
}
