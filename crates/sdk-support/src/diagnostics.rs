//! 诊断只保存静态脱敏文本；返回指针在所属DLL装载期间始终有效。
use crate::{handles::Domain, Result};
use std::{cell::Cell, ffi::CStr};

#[derive(Clone, Copy)]
pub struct Detail {
    pub message: &'static CStr,
    pub source: &'static CStr,
    pub fault_class: u32,
}
#[derive(Clone, Copy)]
struct Pending {
    domain: usize,
    key: usize,
    detail: Option<(i32, Detail)>,
}
thread_local! {
    static CURRENT: Cell<Option<Pending>> = const { Cell::new(None) };
}
pub(crate) struct Scope(Option<Pending>);
impl Scope {
    pub fn begin(domain: &Domain, key: usize) -> Self {
        Self(CURRENT.replace(Some(Pending {
            domain: domain as *const Domain as usize,
            key,
            detail: None,
        })))
    }
    pub fn detail(&self, code: i32) -> Option<Detail> {
        CURRENT
            .get()?
            .detail
            .filter(|(c, _)| *c == code)
            .map(|(_, d)| d)
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        CURRENT.set(self.0);
    }
}
pub fn record(domain: &Domain, key: usize, code: i32, detail: Detail) {
    CURRENT.with(|slot| {
        if let Some(mut pending) = slot.get() {
            if pending.domain == domain as *const Domain as usize && pending.key == key {
                pending.detail = Some((code, detail));
                slot.set(Some(pending));
                return;
            }
        }
        // 后台工作线程没有同步入口作用域，直接更新其拥有者的诊断。
        domain.record_error_detail(key, code, Some(detail));
    });
}
pub fn snapshot(domain: &Domain, key: usize) -> Result<(i32, Option<Detail>)> {
    let entry = domain.get(key)?;
    let record = entry
        .error
        .lock()
        .map_err(|_| crate::abi::AM_ERR_SDK_INTERNAL_ERROR)?;
    Ok(*record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{abi::*, handles::Managed};
    use std::{any::Any, ffi::CStr, sync::Arc};
    struct Object;
    impl Managed for Object {
        fn as_any(&self) -> &dyn Any {
            self
        }
    }
    fn detail() -> Detail {
        Detail {
            message: c"Missing avcodec-63.dll",
            source: c"camera.ffmpeg.avcodec-63.dll",
            fault_class: 6,
        }
    }
    #[test]
    fn sync_detail_is_coherent_static_and_does_not_leak_into_next_failure() {
        let domain = Domain::new(false);
        let key = domain.insert(0, Arc::new(Object)).unwrap();
        let code = AM_ERR_SDK_NOT_SUPPORTED;
        assert_eq!(
            crate::status(&domain, key, || {
                record(&domain, key, code, detail());
                Err(code)
            }),
            code
        );
        let (actual, failure) = snapshot(&domain, key).unwrap();
        assert_eq!(actual, code);
        let pointer = failure.unwrap().message.as_ptr();
        assert_eq!(crate::status(&domain, key, || Err(code)), code);
        assert!(snapshot(&domain, key).unwrap().1.is_none());
        // 旧错误输出的字符串不随后续失败失效。
        assert_eq!(unsafe { CStr::from_ptr(pointer) }, detail().message);
    }
    #[test]
    fn nested_scope_and_background_record_stay_with_their_owners() {
        let domain = Arc::new(Domain::new(false));
        let first = domain.insert(0, Arc::new(Object)).unwrap();
        let second = domain.insert(0, Arc::new(Object)).unwrap();
        let code = AM_ERR_SDK_NOT_SUPPORTED;
        crate::status(&domain, first, || {
            record(&domain, first, code, detail());
            crate::status(&domain, second, || Err(AM_ERR_SDK_INVALID_ARGUMENT));
            Err(code)
        });
        assert!(snapshot(&domain, first).unwrap().1.is_some());
        assert_eq!(
            snapshot(&domain, second).unwrap().0,
            AM_ERR_SDK_INVALID_ARGUMENT
        );
        let worker = domain.clone();
        std::thread::spawn(move || record(&worker, second, code, detail()))
            .join()
            .unwrap();
        assert!(snapshot(&domain, second).unwrap().1.is_some());
    }
}
