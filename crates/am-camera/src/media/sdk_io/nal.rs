use super::IoError;
pub(super) const MAX_PACKET: usize = 16 * 1024 * 1024;

pub(super) struct Framing {
    length_bytes: usize,
    pub parameters: Vec<u8>,
}
impl Framing {
    pub fn new(codec: u32, extra: &[u8]) -> Result<Self, IoError> {
        if extra.len() > MAX_PACKET {
            return Err(IoError::Resource);
        }
        if codec == 4 || extra.is_empty() || annex_b(extra) {
            return Ok(Self {
                length_bytes: 0,
                parameters: extra.to_vec(),
            });
        }
        let mut output = Vec::new();
        let length_bytes;
        if codec == 1 && extra.len() >= 7 && extra[0] == 1 {
            length_bytes = (extra[4] & 3) as usize + 1;
            let mut pos = 6;
            for _ in 0..(extra[5] & 31) {
                append_config(extra, &mut pos, &mut output)?;
            }
            let count = *extra.get(pos).ok_or(IoError::Invalid)?;
            pos += 1;
            for _ in 0..count {
                append_config(extra, &mut pos, &mut output)?;
            }
        } else if codec == 2 && extra.len() >= 23 && extra[0] == 1 {
            length_bytes = (extra[21] & 3) as usize + 1;
            let mut pos = 23;
            for _ in 0..extra[22] {
                pos += 1;
                let count = number(extra, &mut pos, 2)?;
                for _ in 0..count {
                    append_config(extra, &mut pos, &mut output)?;
                }
            }
        } else {
            return Err(IoError::Unsupported);
        }
        Ok(Self {
            length_bytes,
            parameters: output,
        })
    }
    pub fn packet(&mut self, data: &[u8], key: bool, codec: u32) -> Result<Vec<u8>, IoError> {
        if data.len() > MAX_PACKET {
            return Err(IoError::Resource);
        }
        if codec == 4 {
            return Ok(data.to_vec());
        }
        let mut converted = Vec::new();
        let data = if annex_b(data) || self.length_bytes == 0 {
            data
        } else {
            let mut pos = 0;
            while pos < data.len() {
                let length = number(data, &mut pos, self.length_bytes)?;
                let end = pos
                    .checked_add(length)
                    .filter(|&end| end <= data.len())
                    .ok_or(IoError::Invalid)?;
                if length == 0 {
                    return Err(IoError::Invalid);
                }
                if converted.len() + 4 + length > MAX_PACKET {
                    return Err(IoError::Resource);
                }
                converted.extend_from_slice(&[0, 0, 0, 1]);
                converted.extend_from_slice(&data[pos..end]);
                pos = end;
            }
            &converted
        };
        self.observe(data, codec)?;
        let mut output = Vec::new();
        output
            .try_reserve(data.len().saturating_add(self.parameters.len()))
            .map_err(|_| IoError::Resource)?;
        if key {
            output.extend_from_slice(&self.parameters);
        }
        if annex_b(data) {
            if output.len() + data.len() > MAX_PACKET {
                return Err(IoError::Resource);
            }
            output.extend_from_slice(data);
        } else {
            return Err(IoError::Invalid);
        }
        Ok(output)
    }
    fn observe(&mut self, data: &[u8], codec: u32) -> Result<(), IoError> {
        let wanted: &[u8] = if codec == 1 { &[7, 8] } else { &[32, 33, 34] };
        let old = parameter_sets(&self.parameters, codec, wanted)?;
        let new = parameter_sets(data, codec, wanted)?;
        if new.is_empty() {
            return Ok(());
        }
        let mut combined = Vec::new();
        for kind in wanted {
            let current = if new.iter().any(|(ty, _)| ty == kind) {
                &new
            } else {
                &old
            };
            for (_, bytes) in current.iter().filter(|(ty, _)| ty == kind) {
                if combined.len() + 4 + bytes.len() > MAX_PACKET {
                    return Err(IoError::Resource);
                }
                combined.extend_from_slice(&[0, 0, 0, 1]);
                combined.extend_from_slice(bytes);
            }
        }
        self.parameters = combined;
        Ok(())
    }
}
fn parameter_sets<'a>(
    data: &'a [u8],
    codec: u32,
    wanted: &[u8],
) -> Result<Vec<(u8, &'a [u8])>, IoError> {
    let mut output = Vec::new();
    let mut pos = 0;
    let mut previous = None;
    while pos + 3 <= data.len() {
        let length = if data[pos..].starts_with(&[0, 0, 0, 1]) {
            4
        } else if data[pos..].starts_with(&[0, 0, 1]) {
            3
        } else {
            0
        };
        if length != 0 {
            if let Some(start) = previous {
                record_parameter(&mut output, &data[start..pos], codec, wanted)?;
            }
            previous = Some(pos + length);
            pos += length;
        } else {
            pos += 1;
        }
    }
    if let Some(start) = previous {
        record_parameter(&mut output, &data[start..], codec, wanted)?;
    }
    Ok(output)
}
fn record_parameter<'a>(
    output: &mut Vec<(u8, &'a [u8])>,
    data: &'a [u8],
    codec: u32,
    wanted: &[u8],
) -> Result<(), IoError> {
    if let Some(first) = data.first() {
        let kind = if codec == 1 {
            first & 31
        } else {
            (first >> 1) & 63
        };
        if wanted.contains(&kind) {
            if output.len() >= 256 {
                return Err(IoError::Resource);
            }
            output.push((kind, data));
        }
    }
    Ok(())
}
fn annex_b(data: &[u8]) -> bool {
    data.starts_with(&[0, 0, 1]) || data.starts_with(&[0, 0, 0, 1])
}
fn number(data: &[u8], pos: &mut usize, length: usize) -> Result<usize, IoError> {
    let end = pos
        .checked_add(length)
        .filter(|&end| end <= data.len())
        .ok_or(IoError::Invalid)?;
    let value = data[*pos..end]
        .iter()
        .fold(0, |value, byte| (value << 8) | *byte as usize);
    *pos = end;
    Ok(value)
}
fn append_config(data: &[u8], pos: &mut usize, output: &mut Vec<u8>) -> Result<(), IoError> {
    let length = number(data, pos, 2)?;
    let end = pos
        .checked_add(length)
        .filter(|&end| end <= data.len())
        .ok_or(IoError::Invalid)?;
    if length == 0 || output.len() + length + 4 > MAX_PACKET {
        return Err(IoError::Invalid);
    }
    output.extend_from_slice(&[0, 0, 0, 1]);
    output.extend_from_slice(&data[*pos..end]);
    *pos = end;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn avcc_and_hvcc_are_bounded_annex_b() {
        let avcc = [1, 66, 0, 30, 255, 225, 0, 2, 103, 1, 1, 0, 2, 104, 2];
        let mut framing = Framing::new(1, &avcc).unwrap();
        assert_eq!(
            framing.packet(&[0, 0, 0, 2, 101, 3], false, 1).unwrap(),
            [0, 0, 0, 1, 101, 3]
        );
        assert!(framing.packet(&[0, 0, 0, 100, 1], false, 1).is_err());
        let mut hvcc = vec![0; 23];
        hvcc[0] = 1;
        hvcc[21] = 3;
        hvcc[22] = 1;
        hvcc.extend_from_slice(&[32, 0, 1, 0, 2, 64, 1]);
        let mut hevc = Framing::new(2, &hvcc).unwrap();
        assert_eq!(hevc.parameters, [0, 0, 0, 1, 64, 1]);
        assert_eq!(
            hevc.packet(&[0, 0, 0, 2, 38, 1], true, 2).unwrap().len(),
            12
        );
    }
    #[test]
    fn keyframes_use_current_inband_parameter_sets() {
        let mut framing = Framing::new(1, &[0, 0, 0, 1, 103, 1, 0, 0, 0, 1, 104, 2]).unwrap();
        framing
            .packet(&[0, 0, 0, 1, 103, 9, 0, 0, 0, 1, 104, 8], false, 1)
            .unwrap();
        let key = framing.packet(&[0, 0, 0, 1, 101, 3], true, 1).unwrap();
        assert!(key.starts_with(&[0, 0, 0, 1, 103, 9, 0, 0, 0, 1, 104, 8]));
        assert!(!key.windows(2).any(|pair| pair == [103, 1]));
        let mut many = Vec::new();
        for _ in 0..257 {
            many.extend_from_slice(&[0, 0, 0, 1, 103, 9]);
        }
        assert_eq!(
            framing.packet(&many, false, 1).err(),
            Some(IoError::Resource)
        );
    }
}
