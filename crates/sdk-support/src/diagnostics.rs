//! 诊断详情：source 为稳定键（键纪律），message 允许运行时构造的脱敏文本
//! （字段名/传入值/版本/合法区间），经 intern 驻留后进程生命周期保活；
//! 返回指针在所属DLL装载期间始终有效。
use crate::{handles::Domain, Result};
use std::{
    cell::Cell,
    ffi::{CStr, CString},
    sync::Mutex,
};

#[derive(Clone, Copy)]
pub struct Detail {
    pub message: &'static CStr,
    pub source: &'static CStr,
    pub fault_class: u32,
}
impl Detail {
    /// 动态详情：message 为运行时构造文本（经 intern 驻留满足 'static 寿命约定）；
    /// source 仍为稳定键——动态值只进 message，不编进键。
    pub fn dynamic(message: &str, source: &'static CStr, fault_class: u32) -> Self {
        Self {
            message: intern(message),
            source,
            fault_class,
        }
    }
}

/// 驻留运行时构造的诊断文本：按内容去重、进程生命周期保活（失败详情种类有限），
/// 使 Detail.message 可携带字段名/版本/区间等动态上下文；旧详情指针不随后续失败失效。
pub fn intern(text: &str) -> &'static CStr {
    static INTERNED: Mutex<Vec<&'static CStr>> = Mutex::new(Vec::new());
    let text = text.split('\0').next().unwrap_or("");
    let mut pool = match INTERNED.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(found) = pool.iter().find(|item| item.to_bytes() == text.as_bytes()) {
        return found;
    }
    let leaked: &'static CStr = match CString::new(text) {
        Ok(value) => Box::leak(value.into_boxed_c_str()),
        // split('\0') 已排除内嵌 NUL；此分支不可达，兜底空串。
        Err(_) => c"",
    };
    pool.push(leaked);
    leaked
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
    fn intern_dedupes_by_content_and_keeps_pointers_stable() {
        let first = intern("avcodec-63.dll reports major 62");
        let again = intern("avcodec-63.dll reports major 62");
        assert_eq!(first.as_ptr(), again.as_ptr());
        let other = intern("avcodec-63.dll reports major 63");
        assert_ne!(first.as_ptr(), other.as_ptr());
        // 内嵌 NUL 截断：驻留文本永远是合法 C 串。
        assert_eq!(intern("a\0b").to_bytes(), b"a");
        // 动态详情与显式驻留共享同一池；旧指针在后续驻留后仍有效。
        let dynamic = Detail::dynamic(
            "total_thread_budget=99 out of range: valid 1..=32",
            c"vision.init",
            0,
        );
        assert_eq!(
            dynamic.message.as_ptr(),
            intern("total_thread_budget=99 out of range: valid 1..=32").as_ptr()
        );
        assert_eq!(
            dynamic.message.to_bytes(),
            b"total_thread_budget=99 out of range: valid 1..=32"
        );
        assert_eq!(
            unsafe { CStr::from_ptr(dynamic.message.as_ptr()) },
            dynamic.message
        );
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
