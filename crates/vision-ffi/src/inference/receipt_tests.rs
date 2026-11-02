//! 审计盲区回归：amv_receipt_release 防篡改校验（指针对位+accepted对账）与双重释放拒绝。
use super::{batch_exports::amv_receipt_release, receipt::Receipt, state::State};
use crate::domain;
use am_sdk_support::{abi::*, context};
use std::mem::size_of;

fn open_context() -> usize {
    let params = amv_init_params_t {
        struct_size: size_of::<amv_init_params_t>() as u32,
        abi_version: AM_ABI_VERSION,
        infer_threads: 2,
        ..Default::default()
    };
    unsafe { context::initialize_vision(domain(), &params) }.expect("vision ctx")
}

#[test]
fn receipt_release_rejects_tampered_views_and_double_release() {
    let ctx = open_context();
    let state = State::get(ctx).unwrap();
    let owner = Receipt::create(&state, 3).unwrap();
    let (ids, codes, accepted) = {
        let mut data = owner.data.lock().unwrap();
        data.ids[0] = 0x0001_0000_0000_0001;
        data.ids[1] = 0x0001_0000_0000_0002;
        data.codes[1] = AM_ERR_VISION_INSUFFICIENT_BATCH_REJECTED;
        (data.ids.as_mut_ptr(), data.codes.as_ptr(), 2u32)
    };
    let key = ids as usize;
    domain().buffers.insert(key, owner.clone()).unwrap();
    let receipt = am_batch_receipt_t {
        accepted_count: accepted,
        request_ids: ids,
        rejects: codes,
        ..Default::default()
    };
    // rejects指针对位失败
    let mut tampered = receipt;
    tampered.rejects = ids as *const am_status_t;
    assert_eq!(
        unsafe { amv_receipt_release(&mut tampered) },
        AM_ERR_SDK_HANDLE_INVALID
    );
    // request_ids指向未登记的外来指针
    let mut foreign = [7u64; 3];
    tampered = receipt;
    tampered.request_ids = foreign.as_mut_ptr();
    assert_eq!(
        unsafe { amv_receipt_release(&mut tampered) },
        AM_ERR_SDK_HANDLE_INVALID
    );
    // accepted_count对账不符（放过=控制预算按size_of+count*12永久泄漏）
    tampered = receipt;
    tampered.accepted_count = accepted + 1;
    assert_eq!(
        unsafe { amv_receipt_release(&mut tampered) },
        AM_ERR_SDK_HANDLE_INVALID
    );
    assert!(domain().buffers.get(key).is_ok(), "防篡改拒绝不得释放登记");
    // 正确视图可释放；随后双重释放（保留副本）被拒绝
    let mut duplicate = receipt;
    let mut value = receipt;
    assert_eq!(unsafe { amv_receipt_release(&mut value) }, AM_ERR_SDK_OK);
    assert!(domain().buffers.get(key).is_err());
    assert_eq!(
        unsafe { amv_receipt_release(&mut duplicate) },
        AM_ERR_SDK_HANDLE_INVALID
    );
    drop(owner);
    drop(state);
    assert_eq!(
        domain().close(ctx, 5000).unwrap(),
        AM_LIFECYCLE_CLOSED as u32
    );
    domain().destroy(ctx).unwrap();
}
