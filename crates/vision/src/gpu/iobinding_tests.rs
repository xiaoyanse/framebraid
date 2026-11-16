//! F04 拒绝测试：TensorData.info 公开可变，构造后改写 shape/dtype 时
//! bind_vocab 的分配前复核必须明确拒绝（不发生按新 shape 分配+按旧字节
//! 长度复制的堆越界写）。bind_vocab 全链需 CUDA 会话，正常路径由 ignore
//! 真机门覆盖（batch_e2e_tests 的词汇绑定臂）；本模块单测其共享复核入口
//! vocab_copy_len——拒绝判定与复制长度均以此为准。
use super::iobinding::vocab_copy_len;
use crate::sdk::{TensorData, TensorInfo};

fn tensor(dtype: u32, shape: &[i64]) -> TensorData {
    let bytes = vec![0x55u8; crate::sdk::byte_count(dtype, shape).unwrap()];
    TensorData::new(
        TensorInfo {
            name: "txt_feats".into(),
            dtype,
            shape: shape.to_vec(),
        },
        &bytes,
    )
    .expect("一致元数据构造必须成功")
}

#[test]
fn consistent_metadata_passes_recheck() {
    let tensor = tensor(0, &[1, 2, 512]);
    assert_eq!(vocab_copy_len(&tensor).expect("一致元数据应通过"), 4096);
}

#[test]
fn shrunk_shape_after_mutation_is_rejected_not_overflow() {
    // f32 [4]＝16 字节；安全 Rust 改写 shape=[1] 后按新声明只应分配 4 字节，
    // 修复前按 bytes.len()=16 复制即越界。复核必须拒绝且错误键明确。
    let mut tensor = tensor(0, &[4]);
    tensor.info.shape = vec![1];
    let error = vocab_copy_len(&tensor).expect_err("shape 改小必须拒绝");
    assert_eq!(error.0, "vision.inputBytesMismatch");
}

#[test]
fn changed_dtype_after_mutation_is_rejected() {
    // f32 [4]＝16 字节；改写 dtype=int64（8 字节/元素）后声明容量 32≠16，
    // 分配与复制长度失配，必须拒绝。
    let mut tensor = tensor(0, &[4]);
    tensor.info.dtype = 2;
    let error = vocab_copy_len(&tensor).expect_err("dtype 改变必须拒绝");
    assert_eq!(error.0, "vision.inputBytesMismatch");
}
