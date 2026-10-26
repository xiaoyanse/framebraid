//! 临时强引用快照只占固定栈空间，不在状态/统计热路径分配堆内存。
use am_sdk_support::abi::SDK_MAX_REGISTERED_HANDLES;
use std::sync::Arc;

pub(super) const CAPACITY: usize = SDK_MAX_REGISTERED_HANDLES as usize;
pub(super) struct Snapshot<T> {
    rows: [Option<Arc<T>>; CAPACITY],
    len: usize,
}
impl<T> Default for Snapshot<T> {
    fn default() -> Self {
        Self {
            rows: std::array::from_fn(|_| None),
            len: 0,
        }
    }
}
impl<T> Snapshot<T> {
    pub(super) fn push(&mut self, value: Arc<T>) {
        // 所有调用方均持登记锁遍历原域4096槽，不能静默截断快照。
        assert!(self.len < CAPACITY);
        self.rows[self.len] = Some(value);
        self.len += 1;
    }
    pub(super) fn iter(&self) -> impl Iterator<Item = &Arc<T>> {
        self.rows[..self.len].iter().flatten()
    }
    pub(super) fn for_each_unique<K: Ord>(
        &mut self,
        key: impl Fn(&T) -> K,
        mut visit: impl FnMut(&Arc<T>),
    ) {
        // unstable原地排序不分配临时数组，复杂度O(n log n)。
        self.rows[..self.len].sort_unstable_by(|left, right| {
            left.as_ref()
                .map(|value| key(value))
                .cmp(&right.as_ref().map(|value| key(value)))
        });
        let mut previous = None;
        for value in self.iter() {
            let current = key(value);
            if previous.as_ref() != Some(&current) {
                visit(value);
            }
            previous = Some(current);
        }
    }
}

#[cfg(test)]
mod tests;
