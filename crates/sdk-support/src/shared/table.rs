use crate::{abi::AM_ERR_SDK_RESOURCE_EXHAUSTED, Result};

/// 协议已有硬上限的小索引，内联存储避免首次登记或扩容分配。
pub struct Table<K, V, const N: usize> {
    rows: [Option<(K, V)>; N],
}
impl<K, V, const N: usize> Default for Table<K, V, N> {
    fn default() -> Self {
        Self {
            rows: std::array::from_fn(|_| None),
        }
    }
}
impl<K: PartialEq, V, const N: usize> Table<K, V, N> {
    pub fn len(&self) -> usize {
        self.rows.iter().flatten().count()
    }
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.rows.iter().flatten().map(|(_, v)| v)
    }
    pub fn get(&self, key: &K) -> Option<&V> {
        self.rows
            .iter()
            .flatten()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }
    pub fn vacant(&self) -> Result<usize> {
        self.rows
            .iter()
            .position(Option::is_none)
            .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)
    }
    pub fn commit(&mut self, slot: usize, key: K, value: V) {
        debug_assert!(self.rows[slot].is_none());
        self.rows[slot] = Some((key, value));
    }
    pub fn insert(&mut self, key: K, value: V) -> Result<()> {
        if let Some(row) = self.rows.iter_mut().flatten().find(|(k, _)| *k == key) {
            row.1 = value;
        } else {
            self.commit(self.vacant()?, key, value);
        }
        Ok(())
    }
    pub fn remove(&mut self, key: &K) -> Option<V> {
        let slot = self
            .rows
            .iter()
            .position(|row| row.as_ref().is_some_and(|(k, _)| k == key))?;
        self.rows[slot].take().map(|(_, v)| v)
    }
    pub fn retain(&mut self, mut keep: impl FnMut(&K, &V) -> bool) {
        for row in &mut self.rows {
            if row.as_ref().is_some_and(|(k, v)| !keep(k, v)) {
                row.take();
            }
        }
    }
}
