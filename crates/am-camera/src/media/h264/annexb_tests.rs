//! Annex B/AVCC 内核单测（批 1.6）：NALU 边界/AVCC 转换/参数集提取/关键帧补齐。
//! 构造合成流锁定字节级行为，另以真源覆盖见 session_tests。

use super::annexb::*;

fn nal(header: u8, payload: u8, len: usize) -> Vec<u8> {
    let mut out = vec![header];
    out.extend(std::iter::repeat_n(payload, len));
    out
}

/// 3/4 字节起始码混合流切分：NALU 边界正确、载荷不含起始码。
#[test]
fn split_nalus_mixed_start_codes() {
    let sps = nal(NAL_SPS, 0xAB, 5);
    let pps = nal(NAL_PPS, 0xCD, 3);
    let idr = nal(NAL_IDR, 0x11, 10);
    let mut stream = Vec::new();
    stream.extend_from_slice(&START_CODE4);
    stream.extend_from_slice(&sps);
    stream.extend_from_slice(&START_CODE3);
    stream.extend_from_slice(&pps);
    stream.extend_from_slice(&START_CODE4);
    stream.extend_from_slice(&idr);
    let nalus = split_nalus(&stream);
    assert_eq!(nalus.len(), 3);
    assert_eq!(nalus[0], sps.as_slice());
    assert_eq!(nalus[1], pps.as_slice());
    assert_eq!(nalus[2], idr.as_slice());
    assert_eq!(nalu_type(nalus[2]), Some(NAL_IDR));
}

/// AVCC（4 字节大端长度前缀）转 Annex B：逐 NAL 换 4 字节起始码；长度越界拒绝。
#[test]
fn avcc_to_annexb_converts_and_validates() {
    let sps = nal(NAL_SPS, 0xAB, 5);
    let idr = nal(NAL_IDR, 0x11, 8);
    let mut avcc = Vec::new();
    for unit in [&sps, &idr] {
        avcc.extend_from_slice(&(unit.len() as u32).to_be_bytes());
        avcc.extend_from_slice(unit);
    }
    let annexb = avcc_to_annexb(&avcc).expect("合法 AVCC 应转换成功");
    assert!(starts_with_start_code(&annexb));
    let nalus = split_nalus(&annexb);
    assert_eq!(nalus, vec![sps.as_slice(), idr.as_slice()]);
    // 长度越界（非 AVCC 数据）拒绝
    let broken = [0u8, 0, 0, 100, 1, 2, 3];
    assert!(avcc_to_annexb(&broken).is_none());
    // 已是 Annex B 的数据 to_annexb 原样透传
    let raw = [0u8, 0, 0, 1, NAL_IDR, 9, 9];
    assert_eq!(to_annexb(&raw).as_deref(), Some(raw.as_slice()));
}

/// avcC（MP4 全局头）与 Annex B（SDP sprop）两种 extradata 均能提取参数集。
#[test]
fn parse_parameter_sets_both_container_formats() {
    let sps = nal(NAL_SPS, 0xAB, 6);
    let pps = nal(NAL_PPS, 0xCD, 4);
    // Annex B extradata
    let mut annexb = Vec::new();
    annexb.extend_from_slice(&START_CODE4);
    annexb.extend_from_slice(&sps);
    annexb.extend_from_slice(&START_CODE3);
    annexb.extend_from_slice(&pps);
    let sets = parse_parameter_sets(&annexb);
    assert_eq!(
        sets,
        ParameterSets {
            sps: sps.clone(),
            pps: pps.clone()
        }
    );
    // avcC extradata：版本 1 + lengthSizeMinusOne=3 + SPS/PPS 计数与 2 字节长度
    let mut avcc = vec![1u8, 0x64, 0x00, 0x1E, 0xFF, 0xE1];
    avcc.extend_from_slice(&(sps.len() as u16).to_be_bytes());
    avcc.extend_from_slice(&sps);
    avcc.push(1);
    avcc.extend_from_slice(&(pps.len() as u16).to_be_bytes());
    avcc.extend_from_slice(&pps);
    assert_eq!(parse_parameter_sets(&avcc), ParameterSets { sps, pps });
    assert_eq!(parse_parameter_sets(&[]), ParameterSets::default());
}

/// 关键帧判定：IDR（5）与高Profile IDR（19/20）命中，P 帧/SEI 不命中。
#[test]
fn idr_detection_covers_high_profile() {
    for idr_type in [NAL_IDR, NAL_IDR_W_RADL, NAL_IDR_N_LP] {
        let stream = [0u8, 0, 0, 1, idr_type | 0x60, 1, 2];
        let au = to_annexb(&stream).expect("Annex B 透传");
        assert!(has_idr_nalu(&au), "type={idr_type} 应判定关键帧");
    }
    let p_frame = [0u8, 0, 0, 1, NAL_SLICE, 1, 2, 3];
    assert!(!has_idr_nalu(&p_frame));
    let sei_only = [0u8, 0, 0, 1, NAL_SEI, 1];
    assert!(!has_idr_nalu(&sei_only));
}

/// 关键帧补齐参数集：缺失前置（SPS 在前 PPS 在后）；已含或参数集为空则原样。
#[test]
fn ensure_parameter_sets_prepends_missing() {
    let sets = ParameterSets {
        sps: nal(NAL_SPS, 0xAB, 5),
        pps: nal(NAL_PPS, 0xCD, 4),
    };
    let idr_only = [0u8, 0, 0, 1, NAL_IDR, 7, 7, 7];
    let patched = ensure_parameter_sets(&idr_only, &sets);
    let types: Vec<_> = split_nalus(&patched).into_iter().map(nalu_type).collect();
    assert_eq!(types, vec![Some(NAL_SPS), Some(NAL_PPS), Some(NAL_IDR)]);
    // 已含参数集：原样
    let mut complete = Vec::new();
    complete.extend_from_slice(&START_CODE4);
    complete.extend_from_slice(&sets.sps);
    complete.extend_from_slice(&START_CODE4);
    complete.extend_from_slice(&sets.pps);
    complete.extend_from_slice(&idr_only);
    assert_eq!(ensure_parameter_sets(&complete, &sets), complete);
    // 参数集为空：不改动
    assert_eq!(
        ensure_parameter_sets(&idr_only, &ParameterSets::default()),
        idr_only.to_vec()
    );
}
