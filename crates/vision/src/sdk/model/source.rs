//! 读取模型前核实际打开文件的尺寸；读取期间增长不能扩容绕过上限。
use super::ModelSource;
use crate::{Error, Result};
use std::{borrow::Cow, fs::File, io::Read};

const MAX_MODEL_BYTES: u64 = 256 * 1024 * 1024;

pub(super) fn read(source: ModelSource<'_>) -> Result<Cow<'_, [u8]>> {
    match source {
        ModelSource::Path(path) => {
            let mut file = File::open(path).map_err(|_| load_failed())?;
            let length = file.metadata().map_err(|_| load_failed())?.len();
            Ok(Cow::Owned(read_sized(&mut file, length)?))
        }
        ModelSource::Memory(bytes) => {
            if bytes.len() as u64 > MAX_MODEL_BYTES {
                return Err(too_large());
            }
            Ok(Cow::Borrowed(bytes))
        }
    }
}

fn read_sized(reader: &mut impl Read, length: u64) -> Result<Vec<u8>> {
    read_sized_with_limit(reader, length, MAX_MODEL_BYTES)
}

fn read_sized_with_limit(reader: &mut impl Read, length: u64, limit: u64) -> Result<Vec<u8>> {
    if length > limit {
        return Err(too_large());
    }
    let length = usize::try_from(length).map_err(|_| too_large())?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| Error("sdk.resourceExhausted".into()))?;
    if bytes.capacity() as u64 > limit {
        return Err(Error("sdk.resourceExhausted".into()));
    }
    bytes.resize(length, 0);
    reader.read_exact(&mut bytes).map_err(|_| load_failed())?;
    let mut extra = [0];
    loop {
        match reader.read(&mut extra) {
            Ok(0) => return Ok(bytes),
            Ok(_) if length as u64 == limit => return Err(too_large()),
            Ok(_) => return Err(load_failed()),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(load_failed()),
        }
    }
}

fn load_failed() -> Error {
    Error("vision.modelLoadFailed".into())
}
fn too_large() -> Error {
    Error("vision.modelTooLarge".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn memory_source_borrows_and_preserves_bytes() {
        let input = [1, 2, 3];
        let bytes = read(ModelSource::Memory(&input)).unwrap();
        assert!(matches!(bytes, Cow::Borrowed(_)));
        assert_eq!(bytes.as_ptr(), input.as_ptr());
        assert_eq!(&*bytes, &input);
    }

    #[test]
    fn memory_model_copy_cost_is_measured_without_native_session() {
        let bytes = vec![7u8; 8 * 1024 * 1024];
        let rounds = 20;
        let started = std::time::Instant::now();
        let old = super::super::validation_tests::measured(|| {
            for _ in 0..rounds {
                std::hint::black_box(std::hint::black_box(&bytes).to_vec());
            }
        });
        let old_us = started.elapsed().as_micros();
        let started = std::time::Instant::now();
        let new = super::super::validation_tests::measured(|| {
            for _ in 0..rounds {
                std::hint::black_box(
                    read(ModelSource::Memory(std::hint::black_box(&bytes))).unwrap(),
                );
            }
        });
        let new_us = started.elapsed().as_micros();
        assert_eq!(old, rounds);
        assert_eq!(new, 0);
        println!("r3_model_memory_copy bytes={} rounds={rounds} old_allocations={old} new_allocations={new} old_us={old_us} new_us={new_us}",bytes.len());
    }

    #[test]
    fn sized_read_rejects_growth_truncation_and_oversize() {
        assert_eq!(read_sized(&mut Cursor::new(b"abc"), 3).unwrap(), b"abc");
        assert_eq!(
            read_sized(&mut Cursor::new(b"abc"), 2).unwrap_err().0,
            "vision.modelLoadFailed"
        );
        assert_eq!(
            read_sized(&mut Cursor::new(b"abc"), 4).unwrap_err().0,
            "vision.modelLoadFailed"
        );
        let mut input = Cursor::new(b"abc");
        assert_eq!(
            read_sized(&mut input, MAX_MODEL_BYTES + 1).unwrap_err().0,
            "vision.modelTooLarge"
        );
        assert_eq!(input.position(), 0);
        assert_eq!(
            read_sized_with_limit(&mut Cursor::new(b"abcd"), 4, 4).unwrap(),
            b"abcd"
        );
        assert_eq!(
            read_sized_with_limit(&mut Cursor::new(b"abcde"), 4, 4)
                .unwrap_err()
                .0,
            "vision.modelTooLarge"
        );
    }
}
