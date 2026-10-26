//! 小型有界请求表：存储可计量，空闲容量持续占控制字节。
use super::state::{ControlCharge, Record, State};
use am_sdk_support::{abi::*, Result};
use std::mem::size_of;

#[derive(Default)]
pub struct Records {
    rows: Vec<(u64, Record)>,
    storage: Option<ControlCharge>,
}

impl Records {
    pub fn reserve(&mut self, state: &State, count: usize, maximum: usize) -> Result<()> {
        let required = self.len().checked_add(count).ok_or(AM_ERR_SDK_QUEUE_FULL)?;
        if required > maximum {
            return Err(AM_ERR_SDK_QUEUE_FULL);
        }
        if required <= self.rows.capacity() {
            return Ok(());
        }
        let capacity = required
            .checked_next_power_of_two()
            .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?
            .min(maximum);
        let bytes = capacity
            .checked_mul(size_of::<(u64, Record)>())
            .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        // 新存储先占额度，旧容量必须在真实释放后才出账。
        let charge = state.record_storage(bytes)?;
        let mut next = Vec::new();
        next.try_reserve_exact(capacity)
            .map_err(|_| AM_ERR_SDK_OUT_OF_MEMORY)?;
        if next.capacity() > capacity {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        next.append(&mut self.rows);
        self.rows = next;
        self.storage = Some(charge);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn get(&self, key: &u64) -> Option<&Record> {
        self.rows
            .binary_search_by_key(key, |(id, _)| *id)
            .ok()
            .map(|i| &self.rows[i].1)
    }

    pub fn get_mut(&mut self, key: &u64) -> Option<&mut Record> {
        let index = self.rows.binary_search_by_key(key, |(id, _)| *id).ok()?;
        Some(&mut self.rows[index].1)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&u64, &Record)> {
        self.rows.iter().map(|(id, record)| (id, record))
    }

    pub fn values(&self) -> impl Iterator<Item = &Record> {
        self.rows.iter().map(|(_, record)| record)
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut Record> {
        self.rows.iter_mut().map(|(_, record)| record)
    }

    pub fn insert(&mut self, key: u64, value: Record) {
        match self.rows.binary_search_by_key(&key, |(id, _)| *id) {
            Ok(index) => self.rows[index].1 = value,
            Err(index) => {
                // 生产入口只在reserve整批成功后插入，禁止Vec隐式扩容漏计。
                assert!(self.rows.len() < self.rows.capacity());
                self.rows.insert(index, (key, value));
            }
        }
    }

    pub fn remove(&mut self, key: &u64) -> Option<Record> {
        let index = self.rows.binary_search_by_key(key, |(id, _)| *id).ok()?;
        Some(self.rows.remove(index).1)
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&u64, &Record) -> bool) {
        self.rows.retain(|(id, record)| keep(id, record));
    }

    #[cfg(test)]
    pub(super) fn isolated(rows: impl IntoIterator<Item = (u64, Record)>) -> Self {
        // 只用于不涉及分配预算的取消/竞争单测，真实分配另测。
        let mut rows: Vec<_> = rows.into_iter().collect();
        rows.sort_by_key(|(id, _)| *id);
        rows.reserve(1);
        Self {
            rows,
            storage: None,
        }
    }
}

#[cfg(test)]
mod tests;
