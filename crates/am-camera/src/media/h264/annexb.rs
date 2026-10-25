//! H.264 Annex B/AVCC 处理内核（批 1.2 搬运模式）：
//! NALU 边界解析、AVCC→Annex B 转换、参数集提取与关键帧判定——纯函数无 FFI。
//! 合同对齐 docs/design/dual-dll-sdk.md §7：包注明 Annex B/SPS-PPS 随关键帧。

/// 3 字节起始码（Annex B）
pub const START_CODE3: [u8; 3] = [0, 0, 1];
/// 4 字节起始码（Annex B，输出统一用 4 字节）
pub const START_CODE4: [u8; 4] = [0, 0, 0, 1];

/// NAL 类型（H.264 Annex B 首字节低 5 位）
pub const NAL_SLICE: u8 = 1;
pub const NAL_IDR: u8 = 5;
pub const NAL_SEI: u8 = 6;
pub const NAL_SPS: u8 = 7;
pub const NAL_PPS: u8 = 8;
/// 多 slice IDR（高Profile 无参考 IDR，按关键帧处理）
pub const NAL_IDR_W_RADL: u8 = 19;
pub const NAL_IDR_N_LP: u8 = 20;

/// NAL 类型号；空切片返回 None。
pub fn nalu_type(nalu: &[u8]) -> Option<u8> {
    nalu.first().map(|b| b & 0x1f)
}

/// 是否 IDR 类 NAL（含高Profile 变体）。
pub fn is_idr_type(nal: u8) -> bool {
    nal == NAL_IDR || nal == NAL_IDR_W_RADL || nal == NAL_IDR_N_LP
}

/// 数据是否以 Annex B 起始码开头（3 或 4 字节）。
pub fn starts_with_start_code(data: &[u8]) -> bool {
    if data.len() < 3 || data[0] != 0 || data[1] != 0 {
        return false;
    }
    data[2] == 1 || (data.len() >= 4 && data[2] == 0 && data[3] == 1)
}

/// 切分 Annex B 流为 NALU 载荷列表（去起始码；尾部零字节保留在末个 NAL 内）。
pub fn split_nalus(data: &[u8]) -> Vec<&[u8]> {
    let mut nalus = Vec::new();
    let mut cursor = 0usize;
    let mut payload_start: Option<usize> = None;
    while cursor + 3 <= data.len() {
        if data[cursor] == 0 && data[cursor + 1] == 0 && data[cursor + 2] == 1 {
            if let Some(start) = payload_start {
                nalus.push(&data[start..cursor]);
            }
            cursor += 3;
            payload_start = Some(cursor);
        } else if cursor + 4 <= data.len()
            && data[cursor] == 0
            && data[cursor + 1] == 0
            && data[cursor + 2] == 0
            && data[cursor + 3] == 1
        {
            if let Some(start) = payload_start {
                nalus.push(&data[start..cursor]);
            }
            cursor += 4;
            payload_start = Some(cursor);
        } else {
            cursor += 1;
        }
    }
    if let Some(start) = payload_start {
        nalus.push(&data[start..]);
    }
    nalus
}

/// AVCC→Annex B：4 字节大端长度前缀逐 NAL 换起始码；长度不吻合返回 None。
pub fn avcc_to_annexb(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len() + 16);
    let mut cursor = 0usize;
    let mut nal_count = 0usize;
    while cursor + 4 <= data.len() {
        let length = u32::from_be_bytes([
            data[cursor],
            data[cursor + 1],
            data[cursor + 2],
            data[cursor + 3],
        ]) as usize;
        cursor += 4;
        if length == 0 {
            continue; // 尾部零填充
        }
        let end = cursor.checked_add(length)?;
        if end > data.len() {
            return None; // 长度越界：不是合法 AVCC
        }
        out.extend_from_slice(&START_CODE4);
        out.extend_from_slice(&data[cursor..end]);
        nal_count += 1;
        cursor = end;
    }
    (nal_count > 0).then_some(out)
}

/// 压缩包统一转 Annex B：已带起始码原样透传，否则按 AVCC 转换。
pub fn to_annexb(data: &[u8]) -> Option<Vec<u8>> {
    if starts_with_start_code(data) {
        return Some(data.to_vec());
    }
    avcc_to_annexb(data)
}

/// 首个指定类型 NAL 载荷（Annex B 输入）。
pub fn first_nal_of_type(data: &[u8], nal: u8) -> Option<&[u8]> {
    split_nalus(data)
        .into_iter()
        .find(|nalu| nalu_type(nalu) == Some(nal))
}

/// 访问单元是否含指定类型 NAL。
pub fn contains_nal_type(data: &[u8], nal: u8) -> bool {
    split_nalus(data)
        .iter()
        .any(|nalu| nalu_type(nalu) == Some(nal))
}

/// 访问单元是否关键帧（含 IDR 类 NAL）。
pub fn has_idr_nalu(au: &[u8]) -> bool {
    split_nalus(au)
        .iter()
        .any(|nalu| nalu_type(nalu).is_some_and(is_idr_type))
}

/// 参数集（不含起始码；各取首个）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParameterSets {
    pub sps: Vec<u8>,
    pub pps: Vec<u8>,
}

/// 从 extradata 提取参数集：兼容 Annex B（RTSP sprop）与 avcC（MP4）两种格式。
pub fn parse_parameter_sets(extradata: &[u8]) -> ParameterSets {
    if extradata.is_empty() {
        return ParameterSets::default();
    }
    if starts_with_start_code(extradata) {
        let mut sets = ParameterSets::default();
        for nalu in split_nalus(extradata) {
            match nalu_type(nalu) {
                Some(NAL_SPS) if sets.sps.is_empty() => sets.sps = nalu.to_vec(),
                Some(NAL_PPS) if sets.pps.is_empty() => sets.pps = nalu.to_vec(),
                _ => {}
            }
        }
        return sets;
    }
    if extradata[0] == 1 {
        return parse_avcc_parameter_sets(extradata);
    }
    ParameterSets::default()
}

/// avcC（MP4/AVCC 容器全局头）：版本字节 1 + SPS/PPS 递归长度前缀。
fn parse_avcc_parameter_sets(extradata: &[u8]) -> ParameterSets {
    let mut sets = ParameterSets::default();
    if extradata.len() < 7 {
        return sets;
    }
    let sps_count = (extradata[5] & 0x1f) as usize;
    let mut cursor = 6usize;
    for _ in 0..sps_count {
        let Some((nalu, next)) = read_avcc_length_prefixed(extradata, cursor) else {
            return sets;
        };
        if sets.sps.is_empty() {
            sets.sps = nalu.to_vec();
        }
        cursor = next;
    }
    if cursor < extradata.len() {
        let pps_count = extradata[cursor] as usize;
        cursor += 1;
        for _ in 0..pps_count {
            let Some((nalu, next)) = read_avcc_length_prefixed(extradata, cursor) else {
                break;
            };
            if sets.pps.is_empty() {
                sets.pps = nalu.to_vec();
            }
            cursor = next;
        }
    }
    sets
}

/// 读 2 字节大端长度前缀 NAL：返回（载荷区间, 下一游标）。
fn read_avcc_length_prefixed(data: &[u8], cursor: usize) -> Option<(&[u8], usize)> {
    if cursor + 2 > data.len() {
        return None;
    }
    let length = u16::from_be_bytes([data[cursor], data[cursor + 1]]) as usize;
    let start = cursor + 2;
    let end = start.checked_add(length)?;
    if end > data.len() {
        return None;
    }
    Some((&data[start..end], end))
}

/// 关键帧补齐参数集：AU 缺 SPS/PPS 时前置（SDP 全局头未随帧下发的相机）。
/// 参数集为空或 AU 已含参数集时原样返回。
pub fn ensure_parameter_sets(au: &[u8], sets: &ParameterSets) -> Vec<u8> {
    let has_sps = contains_nal_type(au, NAL_SPS);
    let has_pps = contains_nal_type(au, NAL_PPS);
    if (has_sps || sets.sps.is_empty()) && (has_pps || sets.pps.is_empty()) {
        return au.to_vec();
    }
    let mut out = Vec::with_capacity(au.len() + sets.sps.len() + sets.pps.len() + 8);
    if !has_sps && !sets.sps.is_empty() {
        out.extend_from_slice(&START_CODE4);
        out.extend_from_slice(&sets.sps);
    }
    if !has_pps && !sets.pps.is_empty() {
        out.extend_from_slice(&START_CODE4);
        out.extend_from_slice(&sets.pps);
    }
    out.extend_from_slice(au);
    out
}
