use super::*;

/// A packed NV12 snapshot owned by the request. The original texture is no longer borrowed.
pub struct Nv12Snapshot {
    ptr: *mut c_void,
    width: u32,
    height: u32,
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
    /// Copy both planes under one resource mapping.
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
        let y_bytes = width * height;
        let mut ptr = std::ptr::null_mut();
        self.runtime.check(
            unsafe { (self.runtime.malloc)(&mut ptr, y_bytes * 3 / 2) },
            "ROI snapshot allocation",
        )?;
        let snapshot = Nv12Snapshot {
            ptr,
            width: frame.width,
            height: frame.height,
            pipeline: self,
        };
        let cached = unsafe {
            registry::global()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .acquire(&self.runtime, frame.texture)?
        };
        let _guard = cached.map_lock.lock().unwrap_or_else(|e| e.into_inner());
        let y = cached.texture.map_subresource(frame.subresource)?;
        let uv = cached.texture.map_subresource(frame.subresource + 1)?;
        for (dst, plane, rows) in [
            (ptr, y.array, height),
            (
                unsafe { ptr.cast::<u8>().add(y_bytes).cast() },
                uv.array,
                height / 2,
            ),
        ] {
            self.runtime.check(
                unsafe {
                    (self.runtime.memcpy2d_from_array)(dst, width, plane, 0, 0, width, rows, 3)
                },
                "ROI snapshot plane copy",
            )?;
        }
        Ok(snapshot)
    }
}

impl Nv12Snapshot {
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
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
        let input =
            InferenceInput::new_batch(session, name, regions.len(), dst as usize, dst as usize)?;
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
                    self.ptr as u64 + u64::from(self.width) * u64::from(self.height),
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
