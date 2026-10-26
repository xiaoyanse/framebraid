//! 域级基础索引沿用冻结项数，一次预留后不扩容；外部载荷不在此重复计费。
use crate::{abi::*, Result};
pub(crate) struct Table<V>(Vec<(usize, V)>);
impl<V> Default for Table<V> {
    fn default() -> Self {
        Self(Vec::new())
    }
}
impl<V> Table<V> {
    pub fn prepare(&mut self) -> Result<()> {
        if self.0.capacity() == 0 {
            self.0
                .try_reserve_exact(SDK_MAX_REGISTERED_HANDLES as usize)
                .map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        }
        Ok(())
    }
    fn search(&self, key: &usize) -> std::result::Result<usize, usize> {
        self.0.binary_search_by_key(key, |(key, _)| *key)
    }
    pub fn get(&self, key: &usize) -> Option<&V> {
        self.search(key).ok().map(|i| &self.0[i].1)
    }
    pub fn get_mut(&mut self, key: &usize) -> Option<&mut V> {
        self.search(key).ok().map(|i| &mut self.0[i].1)
    }
    pub fn contains_key(&self, key: &usize) -> bool {
        self.search(key).is_ok()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    #[cfg(test)]
    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.0.iter().map(|(_, value)| value)
    }
    pub fn after(&self, key: usize) -> impl Iterator<Item = &(usize, V)> {
        let start = self.0.partition_point(|(current, _)| *current <= key);
        self.0[start..].iter()
    }
    pub fn insert(&mut self, key: usize, value: V) -> Result<Option<V>> {
        let at = match self.search(&key) {
            Ok(at) => return Ok(Some(std::mem::replace(&mut self.0[at].1, value))),
            Err(at) => at,
        };
        if self.len() == SDK_MAX_REGISTERED_HANDLES as usize {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        self.prepare()?;
        self.0.insert(at, (key, value));
        Ok(None)
    }
    pub fn remove(&mut self, key: &usize) -> Option<V> {
        self.search(key).ok().map(|at| self.0.remove(at).1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn insertion_and_lookup_storage_tradeoff_is_measured_without_speed_assertion() {
        use std::{collections::HashMap, time::Instant};
        for round in 0..3 {
            let mut old = HashMap::new();
            let at = Instant::now();
            for n in 0..4096usize {
                let key = n.wrapping_mul(2654435761) % 4096 + 1;
                old.insert(key, n);
            }
            let old_time = at.elapsed().as_nanos();
            let mut new = Table::default();
            let at = Instant::now();
            for n in 0..4096usize {
                let key = n.wrapping_mul(2654435761) % 4096 + 1;
                new.insert(key, n).unwrap();
            }
            let new_time = at.elapsed().as_nanos();
            for (key, value) in &old {
                assert_eq!(new.get(key), Some(value));
            }
            println!("insert_round={} items=4096 old_hash_capacity={} new_sorted_capacity={} old_total_ns={} new_total_ns={}",round,old.capacity(),new.capacity(),old_time,new_time);
        }
    }
    #[test]
    fn full_table_keeps_storage_and_existing_values_on_refusal() {
        let mut table = Table::default();
        for key in (1..=SDK_MAX_REGISTERED_HANDLES as usize).rev() {
            table.insert(key, key).unwrap();
        }
        let capacity = table.0.capacity();
        assert_eq!(capacity, SDK_MAX_REGISTERED_HANDLES as usize);
        assert_eq!(
            table.insert(capacity + 1, 99),
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
        );
        assert_eq!(table.insert(5, 99), Ok(Some(5)));
        assert_eq!(table.get(&5), Some(&99));
        assert_eq!(table.remove(&5), Some(99));
        table.insert(capacity + 1, 7).unwrap();
        assert_eq!(
            table
                .after(capacity)
                .map(|(key, _)| *key)
                .collect::<Vec<_>>(),
            vec![capacity + 1]
        );
        for key in 1..=capacity + 1 {
            table.remove(&key);
        }
        assert!(table.is_empty());
        assert_eq!(table.0.capacity(), capacity);
        println!(
            "registry_pair_bytes={} registry_slots={} registry_index_bytes={}",
            std::mem::size_of::<(usize, usize)>(),
            capacity,
            capacity * std::mem::size_of::<(usize, usize)>()
        );
    }
}
