//! 开放词汇词表绑定（A1，合同 §4.1）：条目校验、文本塔前向编码与嵌入快照。
//! tokenization 归调用方（与文本塔配对的 tokenizer），本层只消费 token ids；
//! 嵌入空间配对不校验（A3 诚实边界：维度一致不证明空间配对）。
use super::{Model, TensorData};
use crate::{Error, Result};
use std::collections::HashSet;

/// I64 dtype（与 C ABI AM_DTYPE_I64=2 对齐；core 层不依赖 sdk-abi，字面量对齐既有约定）。
const DTYPE_I64: u32 = 2;

/// 条目与提示词上限（合同 §4.1：entry 1..=256、组内提示词 1..=64）。
pub const MAX_ENTRIES: usize = 256;
pub const MAX_PROMPTS_PER_ENTRY: usize = 64;

pub struct EntrySpec<'a> {
    pub label: &'a str,
    pub token_rows: u32,
}

/// 绑定快照：构建完成后只读；换代=整体替换（调用侧 Arc 原子换）。
pub struct VocabEmbedding {
    /// (label, prompt_rows)；boxes.class_id 即本表索引。
    pub entries: Vec<(String, u32)>,
    /// Σrows × dim 行主序 f32；条目行区由 row_ranges 查询。
    pub matrix: Vec<f32>,
    pub dim: usize,
    row_ranges: Vec<(usize, usize)>,
}

impl VocabEmbedding {
    /// 条目在矩阵中的行区间 [start, end)。
    pub fn row_range(&self, entry: usize) -> (usize, usize) {
        self.row_ranges[entry]
    }
    pub fn total_rows(&self) -> usize {
        self.row_ranges.last().map_or(0, |(_, end)| *end)
    }
}

/// 校验词表结构（不触模型）：label 非空且全表唯一、条目数与组内行数在合同区间。
pub fn validate(entries: &[EntrySpec<'_>]) -> Result<()> {
    if entries.is_empty() || entries.len() > MAX_ENTRIES {
        return Err(Error(format!(
            "sdk.invalidArgument|field=entry_count,value={},range=1..{MAX_ENTRIES}",
            entries.len()
        )));
    }
    let mut seen = HashSet::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        if entry.label.is_empty() {
            return Err(Error(format!(
                "sdk.invalidArgument|field=label,index={index},reason=empty"
            )));
        }
        if !seen.insert(entry.label) {
            return Err(Error(format!(
                "sdk.invalidArgument|field=label,index={index},reason=duplicate,value={}",
                entry.label
            )));
        }
        if entry.token_rows == 0 || entry.token_rows as usize > MAX_PROMPTS_PER_ENTRY {
            return Err(Error(format!(
                "sdk.invalidArgument|field=token_rows,index={index},value={},range=1..{MAX_PROMPTS_PER_ENTRY}",
                entry.token_rows
            )));
        }
    }
    Ok(())
}

/// 文本塔前置校验：档案 task=text-encoder、单输入单输出、seq_len 与 token 列数一致。
/// 返回 (输入名, seq_len, embed_dim)；不满足返回错误详情（§7 约定）。
pub fn encoder_signature(encoder: &Model) -> Result<(String, usize, usize)> {
    let profile = encoder
        .profile
        .filter(|p| p.task == crate::profiles::Task::TextEncoder)
        .ok_or_else(|| {
            Error("sdk.invalidArgument|field=text_encoder,reason=notTextEncoderProfile".into())
        })?;
    let input = &encoder.inputs[0];
    let output = &encoder.outputs[0];
    let seq_len = input.shape[1];
    let dim = output.shape[1];
    if seq_len == 0 || dim == 0 {
        return Err(Error(format!(
            "vision.profileMismatch|profile={}",
            profile.id
        )));
    }
    Ok((input.name.clone(), seq_len as usize, dim as usize))
}

/// 逐条目执行文本塔前向，拼装嵌入矩阵。tokens 为全部条目展平的行主序
/// token ids（Σrows × cols）。detector_dim 提供时校验嵌入维度一致
///（合同：与检测档案第二输入 embed 维失配拒绝）。
pub fn encode(
    encoder: &mut Model,
    entries: &[EntrySpec<'_>],
    tokens: &[i64],
    cols: usize,
    detector_dim: Option<usize>,
) -> Result<VocabEmbedding> {
    let (_, seq_len, dim) = encoder_signature(encoder)?;
    if cols != seq_len {
        return Err(Error(format!(
            "sdk.invalidArgument|field=token_cols,value={cols},expected={seq_len}"
        )));
    }
    if let Some(expected) = detector_dim {
        if expected != dim {
            return Err(Error(format!(
                "vision.profileMismatch|field=embed_dim,value={dim},expected={expected}"
            )));
        }
    }
    let total_rows: usize = entries.iter().map(|e| e.token_rows as usize).sum();
    if tokens.len()
        != total_rows.checked_mul(cols).ok_or_else(|| {
            Error(format!(
                "sdk.invalidArgument|field=tokens,value={},expected={total_rows}x{cols}",
                tokens.len()
            ))
        })?
    {
        return Err(Error(format!(
            "sdk.invalidArgument|field=tokens,value={},expected={total_rows}x{cols}",
            tokens.len()
        )));
    }
    let input_name = encoder.inputs[0].name.clone();
    let mut matrix = Vec::with_capacity(total_rows * dim);
    let mut row_ranges = Vec::with_capacity(entries.len());
    let mut offset = 0usize;
    for entry in entries {
        let rows = entry.token_rows as usize;
        let bytes = rows
            .checked_mul(cols)
            .and_then(|n| n.checked_mul(8))
            .ok_or_else(|| Error("sdk.resourceExhausted|field=tokens".into()))?;
        let slice = &tokens[offset * cols..(offset + rows) * cols];
        // SAFETY：i64 切片到字节的平台表示转换（小端 Windows x64，ORT 输入同序）。
        let raw = unsafe { std::slice::from_raw_parts(slice.as_ptr().cast::<u8>(), bytes) };
        let shape = vec![rows as i64, cols as i64];
        let input = TensorData::new(
            super::TensorInfo {
                name: input_name.clone(),
                dtype: DTYPE_I64,
                shape,
            },
            raw,
        )?;
        let outputs = encoder.run(std::slice::from_ref(&input), dim * rows * 4)?;
        let embedding = outputs.first().ok_or_else(|| {
            Error(format!(
                "vision.engineFailed|model=text_encoder,reason=noOutput,entry={}",
                entry.label
            ))
        })?;
        let floats = embedding.floats()?;
        if floats.len() != rows * dim {
            return Err(Error(format!(
                "vision.profileMismatch|field=embedding_rows,entry={},value={},expected={rows}x{dim}",
                entry.label,
                floats.len()
            )));
        }
        matrix.extend_from_slice(floats);
        row_ranges.push((offset, offset + rows));
        offset += rows;
    }
    Ok(VocabEmbedding {
        entries: entries
            .iter()
            .map(|e| (e.label.to_string(), e.token_rows))
            .collect(),
        matrix,
        dim,
        row_ranges,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_empty_duplicate_and_overflow() {
        assert!(validate(&[EntrySpec {
            label: "hand",
            token_rows: 1
        }])
        .is_ok());
        let err = validate(&[]).err().unwrap().0;
        assert!(err.contains("entry_count"), "{err}");
        let err = validate(&[EntrySpec {
            label: "",
            token_rows: 1,
        }])
        .err()
        .unwrap()
        .0;
        assert!(err.contains("reason=empty"), "{err}");
        let err = validate(&[
            EntrySpec {
                label: "a",
                token_rows: 1,
            },
            EntrySpec {
                label: "a",
                token_rows: 2,
            },
        ])
        .err()
        .unwrap()
        .0;
        assert!(err.contains("reason=duplicate"), "{err}");
        let err = validate(&[EntrySpec {
            label: "a",
            token_rows: 0,
        }])
        .err()
        .unwrap()
        .0;
        assert!(err.contains("token_rows"), "{err}");
        let many: Vec<EntrySpec> = (0..=MAX_ENTRIES)
            .map(|i| EntrySpec {
                label: Box::leak(format!("l{i}").into_boxed_str()),
                token_rows: 1,
            })
            .collect();
        assert!(validate(&many).is_err());
    }

    #[test]
    fn row_ranges_partition_the_matrix() {
        // 直接构造（encode 需要真模型，快照结构自验）。
        let mut vocab = VocabEmbedding {
            entries: vec![("a".into(), 2), ("b".into(), 3)],
            matrix: vec![0.0; 5 * 4],
            dim: 4,
            row_ranges: vec![(0, 2), (2, 5)],
        };
        vocab.matrix[8] = 1.5; // entry b 首行首列
        assert_eq!(vocab.total_rows(), 5);
        assert_eq!(vocab.row_range(0), (0, 2));
        assert_eq!(vocab.row_range(1), (2, 5));
        assert_eq!(vocab.matrix[vocab.row_range(1).0 * vocab.dim], 1.5);
    }
}
