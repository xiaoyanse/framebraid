use super::*;
use std::ffi::{c_char, CStr};

/// 调用方须让全部字符串保持可读且不变直到探测返回。
pub(super) unsafe fn borrowed<'a>(pointer: *const c_char) -> Result<&'a str> {
    if pointer.is_null() {
        return Err(AM_ERR_SDK_NULL_ARGUMENT);
    }
    for length in 0..SDK_MAX_STRING_BYTES as usize {
        if unsafe { pointer.add(length).read() } == 0 {
            let bytes = unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), length + 1) };
            return CStr::from_bytes_with_nul(bytes)
                .map_err(|_| AM_ERR_SDK_INVALID_ARGUMENT)?
                .to_str()
                .map_err(|_| AM_ERR_SDK_UTF8_INVALID);
        }
    }
    Err(AM_ERR_SDK_INVALID_ARGUMENT)
}
pub(super) fn exact_vec<T>(count: usize) -> Result<Vec<T>> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
    if values.capacity() != count {
        return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
    }
    Ok(values)
}
fn owned(value: &str) -> Result<String> {
    let mut output = String::new();
    output
        .try_reserve_exact(value.len())
        .map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
    if output.capacity() != value.len() {
        return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
    }
    output.push_str(value);
    Ok(output)
}
pub(super) struct Plan<'a> {
    pub(super) request: am_probe_request_t,
    urls: [&'a str; MAX_CANDIDATES],
    username: &'a str,
    password: &'a str,
}
impl<'a> Plan<'a> {
    pub(super) unsafe fn read(pointer: *const am_probe_request_t) -> Result<Self> {
        let request: am_probe_request_t =
            unsafe { args::read(pointer, size_of::<am_probe_request_t>()) }?;
        if request.candidate_count == 0 || request.candidate_count as usize > MAX_CANDIDATES {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        if request.force_refresh > 1 || request.timeout_ms > 60_000 {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        args::pointer(request.references_utf8)?;
        let (username, password) = if request.creds.is_null() {
            ("", "")
        } else {
            let creds: am_credential_data_t =
                unsafe { args::read(request.creds, size_of::<am_credential_data_t>()) }?;
            (unsafe { borrowed(creds.username_utf8) }?, unsafe {
                borrowed(creds.password_utf8)
            }?)
        };
        let mut urls = [""; MAX_CANDIDATES];
        for (i, url) in urls
            .iter_mut()
            .enumerate()
            .take(request.candidate_count as usize)
        {
            *url = unsafe { borrowed(request.references_utf8.add(i).read()) }?;
        }
        Ok(Self {
            request,
            urls,
            username,
            password,
        })
    }
    pub(super) fn result_bytes(&self) -> usize {
        let count = self.request.candidate_count as usize;
        size_of::<ProbeResult>()
            + 2 * size_of::<usize>()
            + size_of::<am_probe_result_t>()
            + count * (size_of::<CString>() + size_of::<am_probe_candidate_t>())
            + self.urls[..count]
                .iter()
                .map(|v| v.len() + 1)
                .sum::<usize>()
    }
    pub(super) fn reservation(&self) -> usize {
        self.result_bytes() + self.username.len() + self.password.len()
    }
    pub(super) fn secret(&self) -> Result<Secret> {
        Ok(Secret {
            username: owned(self.username)?,
            password: owned(self.password)?,
        })
    }
    pub(super) fn references(&self) -> Result<Vec<CString>> {
        let count = self.request.candidate_count as usize;
        let mut references = exact_vec(count)?;
        for url in &self.urls[..count] {
            let scheme = url.split_once("://").map(|(s, _)| s);
            if matches!(scheme, Some("http" | "https")) {
                sdk_io::validate_url(url, true).map_err(|_| AM_ERR_CAMERA_URL_INVALID)?;
            } else if matches!(scheme, Some("rtsp" | "rtsps")) {
                sdk_io::validate_url(url, false).map_err(|_| AM_ERR_CAMERA_URL_INVALID)?;
            } else if url.contains('@') || url.len() > 16 * 1024 {
                return Err(AM_ERR_CAMERA_URL_INVALID);
            }
            let mut bytes = exact_vec(url.len() + 1)?;
            bytes.extend_from_slice(url.as_bytes());
            bytes.push(0);
            references
                .push(CString::from_vec_with_nul(bytes).map_err(|_| AM_ERR_CAMERA_URL_INVALID)?);
        }
        Ok(references)
    }
}

pub(super) fn reserve(
    state: &Arc<CameraState>,
    cache: &Cache,
    bytes: usize,
) -> Result<ControlCharge> {
    match state.control(bytes, true) {
        Ok(charge) => Ok(charge),
        Err(AM_ERR_SDK_RESOURCE_EXHAUSTED) => {
            // 容量真实销毁后才退账，给本次结果重试准入。
            cache.clear(true)?;
            state.control(bytes, true)
        }
        Err(error) => Err(error),
    }
}
