//! Rust静态变量不自动析构；仅让已排空的DLL终止钩子回收自己的静态堆。
use std::{cell::UnsafeCell, sync::OnceLock};

pub struct UnloadCell<T>(UnsafeCell<OnceLock<T>>);
// 正常访问与OnceLock一致；唯一可变入口必须由调用方保证模块终止独占。
unsafe impl<T: Send + Sync> Sync for UnloadCell<T> {}
unsafe impl<T: Send> Send for UnloadCell<T> {}
impl<T> UnloadCell<T> {
    pub const fn new() -> Self {
        Self(UnsafeCell::new(OnceLock::new()))
    }
    pub fn get(&self) -> Option<&T> {
        unsafe { &*self.0.get() }.get()
    }
    pub fn get_or_init(&self, create: impl FnOnce() -> T) -> &T {
        unsafe { &*self.0.get() }.get_or_init(create)
    }
    /// # Safety
    /// 仅在模块即将终止、全部调用已退出且不会再进入时调用；condition须非阻塞，
    /// 并保证T析构不关闭库、等待线程或调用外部回调。不是普通shutdown入口。
    pub unsafe fn clear_if(&self, condition: impl FnOnce(&T) -> bool) -> bool {
        let slot = unsafe { &mut *self.0.get() };
        if slot.get().is_some_and(|value| !condition(value)) {
            return false;
        }
        drop(slot.take());
        true
    }
}
impl<T> Default for UnloadCell<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    struct Count(Arc<AtomicUsize>);
    impl Drop for Count {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::AcqRel);
        }
    }
    #[test]
    fn module_cleanup_requires_idle_and_drops_exactly_once() {
        let count = Arc::new(AtomicUsize::new(0));
        let slot = UnloadCell::new();
        slot.get_or_init(|| Count(count.clone()));
        assert!(!unsafe { slot.clear_if(|_| false) });
        assert_eq!(count.load(Ordering::Acquire), 0);
        assert!(slot.get().is_some());
        assert!(unsafe { slot.clear_if(|_| true) });
        assert_eq!(count.load(Ordering::Acquire), 1);
        assert!(unsafe { slot.clear_if(|_| true) });
        drop(slot);
        assert_eq!(count.load(Ordering::Acquire), 1);
    }
    #[test]
    fn closed_but_undestroyed_objects_are_not_unload_ready() {
        struct Object;
        impl crate::handles::Managed for Object {
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
            fn is_context(&self) -> bool {
                true
            }
        }
        let domain = crate::handles::Domain::new(false);
        assert!(domain.idle_for_unload());
        let key = domain.insert(0, Arc::new(Object)).unwrap();
        assert!(!domain.idle_for_unload());
        domain.close(key, 0).unwrap();
        assert!(!domain.idle_for_unload());
        domain.destroy(key).unwrap();
        assert!(domain.idle_for_unload());
    }
    #[test]
    fn outstanding_buffers_keep_domain_not_ready() {
        struct Buffer;
        impl crate::buffers::Buffer for Buffer {
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
        }
        let domain = crate::handles::Domain::new(false);
        domain.buffers.insert(1, Arc::new(Buffer)).unwrap();
        assert!(!domain.idle_for_unload());
        domain.buffers.release(1).unwrap();
        assert!(domain.idle_for_unload());
    }
}
