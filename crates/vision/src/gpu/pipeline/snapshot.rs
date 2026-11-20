use super::*;

/// 级联 ROI 快照（NV12 帧经 VP 平面转换后的 BGRA 打包设备缓冲，请求持有，
/// 原纹理借用已闭合——P1 修复起快照内容为 BT.601 full RGB，色彩/几何语义
/// 见 vp_transit.rs 与 bgra_letterbox.ptx 头注）。
pub struct Nv12Snapshot {
    pub(super) ptr: *mut c_void,
    pub(super) width: u32,
    pub(super) height: u32,
    pipeline: &'static GpuPipeline,
}

// The snapshot is immutable, and every GPU operation selects its CUDA context.
unsafe impl Send for Nv12Snapshot {}

impl Drop for Nv12Snapshot {
    fn drop(&mut self) {
        unsafe {
            if (self.pipeline.driver.ctx_set_current)(self.pipeline.context) == 0 {
                (self.pipeline.runtime.free)(self.ptr);
            }
        }
    }
}

impl GpuPipeline {
    /// 帧快照：VP 转换（NV12 slice→BGRA）+ GPU 同步 + 单次 D2D 打包。
    /// 源纹理借用在本函数返回时闭合（借用窗口内的全部设备操作）。
    ///
    /// # Safety
    /// The texture and its selected NV12 slice must remain valid and immutable until return.
    pub unsafe fn snapshot_nv12(
        &'static self,
        frame: &GpuFrameData,
    ) -> Result<Nv12Snapshot, Error> {
        if frame.texture.is_null()
            || frame.d3d_format != 103
            || !(2..=8192).contains(&frame.width)
            || !(2..=8192).contains(&frame.height)
            || !frame.width.is_multiple_of(2)
            || !frame.height.is_multiple_of(2)
            || frame.subresource == u32::MAX
        {
            return Err(Error("vision.imageViewInvalid".into()));
        }
        self.driver.check(
            unsafe { (self.driver.ctx_set_current)(self.context) },
            "ROI snapshot context",
        )?;
        let width = frame.width as usize;
        let height = frame.height as usize;
        let bgra_bytes = width * height * 4;
        let mut ptr = std::ptr::null_mut();
        self.runtime.check(
            unsafe { (self.runtime.malloc)(&mut ptr, bgra_bytes) },
            "ROI snapshot allocation",
        )?;
        let snapshot = Nv12Snapshot {
            ptr,
            width: frame.width,
            height: frame.height,
            pipeline: self,
        };
        let (entry, mapped) = {
            let mut pool = self.transit.lock().unwrap_or_else(|e| e.into_inner());
            pool.acquire(&self.runtime, frame)?
        };
        let _keepalive = entry; // 映射存续期背书（Arc 保活到 D2D 完成后）
        let code = unsafe {
            (self.runtime.memcpy2d_from_array)(
                ptr,
                width * 4,
                mapped.array,
                0,
                0,
                width * 4,
                height,
                3, // DtoD（同步语义，完成即打包完成）
            )
        };
        drop(mapped);
        self.runtime.check(code, "ROI snapshot plane copy")?;
        Ok(snapshot)
    }
}

impl Nv12Snapshot {
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// 测试辅助：BGRA 打包缓冲的设备指针（kernel 同输入镜像回读用）。
    #[cfg(test)]
    pub(crate) fn bgra_ptr_for_test(&self) -> *const c_void {
        self.ptr
    }

    /// Each region is [x, y, width, height] in source pixels.
    pub fn bind_regions(
        &self,
        session: &ort::session::Session,
        name: &str,
        dst: u32,
        regions: &[[u32; 4]],
        quantize_crop: bool,
    ) -> Result<(InferenceInput, Vec<crate::sdk::ImageTransform>), Error> {
        if !(1..=1024).contains(&dst)
            || !(1..=4).contains(&regions.len())
            || regions.iter().any(|&[x, y, w, h]| {
                w == 0
                    || h == 0
                    || x.checked_add(w).is_none_or(|end| end > self.width)
                    || y.checked_add(h).is_none_or(|end| end > self.height)
            })
        {
            return Err(Error("vision.imageViewInvalid".into()));
        }
        let gpu = self.pipeline;
        gpu.driver.check(
            unsafe { (gpu.driver.ctx_set_current)(gpu.context) },
            "ROI tensor context",
        )?;
        let input = InferenceInput::new_batch(
            session,
            name,
            regions.len(),
            dst as usize,
            dst as usize,
            gpu.ordinal,
        )?;
        let stride = input.tensor_bytes() / regions.len();
        let event = EventLease::acquire(&gpu.events, &gpu.driver)?;
        let mut transforms = Vec::with_capacity(regions.len());
        let launch = (|| {
            for (row, &[x, y, w, h]) in regions.iter().enumerate() {
                let scale = (dst as f32 / w as f32).min(dst as f32 / h as f32);
                let rw = (w as f32 * scale).round().clamp(1.0, dst as f32) as u32;
                let rh = (h as f32 * scale).round().clamp(1.0, dst as f32) as u32;
                gpu.kernel.launch_region(
                    self.ptr as u64,
                    w,
                    h,
                    dst,
                    dst,
                    rw,
                    rh,
                    input.device_ptr() + (row * stride) as u64,
                    self.width,
                    x,
                    y,
                    if quantize_crop { 3 } else { 2 },
                    scale,
                    0,
                    0.0,
                    0.0,
                    1.0,
                )?;
                transforms.push(crate::sdk::ImageTransform {
                    width: w,
                    height: h,
                    dst_w: dst,
                    dst_h: dst,
                    scale_x: scale,
                    scale_y: scale,
                    crop_x: 0,
                    crop_y: 0,
                    left: (dst - rw) / 2,
                    top: (dst - rh) / 2,
                });
            }
            gpu.driver.check(
                unsafe { (gpu.driver.event_record)(event.get(), std::ptr::null_mut()) },
                "ROI tensor event",
            )?;
            gpu.driver.check(
                unsafe { (gpu.driver.event_synchronize)(event.get()) },
                "ROI tensor wait",
            )
        })();
        if let Err(error) = launch {
            // Synchronize even when only part of the batch was launched.
            // CUDA synchronization returns execution errors after the work completes.
            unsafe {
                (gpu.driver.ctx_synchronize)();
            }
            return Err(error);
        }
        Ok((input, transforms))
    }
}
