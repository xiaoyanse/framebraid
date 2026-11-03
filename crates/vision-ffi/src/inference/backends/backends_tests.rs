//! 审计盲区回归（A10）：backend retry 失败路径完整回滚——注册槽不滞留，失败后保持可重试。
//! 共享进程域内存在其它测试刻意保留至进程结束的上下文（telemetry 测试按设计不关闭），
//! 公开导出的 context_count 前置门禁无法在其上确定性测试；A10 回滚语义位于
//! initialize_inner，用隔离域直调覆盖，导出层仅测自持上下文的确定性拒绝。
use super::{amv_backend_retry, initialize_inner};
use crate::domain;
use am_sdk_support::{abi::*, context, handles::Domain};
use std::mem::size_of;

fn params() -> amv_init_params_t {
    amv_init_params_t {
        struct_size: size_of::<amv_init_params_t>() as u32,
        abi_version: AM_ABI_VERSION,
        infer_threads: 2,
        ..Default::default()
    }
}

#[test]
fn backend_retry_rolls_back_registration_and_stays_retryable_after_failure() {
    // 前置门禁：存在存活上下文时重试必须拒绝（本测试自持上下文，确定性触发）
    let mut occupied = None;
    for _ in 0..600 {
        match unsafe { context::initialize_vision(domain(), &params()) } {
            Ok(key) => {
                occupied = Some(key);
                break;
            }
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED) => {
                std::thread::sleep(std::time::Duration::from_millis(5))
            }
            Err(other) => panic!("vision ctx init: {other}"),
        }
    }
    let occupied = occupied.expect("vision ctx admission never freed");
    assert_eq!(
        unsafe { amv_backend_retry(&params()) },
        AM_ERR_SDK_INVALID_ARGUMENT
    );
    domain().close(occupied, 5000).unwrap();
    domain().destroy(occupied).unwrap();
    // A10回归位（隔离域，retry=true直调初始化内核）：失败路径 close+destroy
    // 完整收尾，注册槽归零；连续重试结果可复现，域仍可正常准入新上下文。
    let isolated = Domain::new(true);
    let mut outcomes = Vec::new();
    for _ in 0..2 {
        let mut failed = None;
        match unsafe { initialize_inner(&isolated, &params(), true, &mut failed) } {
            Ok(key) => {
                // 成功路径按 amv_backend_retry 同样由调用方收口
                isolated.close(key, 5000).unwrap();
                isolated.destroy(key).unwrap();
                outcomes.push(AM_ERR_SDK_OK);
            }
            Err(code) => outcomes.push(code),
        }
        assert_eq!(
            isolated.context_count().unwrap(),
            0,
            "每次重试后（成功经调用方收口/失败经回滚收口）注册槽必须归零，不得滞留"
        );
    }
    assert_eq!(
        outcomes[0], outcomes[1],
        "失败回滚后重试必须得到一致结果，不得滞留注册槽锁死后续重试"
    );
    assert!(
        outcomes[0] == AM_ERR_SDK_OK || outcomes[0] == AM_ERR_VISION_RUNTIME_MISSING,
        "无注入运行库时应为RUNTIME_MISSING失败回滚，环境提供运行库时为成功收口，实际 {}",
        outcomes[0]
    );
    let revived = unsafe { context::initialize_vision(&isolated, &params()) }.unwrap();
    assert_eq!(
        isolated.close(revived, 5000).unwrap(),
        AM_LIFECYCLE_CLOSED as u32,
        "失败/重试后域必须仍可正常准入并关闭新上下文"
    );
    assert!(isolated.destroy(revived).is_ok());
}
