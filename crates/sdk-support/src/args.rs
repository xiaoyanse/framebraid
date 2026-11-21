use crate::{abi::*, Result};
use std::{
    ffi::{c_char, CStr},
    mem::size_of,
};

pub fn pointer<T>(pointer: *const T) -> Result<()> {
    if pointer.is_null() {
        return Err(AM_ERR_SDK_NULL_ARGUMENT);
    }
    if !(pointer as usize).is_multiple_of(std::mem::align_of::<T>()) {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    Ok(())
}

/// # Safety
/// pointer首字段可读，且调用方保证声明的输出内存可写。
pub unsafe fn validate_output<T>(pointer: *mut T, minimum: usize) -> Result<()> {
    if pointer.is_null() {
        return Err(AM_ERR_SDK_NULL_ARGUMENT);
    }
    if !(pointer as usize).is_multiple_of(std::mem::align_of::<T>()) {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    let size = unsafe { pointer.cast::<u32>().read() } as usize;
    if size < minimum || size > 65536 {
        return Err(AM_ERR_SDK_STRUCT_SIZE_INVALID);
    }
    Ok(())
}

/// # Safety
/// 指针须指向调用方声明的可读结构；不能以此拦截任意野指针。
pub unsafe fn read<T: Copy + Default>(pointer: *const T, minimum: usize) -> Result<T> {
    let size = unsafe { read_size(pointer, minimum) }?;
    Ok(unsafe { copy_prefix(pointer, size.min(size_of::<T>())) })
}

/// 在旧前缀与当前完整结构之间，只解释已发布的旧前缀；不读取部分尾字段。
/// # Safety
/// `T` 是首字段为 `u32 struct_size` 的 C 结构。`previous` 是与 `T` 布局兼容的
/// 完整旧结构大小，调用方提供声明大小的可读内存。
pub unsafe fn read_versioned<T: Copy + Default>(pointer: *const T, previous: usize) -> Result<T> {
    let size = unsafe { read_size(pointer, previous) }?;
    let current = size_of::<T>();
    let readable = if size < current { previous } else { current };
    Ok(unsafe { copy_prefix(pointer, readable) })
}

unsafe fn read_size<T>(input: *const T, minimum: usize) -> Result<usize> {
    pointer(input)?;
    let size = unsafe { input.cast::<u32>().read() } as usize;
    if !(minimum..=65536).contains(&size) {
        return Err(AM_ERR_SDK_STRUCT_SIZE_INVALID);
    }
    Ok(size)
}

unsafe fn copy_prefix<T: Copy + Default>(pointer: *const T, bytes: usize) -> T {
    let mut value = T::default();
    unsafe {
        std::ptr::copy_nonoverlapping(
            pointer.cast::<u8>(),
            (&mut value as *mut T).cast::<u8>(),
            bytes,
        );
    }
    value
}

/// # Safety
/// pointer须指向至少struct_size字节可写内存，首字段已由调用方初始化。
pub unsafe fn write<T: Copy + AbiLayout>(pointer: *mut T, value: T) -> Result<()> {
    if pointer.is_null() {
        return Err(AM_ERR_SDK_NULL_ARGUMENT);
    }
    if !(pointer as usize).is_multiple_of(std::mem::align_of::<T>()) {
        return Err(AM_ERR_SDK_INVALID_ARGUMENT);
    }
    let size = unsafe { pointer.cast::<u32>().read() } as usize;
    if !(4..=65536).contains(&size) {
        return Err(AM_ERR_SDK_STRUCT_SIZE_INVALID);
    }
    unsafe {
        std::ptr::write_bytes(pointer.cast::<u8>(), 0, size.min(size_of::<T>()));
        for &(offset, length) in T::FIELDS {
            if offset + length <= size {
                std::ptr::copy_nonoverlapping(
                    (&value as *const T).cast::<u8>().add(offset),
                    pointer.cast::<u8>().add(offset),
                    length,
                );
            }
        }
        pointer.cast::<u32>().write(size as u32);
    }
    Ok(())
}

/// # Safety
/// 非空字符串须指向有界NUL结尾内存；上限以内的字节必须可读。
pub unsafe fn string(pointer: *const c_char) -> Result<String> {
    if pointer.is_null() {
        return Err(AM_ERR_SDK_NULL_ARGUMENT);
    }
    for length in 0..SDK_MAX_STRING_BYTES as usize {
        if unsafe { pointer.add(length).read() } == 0 {
            let bytes = unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), length + 1) };
            return CStr::from_bytes_with_nul(bytes)
                .map_err(|_| AM_ERR_SDK_INVALID_ARGUMENT)?
                .to_str()
                .map(str::to_owned)
                .map_err(|_| AM_ERR_SDK_UTF8_INVALID);
        }
    }
    Err(AM_ERR_SDK_INVALID_ARGUMENT)
}

// 仅借用输入完成全量校验；先取得域准入，再分配目录副本。
pub(crate) struct RuntimeDirectories<'a> {
    values: [&'a str; SDK_MAX_RUNTIME_DIRECTORIES as usize],
    count: usize,
}
impl RuntimeDirectories<'_> {
    pub(crate) fn copy_owned(&self) -> Result<Vec<String>> {
        let mut result = Vec::new();
        result
            .try_reserve_exact(self.count)
            .map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        if result.capacity() != self.count {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        for directory in &self.values[..self.count] {
            let mut value = String::new();
            value
                .try_reserve_exact(directory.len())
                .map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
            if value.capacity() != directory.len() {
                return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
            }
            value.push_str(directory);
            result.push(value);
        }
        Ok(result)
    }
}
/// # Safety
/// 指针数组和字符串须保持可读且不被修改，直到借用及复制完成。
pub(crate) unsafe fn runtime_directories<'a>(
    pointer: *const *const c_char,
    count: u32,
) -> Result<RuntimeDirectories<'a>> {
    if count > SDK_MAX_RUNTIME_DIRECTORIES as u32 {
        crate::common::init_note(format!(
            "runtime_dir_count={} out of range: valid 0..={}",
            count, SDK_MAX_RUNTIME_DIRECTORIES
        ));
        return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
    }
    if count > 0 {
        self::pointer(pointer)?;
    }
    let mut result = RuntimeDirectories {
        values: [""; SDK_MAX_RUNTIME_DIRECTORIES as usize],
        count: count as usize,
    };
    for i in 0..count as usize {
        let pointer = unsafe { pointer.add(i).read() };
        self::pointer(pointer)?;
        let mut found = None;
        for length in 0..SDK_MAX_STRING_BYTES as usize {
            if unsafe { pointer.add(length).read() } == 0 {
                let bytes = unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), length) };
                found = Some(std::str::from_utf8(bytes).map_err(|_| AM_ERR_SDK_UTF8_INVALID)?);
                break;
            }
        }
        let directory = found.ok_or(AM_ERR_SDK_INVALID_ARGUMENT)?;
        if !std::path::Path::new(directory).is_absolute() {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        result.values[i] = directory;
    }
    Ok(result)
}
/// # Safety
/// 指针数组及字符串由调用方保持到函数返回。
pub unsafe fn runtime_dirs(pointer: *const *const c_char, count: u32) -> Result<Vec<String>> {
    unsafe { runtime_directories(pointer, count) }?.copy_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn limits_legacy_buffers_work_in_both_domains() {
        use crate::{common, context, handles::Domain};

        const OLD: usize = 160;
        const NEW: usize = 168;
        const CANARY: u64 = 0xa5a5_a5a5_a5a5_a5a5;
        assert_eq!(
            std::mem::offset_of!(am_limits_t, device_resident_budget_bytes),
            OLD
        );
        assert_eq!(size_of::<am_limits_t>(), NEW);
        let camera = amc_init_params_t {
            abi_version: AM_ABI_VERSION,
            ..Default::default()
        };
        let vision_params = amv_init_params_t {
            abi_version: AM_ABI_VERSION,
            ..Default::default()
        };
        for vision in [false, true] {
            let domain = Domain::new(vision);
            let ctx = unsafe {
                if vision {
                    context::initialize_vision(&domain, &vision_params)
                } else {
                    context::initialize_camera(&domain, &camera)
                }
            }
            .unwrap();
            let mut current = am_limits_t::default();
            unsafe { common::limits(&domain, ctx, &mut current) }.unwrap();
            current.device_resident_budget_bytes = 4096;
            unsafe { common::set_limits(&domain, ctx, &current) }.unwrap();

            let mut words = vec![CANARY; 8195];
            let buffer = unsafe { words.as_mut_ptr().add(1) }.cast::<am_limits_t>();
            unsafe { buffer.cast::<u32>().write(OLD as u32) };
            unsafe { common::limits(&domain, ctx, buffer) }.unwrap();
            assert_eq!(unsafe { buffer.cast::<u32>().read() }, OLD as u32);
            assert_eq!(words[0], CANARY);
            assert_eq!(words[1 + OLD / 8], CANARY);
            unsafe { std::ptr::addr_of_mut!((*buffer).result_max_items).write(32) };
            for size in OLD..NEW {
                unsafe { buffer.cast::<u32>().write(size as u32) };
                unsafe { common::set_limits(&domain, ctx, buffer) }.unwrap();
                unsafe { common::limits(&domain, ctx, &mut current) }.unwrap();
                assert_eq!(current.result_max_items, 32);
                assert_eq!(current.device_resident_budget_bytes, 4096);
            }
            for size in [4, OLD - 1, 65537] {
                unsafe { buffer.cast::<u32>().write(size as u32) };
                assert_eq!(
                    unsafe { common::set_limits(&domain, ctx, buffer) },
                    Err(AM_ERR_SDK_STRUCT_SIZE_INVALID)
                );
            }
            assert_eq!(
                unsafe { common::set_limits(&domain, ctx, std::ptr::null()) },
                Err(AM_ERR_SDK_NULL_ARGUMENT)
            );
            unsafe {
                buffer.cast::<u32>().write((NEW + 1) as u32);
                std::ptr::addr_of_mut!((*buffer).device_resident_budget_bytes).write(2048);
                common::set_limits(&domain, ctx, buffer).unwrap();
                common::limits(&domain, ctx, &mut current).unwrap();
            }
            assert_eq!(current.device_resident_budget_bytes, 2048);
        }
    }

    #[test]
    fn output_padding_is_zero_and_short_buffer_canary_is_untouched() {
        let value = am_version_info_t {
            abi_major: 1,
            package_version_utf8: c"test".as_ptr(),
            ..Default::default()
        };
        for length in 4..=size_of::<am_version_info_t>() {
            let mut bytes = std::mem::MaybeUninit::<[u64; 16]>::uninit();
            let pointer = bytes.as_mut_ptr().cast::<u8>();
            unsafe {
                std::ptr::write_bytes(pointer, 0xa5, 128);
                pointer.cast::<u32>().write(length as u32);
                write(pointer.cast::<am_version_info_t>(), value).unwrap();
                let actual = std::slice::from_raw_parts(pointer, 128);
                assert!(actual[length..].iter().all(|b| *b == 0xa5));
                assert_eq!(pointer.cast::<u32>().read(), length as u32);
                if length >= 16 {
                    assert_eq!(&actual[12..16], &[0; 4]);
                }
            }
        }
    }
}
