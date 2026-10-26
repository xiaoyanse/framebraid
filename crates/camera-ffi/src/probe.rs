use super::*;
use crate::frames::{camera_state, CameraState, ControlCharge};
use crate::reader::BudgetedReader;
use am_camera_core::{
    credentials::Secret,
    media::sdk_io::{self, IoError},
};
use am_sdk_support::{args, buffers::Buffer, clock, context::Context, Result};
use std::{
    any::{Any, TypeId},
    collections::{hash_map::RandomState, VecDeque},
    ffi::CString,
    hash::{BuildHasher, Hash, Hasher},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

const MAX_CANDIDATES: usize = 16;
const CACHE_ITEMS: usize = 64;
const CACHE_BYTES: usize = 1024 * 1024;

#[derive(Clone)]
struct Observation {
    status: i32,
    codec: Option<&'static std::ffi::CStr>,
    width: u32,
    height: u32,
    fps: f64,
    observed: i64,
    valid_until: i64,
    expires: Instant,
}
impl Observation {
    fn failed(status: i32) -> Self {
        let observed = clock::monotonic_ms();
        Self {
            status,
            codec: None,
            width: 0,
            height: 0,
            fps: -1.0,
            observed,
            valid_until: observed.saturating_add(30_000),
            expires: Instant::now() + Duration::from_secs(30),
        }
    }
    fn success(codec: &'static std::ffi::CStr, width: u32, height: u32, fps: f64) -> Self {
        let observed = clock::monotonic_ms();
        Self {
            status: AM_PROBE_OK,
            codec: Some(codec),
            width,
            height,
            fps: if fps.is_finite() && fps > 0.0 {
                fps
            } else {
                -1.0
            },
            observed,
            valid_until: observed.saturating_add(300_000),
            expires: Instant::now() + Duration::from_secs(300),
        }
    }
}
struct Cached {
    url: String,
    credential: [u64; 2],
    value: Observation,
    _charge: ControlCharge,
}
struct Cache {
    seeds: [RandomState; 2],
    entries: Mutex<VecDeque<Cached>>,
    generation: AtomicU64,
}
impl Cache {
    fn new() -> Self {
        Self {
            seeds: [RandomState::new(), RandomState::new()],
            entries: Mutex::default(),
            generation: AtomicU64::new(0),
        }
    }
    fn identity(&self, secret: &Secret) -> [u64; 2] {
        self.seeds.each_ref().map(|seed| {
            let mut h = seed.build_hasher();
            secret.username.hash(&mut h);
            secret.password.hash(&mut h);
            h.finish()
        })
    }
    fn get(&self, url: &str, credential: [u64; 2]) -> Result<Option<Observation>> {
        let mut entries = self.entries.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        entries.retain(|entry| Instant::now() < entry.value.expires);
        Ok(entries
            .iter()
            .find(|entry| entry.url == url && entry.credential == credential)
            .map(|entry| entry.value.clone()))
    }
    fn put(
        &self,
        state: &Arc<CameraState>,
        generation: u64,
        url: String,
        credential: [u64; 2],
        value: Observation,
    ) -> Result<()> {
        let limit = state.limits()?.control_max_items as usize;
        let queue_limit = state
            .ctx
            .data::<Context>()?
            .queues
            .lock()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?[5]
            .max_items as usize;
        let cache_limit = CACHE_ITEMS.min(limit.min(queue_limit) / 2);
        let mut entries = self.entries.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        if self.generation.load(Ordering::Acquire) != generation {
            return Ok(());
        }
        entries.retain(|entry| {
            Instant::now() < entry.value.expires
                && !(entry.url == url && entry.credential == credential)
        });
        let cost = url.len() + size_of::<Cached>();
        while !entries.is_empty()
            && (entries.len() >= cache_limit
                || entries
                    .iter()
                    .map(|entry| entry.url.len() + size_of::<Cached>())
                    .sum::<usize>()
                    + cost
                    > CACHE_BYTES)
        {
            entries.pop_front();
        }
        if cache_limit > 0 && cost <= CACHE_BYTES {
            // 缓存让位给调用结果；预算不足只跳过缓存，不丢失本次探测结果。
            if let Ok(charge) = state.control(cost, false) {
                entries.push_back(Cached {
                    url,
                    credential,
                    value,
                    _charge: charge,
                });
            }
        }
        Ok(())
    }
}
pub(crate) fn invalidate(ctx: usize) -> Result<()> {
    let state = camera_state(ctx)?;
    let cache = cache(&state)?;
    cache.generation.fetch_add(1, Ordering::AcqRel);
    cache
        .entries
        .lock()
        .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?
        .clear();
    Ok(())
}
fn cache(state: &CameraState) -> Result<Arc<Cache>> {
    let ctx = state.ctx.data::<Context>()?;
    let mut services = ctx.services.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
    let stored = services
        .entry(TypeId::of::<Cache>())
        .or_insert_with(|| Arc::new(Cache::new()));
    stored
        .clone()
        .downcast()
        .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)
}

struct ProbeResult {
    abi: Box<am_probe_result_t>,
    _rows: Vec<am_probe_candidate_t>,
    _references: Vec<CString>,
    _charge: ControlCharge,
}
// 指针仅指向本对象拥有的不可变堆数据或静态编码名。
unsafe impl Send for ProbeResult {}
unsafe impl Sync for ProbeResult {}
impl Buffer for ProbeResult {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn probe(state: &Arc<CameraState>, url: &str, secret: &Secret, timeout: Duration) -> Observation {
    let result = if url.starts_with("http://") || url.starts_with("https://") {
        let charge = state
            .charge(4 * 1024 * 1024, false)
            .map_err(|_| IoError::Resource);
        charge.and_then(|_charge| {
            sdk_io::capture(
                url,
                secret,
                timeout,
                4 * 1024 * 1024,
                16 * 1024 * 1024,
                false,
            )
            .map(|image| {
                let codec = if image.encoded.starts_with(&[0x89, b'P', b'N', b'G']) {
                    c"png"
                } else {
                    c"mjpeg"
                };
                Observation::success(codec, image.width, image.height, -1.0)
            })
        })
    } else if url.starts_with("rtsp://") || url.starts_with("rtsps://") {
        match crate::decoder::config::runtime(state) {
            Ok(api) => BudgetedReader::open(
                state.clone(),
                api,
                url,
                secret,
                Arc::new(AtomicBool::new(false)),
                timeout,
            )
            .and_then(|reader| {
                let info = reader.info();
                let codec = match info.codec {
                    AM_CODEC_H264 => c"h264",
                    AM_CODEC_H265 => c"h265",
                    AM_CODEC_MJPEG => c"mjpeg",
                    _ => return Err(IoError::Unsupported),
                };
                Ok(Observation::success(
                    codec,
                    info.width,
                    info.height,
                    info.fps,
                ))
            }),
            Err(_) => Err(IoError::Unsupported),
        }
    } else {
        Err(IoError::Unsupported)
    };
    result.unwrap_or_else(|error| {
        Observation::failed(match error {
            IoError::Timeout => AM_PROBE_TIMEOUT,
            IoError::Auth => AM_PROBE_AUTH_FAILED,
            IoError::Unsupported | IoError::Invalid => AM_PROBE_UNSUPPORTED,
            IoError::Resource | IoError::Closed => AM_PROBE_PENDING,
            IoError::Network => AM_PROBE_UNREACHABLE,
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn amc_probe_streams(
    ctx: *mut amc_context_t,
    req: *const am_probe_request_t,
    out: *mut *mut am_probe_result_t,
) -> i32 {
    status(domain(), ctx as usize, || unsafe {
        am_sdk_support::callbacks::reject_reentry()?;
        args::pointer(out)?;
        out.write(std::ptr::null_mut());
        let req: am_probe_request_t = args::read(req, size_of::<am_probe_request_t>())?;
        if req.candidate_count == 0 || req.candidate_count as usize > MAX_CANDIDATES {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        if req.force_refresh > 1 || req.timeout_ms > 60_000 {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        args::pointer(req.references_utf8)?;
        let secret = if req.creds.is_null() {
            Secret {
                username: String::new(),
                password: String::new(),
            }
        } else {
            let creds: am_credential_data_t =
                args::read(req.creds, size_of::<am_credential_data_t>())?;
            Secret {
                username: args::string(creds.username_utf8)?,
                password: args::string(creds.password_utf8)?,
            }
        };
        let mut references = Vec::with_capacity(req.candidate_count as usize);
        for i in 0..req.candidate_count as usize {
            let url = args::string(req.references_utf8.add(i).read())?;
            // 拒绝内嵌凭据，避免输出引用或原生错误意外暴露密码。
            let scheme = url.split_once("://").map(|(s, _)| s);
            if matches!(scheme, Some("http" | "https")) {
                sdk_io::validate_url(&url, true).map_err(|_| AM_ERR_CAMERA_URL_INVALID)?;
            } else if matches!(scheme, Some("rtsp" | "rtsps")) {
                sdk_io::validate_url(&url, false).map_err(|_| AM_ERR_CAMERA_URL_INVALID)?;
            } else if url.contains('@') || url.len() > 16 * 1024 {
                return Err(AM_ERR_CAMERA_URL_INVALID);
            }
            references.push(CString::new(url).map_err(|_| AM_ERR_CAMERA_URL_INVALID)?);
        }
        let _active = domain().active(ctx as usize)?;
        let state = camera_state(ctx as usize)?;
        let bytes = size_of::<ProbeResult>()
            + references
                .iter()
                .map(|s| s.as_bytes_with_nul().len())
                .sum::<usize>()
            + references.len() * size_of::<am_probe_candidate_t>();
        let cache = cache(&state)?;
        let generation = cache.generation.load(Ordering::Acquire);
        let charge = match state.control(bytes, true) {
            Ok(charge) => charge,
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED) => {
                cache
                    .entries
                    .lock()
                    .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?
                    .clear();
                state.control(bytes, true)?
            }
            Err(error) => return Err(error),
        };
        let identity = cache.identity(&secret);
        let mut values = Vec::with_capacity(references.len());
        let mut misses = Vec::new();
        for (i, reference) in references.iter().enumerate() {
            let url = reference.to_str().map_err(|_| AM_ERR_SDK_UTF8_INVALID)?;
            let hit = if req.force_refresh == 0 {
                cache.get(url, identity)?
            } else {
                None
            };
            if hit.is_none() {
                misses.push(i);
            }
            values.push(hit);
        }
        // 全进程至多四个探测工作线程；并发调用排队，不随候选或调用量膨胀线程。
        static RUN: OnceLock<Mutex<()>> = OnceLock::new();
        let _run = RUN
            .get_or_init(Mutex::default)
            .lock()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        let timeout = Duration::from_millis(u64::from(if req.timeout_ms == 0 {
            5000
        } else {
            req.timeout_ms
        }));
        let width = state.metrics.threads.limit()?.min(4) as usize;
        if width == 0 && !misses.is_empty() {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        for group in misses.chunks(width.max(1)) {
            let results = std::thread::scope(|scope| {
                let mut threads = Vec::with_capacity(group.len());
                for &index in group {
                    let url = references[index]
                        .to_str()
                        .map_err(|_| AM_ERR_SDK_UTF8_INVALID)?;
                    let state = &state;
                    let secret = &secret;
                    let handle = state
                        .metrics
                        .threads
                        .spawn_scoped(scope, "am-sdk-probe", move || {
                            probe(state, url, secret, timeout)
                        })
                        .map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
                    threads.push((index, handle));
                }
                let mut result = Vec::with_capacity(threads.len());
                for (index, handle) in threads {
                    result.push((index, handle.join().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?));
                }
                Ok::<_, i32>(result)
            })?;
            for (index, value) in results {
                cache.put(
                    &state,
                    generation,
                    references[index].to_string_lossy().into_owned(),
                    identity,
                    value.clone(),
                )?;
                values[index] = Some(value);
            }
        }
        let mut rows = Vec::with_capacity(values.len());
        for (reference, value) in references.iter().zip(values) {
            let value = value.ok_or(AM_ERR_SDK_INTERNAL_ERROR)?;
            rows.push(am_probe_candidate_t {
                struct_size: size_of::<am_probe_candidate_t>() as u32,
                reference_utf8: reference.as_ptr(),
                status: value.status,
                codec_utf8: value.codec.map_or(std::ptr::null(), |codec| codec.as_ptr()),
                width: value.width,
                height: value.height,
                fps: value.fps,
                observed_at_ms: value.observed,
                valid_until_ms: value.valid_until,
            });
        }
        let body = Arc::new(ProbeResult {
            abi: Box::new(am_probe_result_t {
                struct_size: size_of::<am_probe_result_t>() as u32,
                candidate_count: rows.len() as u32,
                candidates: rows.as_ptr(),
            }),
            _rows: rows,
            _references: references,
            _charge: charge,
        });
        let pointer = (&*body.abi as *const am_probe_result_t).cast_mut();
        domain().buffers.insert(pointer as usize, body)?;
        out.write(pointer);
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn amc_probe_result_release(result: *mut am_probe_result_t) -> i32 {
    status(domain(), 0, || {
        let body = domain().buffers.get(result as usize)?;
        body.as_any()
            .downcast_ref::<ProbeResult>()
            .ok_or(AM_ERR_SDK_HANDLE_INVALID)?;
        domain().buffers.release(result as usize)
    })
}
