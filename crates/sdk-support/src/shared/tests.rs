use super::{api, pool::Pool};
use crate::{abi::*, common, context, handles::Domain};
use std::{ptr, sync::atomic::Ordering, time::Duration};
unsafe extern "C" fn noop(_: *mut std::ffi::c_void) {}

fn contexts() -> (Domain, usize, Domain, usize) {
    let c = Domain::with_shared(false, true);
    let v = Domain::with_shared(true, true);
    let cp = amc_init_params_t {
        abi_version: AM_ABI_VERSION,
        feature_flags: 1,
        ..Default::default()
    };
    let vp = amv_init_params_t {
        abi_version: AM_ABI_VERSION,
        feature_flags: 1,
        ..Default::default()
    };
    let ch = unsafe { context::initialize_camera(&c, &cp) }.unwrap();
    let vh = unsafe { context::initialize_vision(&v, &vp) }.unwrap();
    (c, ch, v, vh)
}
#[test]
fn compiled_capability_is_distinct_from_runtime_opt_in() {
    unsafe {
        let c = Domain::with_shared(false, true);
        let p = amc_init_params_t {
            abi_version: AM_ABI_VERSION,
            ..Default::default()
        };
        let ctx = context::initialize_camera(&c, &p).unwrap();
        let mut capability = amc_capability_t::default();
        common::camera_capability(&c, ctx, &mut capability).unwrap();
        assert_eq!(capability.shared_pool_state, AM_SFP_DISABLED as u32);
        assert_ne!(
            u64::from(capability.feature_bits) & AMC_CAP_SHARED_FRAME_POOL,
            0
        );
        let mut desc = am_shared_pool_attach_desc_t::default();
        assert_eq!(
            api::create(&c, ctx, std::ptr::null(), &mut desc),
            Err(AM_ERR_SDK_NOT_SUPPORTED)
        );
        c.close(ctx, 0).unwrap();
        c.destroy(ctx).unwrap();
    }
}
fn config() -> am_shared_pool_config_t {
    am_shared_pool_config_t {
        slot_count: 1,
        max_frame_bytes: 64,
        total_bytes: 64,
        max_consumers: 2,
        max_outstanding_borrows: 4,
        ..Default::default()
    }
}
fn input(bytes: &[u8]) -> (am_frame_view_t, am_frame_assoc_t) {
    (
        am_frame_view_t {
            width: 4,
            height: 4,
            format: AM_PF_BGR8 as u32,
            plane_count: 1,
            planes: [bytes.as_ptr(), ptr::null(), ptr::null(), ptr::null()],
            strides: [12, 0, 0, 0],
            plane_size_bytes: [48, 0, 0, 0],
            ..Default::default()
        },
        am_frame_assoc_t {
            source_id: 7,
            frame_id: 900,
            source_generation: 2,
            config_generation: 5,
            valid: 7,
            pts: i64::MIN,
            captured_at_ms: i64::MIN,
            received_at_ms: i64::MIN,
            ..Default::default()
        },
    )
}
#[test]
fn real_readonly_mapping_lease_bridge_and_parent_close() {
    unsafe {
        let (c, ch, v, vh) = contexts();
        let cfg = config();
        let mut attach = am_shared_pool_attach_desc_t::default();
        let pubh = api::create(&c, ch, &cfg, &mut attach).unwrap();
        let conh = api::attach(&v, vh, &attach).unwrap();
        let con2 = api::attach(&v, vh, &attach).unwrap();
        assert_eq!(
            api::attach(&v, vh, &attach),
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
        );
        let bytes = [19u8; 48];
        let (frame, a) = input(&bytes);
        let pixel = am_pixel_view_t {
            magic: AM_PIXEL_VIEW_MAGIC,
            view: &frame,
            assoc: &a,
            ..Default::default()
        };
        let id = api::publish_view(&c, pubh, &pixel).unwrap();
        assert_eq!(id, 1);
        let mut one = am_shared_frame_view_t::default();
        let mut two = am_shared_frame_view_t::default();
        api::borrow(&v, conh, id, 0, &mut one).unwrap();
        api::borrow(&v, con2, id, 0, &mut two).unwrap();
        assert_ne!(one.borrow_id, two.borrow_id);
        assert_eq!(one.desc.frame_id, 900);
        assert_eq!(
            std::slice::from_raw_parts(one.plane_data[0].cast::<u8>(), 48),
            bytes
        );
        let mut bridge = am_pixel_view_t::default();
        api::pixel(&v, conh, &one, &mut bridge, noop, noop).unwrap();
        assert_eq!((*bridge.assoc).config_generation, 5);
        assert_eq!((*bridge.view).planes[0], one.plane_data[0].cast());
        // 模拟两个已接受模型各保留同一个CPU视图；此处不冒充实际推理运行。
        v.buffers.retain(bridge.owner_handle as usize).unwrap();
        v.buffers.retain(bridge.owner_handle as usize).unwrap();
        api::return_view(&v, conh, &one).unwrap();
        api::return_view(&v, con2, &two).unwrap();
        assert_eq!(
            api::return_view(&v, conh, &one),
            Err(AM_ERR_SDK_HANDLE_INVALID)
        );
        assert_eq!(
            api::publish_view(&c, pubh, &pixel),
            Err(AM_ERR_SDK_QUEUE_FULL)
        );
        assert_eq!(
            api::close(&v, conh, 0, false).unwrap(),
            AM_LIFECYCLE_CLOSING as u32
        );
        v.buffers.release(bridge.owner_handle as usize).unwrap();
        assert_eq!(
            api::publish_view(&c, pubh, &pixel),
            Err(AM_ERR_SDK_QUEUE_FULL)
        );
        v.buffers.release(bridge.owner_handle as usize).unwrap();
        assert_eq!(
            api::close(&v, conh, 10, false).unwrap(),
            AM_LIFECYCLE_CLOSED as u32
        );
        assert_eq!(api::publish_view(&c, pubh, &pixel).unwrap(), 2);
        assert_eq!(
            api::borrow(&v, con2, 1, 0, &mut two),
            Err(AM_ERR_SDK_RESULT_EXPIRED)
        );
        assert_eq!(
            api::close(&c, pubh, 0, true).unwrap(),
            AM_LIFECYCLE_CLOSING as u32
        );
        assert_eq!(
            api::borrow(&v, con2, 2, 0, &mut two),
            Err(AM_ERR_SDK_HANDLE_CLOSED)
        );
        assert_eq!(
            api::close(&v, con2, 10, false).unwrap(),
            AM_LIFECYCLE_CLOSED as u32
        );
        assert_eq!(
            api::close(&c, pubh, 10, true).unwrap(),
            AM_LIFECYCLE_CLOSED as u32
        );
        c.destroy(pubh).unwrap();
        v.destroy(conh).unwrap();
        v.destroy(con2).unwrap();
        assert_eq!(c.close(ch, 10).unwrap(), AM_LIFECYCLE_CLOSED as u32);
        assert_eq!(v.close(vh, 10).unwrap(), AM_LIFECYCLE_CLOSED as u32);
        c.destroy(ch).unwrap();
        v.destroy(vh).unwrap();
    }
}
#[test]
fn multi_slot_and_lease_record_exhaustion_is_bounded_and_recoverable() {
    unsafe {
        let (c, ch, v, vh) = contexts();
        let cfg = am_shared_pool_config_t {
            slot_count: 2,
            max_frame_bytes: 64,
            total_bytes: 128,
            max_consumers: 2,
            max_outstanding_borrows: 2,
            ..Default::default()
        };
        let mut attach = am_shared_pool_attach_desc_t::default();
        let pubh = api::create(&c, ch, &cfg, &mut attach).unwrap();
        let con1 = api::attach(&v, vh, &attach).unwrap();
        let con2 = api::attach(&v, vh, &attach).unwrap();
        let bytes = [11u8; 48];
        let (frame, a) = input(&bytes);
        let pixel = am_pixel_view_t {
            magic: AM_PIXEL_VIEW_MAGIC,
            view: &frame,
            assoc: &a,
            ..Default::default()
        };
        let first = api::publish_view(&c, pubh, &pixel).unwrap();
        let second = api::publish_view(&c, pubh, &pixel).unwrap();
        assert_eq!((first, second), (1, 2));
        let mut one = am_shared_frame_view_t::default();
        let mut two = am_shared_frame_view_t::default();
        api::borrow(&v, con1, first, 0, &mut one).unwrap();
        api::borrow(&v, con1, second, 0, &mut two).unwrap();
        // 借用记录占满时拒绝新借用，不覆盖他人记录。
        let mut denied = am_shared_frame_view_t::default();
        assert_eq!(
            api::borrow(&v, con2, second, 0, &mut denied),
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
        );
        // 归还释放借用记录后，同帧可被其他消费者再借用。
        api::return_view(&v, con1, &two).unwrap();
        api::borrow(&v, con2, second, 0, &mut two).unwrap();
        // 两个槽都被活租约占用时发布有界拒绝，不覆盖活帧。
        assert_eq!(
            api::publish_view(&c, pubh, &pixel),
            Err(AM_ERR_SDK_QUEUE_FULL)
        );
        let mut still = am_shared_frame_view_t::default();
        assert_eq!(
            std::slice::from_raw_parts(one.plane_data[0].cast::<u8>(), 48),
            bytes
        );
        api::return_view(&v, con1, &one).unwrap();
        api::return_view(&v, con2, &two).unwrap();
        // 全部归还后槽位复用，旧发布号过期、新发布可借。
        let third = api::publish_view(&c, pubh, &pixel).unwrap();
        assert_eq!(third, 3);
        assert_eq!(
            api::borrow(&v, con1, first, 0, &mut still),
            Err(AM_ERR_SDK_RESULT_EXPIRED)
        );
        api::borrow(&v, con1, third, 0, &mut still).unwrap();
        api::return_view(&v, con1, &still).unwrap();
        api::close(&v, con1, 10, false).unwrap();
        api::close(&v, con2, 10, false).unwrap();
        api::close(&c, pubh, 10, true).unwrap();
        v.destroy(con1).unwrap();
        v.destroy(con2).unwrap();
        c.destroy(pubh).unwrap();
        c.close(ch, 10).unwrap();
        v.close(vh, 10).unwrap();
        c.destroy(ch).unwrap();
        v.destroy(vh).unwrap();
    }
}

#[test]
fn waiting_borrow_exits_bounded_when_consumer_closes() {
    unsafe {
        let (c, ch, v, vh) = contexts();
        let cfg = config();
        let mut attach = am_shared_pool_attach_desc_t::default();
        let pubh = api::create(&c, ch, &cfg, &mut attach).unwrap();
        let conh = api::attach(&v, vh, &attach).unwrap();
        let outcome = std::thread::scope(|scope| {
            let waiter = scope.spawn(|| {
                let mut out = am_shared_frame_view_t::default();
                // 空池等待下一帧，截止设长以便只由关闭打断。
                api::borrow(&v, conh, 0, 5000, &mut out)
            });
            std::thread::sleep(Duration::from_millis(50));
            let state = api::close(&v, conh, 500, false).unwrap();
            assert_eq!(state, AM_LIFECYCLE_CLOSED as u32);
            waiter.join().unwrap()
        });
        assert_eq!(outcome, Err(AM_ERR_SDK_HANDLE_CLOSED));
        // 等待线程退出后本地租约为零，关闭已完整落盘。
        let entry = v.get(conh).unwrap();
        assert_eq!(
            entry
                .data::<Pool>()
                .unwrap()
                .local_leases
                .load(Ordering::Acquire),
            0
        );
        drop(entry);
        api::close(&c, pubh, 10, true).unwrap();
        v.destroy(conh).unwrap();
        c.destroy(pubh).unwrap();
        c.close(ch, 10).unwrap();
        v.close(vh, 10).unwrap();
        c.destroy(ch).unwrap();
        v.destroy(vh).unwrap();
    }
}

#[test]
fn waiting_publish_exits_bounded_when_publisher_closes() {
    unsafe {
        let (c, ch, v, vh) = contexts();
        let cfg = am_shared_pool_config_t {
            drop_policy: AM_SFP_DROP_WAIT,
            publish_wait_ms: 5000,
            ..config()
        };
        let mut attach = am_shared_pool_attach_desc_t::default();
        let pubh = api::create(&c, ch, &cfg, &mut attach).unwrap();
        let conh = api::attach(&v, vh, &attach).unwrap();
        let bytes = [13u8; 48];
        let (frame, a) = input(&bytes);
        let pixel = am_pixel_view_t {
            magic: AM_PIXEL_VIEW_MAGIC,
            view: &frame,
            assoc: &a,
            ..Default::default()
        };
        let held = api::publish_view(&c, pubh, &pixel).unwrap();
        let mut view = am_shared_frame_view_t::default();
        api::borrow(&v, conh, held, 0, &mut view).unwrap();
        let outcome = std::thread::scope(|scope| {
            let publisher = scope.spawn(|| {
                let (frame, a) = input(&[13u8; 48]);
                let pixel = am_pixel_view_t {
                    magic: AM_PIXEL_VIEW_MAGIC,
                    view: &frame,
                    assoc: &a,
                    ..Default::default()
                };
                // 单槽被活租约占用，按策略等待空位，只由关闭打断。
                api::publish_view(&c, pubh, &pixel)
            });
            std::thread::sleep(Duration::from_millis(50));
            // 等待中的发布者此时开始关闭；消费者仍持有租约，只能先到关闭中。
            let state = api::close(&c, pubh, 200, true).unwrap();
            let result = publisher.join().unwrap();
            api::return_view(&v, conh, &view).unwrap();
            api::close(&v, conh, 100, false).unwrap();
            (state, result)
        });
        let (state, result) = outcome;
        assert_eq!(state, AM_LIFECYCLE_CLOSING as u32);
        assert_eq!(result, Err(AM_ERR_SDK_HANDLE_CLOSED));
        assert_eq!(
            api::close(&c, pubh, 500, true).unwrap(),
            AM_LIFECYCLE_CLOSED as u32
        );
        v.destroy(conh).unwrap();
        c.destroy(pubh).unwrap();
        c.close(ch, 10).unwrap();
        v.close(vh, 10).unwrap();
        c.destroy(ch).unwrap();
        v.destroy(vh).unwrap();
    }
}

#[test]
fn budgets_timeouts_bad_identity_and_failed_borrow_are_bounded() {
    unsafe {
        let (c, ch, v, vh) = contexts();
        let cfg = config();
        let mut limits = default_limits(false);
        limits.shared_pool_max_total_bytes = 64;
        common::set_limits(&c, ch, &limits).unwrap();
        let mut attach = am_shared_pool_attach_desc_t::default();
        let pubh = api::create(&c, ch, &cfg, &mut attach).unwrap();
        let mut extra = am_shared_pool_attach_desc_t::default();
        assert_eq!(
            api::create(&c, ch, &cfg, &mut extra),
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
        );
        let mut bad = attach;
        bad.pool_generation += 1;
        assert_eq!(
            api::attach(&v, vh, &bad),
            Err(AM_ERR_SDK_GENERATION_MISMATCH)
        );
        bad = attach;
        bad.owner_started_at_100ns += 1;
        assert_eq!(
            api::attach(&v, vh, &bad),
            Err(AM_ERR_SDK_GENERATION_MISMATCH)
        );
        let conh = api::attach(&v, vh, &attach).unwrap();
        let mut view = am_shared_frame_view_t::default();
        assert_eq!(
            api::borrow(&v, conh, 0, 0, &mut view),
            Err(AM_ERR_SDK_TRY_AGAIN)
        );
        assert_eq!(
            api::borrow(&v, conh, 0, 5, &mut view),
            Err(AM_ERR_SDK_DEADLINE_EXCEEDED)
        );
        assert_eq!(
            api::borrow(&v, conh, 0, 5001, &mut view),
            Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE)
        );
        view.struct_size = 8;
        assert_eq!(
            api::borrow(&v, conh, 0, 0, &mut view),
            Err(AM_ERR_SDK_STRUCT_SIZE_INVALID)
        );
        let entry = v.get(conh).unwrap();
        assert_eq!(
            entry
                .data::<Pool>()
                .unwrap()
                .local_leases
                .load(Ordering::Acquire),
            0
        );
        drop(entry);
        api::close(&v, conh, 10, false).unwrap();
        v.destroy(conh).unwrap();
        api::close(&c, pubh, 10, true).unwrap();
        c.destroy(pubh).unwrap();
        c.close(ch, 10).unwrap();
        v.close(vh, 10).unwrap();
        c.destroy(ch).unwrap();
        v.destroy(vh).unwrap();
    }
}
