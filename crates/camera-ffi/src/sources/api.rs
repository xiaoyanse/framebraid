use super::*;

#[no_mangle]
pub unsafe extern "C" fn amc_source_open(
    ctx: *mut amc_context_t,
    desc: *const am_source_desc_t,
    out: *mut *mut amc_source_t,
) -> i32 {
    status(domain(), ctx as usize, || {
        am_sdk_support::callbacks::reject_reentry()?;
        args::pointer(out)?;
        unsafe { out.write(std::ptr::null_mut()) };
        let active = domain().active(ctx as usize)?;
        active.data::<am_sdk_support::context::Context>()?;
        let desc = unsafe { args::read(desc, size_of::<am_source_desc_t>()) }?;
        if desc.kind > AM_SRC_DEVICE_REF as u32
            || desc.reuse > 1
            || desc.reserved.iter().any(|v| *v != 0)
            || !desc.creds.is_null() && !desc.provider.is_null()
        {
            return Err(AM_ERR_SDK_INVALID_ARGUMENT);
        }
        let url = unsafe { args::string(desc.url_utf8) }?;
        if !desc.stream_kind_utf8.is_null() {
            let kind = unsafe { args::string(desc.stream_kind_utf8) }?;
            if !["main", "sub", "isapi", ""].contains(&kind.as_str()) {
                return Err(AM_ERR_SDK_INVALID_ARGUMENT);
            }
        }
        let isapi = desc.kind == AM_SRC_ISAPI_URL as u32
            || desc.kind == AM_SRC_DEVICE_REF as u32
                && (url.starts_with("http://") || url.starts_with("https://"));
        am_camera_core::media::sdk_io::validate_url(&url, isapi)
            .map_err(|_| AM_ERR_CAMERA_URL_INVALID)?;
        let secret = if desc.creds.is_null() {
            None
        } else {
            Some(unsafe { credentials::copy_secret(desc.creds) }?)
        };
        let provider = if desc.provider.is_null() {
            None
        } else {
            Some(unsafe { Provider::read(desc.provider) }?)
        };
        let state = camera_state(ctx as usize)?;
        let mut registry = registry().lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        registry.sources.retain(|s| s.strong_count() > 0);
        registry.shared.retain(|s| s.strong_count() > 0);
        let count = registry
            .sources
            .iter()
            .filter_map(Weak::upgrade)
            .filter(|s| Arc::ptr_eq(&s.shared.state, &state) && !s.closed.load(Ordering::Acquire))
            .count();
        if count >= state.limits()?.max_sources as usize {
            return Err(AM_ERR_CAMERA_SOURCE_LIMIT_REACHED);
        }
        reserve_registry(&mut registry.sources)?;
        let reused = if desc.reuse == 1 {
            registry.shared.iter().filter_map(Weak::upgrade).find(|s| {
                !s.stop.load(Ordering::Acquire)
                    && Arc::ptr_eq(&s.state, &state)
                    && s.url == url
                    && s.isapi == isapi
                    && s.credentials
                        .claim(secret.as_ref(), provider.as_ref(), &s.owners, &s.stop)
            })
        } else {
            None
        };
        let fresh = reused.is_none();
        let shared = match reused {
            Some(shared) => shared,
            None => make_shared(state, url, isapi)?,
        };
        if fresh && desc.reuse == 1 {
            // 尚未构造Source时无Drop回滚；fresh共享所有权随Arc一起销毁。
            reserve_registry(&mut registry.shared)?;
        }
        let source = Arc::new(Source {
            shared: shared.clone(),
            key: AtomicUsize::new(0),
            closing: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            events: Arc::new(events::Events::new(shared.state.clone())),
        });
        // 句柄登记前取得后续必需的锁，避免登记成功后因锁失败留下未移交句柄。
        let mut sources = shared
            .sources
            .lock()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        // 增长先核新旧存储重叠；失败不能发布句柄或泄漏共享所有权。
        sources.retain(|_, source| source.strong_count() > 0);
        sources.reserve(&shared.state, 1, SDK_MAX_REGISTERED_HANDLES as usize)?;
        let key = match domain().insert(ctx as usize, source.clone()) {
            Ok(key) => key,
            Err(code) => {
                drop(sources);
                drop(registry);
                source.request_close();
                if source.drained() {
                    source.close_resources();
                }
                return Err(code);
            }
        };
        source.key.store(key, Ordering::Release);
        // 共享源可能长期没有事件；登记时也回收旧Weak，避免重开累积Arc分配体。
        sources.insert(key, Arc::downgrade(&source));
        drop(sources);
        registry.sources.push(Arc::downgrade(&source));
        if fresh && desc.reuse == 1 {
            registry.shared.push(Arc::downgrade(&shared));
        }
        drop(registry);
        if fresh && (secret.is_some() || provider.is_some()) {
            shared.report(
                AM_SRC_CONNECTING as u32,
                AM_FAULT_NONE as u32,
                c"camera.connecting",
            );
            if let Err(code) = shared
                .credentials
                .replace(Arc::downgrade(&shared), secret, provider)
            {
                domain().request_close(key)?;
                // 尚未移交的失败句柄由此调用负责收回；正常路径遵循显式销毁。
                drop(source);
                let _ = domain().close(key, 0);
                let _ = domain().destroy(key);
                return Err(code);
            }
            shared.credentials.refresh();
            shared.credentials_ready();
        }
        unsafe { out.write(key as *mut amc_source_t) };
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_source_close(
    src: *mut amc_source_t,
    timeout_ms: u32,
    out: *mut am_lifecycle_state_t,
) -> i32 {
    status(domain(), src as usize, || {
        am_sdk_support::callbacks::reject_reentry()?;
        args::pointer(out)?;
        get(src as usize)?;
        let state = domain().close(src as usize, timeout_ms)?;
        unsafe { out.write(state) };
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_source_get_status(
    src: *mut amc_source_t,
    out: *mut am_source_status_t,
) -> i32 {
    status(domain(), src as usize, || {
        unsafe { args::validate_output(out, size_of::<am_source_status_t>()) }?;
        let entry = get(src as usize)?;
        let source = entry.data::<Source>()?;
        let shared = &source.shared;
        let credential_timeout = matches!(shared.secret(), Err(AM_ERR_CAMERA_TIMEOUT));
        let lifecycle = domain().state(src as usize)?;
        let state = if lifecycle == AM_LIFECYCLE_CLOSED as u32 {
            AM_SRC_CLOSED
        } else if source.closing.load(Ordering::Acquire) {
            AM_SRC_CLOSING
        } else if credential_timeout {
            AM_SRC_AUTH_WAITING
        } else {
            shared.state_code.load(Ordering::Acquire) as i32
        };
        unsafe {
            args::write(
                out,
                am_source_status_t {
                    state,
                    source_id: shared.source_id,
                    generation: shared.generation.load(Ordering::Acquire),
                    config_generation: shared.config_generation.load(Ordering::Acquire),
                    fault_class: if credential_timeout {
                        AM_FAULT_AUTH as u32
                    } else {
                        shared.fault.load(Ordering::Acquire)
                    },
                    reconnect_attempts: shared.reconnect_attempts.load(Ordering::Acquire),
                    last_frame_at_ms: shared.last_frame.load(Ordering::Acquire),
                    ..Default::default()
                },
            )
        }
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_source_set_reconnect_policy(
    src: *mut amc_source_t,
    policy: *const am_reconnect_policy_t,
) -> i32 {
    status(domain(), src as usize, || {
        am_sdk_support::callbacks::reject_reentry()?;
        let active = domain().active(src as usize)?;
        let source = active.data::<Source>()?;
        let policy = unsafe { args::read(policy, size_of::<am_reconnect_policy_t>()) }?;
        if policy.initial_backoff_ms == 0
            || policy.initial_backoff_ms > policy.max_backoff_ms
            || policy.max_backoff_ms > 60000
            || policy.jitter_ms > policy.max_backoff_ms
        {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        *source
            .shared
            .reconnect
            .lock()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)? = policy;
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_credentials_set(
    src: *mut amc_source_t,
    value: *const am_credential_data_t,
) -> i32 {
    status(domain(), src as usize, || {
        am_sdk_support::callbacks::reject_reentry()?;
        let secret = unsafe { credentials::copy_secret(value) }?;
        let active = domain().active(src as usize)?;
        let source = active.data::<Source>()?;
        source
            .shared
            .credentials
            .replace(Arc::downgrade(&source.shared), Some(secret), None)?;
        source.shared.changed()?;
        source.shared.credentials_ready();
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_credentials_set_provider(
    src: *mut amc_source_t,
    value: *const am_credential_provider_t,
) -> i32 {
    status(domain(), src as usize, || {
        am_sdk_support::callbacks::reject_reentry()?;
        let provider = if value.is_null() {
            None
        } else {
            Some(unsafe { Provider::read(value) }?)
        };
        let active = domain().active(src as usize)?;
        let source = active.data::<Source>()?;
        source.shared.report(
            if provider.is_some() {
                AM_SRC_CONNECTING as u32
            } else {
                AM_SRC_WAITING_CREDENTIALS as u32
            },
            AM_FAULT_NONE as u32,
            c"camera.credentialsChanged",
        );
        source
            .shared
            .credentials
            .replace(Arc::downgrade(&source.shared), None, provider)?;
        source.shared.changed()?;
        source.shared.credentials.refresh();
        source.shared.credentials_ready();
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_credentials_invalidate(
    src: *mut amc_source_t,
    known_version: u32,
) -> i32 {
    status(domain(), src as usize, || {
        am_sdk_support::callbacks::reject_reentry()?;
        let active = domain().active(src as usize)?;
        let source = active.data::<Source>()?;
        if source.shared.credentials.invalidate(known_version)? {
            source.shared.changed()?;
            source.shared.report(
                AM_SRC_AUTH_WAITING as u32,
                AM_FAULT_AUTH as u32,
                c"camera.credentialsInvalidated",
            );
            source.shared.credentials.refresh();
        }
        Ok(())
    })
}
#[no_mangle]
pub unsafe extern "C" fn amc_source_set_event_callback(
    src: *mut amc_source_t,
    cb: am_source_event_fn,
    user: *mut c_void,
) -> i32 {
    status(domain(), src as usize, || {
        am_sdk_support::callbacks::reject_reentry()?;
        let active = domain().active(src as usize)?;
        let source = active.data::<Source>()?;
        let weak = source
            .shared
            .sources
            .lock()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?
            .get(&(src as usize))
            .cloned()
            .ok_or(AM_ERR_SDK_HANDLE_INVALID)?;
        source.events.set(weak, cb, user as usize)
    })
}
