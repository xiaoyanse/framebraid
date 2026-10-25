use super::{model::validate_input_data, Model, TensorData, TensorInfo};
use crate::{Error, Result};

pub(super) fn from_session(
    session: &ort::session::Session,
    inputs: &[TensorInfo],
    outputs: &[TensorInfo],
) -> Result<u32> {
    let metadata = session
        .metadata()
        .map_err(|_| Error("vision.modelLoadFailed".into()))?;
    // 先查键是否存在，避免底层把缺失值的NULL转换成空字符串。
    let declaration = if metadata
        .custom_keys()
        .map_err(|_| Error("vision.modelLoadFailed".into()))?
        .iter()
        .any(|k| k == "am.sdk.batch_axis")
    {
        Some(
            metadata
                .custom("am.sdk.batch_axis")
                .ok_or_else(unsupported)?,
        )
    } else {
        None
    };
    declared_batch(declaration.as_deref(), inputs, outputs)
}

/// 仅接受模型作者的显式共同首轴声明；不从动态维或名称猜测batch。
pub(super) fn declared_batch(
    axis: Option<&str>,
    inputs: &[TensorInfo],
    outputs: &[TensorInfo],
) -> Result<u32> {
    let Some(axis) = axis else { return Ok(0) };
    if axis != "0" || inputs.is_empty() || outputs.is_empty() {
        return Err(unsupported());
    }
    let size = *inputs[0].shape.first().ok_or_else(unsupported)?;
    if (size != -1 && !(1..=64).contains(&size))
        || inputs
            .iter()
            .chain(outputs)
            .any(|v| v.shape.first() != Some(&size))
    {
        return Err(unsupported());
    }
    Ok(match size {
        -1 => 1,
        1 => 0,
        n => n as u32,
    })
}
fn unsupported() -> Error {
    Error("vision.batchUnsupported".into())
}
pub fn validate_count(batch: u32, inputs: &[TensorInfo]) -> Result<()> {
    if batch == 0 {
        return Ok(());
    }
    let size = inputs
        .first()
        .and_then(|v| v.shape.first())
        .copied()
        .ok_or_else(unsupported)?;
    if !(1..=64).contains(&size) {
        return Err(Error("vision.dynamicRangeExceeded".into()));
    }
    if (batch >= 2 && size != i64::from(batch))
        || inputs.iter().any(|v| v.shape.first() != Some(&size))
    {
        return Err(Error("vision.inputShapeMismatch".into()));
    }
    Ok(())
}
fn allocated(bytes: usize) -> Result<usize> {
    bytes.checked_next_multiple_of(8).ok_or_else(exhausted)
}
fn exhausted() -> Error {
    Error("sdk.resourceExhausted".into())
}

pub(super) fn combine(
    specs: &[TensorInfo],
    items: &[&[TensorData]],
    budget: usize,
) -> Result<Vec<TensorData>> {
    let single = specs
        .iter()
        .cloned()
        .map(|mut s| {
            s.shape[0] = 1;
            s
        })
        .collect::<Vec<_>>();
    for item in items {
        validate_input_data(&single, item)?;
    }
    let mut plans = Vec::new();
    let mut total = 0usize;
    for spec in specs {
        let tensors = items
            .iter()
            .map(|item| {
                item.iter()
                    .find(|v| v.info.name == spec.name)
                    .ok_or_else(|| Error("vision.inputNameUnknown".into()))
            })
            .collect::<Result<Vec<_>>>()?;
        let first = &tensors[0].info;
        if tensors.iter().any(|t| t.info.shape != first.shape) {
            return Err(Error("vision.inputShapeMismatch".into()));
        }
        let mut info = first.clone();
        info.shape[0] = items.len() as i64;
        let bytes = super::byte_count(info.dtype, &info.shape)?;
        total = total.checked_add(allocated(bytes)?).ok_or_else(exhausted)?;
        if total > budget {
            return Err(exhausted());
        }
        plans.push((info, tensors));
    }
    // 全组元数据和字节预算通过后才分配、复制合并输入。
    plans
        .into_iter()
        .map(|(info, tensors)| {
            let mut joined = TensorData::zeroed(info)?;
            let row_bytes = tensors[0].bytes().len();
            for (index, tensor) in tensors.iter().enumerate() {
                joined.bytes_mut()[index * row_bytes..(index + 1) * row_bytes]
                    .copy_from_slice(tensor.bytes());
            }
            Ok(joined)
        })
        .collect()
}
fn split(raw: Vec<TensorData>, count: usize, budget: usize) -> Result<Vec<Vec<TensorData>>> {
    let mut total = 0usize;
    for tensor in &raw {
        if tensor.info.shape.first() != Some(&(count as i64))
            || !tensor.bytes().len().is_multiple_of(count)
        {
            return Err(unsupported());
        }
        total = total
            .checked_add(allocated(tensor.bytes().len())?)
            .and_then(|n| {
                allocated(tensor.bytes().len() / count)
                    .ok()
                    .and_then(|r| r.checked_mul(count))
                    .and_then(|r| n.checked_add(r))
            })
            .ok_or_else(exhausted)?;
    }
    if total > budget {
        return Err(exhausted());
    }
    let mut result = (0..count)
        .map(|_| Vec::with_capacity(raw.len()))
        .collect::<Vec<_>>();
    for tensor in &raw {
        let bytes = tensor.bytes().len() / count;
        let mut info = tensor.info.clone();
        info.shape[0] = 1;
        for (row, output) in result.iter_mut().enumerate() {
            output.push(TensorData::new(
                info.clone(),
                &tensor.bytes()[row * bytes..(row + 1) * bytes],
            )?);
        }
    }
    Ok(result)
}
impl Model {
    pub fn native_batch(&self) -> u32 {
        self.batch
    }

    /// 一次原生Run；每项首轴为1，输入按名称合并，返回顺序保持逐项身份。
    /// input_budget覆盖额外合并缓冲与ORT副本；output_budget覆盖完整输出和拆分副本的并存峰值。
    pub fn run_batch(
        &mut self,
        items: &[&[TensorData]],
        input_budget: usize,
        output_budget: usize,
    ) -> Result<Vec<Vec<TensorData>>> {
        if self.batch == 0 || declared_batch(Some("0"), &self.inputs, &self.outputs)? != self.batch
        {
            return Err(unsupported());
        }
        if items.is_empty() || items.len() > 64 {
            return Err(Error("vision.dynamicRangeExceeded".into()));
        }
        if self.batch >= 2 && items.len() < self.batch as usize {
            return Err(Error("vision.insufficientBatchRejected".into()));
        }
        if self.batch >= 2 && items.len() != self.batch as usize {
            return Err(Error("vision.inputShapeMismatch".into()));
        }
        if input_budget == 0 || output_budget == 0 {
            return Err(exhausted());
        }
        let tensors = combine(&self.inputs, items, input_budget / 2)?;
        let raw = self.run(&tensors, output_budget / 2)?;
        split(raw, items.len(), output_budget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn spec(shape: &[i64]) -> TensorInfo {
        TensorInfo {
            name: "x".into(),
            dtype: 4,
            shape: shape.to_vec(),
        }
    }
    #[test]
    fn only_explicit_consistent_axis_is_batch_proof() {
        let s = [spec(&[-1, 2])];
        assert_eq!(declared_batch(None, &s, &s).unwrap(), 0);
        assert_eq!(declared_batch(Some("0"), &s, &s).unwrap(), 1);
        assert!(declared_batch(Some("1"), &s, &s).is_err());
        assert!(declared_batch(Some(""), &s, &s).is_err());
        assert!(declared_batch(Some("0"), &s, &[spec(&[])]).is_err());
        assert!(declared_batch(Some("0"), &[spec(&[4, 2])], &s).is_err());
        assert_eq!(
            declared_batch(Some("0"), &[spec(&[4, 2])], &[spec(&[4, 3])]).unwrap(),
            4
        );
        assert_eq!(
            declared_batch(Some("0"), &[spec(&[1, 2])], &[spec(&[1, 3])]).unwrap(),
            0
        );
    }
    #[test]
    fn aligned_peak_budget_and_rows_are_checked_before_split() {
        let raw = TensorData::new(spec(&[3, 1]), &[1, 2, 3]).unwrap();
        assert!(split(vec![raw], 3, 31).is_err());
        let raw = TensorData::new(spec(&[3, 1]), &[1, 2, 3]).unwrap();
        let rows = split(vec![raw], 3, 32).unwrap();
        assert_eq!(rows[2][0].bytes(), &[3]);
        let a = [TensorData::new(spec(&[1, 2]), &[1, 2]).unwrap()];
        let b = [TensorData::new(spec(&[1, 3]), &[1, 2, 3]).unwrap()];
        assert!(combine(&[spec(&[-1, -1])], &[&a, &b], 1024).is_err());
        assert!(combine(&[spec(&[-1, 2])], &[&a, &a], 7).is_err());
    }
}
