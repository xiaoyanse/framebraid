use super::super::submission::Prepared;
use super::*;
use am_vision_core::sdk;

pub(super) fn execute(
    entry: Arc<Entry>,
    inputs: Vec<Input>,
    ids: Vec<u64>,
    params: Vec<am_infer_params_t>,
    admissions: Vec<Admission>,
) {
    let Ok(model) = entry.data::<ModelHandle>() else {
        return;
    };
    let canceled = lock(&model.state.records)
        .map(|mut records| {
            let mut live = false;
            for id in &ids {
                if let Some(r) = records.get_mut(id) {
                    if !r.canceled && r.state < AM_REQ_DONE as u32 {
                        r.state = AM_REQ_RUNNING as u32;
                        r.started = am_sdk_support::clock::monotonic_ms();
                        live = true;
                    }
                }
            }
            !live
        })
        .unwrap_or(true);
    let input_budget = Admission::totals(&admissions).1;
    let output = if canceled || model.stopping.load(Ordering::Acquire) {
        Err(AM_ERR_SDK_HANDLE_CLOSED)
    } else {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run(model, &inputs, &ids, &params, input_budget)
        }))
        .unwrap_or(Err(AM_ERR_SDK_INTERNAL_ERROR))
    };
    // 固定批次内取消只改变该项终态；仍在执行的整批输入不得提前归还。
    drop(inputs);
    drop(admissions);
    match output {
        Ok(results) => {
            for (id, result) in ids.iter().zip(results) {
                execution::finish(model, *id, Ok(result));
            }
        }
        Err(code) => {
            for id in &ids {
                execution::finish(model, *id, Err(code));
            }
        }
    }
    model.inflight.fetch_sub(ids.len(), Ordering::AcqRel);
    model.state.running.fetch_sub(1, Ordering::AcqRel);
    model.busy.store(false, Ordering::Release);
    model.state.changed.notify_all();
}
fn run(
    model: &ModelHandle,
    inputs: &[Input],
    ids: &[u64],
    params: &[am_infer_params_t],
    input_budget: usize,
) -> Result<Vec<Arc<ResultBuffer>>> {
    let ctx = model.state.context()?;
    let context = ctx.data::<Context>()?;
    let limits = *lock(&context.limits)?;
    let mut session = lock(&model.session)?;
    let session = session.as_mut().ok_or(AM_ERR_SDK_HANDLE_CLOSED)?;
    // 批 C：PROFILE 图像项不再整批拒绝——按执行入口分派物理批执行（CPU 张量
    // 合并单次 run_batch / GPU 设备侧组装单次 run_gpu），拆分后逐项解码与回执。
    if session.profile.is_some() {
        return run_profile(model, session, inputs, ids, params, input_budget, &limits);
    }
    let prepared = inputs
        .iter()
        .map(|v| {
            v.prepare(
                &session.inputs[0].name,
                limits.preprocess_budget_bytes as usize,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let known = session.outputs.iter().try_fold(0usize, |n, s| {
        let mut shape = s.shape.clone();
        *shape.first_mut()? = 1;
        let bytes = sdk::byte_count(s.dtype, &shape)
            .ok()?
            .checked_next_multiple_of(8)?;
        n.checked_add(bytes)
    });
    let overhead = 512 * 1024;
    let available = limits
        .result_budget_bytes
        .min(lock(&context.queues)?[AM_Q_RESULTS as usize].max_bytes) as usize;
    let maximum = available
        .checked_sub(overhead * inputs.len())
        .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?
        / 2
        / inputs.len();
    let row_bytes = known.unwrap_or(maximum);
    if row_bytes == 0 || row_bytes > maximum {
        return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
    }
    let temporary = model.state.temporary(row_bytes * inputs.len())?;
    let charges = (0..inputs.len())
        .map(|_| model.state.charge(row_bytes + overhead))
        .collect::<Result<Vec<_>>>()?;
    let start = am_sdk_support::clock::monotonic_ms();
    let output = session.run_batch(
        &prepared.iter().map(|p| p.tensors()).collect::<Vec<_>>(),
        input_budget,
        row_bytes * inputs.len() * 2,
    );
    model.refresh_selection(session);
    let output = output.map_err(error)?;
    let end = am_sdk_support::clock::monotonic_ms();
    drop(temporary);
    output
        .into_iter()
        .zip(charges)
        .enumerate()
        .map(|(i, (raw, charge))| {
            ResultBuffer::build(
                model,
                ids[i],
                inputs[i].assoc(),
                prepared[i].transform(),
                raw,
                None,
                None,
                start,
                end,
                charge,
            )
        })
        .collect()
}

/// 批 C PROFILE 物理批执行：CPU 图像项合并张量单次 run_batch；GPU 帧项设备侧
/// 组装单次 run_gpu（聚合键已按执行入口分窗不混批；显式 NATIVE 组内混入时拆为
/// 两个物理批顺序执行）。逐项 params（confidence/nms/max_det）逐项解码生效。
/// 批 D：词汇注入先行提升到物理批入口（vocab 布局驱动亲和头单帧输出估计，
/// 双路径共用同一快照——CPU 按项复制合并、GPU 3-D 平铺，语义统一）。
#[allow(clippy::too_many_arguments)]
fn run_profile(
    model: &ModelHandle,
    session: &mut sdk::Model,
    inputs: &[Input],
    ids: &[u64],
    params: &[am_infer_params_t],
    input_budget: usize,
    limits: &am_limits_t,
) -> Result<Vec<Arc<ResultBuffer>>> {
    let available = limits
        .result_budget_bytes
        .min(
            lock(&model.state.context()?.data::<Context>()?.queues)?[AM_Q_RESULTS as usize]
                .max_bytes,
        )
        .saturating_sub(model.state.usage.bytes.load(Ordering::Acquire) as u64)
        as usize;
    if available == 0 {
        return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
    }
    let segment = session
        .profile
        .is_some_and(|p| p.task == am_vision_core::profiles::Task::Segment);
    let overhead = 512 * 1024;
    let injected = execution::vocab_injection(model, session)?;
    // 亲和头（双输入 detect）：单帧输出行数=4+提示词行运行期已知，估计替代
    // 符号维输出形状积（否则全动态输出按 None 拒）；其余档案维持形状积口径。
    let row_bytes = session
        .profile
        .zip(injected.as_ref().map(|(_, l)| l))
        .and_then(|(p, l)| am_vision_core::sdk::vocab::affinity_row_bytes(p, l))
        .or_else(|| profile_row_bytes(session))
        .ok_or(AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
    let mut cpu = Vec::new();
    let mut gpu = Vec::new();
    for (index, input) in inputs.iter().enumerate() {
        match input {
            Input::Image(image) if image.gpu.is_some() => gpu.push(index),
            _ => cpu.push(index),
        }
    }
    let mut results: Vec<Option<Arc<ResultBuffer>>> = inputs.iter().map(|_| None).collect();
    if !cpu.is_empty() {
        for (index, result) in run_profile_cpu(
            model,
            session,
            inputs,
            ids,
            params,
            &cpu,
            &injected,
            input_budget,
            limits,
            segment,
            row_bytes,
            overhead,
        )? {
            results[index] = Some(result);
        }
    }
    if !gpu.is_empty() {
        for (index, result) in run_profile_gpu(
            model, session, inputs, ids, params, &gpu, &injected, available, segment, row_bytes,
            overhead,
        )? {
            results[index] = Some(result);
        }
    }
    results
        .into_iter()
        .map(|r| r.ok_or(AM_ERR_SDK_INTERNAL_ERROR))
        .collect()
}

/// 单帧输出字节估计（输出形状首轴置 1；动态首轴按 [1,..] 行估）。
fn profile_row_bytes(session: &sdk::Model) -> Option<usize> {
    session
        .outputs
        .iter()
        .try_fold(0usize, |n, s| {
            let mut shape = s.shape.clone();
            *shape.first_mut()? = 1;
            let bytes = sdk::byte_count(s.dtype, &shape)
                .ok()?
                .checked_next_multiple_of(8)?;
            n.checked_add(bytes)
        })
        .filter(|v| *v > 0)
}

fn item_params(params: &[am_infer_params_t], index: usize) -> am_infer_params_t {
    params.get(index).copied().unwrap_or_default()
}

fn item_max_det(params: am_infer_params_t) -> usize {
    if params.max_det == 0 {
        300
    } else {
        params.max_det as usize
    }
}

/// 逐项掩膜预留（仅 Segment 档案；映射尺寸逐项取该项目自己的预处理映射）。
fn mask_of(segment: bool, mapping: &sdk::ImageTransform, max_det: usize) -> usize {
    if segment {
        (mapping.width as usize).div_ceil(8) * mapping.height as usize * max_det
    } else {
        0
    }
}

/// CPU 项物理批：逐项 prepare → run_batch 合并（词汇注入张量按名称随行合并——
/// 3-D 文本档案逐项 [1,rows,dim] 快照合并为 [N,rows,dim]，每帧完整嵌入）
/// → 拆分逐项解码（逐项映射/params）。
#[allow(clippy::too_many_arguments)]
fn run_profile_cpu(
    model: &ModelHandle,
    session: &mut sdk::Model,
    inputs: &[Input],
    ids: &[u64],
    params: &[am_infer_params_t],
    indices: &[usize],
    injected: &Option<(sdk::TensorData, am_vision_core::profiles::VocabLayout)>,
    input_budget: usize,
    limits: &am_limits_t,
    segment: bool,
    row_bytes: usize,
    overhead: usize,
) -> Result<Vec<(usize, Arc<ResultBuffer>)>> {
    let name = session.inputs[0].name.clone();
    let prepared = indices
        .iter()
        .map(|i| inputs[*i].prepare(&name, limits.preprocess_budget_bytes as usize))
        .collect::<Result<Vec<_>>>()?;
    let mappings: Vec<sdk::ImageTransform> =
        prepared.iter().map(|p| mapping_of(p, session)).collect();
    let vocab_layout = injected.as_ref().map(|(_, l)| l);
    // 逐项行：图像张量按值移入（免复制）；双输入档案追加词汇注入张量副本
    // （combine 按名称随行合并——注入语义与单帧一致）。
    let rows: Vec<Vec<sdk::TensorData>> = prepared
        .into_iter()
        .map(|p| {
            let mut row: Vec<sdk::TensorData> = match p {
                Prepared::Image(tensor, _) => vec![tensor],
                Prepared::Tensors(tensors) => tensors
                    .iter()
                    .map(|t| sdk::TensorData::new(t.info.clone(), t.bytes()).map_err(error))
                    .collect::<Result<Vec<_>>>()?,
            };
            if let Some((tensor, _)) = injected.as_ref() {
                row.push(sdk::TensorData::new(tensor.info.clone(), tensor.bytes()).map_err(error)?);
            }
            Ok(row)
        })
        .collect::<Result<Vec<_>>>()?;
    let refs: Vec<&[sdk::TensorData]> = rows.iter().map(Vec::as_slice).collect();
    let charges: Vec<_> = indices
        .iter()
        .zip(&mappings)
        .map(|(index, mapping)| {
            let p = item_params(params, *index);
            model
                .state
                .charge(row_bytes + mask_of(segment, mapping, item_max_det(p)) + overhead)
        })
        .collect::<Result<Vec<_>>>()?;
    let start = am_sdk_support::clock::monotonic_ms();
    let output = session.run_batch(&refs, input_budget, row_bytes * indices.len() * 2);
    model.refresh_selection(session);
    let output = output.map_err(error)?;
    let mut results = Vec::with_capacity(indices.len());
    for (((slot, index), raw), charge) in indices.iter().enumerate().zip(output).zip(charges) {
        let p = item_params(params, *index);
        let max_det = item_max_det(p);
        let mapping = mappings[slot];
        let reserve = row_bytes + mask_of(segment, &mapping, max_det) + overhead;
        let raw_bytes = tensors_bytes(&raw);
        let prediction = execution::decode_profile(
            session,
            &raw,
            &mapping,
            p,
            max_det,
            reserve,
            raw_bytes,
            vocab_layout,
        )?;
        let end = am_sdk_support::clock::monotonic_ms();
        results.push((
            *index,
            ResultBuffer::build(
                model,
                ids[*index],
                inputs[*index].assoc(),
                Some(mapping),
                raw,
                prediction,
                None,
                start,
                end,
                charge,
            )?,
        ));
    }
    Ok(results)
}

/// GPU 帧物理批：各帧接受段预处理产物（设备张量）经管线设备侧组装为
/// [N,3,H,W] 批绑定，单次 run_gpu；输出按首轴拆分逐项解码。词汇注入（批 D
/// 语义统一）：3-D 文本档案（txt_feats 每帧独立）单帧快照平铺 N 份绑定为
/// [N,rows,dim]——与 CPU 路径 combine 按项合并字节级同构；2-D 档案单份快照。
#[allow(clippy::too_many_arguments)]
fn run_profile_gpu(
    model: &ModelHandle,
    session: &mut sdk::Model,
    inputs: &[Input],
    ids: &[u64],
    params: &[am_infer_params_t],
    indices: &[usize],
    injected: &Option<(sdk::TensorData, am_vision_core::profiles::VocabLayout)>,
    available: usize,
    segment: bool,
    row_bytes: usize,
    overhead: usize,
) -> Result<Vec<(usize, Arc<ResultBuffer>)>> {
    use am_vision_core::gpu::pipeline::GpuPipeline;
    let pipeline = GpuPipeline::global().map_err(error)?;
    let frames: Vec<&am_vision_core::gpu::iobinding::InferenceInput> = indices
        .iter()
        .map(|i| match &inputs[*i] {
            Input::Image(image) => image.gpu.as_ref().map(|g| &g.binding),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()
        .ok_or(AM_ERR_SDK_INTERNAL_ERROR)?;
    let mappings: Vec<sdk::ImageTransform> = indices
        .iter()
        .map(|i| match &inputs[*i] {
            Input::Image(image) => image
                .gpu
                .as_ref()
                .map(|g| g.transform)
                .ok_or(AM_ERR_SDK_INTERNAL_ERROR),
            _ => Err(AM_ERR_SDK_INTERNAL_ERROR),
        })
        .collect::<Result<Vec<_>>>()?;
    // 批 D：模型 H/W 符号维（world 形态）回落档案钉死目标尺寸（capture 段同则）。
    let Some((h, w)) = session.input_hw() else {
        return Err(AM_ERR_VISION_PROFILE_UNSUPPORTED);
    };
    let mut batch = pipeline
        .assemble_frames(
            &frames,
            session.ort_session(),
            &session.inputs[0].name,
            h,
            w,
        )
        .map_err(error)?;
    let vocab_layout = injected.as_ref().map(|(_, l)| l);
    if let Some((vocab, _)) = injected.as_ref() {
        let tiled =
            am_vision_core::sdk::vocab::replicate_frames(vocab, indices.len()).map_err(error)?;
        batch.bind_vocab(&tiled).map_err(error)?;
        // vocab 为 CPU 内存绑定（H2D 异步搬运），run 前显式同步（与单帧段同则）。
        batch.binding.synchronize_inputs().map_err(|e| {
            error(am_vision_core::Error(format!(
                "vision.inferenceFailed|vocab输入同步|{e}"
            )))
        })?;
    }
    let charges: Vec<_> = indices
        .iter()
        .zip(&mappings)
        .map(|(index, mapping)| {
            let p = item_params(params, *index);
            model
                .state
                .charge(row_bytes + mask_of(segment, mapping, item_max_det(p)) + overhead)
        })
        .collect::<Result<Vec<_>>>()?;
    let output_budget = row_bytes
        .saturating_mul(indices.len())
        .saturating_add(overhead)
        .min(available);
    let start = am_sdk_support::clock::monotonic_ms();
    let output = session.run_gpu(&batch, output_budget);
    model.refresh_selection(session);
    let raw = output.map_err(error)?;
    let rows = split_batch_rows(raw, indices.len())?;
    let mut results = Vec::with_capacity(indices.len());
    for (((slot, index), row), charge) in indices.iter().enumerate().zip(rows).zip(charges) {
        let p = item_params(params, *index);
        let max_det = item_max_det(p);
        let mapping = mappings[slot];
        let reserve = row_bytes + mask_of(segment, &mapping, max_det) + overhead;
        let raw_bytes = tensors_bytes(&row);
        let prediction = execution::decode_profile(
            session,
            &row,
            &mapping,
            p,
            max_det,
            reserve,
            raw_bytes,
            vocab_layout,
        )?;
        let end = am_sdk_support::clock::monotonic_ms();
        results.push((
            *index,
            ResultBuffer::build(
                model,
                ids[*index],
                inputs[*index].assoc(),
                Some(mapping),
                row,
                prediction,
                None,
                start,
                end,
                charge,
            )?,
        ));
    }
    Ok(results)
}

/// 批输出按首轴拆分回逐项 [1,..]（与 sdk::batch::split 同语义，预算外层已计费）。
fn split_batch_rows(raw: Vec<sdk::TensorData>, count: usize) -> Result<Vec<Vec<sdk::TensorData>>> {
    if count == 0 {
        return Err(AM_ERR_SDK_INTERNAL_ERROR);
    }
    for tensor in &raw {
        if tensor.info.shape.first() != Some(&(count as i64))
            || !tensor.bytes().len().is_multiple_of(count)
        {
            return Err(AM_ERR_VISION_BATCH_UNSUPPORTED);
        }
    }
    let mut rows: Vec<Vec<sdk::TensorData>> =
        (0..count).map(|_| Vec::with_capacity(raw.len())).collect();
    for tensor in &raw {
        let bytes = tensor.bytes().len() / count;
        let mut info = tensor.info.clone();
        info.shape[0] = 1;
        for (row, output) in rows.iter_mut().enumerate() {
            output.push(
                sdk::TensorData::new(
                    info.clone(),
                    &tensor.bytes()[row * bytes..(row + 1) * bytes],
                )
                .map_err(error)?,
            );
        }
    }
    Ok(rows)
}

/// 结果张量字节合计（八字节对齐；与拆分侧 allocated 口径一致）。
fn tensors_bytes(raw: &[sdk::TensorData]) -> usize {
    raw.iter()
        .map(|t| {
            sdk::byte_count(t.info.dtype, &t.info.shape)
                .unwrap_or(0)
                .checked_next_multiple_of(8)
                .unwrap_or(0)
        })
        .fold(0usize, |n, v| n.saturating_add(v))
}

/// CPU 项映射：图像项取预处理映射；张量项无原图变换，按档案输入形状构造
/// 归一化平面映射（与单帧执行路径同规则）。
fn mapping_of(prepared: &Prepared<'_>, session: &sdk::Model) -> sdk::ImageTransform {
    if let Some(m) = prepared.transform() {
        return m;
    }
    let shape = &session.inputs[0].shape;
    let (width, height) = if shape.len() == 4 && shape[2] > 0 && shape[3] > 0 {
        (shape[3] as u32, shape[2] as u32)
    } else {
        (0, 0)
    };
    sdk::ImageTransform {
        width,
        height,
        dst_w: width,
        dst_h: height,
        scale_x: 1.0,
        scale_y: 1.0,
        crop_x: 0,
        crop_y: 0,
        left: 0,
        top: 0,
    }
}

#[cfg(test)]
mod split_tests {
    use super::*;

    fn tensor(shape: &[i64], values: &[u8]) -> sdk::TensorData {
        sdk::TensorData::new(
            sdk::TensorInfo {
                name: "out".into(),
                dtype: 4,
                shape: shape.to_vec(),
            },
            values,
        )
        .unwrap()
    }

    #[test]
    fn split_rows_preserve_identity_by_position() {
        let raw = vec![tensor(&[3, 2], &[1, 2, 3, 4, 5, 6])];
        let rows = split_batch_rows(raw, 3).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0][0].info.shape, vec![1, 2]);
        assert_eq!(rows[0][0].bytes(), &[1, 2]);
        assert_eq!(rows[1][0].bytes(), &[3, 4]);
        assert_eq!(rows[2][0].bytes(), &[5, 6]);
    }

    #[test]
    fn split_rows_reject_batch_axis_drift() {
        let reject = |shape: &[i64], values: &[u8], count: usize| {
            let raw = vec![tensor(shape, values)];
            match split_batch_rows(raw, count) {
                Ok(_) => panic!("首轴漂移应拒绝 shape={shape:?} count={count}"),
                Err(code) => code,
            }
        };
        assert_eq!(
            reject(&[2, 2], &[1, 2, 3, 4], 3),
            AM_ERR_VISION_BATCH_UNSUPPORTED
        );
        assert_eq!(reject(&[3], &[1, 2, 3], 2), AM_ERR_VISION_BATCH_UNSUPPORTED);
    }
}
