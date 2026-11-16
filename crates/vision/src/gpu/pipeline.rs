//! GPU 帧直传管线编排（M2b 3.6）：GpuFrameData → 互操作映射 → D2D 搬运 →
//! 预处理 kernel → IOBinding 输入。全程 device 内完成（M2c 搬运审计口径）。
//! A 线三靶点优化：设备缓冲池化复用、纹理注册缓存（COM 保活）、kernel
//! 完成事件同步替代全上下文同步——纯内部优化，无合同面变化。
use super::bufpool::{DeviceBufPool, EventLease, EventPool, PooledBuf};
use super::cuda_sys::CudaApi;
use super::driver_sys::{DriverApi, KernelModule};
use super::iobinding::InferenceInput;
use super::registry::TextureRegistry;
use crate::Error;

#[cfg(test)]
mod roi_prototype;
mod snapshot;
pub use snapshot::Nv12Snapshot;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Arc, Mutex, OnceLock};

/// 跨 DLL 传入的 GPU 帧引用（ABI am_gpu_frame_ref_t 的内部形态）。
/// `texture` 借用至推理接受（合同 §3.3a：同步至返回/异步至接受）。
pub struct GpuFrameData {
    pub width: u32,
    pub height: u32,
    pub d3d_format: u32,
    pub subresource: u32,
    pub texture: *mut c_void,
}

/// 进程级 GPU 管线（driver ctx + JIT kernel），按 CUDA 设备 ordinal 键控缓存
/// （F03：会话定向非 0 ordinal 时按同设备建管线——上下文/注册缓存/输入张量
/// 分配域一致；此前全局单例恒设备 0 与定向脱节）。CUDA context 与模块句柄
/// 线程安全（驱动保证），裸指针跨线程只读使用。
unsafe impl Send for GpuPipeline {}
unsafe impl Sync for GpuPipeline {}
pub struct GpuPipeline {
    pub runtime: Arc<CudaApi>,
    pub driver: Arc<DriverApi>,
    kernel: KernelModule,
    /// 建立时的上下文句柄：每次管线操作前显式设为当前（runtime/ORT 调用
    /// 可能扰动线程 current，不依赖隐式延续）。
    context: *mut std::ffi::c_void,
    /// 管线绑定的 CUDA 设备 ordinal（F03）：输入张量分配与注册缓存同设备。
    ordinal: u32,
    /// 设备级纹理注册缓存（F03：注册资源绑定创建时上下文，多卡各持独立
    /// LRU；单卡行为与原全局缓存一致）。
    registry: Mutex<TextureRegistry>,
    /// 设备缓冲复用池（三靶点：消除每帧 cudaMalloc/cudaFree）。
    bufs: Mutex<DeviceBufPool>,
    /// kernel 完成事件池（三靶点：事件同步替代 cuCtxSynchronize）。
    events: Mutex<EventPool>,
}

/// 进程级管线缓存：每个 ordinal 首次构造结果（含失败）缓存；成功管线进程级
/// 保活（与旧全局单例同寿命语义，泄漏上限=设备数）。
static PIPELINES: OnceLock<Mutex<HashMap<u32, Result<&'static GpuPipeline, String>>>> =
    OnceLock::new();

impl GpuPipeline {
    /// 惰性初始化（按 CUDA 设备 ordinal）：CUDA 不可用时报错（能力位查询用
    /// available()，不触发初始化）。
    pub fn for_device(ordinal: u32) -> Result<&'static GpuPipeline, Error> {
        let cache = PIPELINES.get_or_init(|| Mutex::new(HashMap::new()));
        let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
        let entry = match guard.get(&ordinal) {
            Some(cached) => cached.clone(),
            None => {
                let created = match GpuPipeline::new(ordinal) {
                    Ok(pipeline) => {
                        let leaked: &'static GpuPipeline = Box::leak(Box::new(pipeline));
                        Ok(leaked)
                    }
                    Err(error) => Err(error.0),
                };
                guard.insert(ordinal, created.clone());
                created
            }
        };
        entry.map_err(|e| Error(format!("vision.gpuUnavailable|{e}")))
    }

    /// 环境探测（能力位用；不建立上下文，只验证运行库可加载）。
    pub fn available() -> bool {
        static AVAILABLE: OnceLock<bool> = OnceLock::new();
        *AVAILABLE.get_or_init(|| CudaApi::load().is_ok() && DriverApi::load().is_ok())
    }

    fn new(ordinal: u32) -> Result<Self, Error> {
        let runtime = CudaApi::load().map_err(|e| Error(format!("cudart 加载失败|{e}")))?;
        let driver = DriverApi::load().map_err(|e| Error(format!("nvcuda 加载失败|{e}")))?;
        // 先建上下文再 JIT：cuModuleLoadData 依赖调用线程已有 current 上下文，
        // 业务线程冷启动首触时无上下文，JIT 前置必败（cuInit+primary 激活须先行）。
        let context = driver.establish_context(ordinal)?;
        let kernel = KernelModule::load_nv12_letterbox(driver.clone())?;
        Ok(Self {
            runtime,
            driver,
            kernel,
            context,
            ordinal,
            registry: Mutex::new(TextureRegistry::new()),
            bufs: Mutex::new(DeviceBufPool::new()),
            events: Mutex::new(EventPool::default()),
        })
    }

    /// GPU 帧 → 模型输入（dst 尺寸按模型输入形状）。center_crop=true 为
    /// classify 档案语义（F02：max 比例+源域中心窗口，与 CPU prepare.rs
    /// resize==2 分支同规则；false 保持 detect letterbox）。返回 IOBinding
    /// 输入（device tensor 由 ORT allocator 持有）。纹理借用至本函数返回
    /// （kernel 完成后归还语义：事件同步等 kernel 结束再归还缓冲/解除映射）。
    pub fn preprocess_to_binding(
        &self,
        frame: &GpuFrameData,
        session: &ort::session::Session,
        input_name: &str,
        dst_w: u32,
        dst_h: u32,
        center_crop: bool,
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
        // 注册缓存命中即免 register/unregister；同纹理并发 map 串行化兜底。
        // Safety：纹理指针经 GPU 帧租约借入（合同 §3.3a），缓存 AddRef 保活。
        let cached = unsafe {
            self.registry
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .acquire(&self.runtime, frame.texture)?
        };
        let _map_guard = cached.map_lock.lock().unwrap_or_else(|e| e.into_inner());
        {
            let y = cached.texture.map_subresource(frame.subresource)?;
            // NV12 每 slice 两个 subresource（M2b 实测）：UV 紧随 Y。
            let uv = cached.texture.map_subresource(frame.subresource + 1)?;
            let y_array = y.array;
            let uv_array = uv.array;
            // D2D 搬运：array → device buffer（GPU 内拷贝，计入搬运审计）；
            // 缓冲池复用，尺寸变化时按需新分配。
            let y_bytes = frame.width as usize * frame.height as usize;
            let uv_bytes = y_bytes / 2;
            let y_buf = PooledBuf::acquire(&self.bufs, &self.runtime, y_bytes, "Y缓冲分配")?;
            let uv_buf = PooledBuf::acquire(&self.bufs, &self.runtime, uv_bytes, "UV缓冲分配")?;
            self.copy_and_launch(
                y_buf.ptr(),
                uv_buf.ptr(),
                y_array,
                uv_array,
                frame,
                dst_w,
                dst_h,
                session,
                input_name,
                center_crop,
            )
        }
        // 作用域出口顺序：copy_and_launch 内事件同步（kernel 完成）→
        // 缓冲归还池 → unmap → 解除同纹理锁 → 注册缓存条目引用释放。
    }

    /// 批 C 图像攒批的设备侧组装：N 帧已预处理设备张量（各 [1,3,h,w]，来自
    /// `preprocess_to_binding`）D2D 拼接为单一 [N,3,h,w] 批绑定——零 CPU 回读
    /// （R1.4 实测 CPU 拼接 ~3ms/样本不可接受，生死线）；逐帧一次
    /// cudaMemcpy DtoD（同步语义，拷贝完成即组装完成）。
    pub fn assemble_frames(
        &self,
        frames: &[&InferenceInput],
        session: &ort::session::Session,
        input_name: &str,
        h: u32,
        w: u32,
    ) -> Result<InferenceInput, Error> {
        if frames.is_empty() || frames.len() > 64 {
            return Err(crate::Error("vision.dynamicRangeExceeded".into()));
        }
        self.driver.check(
            unsafe { (self.driver.ctx_set_current)(self.context) },
            "上下文设置",
        )?;
        let batch = InferenceInput::new_batch(
            session,
            input_name,
            frames.len(),
            h as usize,
            w as usize,
            self.ordinal,
        )?;
        let frame_bytes = batch.tensor_bytes() / frames.len();
        let base = batch.device_ptr();
        for (index, frame) in frames.iter().enumerate() {
            if frame.tensor_bytes() != frame_bytes {
                return Err(crate::Error("vision.inputShapeMismatch".into()));
            }
            let code = unsafe {
                (self.runtime.memcpy_dtod)(
                    (base + index as u64 * frame_bytes as u64) as *mut c_void,
                    frame.device_ptr() as *const c_void,
                    frame_bytes,
                    3, // cudaMemcpyDeviceToDevice
                )
            };
            self.runtime.check(code, "批帧设备侧组装")?;
        }
        Ok(batch)
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
        center_crop: bool,
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
        let input = InferenceInput::new(
            session,
            input_name,
            dst_h as usize,
            dst_w as usize,
            self.ordinal,
        )?;
        // 目标几何（与 CPU 参考 prepare.rs 同规则）：detect=letterbox（min 比例+
        // round+clamp+填边）；classify=center crop（max 比例+源域中心窗口偏移，
        // F02：此前恒 letterbox 与 CPU 语义分叉）。crop 模式 rw/rh=dst（无填充带，
        // kernel pad 门不触发）。
        let (scale, crop_x, crop_y) =
            crop_geometry(frame.width, frame.height, dst_w, dst_h, center_crop);
        let (rw, rh) = if center_crop {
            (dst_w, dst_h)
        } else {
            (
                ((frame.width as f32 * scale).round() as u32).clamp(1, dst_w),
                ((frame.height as f32 * scale).round() as u32).clamp(1, dst_h),
            )
        };
        let left = (dst_w - rw) / 2;
        let top = (dst_h - rh) / 2;
        let transform = crate::sdk::ImageTransform {
            width: frame.width,
            height: frame.height,
            dst_w,
            dst_h,
            scale_x: scale,
            scale_y: scale,
            crop_x: crop_x as i32,
            crop_y: crop_y as i32,
            left,
            top,
        };
        let launched = self.kernel.launch_region(
            y_buf as u64,
            uv_buf as u64,
            frame.width,
            frame.height,
            dst_w,
            dst_h,
            rw,
            rh,
            input.device_ptr(),
            frame.width,
            0,
            0,
            0,
            scale,
            u32::from(center_crop),
            crop_x,
            crop_y,
            scale,
        );
        launched?;
        // kernel 完成=本帧 legacy 流到 record 点：事件同步替代全上下文同步
        // （后者在多路并发下连带等待其它流上全部推理 kernel，R1.3 实证空隙
        // 来源）。同步完成后才归还中间 buffer/解除纹理映射（借用窗口闭合）。
        let event = EventLease::acquire(&self.events, &self.driver)?;
        self.driver.check(
            unsafe { (self.driver.event_record)(event.get(), std::ptr::null_mut()) },
            "kernel事件记录",
        )?;
        self.driver.check(
            unsafe { (self.driver.event_synchronize)(event.get()) },
            "kernel事件同步",
        )?;
        drop(event);
        audit_frame();
        Ok((input, transform))
    }
}

/// 预处理目标几何（与 CPU 参考 prepare.rs 的 resize 分支同式同序）：classify
///（center_crop=true）取 max 比例与源域中心窗口偏移 floor((src-dst/scale)/2)
/// （F02）；detect 取 min 比例、偏移恒 0。返回 (scale, crop_x, crop_y)。
pub(crate) fn crop_geometry(
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
    center_crop: bool,
) -> (f32, f32, f32) {
    let (scale_x, scale_y) = (dst_w as f32 / src_w as f32, dst_h as f32 / src_h as f32);
    let scale = if center_crop {
        scale_x.max(scale_y)
    } else {
        scale_x.min(scale_y)
    };
    if !center_crop {
        return (scale, 0.0, 0.0);
    }
    let crop_x = ((src_w as f32 - dst_w as f32 / scale) / 2.0)
        .floor()
        .max(0.0);
    let crop_y = ((src_h as f32 - dst_h as f32 / scale) / 2.0)
        .floor()
        .max(0.0);
    (scale, crop_x, crop_y)
}

/// 显式关闭路径的注册清理（sdk-p2 T1，F03 起逐设备管线各自清空）：最后一个
/// 推理会话释放时由 FFI 层调用，unregister/COM Release 均发生在调用方线程
/// （无 loader lock、驱动工作线程存活），D3D11 对象销毁可正常完成。
///
/// 不得在 DLL 的 onexit/DLL_PROCESS_DETACH 上下文做本清理（旧 atexit 方案）：
/// 该表在 DllMain 内执行——持 loader lock，COM Release 触发的纹理销毁在
/// nvwgf2umx 内部同步等待永不完成（实测挂死）；ExitProcess 直退路径下更
/// 早于本清理的 cudart detach 已带着残留注册拆 primary 上下文（实测
/// 0xC0000005 回调已释放地址）。未显式 close 的宿主由 OS 回收，SDK 不在
/// detach 上下文补做阻塞清理。
pub fn release_for_shutdown() {
    if let Some(cache) = PIPELINES.get() {
        let guard = cache.lock().unwrap_or_else(|e| e.into_inner());
        // Err 条目（构造失败未建成管线）无注册缓存，flatten 跳过。
        for pipeline in guard.values().flatten() {
            let mut registry = pipeline.registry.lock().unwrap_or_else(|e| e.into_inner());
            registry.drain();
        }
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

#[cfg(test)]
mod crop_geometry_tests {
    use super::crop_geometry;
    use crate::sdk::{pixels::Pixels, ImageTransform, Preprocess};

    /// F02 定向：GPU classify 几何与 CPU 生产路径（prepare.rs）变换元数据逐字段
    /// 一致——非方形输入（修复前 GPU 恒 letterbox，与 CPU center crop 分叉）。
    #[test]
    fn classify_crop_geometry_matches_cpu_prepare_transform() {
        for (w, h, dst) in [
            (640u32, 480u32, 224u32),
            (480, 640, 224),
            (1920, 1080, 224),
            (800, 600, 224),
            (224, 224, 224),
            (123, 321, 224),
        ] {
            let data = vec![0u8; w as usize * h as usize * 3];
            let image = Pixels {
                width: w,
                height: h,
                format: 1, // RGB8
                planes: vec![&data[..]],
                strides: vec![w as usize * 3],
                color_matrix: 0,
                color_range: 0,
            };
            let (tensor, transform): (_, ImageTransform) = crate::sdk::prepare(
                &image,
                "images",
                Preprocess::profile(dst, true),
                64 * 1024 * 1024,
            )
            .expect("CPU classify prepare");
            drop(tensor);
            let (scale, crop_x, crop_y) = crop_geometry(w, h, dst, dst, true);
            assert_eq!(transform.scale_x, scale, "scale_x {w}x{h}->{dst}");
            assert_eq!(transform.scale_y, scale, "scale_y {w}x{h}->{dst}");
            assert_eq!(transform.crop_x, crop_x as i32, "crop_x {w}x{h}->{dst}");
            assert_eq!(transform.crop_y, crop_y as i32, "crop_y {w}x{h}->{dst}");
            assert_eq!((transform.left, transform.top), (0, 0), "{w}x{h}->{dst}");
        }
    }
}

#[cfg(test)]
mod coldstart_tests {
    use super::GpuPipeline;

    #[test]
    #[ignore = "需要真实 NVIDIA GPU（AM_SDK_ORT_DIR/AM_SDK_CUDA_DIR 提供 cudart）"]
    fn cold_thread_first_touch_initializes_pipeline() {
        // 新线程无 thread-local current 上下文，复现接入方业务线程冷启动首触。
        std::thread::spawn(|| {
            GpuPipeline::for_device(0).expect("冷线程首触 GpuPipeline 初始化应成功");
        })
        .join()
        .expect("冷启动线程执行");
    }
}
