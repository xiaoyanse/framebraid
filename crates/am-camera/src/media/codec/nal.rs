use super::{DecodeError, Result, VideoCodec};

/// 对完整Annex B包做边界检查；保留原有Vec返回形态。
pub fn access_units(codec: VideoCodec, data: &[u8]) -> Result<Vec<&[u8]>> {
    let mut units = Vec::new();
    visit_access_units(codec, data, |nal| units.push(nal))?;
    Ok(units)
}

/// 同步借用各NAL，不分配列表；借用不超过输入寿命。
/// 错误前可能已访问部分NAL，调用方应在整体成功后才提交外部状态。
pub fn visit_access_units<'a>(
    codec: VideoCodec,
    data: &'a [u8],
    mut visit: impl FnMut(&'a [u8]),
) -> Result<()> {
    if data.is_empty() || data.len() > 16 * 1024 * 1024 {
        return Err(DecodeError::InvalidInput);
    }
    let start = |at: usize| -> usize {
        if data.get(at..at + 4) == Some(&[0, 0, 0, 1]) {
            4
        } else if data.get(at..at + 3) == Some(&[0, 0, 1]) {
            3
        } else {
            0
        }
    };
    let first = start(0);
    if first == 0 {
        return Err(DecodeError::InvalidInput);
    }
    let mut emit = |nal: &'a [u8]| -> Result<()> {
        let first = nal.first().ok_or(DecodeError::InvalidInput)?;
        match codec {
            VideoCodec::H264 if first & 128 != 0 || first & 31 == 0 || first & 31 > 23 => {
                return Err(DecodeError::InvalidInput);
            }
            VideoCodec::H265 if nal.len() < 2 || first & 128 != 0 || nal[1] & 7 == 0 => {
                return Err(DecodeError::InvalidInput);
            }
            _ => {}
        }
        visit(nal);
        Ok(())
    };
    let (mut begin, mut at, mut count) = (first, first, 0usize);
    while at < data.len() {
        let width = start(at);
        if width == 0 {
            at += 1;
            continue;
        }
        count += 1;
        // 历史实现允许4096个中间NAL加最后一项，不能静默收窄为4096。
        if count > 4096 {
            return Err(DecodeError::InvalidInput);
        }
        emit(&data[begin..at])?;
        begin = at + width;
        at = begin;
    }
    emit(&data[begin..])
}
