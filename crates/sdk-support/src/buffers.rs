use crate::{abi::*, Result};
use std::{
    any::Any,
    sync::{Arc, Mutex},
};

pub trait Buffer: Any + Send + Sync {
    fn as_any(&self) -> &dyn Any;
}
struct Holder {
    references: u64,
    body: Arc<dyn Buffer>,
}
#[derive(Default)]
pub struct Buffers(Mutex<crate::registry::Table<Holder>>);
impl Buffers {
    pub fn idle_for_unload(&self) -> bool {
        self.0.try_lock().is_ok_and(|table| table.is_empty())
    }
    /// 缓存结果再次移交时须原子地登记或加引用，不能先get后retain。
    pub fn transfer(&self, key: usize, body: Arc<dyn Buffer>) -> Result<()> {
        let mut table = self.0.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        if let Some(holder) = table.get_mut(&key) {
            holder.references = holder
                .references
                .checked_add(1)
                .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        } else {
            if key == 0 || table.len() >= SDK_MAX_REGISTERED_HANDLES as usize {
                return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
            }
            table.insert(
                key,
                Holder {
                    references: 1,
                    body,
                },
            )?;
        }
        Ok(())
    }
    pub fn insert(&self, key: usize, body: Arc<dyn Buffer>) -> Result<()> {
        let mut table = self.0.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        if key == 0 || table.contains_key(&key) {
            return Err(AM_ERR_SDK_HANDLE_INVALID);
        }
        if table.len() >= SDK_MAX_REGISTERED_HANDLES as usize {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        table.insert(
            key,
            Holder {
                references: 1,
                body,
            },
        )?;
        Ok(())
    }
    pub fn get(&self, key: usize) -> Result<Arc<dyn Buffer>> {
        self.0
            .lock()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?
            .get(&key)
            .map(|h| h.body.clone())
            .ok_or(AM_ERR_SDK_HANDLE_INVALID)
    }
    pub fn retain(&self, key: usize) -> Result<()> {
        let mut table = self.0.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        let holder = table.get_mut(&key).ok_or(AM_ERR_SDK_HANDLE_INVALID)?;
        holder.references = holder
            .references
            .checked_add(1)
            .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        Ok(())
    }
    pub fn release(&self, key: usize) -> Result<()> {
        let dropped = {
            let mut table = self.0.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
            let holder = table.get_mut(&key).ok_or(AM_ERR_SDK_HANDLE_INVALID)?;
            holder.references -= 1;
            if holder.references == 0 {
                table.remove(&key)
            } else {
                None
            }
        };
        drop(dropped);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Value;
    impl Buffer for Value {
        fn as_any(&self) -> &dyn Any {
            self
        }
    }
    #[test]
    fn full_buffer_registry_keeps_retain_release_and_reuses_storage() {
        let buffers = Buffers::default();
        let value: Arc<dyn Buffer> = Arc::new(Value);
        for key in 1..=SDK_MAX_REGISTERED_HANDLES as usize {
            buffers.insert(key, value.clone()).unwrap();
        }
        assert_eq!(
            buffers.insert(SDK_MAX_REGISTERED_HANDLES as usize + 1, value.clone()),
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
        );
        buffers.transfer(1, value.clone()).unwrap();
        buffers.release(1).unwrap();
        assert!(buffers.get(1).is_ok());
        buffers.release(1).unwrap();
        assert!(buffers.get(1).is_err());
        buffers
            .insert(SDK_MAX_REGISTERED_HANDLES as usize + 1, value.clone())
            .unwrap();
        for key in 2..=SDK_MAX_REGISTERED_HANDLES as usize + 1 {
            buffers.release(key).unwrap();
        }
        assert!(buffers.idle_for_unload());
        let table = buffers.0.lock().unwrap();
        assert_eq!(table.capacity(), SDK_MAX_REGISTERED_HANDLES as usize);
        println!(
            "buffer_index_bytes={}",
            table.capacity() * std::mem::size_of::<(usize, Holder)>()
        );
    }
}
