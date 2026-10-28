//! GPU 帧直传管线编排（M2b 3.6）：GpuFrameData → 互操作映射 → D2D 搬运 →
//! 预处理 kernel → IOBinding 输入。全程 device 内完成（M2c 搬运审计口径）。
use super::cuda_sys::CudaApi;
use super::driver_sys::{DriverApi, KernelModule};
use super::interop::RegisteredTexture;
use super::iobinding::InferenceInput;
use crate::Error;
use std::ffi::c_void;
use std::sync::{Arc, OnceLock};

/// 跨 DLL 传入的 GPU 帧引用（ABI am_gpu_frame_ref_t 的内部形态）。
/// `texture` 借用至推理接受（合同 §3.3a：同步至返回/异步至接受）。
pub struct GpuFrameData {
    pub width: u32,
    pub height: u32,
    pub d3d_format: u32,
    pub subresource: u32,
    pub texture: *mut c_void,
}

/// 进程级 GPU 管线单例（driver ctx + JIT kernel；CUDA 环境进程内共享）。
/// CUDA context 与模块句柄线程安全（驱动保证），裸指针跨线程只读使用。
unsafe impl Send for GpuPipeline {}
unsafe impl Sync for GpuPipeline {}
pub struct GpuPipeline {
    pub runtime: Arc<CudaApi>,
    pub driver: Arc<DriverApi>,
    kernel: KernelModule,
    /// 建立时的上下文句柄：每次管线操作前显式设为当前（runtime/ORT 调用
    /// 可能扰动线程 current，不依赖隐式延续）。
    context: *mut std::ffi::c_void,
}

impl GpuPipeline {
    /// 惰性初始化：CUDA 不可用时报错（能力位查询用 available()，不触发初始化）。
    pub fn global() -> Result<&'static GpuPipeline, Error> {
        static PIPELINE: OnceLock<Result<GpuPipeline, String>> = OnceLock::new();
        let cached = PIPELINE.get_or_init(|| GpuPipeline::new().map_err(|e| e.0));
        cached
            .as_ref()
            .map_err(|e| Error(format!("vision.gpuUnavailable|{e}")))
    }

    /// 环境探测（能力位用；不建立上下文，只验证运行库可加载）。
    pub fn available() -> bool {
        static AVAILABLE: OnceLock<bool> = OnceLock::new();
        *AVAILABLE.get_or_init(|| CudaApi::load().is_ok() && DriverApi::load().is_ok())
    }

    fn new() -> Result<Self, Error> {
        let runtime = CudaApi::load().map_err(|e| Error(format!("cudart 加载失败|{e}")))?;
        let driver = DriverApi::load().map_err(|e| Error(format!("nvcuda 加载失败|{e}")))?;
        let kernel = KernelModule::load_nv12_letterbox(driver.clone())?;
        let context = driver.establish_context()?;
        Ok(Self {
            runtime,
            driver,
            kernel,
            context,
        })
    }

    /// GPU 帧 → 模型输入（dst 尺寸按模型输入形状）。返回 IOBinding 输入
    /// （device tensor 由 ORT allocator 持有）。纹理借用至本函数返回（kernel
    /// 完成后归还语义：同步等待 kernel 结束再释放中间 buffer）。
    pub fn preprocess_to_binding(
        &self,
        frame: &GpuFrameData,
        session: &ort::session::Session,
        input_name: &str,
        dst_w: u32,
        dst_h: u32,
    ) -> Result<(InferenceInput, crate::sdk::ImageTransform), Error> {
        self.driver.check(
            unsafe { (self.driver.ctx_set_current)(self.context) },
            "上下文设置",
        )?;
        if frame.d3d_format != 103 {
            // DXGI_FORMAT_NV12=103；首版仅 NV12（解码默认输出），其余格式回退像素路径。
            return Err(crate::Error("vision.gpuFormatUnsupported".into()));
        }
        if frame.texture.is_null() || frame.width == 0 || frame.height == 0 {
            return Err(crate::Error("vision.gpuFrameInvalid".into()));
        }
        let registered =
            unsafe { RegisteredTexture::register(self.runtime.clone(), frame.texture)? };
        {
            let y = registered.map_subresource(frame.subresource)?;
            // NV12 每 slice 两个 subresource（M2b 实测）：UV 紧随 Y。
            let uv = registered.map_subresource(frame.subresource + 1)?;
            let y_array = y.array;
            let uv_array = uv.array;
            // D2D 搬运：array → device buffer（GPU 内拷贝，计入搬运审计）。
            let y_bytes = frame.width as usize * frame.height as usize;
            let uv_bytes = y_bytes / 2;
            let mut y_buf: *mut c_void = std::ptr::null_mut();
            let mut uv_buf: *mut c_void = std::ptr::null_mut();
            self.driver.check(
                unsafe { (self.runtime.malloc)(&mut y_buf, y_bytes) },
                "Y缓冲分配",
            )?;
            self.driver.check(
                unsafe { (self.runtime.malloc)(&mut uv_buf, uv_bytes) },
                "UV缓冲分配",
            )?;
            let result = self.copy_and_launch(
                y_buf, uv_buf, y_array, uv_array, frame, dst_w, dst_h, session, input_name, y_bytes,
            );
            unsafe {
                (self.runtime.free)(y_buf);
                (self.runtime.free)(uv_buf);
            }
            result
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn copy_and_launch(
        &self,
        y_buf: *mut c_void,
        uv_buf: *mut c_void,
        y_array: super::cuda_sys::CudaArray,
        uv_array: super::cuda_sys::CudaArray,
        frame: &GpuFrameData,
        dst_w: u32,
        dst_h: u32,
        session: &ort::session::Session,
        input_name: &str,
        y_bytes: usize,
    ) -> Result<(InferenceInput, crate::sdk::ImageTransform), Error> {
        let width = frame.width as usize;
        let height = frame.height as usize;
        // Y 平面：行距=width（紧凑）；UV 平面：行距=width（每行 w/2 个双字节 texel）。
        let code = unsafe {
            (self.runtime.memcpy2d_from_array)(
                y_buf, width, y_array, 0, 0, width, height, 3, // DtoD
            )
        };
        self.runtime.check(code, "Y平面搬运")?;
        let code = unsafe {
            (self.runtime.memcpy2d_from_array)(uv_buf, width, uv_array, 0, 0, width, height / 2, 3)
        };
        self.runtime.check(code, "UV平面搬运")?;
        let input = InferenceInput::new(session, input_name, dst_h as usize, dst_w as usize)?;
        // letterbox 目标尺寸（与 CPU 参考同规则）
        let scale = (dst_w as f32 / frame.width as f32).min(dst_h as f32 / frame.height as f32);
        let rw = ((frame.width as f32 * scale).round() as u32).clamp(1, dst_w);
        let rh = ((frame.height as f32 * scale).round() as u32).clamp(1, dst_h);
        let left = (dst_w - rw) / 2;
        let top = (dst_h - rh) / 2;
        let transform = crate::sdk::ImageTransform {
            width: frame.width,
            height: frame.height,
            dst_w,
            dst_h,
            scale_x: scale,
            scale_y: scale,
            crop_x: 0,
            crop_y: 0,
            left,
            top,
        };
        let _ = y_bytes;
        let launched = self.kernel.launch(
            y_buf as u64,
            uv_buf as u64,
            frame.width,
            frame.height,
            dst_w,
            dst_h,
            rw,
            rh,
            input.device_ptr(),
        );
        launched?;
        // kernel 完成后再释放中间 buffer（launch 异步；同步等待借用窗口闭合）。
        self.driver
            .check(unsafe { (self.driver.ctx_synchronize)() }, "kernel同步")?;
        audit_frame();
        Ok((input, transform))
    }
}

/// 搬运审计计数（M2c 主指标；模型级细分随后续任务挂 ModelHandle）。
pub mod audit {
    use std::sync::atomic::{AtomicU64, Ordering};
    pub static GPU_INPUTS: AtomicU64 = AtomicU64::new(0);
    pub static GPU_COPIES_PER_FRAME: AtomicU64 = AtomicU64::new(2); // Y+UV D2D 各一
    pub fn snapshot() -> (u64, u64) {
        (
            GPU_INPUTS.load(Ordering::Relaxed),
            GPU_COPIES_PER_FRAME.load(Ordering::Relaxed),
        )
    }
}

fn audit_frame() {
    use std::sync::atomic::Ordering;
    audit::GPU_INPUTS.fetch_add(1, Ordering::Relaxed);
}
