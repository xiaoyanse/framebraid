use crate::{Error, Result};
use std::path::{Component, Path};

// 仅遍历ONNX已知消息中的external_data；不以字符串匹配猜模型是否含外部权重。
fn varint(b: &[u8], at: &mut usize) -> Result<u64> {
    let mut value = 0;
    for shift in (0..70).step_by(7) {
        let v = *b.get(*at).ok_or_else(invalid)?;
        *at += 1;
        if shift == 63 && v > 1 {
            return Err(invalid());
        }
        value |= u64::from(v & 127) << shift;
        if v & 128 == 0 {
            return Ok(value);
        }
    }
    Err(invalid())
}
fn invalid() -> Error {
    Error("vision.modelLoadFailed".into())
}
fn fields<'a>(mut b: &'a [u8], mut f: impl FnMut(u64, &'a [u8]) -> Result<()>) -> Result<()> {
    while !b.is_empty() {
        let mut at = 0;
        let key = varint(b, &mut at)?;
        if key >> 3 == 0 {
            return Err(invalid());
        }
        match key & 7 {
            0 => {
                varint(b, &mut at)?;
            }
            1 => at += 8,
            5 => at += 4,
            2 => {
                let len = usize::try_from(varint(b, &mut at)?).map_err(|_| invalid())?;
                let end = at.checked_add(len).ok_or_else(invalid)?;
                f(key >> 3, b.get(at..end).ok_or_else(invalid)?)?;
                at = end;
            }
            _ => return Err(invalid()),
        }
        b = b.get(at..).ok_or_else(invalid)?;
    }
    Ok(())
}
fn visit(kind: u8, b: &[u8], depth: usize, base: Option<&Path>) -> Result<()> {
    if depth > 64 {
        return Err(invalid());
    }
    fields(b, |field, bytes| {
        let next = match (kind, field) {
            (0, 7) => Some(1),
            (0, 20) => Some(6),
            (0, 25) => Some(7),
            (1, 1) | (7, 7) => Some(2),
            (1, 5) => Some(4),
            (1, 15) => Some(5),
            (2, 5) => Some(3),
            (3, 5 | 10) | (5, 1 | 2) => Some(4),
            (3, 6 | 11) | (6, 1 | 2) => Some(1),
            (3, 22 | 23) => Some(5),
            (4, 13) => {
                let mut key = None;
                let mut value = None;
                fields(bytes, |n, b| {
                    if n == 1 {
                        key = Some(b);
                    }
                    if n == 2 {
                        value = Some(b);
                    }
                    Ok(())
                })?;
                let Some(base) = base else {
                    return Err(Error("vision.externalDataUnsupported".into()));
                };
                if key == Some(b"location".as_slice()) {
                    let location =
                        std::str::from_utf8(value.ok_or_else(invalid)?).map_err(|_| invalid())?;
                    let path = Path::new(location);
                    if location.is_empty()
                        || location.contains(':')
                        || !path.components().all(|c| matches!(c, Component::Normal(_)))
                    {
                        return Err(Error("vision.externalDataUnsupported".into()));
                    }
                    let target = base
                        .join(path)
                        .canonicalize()
                        .map_err(|_| Error("vision.externalDataUnsupported".into()))?;
                    if !target.starts_with(base) {
                        return Err(Error("vision.externalDataUnsupported".into()));
                    }
                }
                None
            }
            _ => None,
        };
        if let Some(next) = next {
            visit(next, bytes, depth + 1, base)?;
        }
        Ok(())
    })
}
pub fn validate_external(bytes: &[u8], path: Option<&Path>) -> Result<()> {
    let directory = path
        .and_then(Path::parent)
        .map(|p| {
            if p.as_os_str().is_empty() {
                Path::new(".")
            } else {
                p
            }
        })
        .map(Path::canonicalize)
        .transpose()
        .map_err(|_| invalid())?;
    visit(0, bytes, 0, directory.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_wire_is_rejected() {
        assert!(validate_external(&[0xff; 20], None).is_err());
        assert!(validate_external(&[0x3a, 50, 1], None).is_err());
    }
    #[test]
    fn relative_model_name_uses_working_directory() {
        assert!(validate_external(&[], Some(Path::new("model.onnx"))).is_ok());
    }
    #[test]
    fn memory_external_weights_are_rejected() {
        let entry = b"\x0a\x08location\x12\x01x";
        let tensor = [vec![0x6a, entry.len() as u8], entry.to_vec()].concat();
        let graph = [vec![0x2a, tensor.len() as u8], tensor].concat();
        let model = [vec![0x3a, graph.len() as u8], graph].concat();
        assert_eq!(
            validate_external(&model, None).unwrap_err().0,
            "vision.externalDataUnsupported"
        );
    }
}
