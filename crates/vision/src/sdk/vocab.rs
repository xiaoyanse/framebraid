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

pub struct EntrySpec {
    pub label: String,
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
    /// 解码用布局（A2）：条目表（label, prompt_rows），行区间由前缀和确定。
    pub fn layout(&self) -> crate::profiles::VocabLayout {
        crate::profiles::VocabLayout {
            entries: self.entries.clone(),
        }
    }
    /// 构造注入检测模型第二输入的张量：2-D 档案（如 world-head）为 [total_rows, dim]；
    /// 3-D 档案（批 D 的 world 形态，txt_feats=[batch, num_prompts, 512] 每帧独立）
    /// 为 [1, total_rows, dim]（单帧快照；攒批 CPU 路径按项复制后首轴合并为
    /// [N, rows, dim]，GPU 路径经 replicate_frames 同语义平铺）。
    /// dtype/名字取检测模型第二输入声明（F32 嵌入）。
    pub fn injection_tensor(&self, declared: &super::TensorInfo) -> Result<TensorData> {
        let rows = self.total_rows();
        let shape = match declared.shape.len() {
            2 => vec![rows as i64, self.dim as i64],
            3 => vec![1, rows as i64, self.dim as i64],
            rank => {
                return Err(Error(format!(
                    "vision.profileMismatch|field=vocab_input_rank,value={rank},expected=2|3"
                )))
            }
        };
        // SAFETY：f32 切片到字节的平台表示转换（引擎输入同序）。
        let raw = unsafe {
            std::slice::from_raw_parts(self.matrix.as_ptr().cast::<u8>(), self.matrix.len() * 4)
        };
        TensorData::new(
            super::TensorInfo {
                name: declared.name.clone(),
                dtype: declared.dtype,
                shape,
            },
            raw,
        )
    }
}

/// 校验词表结构（不触模型）：label 非空且全表唯一、条目数与组内行数在合同区间。
pub fn validate(entries: &[EntrySpec]) -> Result<()> {
    if entries.is_empty() || entries.len() > MAX_ENTRIES {
        return Err(Error(format!(
            "sdk.invalidArgument|field=entry_count,value={},range=1..{MAX_ENTRIES}",
            entries.len()
        )));
    }
    let mut seen = HashSet::<&str>::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        if entry.label.is_empty() {
            return Err(Error(format!(
                "sdk.invalidArgument|field=label,index={index},reason=empty"
            )));
        }
        if !seen.insert(entry.label.as_str()) {
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

/// 文本塔前置校验：档案 task=text-encoder、单输入单输出。
/// 返回 (输入名, seq_len, embed_dim)；seq_len 为 None 表示模型 seq 维动态
/// （批 D 的 dynseq 档案——token 列数不受档案限定，由调用方词表决定）。
/// 不满足返回错误详情（§7 约定）。
pub fn encoder_signature(encoder: &Model) -> Result<(String, Option<usize>, usize)> {
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
    let seq = (seq_len > 0).then_some(seq_len as usize);
    Ok((input.name.clone(), seq, dim as usize))
}

/// 3-D 文本输入的批注入平铺（批 D vocab 批语义统一）：单帧快照 [1, rows, dim]
/// 复制 frames 份为 [frames, rows, dim]——txt_feats 每帧独立，逐项完整快照
/// （与 CPU 攒批路径 combine 按项合并 [N, rows, dim] 字节级同构）。2-D 快照
/// （无批量轴）原样返回。
pub fn replicate_frames(tensor: &TensorData, frames: usize) -> Result<TensorData> {
    if tensor.info.shape.len() == 2 {
        // 2-D 文本输入无批量轴：单份快照语义（现网 world-head 形态）。
        return TensorData::new(tensor.info.clone(), tensor.bytes());
    }
    if frames == 0 || tensor.info.shape.first() != Some(&1) {
        return Err(Error("vision.inputShapeMismatch|field=vocab_batch".into()));
    }
    let mut shape = tensor.info.shape.clone();
    shape[0] = frames as i64;
    let bytes = tensor.bytes();
    let mut tiled = Vec::with_capacity(bytes.len() * frames);
    for _ in 0..frames {
        tiled.extend_from_slice(bytes);
    }
    TensorData::new(
        super::TensorInfo {
            name: tensor.info.name.clone(),
            dtype: tensor.info.dtype,
            shape,
        },
        &tiled,
    )
}

/// 亲和头档案（双输入 detect、classes 未声明）的单帧输出字节估计（批 D）：
/// 输出行数=4+提示词行（vocab 布局运行期已知），anchors 取档案输出末维固定声明
/// （world 形态 640²→8400，由钉死的档案输入尺寸保证）。非亲和档案/全动态无
/// 声明返回 None（调用方维持既有口径）。
pub fn affinity_row_bytes(
    profile: &crate::profiles::Profile,
    layout: &crate::profiles::VocabLayout,
) -> Option<usize> {
    if profile.task != crate::profiles::Task::Detect
        || profile.classes.is_some()
        || profile.inputs.len() < 2
    {
        return None;
    }
    let shape = profile.outputs.first()?.shape.as_slice();
    let anchors = match profile.postprocess.layout {
        crate::profiles::Layout::Transposed => shape[shape.len() - 1],
        crate::profiles::Layout::RowMajor => shape[shape.len() - 2],
    };
    if anchors == 0 {
        return None;
    }
    let prompt_rows = layout
        .entries
        .iter()
        .map(|(_, rows)| *rows as usize)
        .sum::<usize>();
    let rows = 4usize.checked_add(prompt_rows)?;
    rows.checked_mul(anchors)?.checked_mul(4)
}

/// 逐条目执行文本塔前向，拼装嵌入矩阵。tokens 为全部条目展平的行主序
/// token ids（Σrows × cols）。detector_dim 提供时校验嵌入维度一致
///（合同：与检测档案第二输入 embed 维失配拒绝）。
pub fn encode(
    encoder: &mut Model,
    entries: &[EntrySpec],
    tokens: &[i64],
    cols: usize,
    detector_dim: Option<usize>,
) -> Result<VocabEmbedding> {
    let (_, seq_len, dim) = encoder_signature(encoder)?;
    if cols == 0 || seq_len.is_some_and(|expected| cols != expected) {
        let expected = seq_len.map_or("dynamic".to_string(), |v| v.to_string());
        return Err(Error(format!(
            "sdk.invalidArgument|field=token_cols,value={cols},expected={expected}"
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
            label: "hand".to_string(),
            token_rows: 1
        }])
        .is_ok());
        let err = validate(&[]).err().unwrap().0;
        assert!(err.contains("entry_count"), "{err}");
        let err = validate(&[EntrySpec {
            label: "".to_string(),
            token_rows: 1,
        }])
        .err()
        .unwrap()
        .0;
        assert!(err.contains("reason=empty"), "{err}");
        let err = validate(&[
            EntrySpec {
                label: "a".to_string(),
                token_rows: 1,
            },
            EntrySpec {
                label: "a".to_string(),
                token_rows: 2,
            },
        ])
        .err()
        .unwrap()
        .0;
        assert!(err.contains("reason=duplicate"), "{err}");
        let err = validate(&[EntrySpec {
            label: "a".to_string(),
            token_rows: 0,
        }])
        .err()
        .unwrap()
        .0;
        assert!(err.contains("token_rows"), "{err}");
        let many: Vec<EntrySpec> = (0..=MAX_ENTRIES)
            .map(|i| EntrySpec {
                label: format!("l{i}"),
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

    fn snapshot() -> VocabEmbedding {
        VocabEmbedding {
            entries: vec![("hand".into(), 2)],
            matrix: (0..8).map(|v| v as f32).collect(),
            dim: 4,
            row_ranges: vec![(0, 2)],
        }
    }

    fn declared(shape: Vec<i64>) -> super::super::model::TensorInfo {
        super::super::model::TensorInfo {
            name: "txt_feats".into(),
            dtype: 0,
            shape,
        }
    }

    /// 批 D：注入张量按检测模型第二输入秩适配——2-D [rows,dim]（world-head
    /// 形态不变），3-D [1,rows,dim]（world 形态每帧快照），其余秩拒绝。
    #[test]
    fn injection_tensor_matches_declared_input_rank() {
        let vocab = snapshot();
        let flat = vocab.injection_tensor(&declared(vec![-1, 4])).unwrap();
        assert_eq!(flat.info.shape, vec![2, 4]);
        assert_eq!(flat.info.name, "txt_feats");
        let batched = vocab.injection_tensor(&declared(vec![-1, -1, 4])).unwrap();
        assert_eq!(batched.info.shape, vec![1, 2, 4]);
        assert_eq!(batched.bytes(), flat.bytes(), "同快照字节不变仅加批轴");
        let error = vocab
            .injection_tensor(&declared(vec![-1, -1, -1, 4]))
            .err()
            .expect("rank 4 拒绝")
            .0;
        assert!(error.contains("vocab_input_rank"), "{error}");
    }

    /// 批 D vocab 批语义统一（CPU/GPU 同构）：3-D 快照 [1,rows,dim] 平铺 N 份为
    /// [N,rows,dim]（每帧完整嵌入，字节=N×单帧）；2-D 快照原样返回（无批量轴）。
    #[test]
    fn replicate_frames_tiles_per_frame_snapshots() {
        let vocab = snapshot();
        let single = vocab.injection_tensor(&declared(vec![-1, -1, 4])).unwrap();
        let tiled = replicate_frames(&single, 3).unwrap();
        assert_eq!(tiled.info.shape, vec![3, 2, 4]);
        let bytes = single.bytes();
        assert_eq!(tiled.bytes().len(), bytes.len() * 3);
        for frame in 0..3 {
            assert_eq!(
                &tiled.bytes()[frame * bytes.len()..(frame + 1) * bytes.len()],
                bytes
            );
        }
        let flat = vocab.injection_tensor(&declared(vec![-1, 4])).unwrap();
        let passed = replicate_frames(&flat, 3).unwrap();
        assert_eq!(passed.info.shape, vec![2, 4], "2-D 快照原样");
        assert!(replicate_frames(&single, 0).is_err(), "零帧拒绝");
    }

    /// 批 D：亲和头单帧输出估计=4字节×(4+提示词行)×档案 anchors；非亲和档案/
    /// 动态 anchors 返回 None。
    #[test]
    fn affinity_row_bytes_estimates_from_layout_and_pinned_anchors() {
        let layout = crate::profiles::VocabLayout {
            entries: vec![("hand".into(), 2), ("glove".into(), 1)],
        };
        let make = |task: crate::profiles::Task, classes: Option<usize>, anchors: usize| {
            crate::profiles::Profile {
                id: "p".into(),
                task,
                sample_sha256: String::new(),
                input: crate::profiles::TensorSpec {
                    name: "images".into(),
                    shape: vec![0, 3, 640, 640],
                },
                inputs: vec![
                    crate::profiles::TensorSpec {
                        name: "images".into(),
                        shape: vec![0, 3, 640, 640],
                    },
                    crate::profiles::TensorSpec {
                        name: "txt_feats".into(),
                        shape: vec![0, 0, 512],
                    },
                ],
                outputs: vec![crate::profiles::TensorSpec {
                    name: "output0".into(),
                    shape: vec![0, 0, anchors],
                }],
                classes,
                postprocess: Default::default(),
            }
        };
        let affinity = make(crate::profiles::Task::Detect, None, 8400);
        assert_eq!(affinity_row_bytes(&affinity, &layout), Some(7 * 8400 * 4));
        let dynamic = make(crate::profiles::Task::Detect, None, 0);
        assert_eq!(affinity_row_bytes(&dynamic, &layout), None);
        let classified = make(crate::profiles::Task::Detect, Some(3), 8400);
        assert_eq!(affinity_row_bytes(&classified, &layout), None);
    }
}
