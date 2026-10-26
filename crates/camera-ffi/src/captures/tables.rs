//! 抓图的小型有界索引，保留容量与增长峰值均先占控制字节。
use crate::frames::{CameraState, ControlCharge};
use am_sdk_support::{abi::*, Result};
use std::{mem::size_of, sync::Arc};

pub struct Table<K, V> {
    rows: Vec<(K, V)>,
    storage: Option<ControlCharge>,
}

impl<K, V> Default for Table<K, V> {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            storage: None,
        }
    }
}

impl<K: Ord, V> Table<K, V> {
    pub fn reserve(
        &mut self,
        camera: &Arc<CameraState>,
        count: usize,
        maximum: usize,
    ) -> Result<()> {
        let required = self
            .len()
            .checked_add(count)
            .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        if required > maximum {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        if required <= self.rows.capacity() {
            return Ok(());
        }
        let capacity = required
            .checked_next_power_of_two()
            .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?
            .min(maximum);
        let bytes = capacity
            .checked_mul(size_of::<(K, V)>())
            .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        let charge = camera.metadata(bytes)?;
        let mut next = Vec::new();
        next.try_reserve_exact(capacity)
            .map_err(|_| AM_ERR_SDK_OUT_OF_MEMORY)?;
        if next.capacity() > capacity {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        next.append(&mut self.rows);
        // 旧buffer真实释放后，才替换旧许可。
        self.rows = next;
        self.storage = Some(charge);
        Ok(())
    }
    pub fn try_insert(
        &mut self,
        camera: &Arc<CameraState>,
        key: K,
        value: V,
        maximum: usize,
    ) -> Result<()> {
        if let Some(old) = self.get_mut(&key) {
            *old = value;
        } else {
            self.reserve(camera, 1, maximum)?;
            self.insert(key, value);
        }
        Ok(())
    }
    pub fn insert(&mut self, key: K, value: V) {
        match self.rows.binary_search_by(|(id, _)| id.cmp(&key)) {
            Ok(index) => self.rows[index].1 = value,
            Err(index) => {
                assert!(self.rows.len() < self.rows.capacity());
                self.rows.insert(index, (key, value));
            }
        }
    }
    pub fn get(&self, key: &K) -> Option<&V> {
        self.rows
            .binary_search_by(|(id, _)| id.cmp(key))
            .ok()
            .map(|i| &self.rows[i].1)
    }
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let index = self.rows.binary_search_by(|(id, _)| id.cmp(key)).ok()?;
        Some(&mut self.rows[index].1)
    }
    pub fn remove(&mut self, key: &K) -> Option<V> {
        let index = self.rows.binary_search_by(|(id, _)| id.cmp(key)).ok()?;
        Some(self.rows.remove(index).1)
    }
    pub fn len(&self) -> usize {
        self.rows.len()
    }
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
    pub fn clear(&mut self) {
        self.rows.clear();
    }
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.rows.iter().map(|(_, value)| value)
    }
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.rows.iter().map(|(key, value)| (key, value))
    }
    pub fn retain(&mut self, mut keep: impl FnMut(&K, &V) -> bool) {
        self.rows.retain(|(key, value)| keep(key, value));
    }
}
#[cfg(test)]
impl<K: Ord, V> std::ops::Index<&K> for Table<K, V> {
    type Output = V;
    fn index(&self, key: &K) -> &V {
        self.get(key).unwrap()
    }
}
#[cfg(test)]
mod tests;
