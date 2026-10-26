//! Context基础登记固定内联，避免哈希表扩容及关闭期间临时分配。
use crate::{abi::*, handles::Managed, Result};
use std::{
    any::{Any, TypeId},
    sync::{Arc, Mutex, Weak},
};

type Value = Arc<dyn Any + Send + Sync>;
type Slot = Option<(TypeId, Value)>;
const SERVICE_SLOTS: usize = 16;
const CLOSE_SLOTS: usize = 8;

pub struct Services([Slot; SERVICE_SLOTS]);
impl Default for Services {
    fn default() -> Self {
        Self(std::array::from_fn(|_| None))
    }
}
impl Services {
    pub fn get(&self, id: &TypeId) -> Option<&Value> {
        self.0
            .iter()
            .flatten()
            .find(|(key, _)| key == id)
            .map(|(_, value)| value)
    }
    /// 必须先有空槽再构造服务；失败不能留下未登记的工作或拥有者。
    pub fn get_or_insert_with(
        &mut self,
        id: TypeId,
        make: impl FnOnce() -> Value,
    ) -> Result<Value> {
        self.try_get_or_insert_with(id, || Ok(make()))
    }
    pub fn try_get_or_insert_with(
        &mut self,
        id: TypeId,
        make: impl FnOnce() -> Result<Value>,
    ) -> Result<Value> {
        if let Some(value) = self.get(&id) {
            return Ok(value.clone());
        }
        let slot = self
            .0
            .iter_mut()
            .find(|slot| slot.is_none())
            .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        let value = make()?;
        *slot = Some((id, value.clone()));
        Ok(value)
    }
}

pub struct CloseHooks(Mutex<[Option<Weak<dyn Managed>>; CLOSE_SLOTS]>);
impl Default for CloseHooks {
    fn default() -> Self {
        Self(Mutex::new(std::array::from_fn(|_| None)))
    }
}
impl CloseHooks {
    pub fn register(&self, value: Weak<dyn Managed>) -> Result<()> {
        let mut slots = self.0.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        for slot in slots.iter_mut() {
            if slot.as_ref().is_some_and(|v| v.strong_count() == 0) {
                *slot = None;
            }
            if slot.as_ref().is_some_and(|v| Weak::ptr_eq(v, &value)) {
                return Ok(());
            }
        }
        let slot = slots
            .iter_mut()
            .find(|slot| slot.is_none())
            .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        *slot = Some(value);
        Ok(())
    }
    pub(crate) fn request_close(&self) {
        let items: [Option<Arc<dyn Managed>>; CLOSE_SLOTS] = match self.0.lock() {
            Ok(slots) => std::array::from_fn(|i| slots[i].as_ref().and_then(Weak::upgrade)),
            Err(_) => return,
        };
        // 回调允许重入登记；持有的强引用只活到本次关闭通知结束。
        for item in items.into_iter().flatten() {
            item.request_close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    fn id<const N: usize>() -> TypeId {
        TypeId::of::<[u8; N]>()
    }
    #[test]
    fn fixed_service_slots_reuse_and_reject_before_construction() {
        let mut values = Services::default();
        let ids = [
            id::<0>(),
            id::<1>(),
            id::<2>(),
            id::<3>(),
            id::<4>(),
            id::<5>(),
            id::<6>(),
            id::<7>(),
            id::<8>(),
            id::<9>(),
            id::<10>(),
            id::<11>(),
            id::<12>(),
            id::<13>(),
            id::<14>(),
            id::<15>(),
        ];
        for (n, key) in ids.into_iter().enumerate() {
            values.get_or_insert_with(key, || Arc::new(n)).unwrap();
        }
        assert_eq!(values.0.iter().flatten().count(), SERVICE_SLOTS);
        assert!(matches!(
            values.get_or_insert_with(id::<16>(), || panic!("must not construct rejected service")),
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
        ));
        let old = values.get(&id::<0>()).unwrap().clone();
        let again = values
            .get_or_insert_with(id::<0>(), || panic!("must not replace service"))
            .unwrap();
        assert!(Arc::ptr_eq(&old, &again));
        println!("services_inline_bytes={}", std::mem::size_of::<Services>());
    }
    #[test]
    fn failed_service_setup_is_not_published_and_can_be_retried() {
        let mut values = Services::default();
        assert!(matches!(
            values.try_get_or_insert_with(id::<0>(), || Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)),
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
        ));
        assert!(values.get(&id::<0>()).is_none());
        let value = values
            .get_or_insert_with(id::<0>(), || Arc::new(42u32))
            .unwrap();
        assert_eq!(*value.downcast_ref::<u32>().unwrap(), 42);
    }
    struct Hook {
        calls: AtomicUsize,
        owner: Weak<CloseHooks>,
    }
    impl Managed for Hook {
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn request_close(&self) {
            self.calls.fetch_add(1, Ordering::AcqRel);
            // 若request_close仍持锁，此处会死锁。
            assert!(self.owner.upgrade().unwrap().0.try_lock().is_ok());
        }
    }
    #[test]
    fn hooks_are_bounded_deduplicated_and_dead_slots_reused_without_locking_callbacks() {
        let hooks = Arc::new(CloseHooks::default());
        let mut owners: Vec<Arc<dyn Managed>> = Vec::new();
        for _ in 0..CLOSE_SLOTS {
            let value: Arc<dyn Managed> = Arc::new(Hook {
                calls: AtomicUsize::new(0),
                owner: Arc::downgrade(&hooks),
            });
            hooks.register(Arc::downgrade(&value)).unwrap();
            hooks.register(Arc::downgrade(&value)).unwrap();
            owners.push(value);
        }
        let extra: Arc<dyn Managed> = Arc::new(Hook {
            calls: AtomicUsize::new(0),
            owner: Arc::downgrade(&hooks),
        });
        assert_eq!(
            hooks.register(Arc::downgrade(&extra)),
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
        );
        hooks.request_close();
        assert!(owners.iter().all(|v| v
            .as_any()
            .downcast_ref::<Hook>()
            .unwrap()
            .calls
            .load(Ordering::Acquire)
            == 1));
        owners.pop();
        hooks.register(Arc::downgrade(&extra)).unwrap();
        hooks.request_close();
        assert_eq!(
            extra
                .as_any()
                .downcast_ref::<Hook>()
                .unwrap()
                .calls
                .load(Ordering::Acquire),
            1
        );
        println!("hooks_inline_bytes={}", std::mem::size_of::<CloseHooks>());
    }
}
