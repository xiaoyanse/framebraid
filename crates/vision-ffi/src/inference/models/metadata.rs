//! 模型IO的固定合同内副本；先校验、再fallible分配，视图只借用最终拥有者。
use super::*;

fn storage<T>(count: usize) -> Result<Vec<T>> {
    let mut value = Vec::new();
    value
        .try_reserve_exact(count)
        .map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
    if value.capacity() != count {
        return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
    }
    Ok(value)
}
fn validate(specs: &[TensorInfo]) -> Result<()> {
    if specs.is_empty()
        || specs.len() > 32
        || specs
            .iter()
            .any(|s| s.name.len() > 4096 || s.name.contains('\0') || s.shape.len() > 8)
    {
        return Err(AM_ERR_VISION_IO_UNSUPPORTED);
    }
    Ok(())
}
fn shape(values: &[i64]) -> Result<Vec<i64>> {
    let mut result = storage(values.len())?;
    result.extend_from_slice(values);
    Ok(result)
}
fn name(value: &str) -> Result<CString> {
    let mut bytes = storage(value.len() + 1)?;
    bytes.extend_from_slice(value.as_bytes());
    bytes.push(0);
    CString::from_vec_with_nul(bytes).map_err(|_| AM_ERR_VISION_IO_UNSUPPORTED)
}
pub(super) fn clone_specs(specs: &[TensorInfo]) -> Result<Vec<TensorInfo>> {
    validate(specs)?;
    let mut result = storage(specs.len())?;
    for spec in specs {
        let mut name = String::new();
        name.try_reserve_exact(spec.name.len())
            .map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        if name.capacity() != spec.name.len() {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        name.push_str(&spec.name);
        result.push(TensorInfo {
            name,
            dtype: spec.dtype,
            shape: shape(&spec.shape)?,
        });
    }
    Ok(result)
}
pub(super) fn ios(specs: &[TensorInfo], batch: u32) -> Result<(Vec<Io>, Vec<am_io_spec_t>)> {
    validate(specs)?;
    let mut owners = storage(specs.len())?;
    for spec in specs {
        let mut min = storage(spec.shape.len())?;
        let mut max = storage(spec.shape.len())?;
        for (axis, dim) in spec.shape.iter().enumerate() {
            min.push(if *dim < 0 { 1 } else { *dim });
            max.push(if *dim >= 0 {
                *dim
            } else if axis == 0 && batch == 1 {
                64
            } else {
                8192
            });
        }
        owners.push(Io {
            name: name(&spec.name)?,
            shape: shape(&spec.shape)?,
            min,
            max,
        });
    }
    let mut views = storage(specs.len())?;
    for (owner, spec) in owners.iter().zip(specs) {
        views.push(am_io_spec_t {
            name: owner.name.as_ptr(),
            dtype: spec.dtype,
            shape: owner.shape.as_ptr(),
            shape_len: owner.shape.len() as u32,
            dyn_min: owner.min.as_ptr(),
            dyn_max: owner.max.as_ptr(),
            ..Default::default()
        });
    }
    Ok((owners, views))
}

#[cfg(test)]
mod tests;
