//! BGRA GPU kernel 对拍测试（P1 修复后形态）：手写 PTX kernel vs CPU 参考。
//! 真实 GPU，无需 D3D11 纹理（kernel 输入是 device buffer，等同 VP 转换+
//! D2D 搬运后的形态）；色彩转换（NV12→RGB BT.601）由 VP 完成，本测试在
//! CPU 侧镜像后只对拍重采样几何——两路径同域（RGB 双线性）。
use super::driver_sys::{DriverApi, KernelModule};

/// 构造渐变 NV12 帧（宽高同 M2a 样本量级）。
fn gradient_nv12(w: usize, h: usize) -> Vec<u8> {
    let mut data = vec![0u8; w * h * 3 / 2];
    for y in 0..h {
        for x in 0..w {
            data[y * w + x] = (16 + (x * 219 / w.max(1))) as u8; // Y 渐变
        }
    }
    let uv_base = w * h;
    for y in 0..h / 2 {
        for x in 0..w / 2 {
            let i = uv_base + y * w + x * 2;
            data[i] = (64 + x * 180 / (w / 2).max(1)) as u8; // U 渐变
            data[i + 1] = (200 - y * 160 / (h / 2).max(1)) as u8; // V 渐变
        }
    }
    data
}

/// CPU 参考：NV12→RGB（BT.601 limited float，VP 转换的镜像——同式）+
/// letterbox bilinear → NCHW。坐标规则与 crates/vision/src/preprocess.rs
/// 完全一致。
fn reference_nchw(nv12: &[u8], src_w: usize, src_h: usize, dst: usize) -> (Vec<u8>, Vec<f32>) {
    // 先 NV12→RGB u8 域（VP 定点转换的镜像；±1 LSB 量化差不影响几何对拍，
    // 此处用 f32 镜像后回 u8——kernel 输入即此 BGRA 字节流）。
    let mut bgra = vec![0u8; src_w * src_h * 4];
    for y in 0..src_h {
        for x in 0..src_w {
            let yy = nv12[y * src_w + x] as f32 / 255.0;
            let uv_i = src_w * src_h + (y / 2) * src_w + (x / 2) * 2;
            let u = nv12[uv_i] as f32 / 255.0;
            let v = nv12[uv_i + 1] as f32 / 255.0;
            let c = 1.164 * (yy - 0.0625);
            let r = (c + 1.596 * (v - 0.5)).clamp(0.0, 1.0);
            let g = (c - 0.391 * (u - 0.5) - 0.813 * (v - 0.5)).clamp(0.0, 1.0);
            let b = (c + 2.018 * (u - 0.5)).clamp(0.0, 1.0);
            let p = (y * src_w + x) * 4;
            bgra[p] = (b * 255.0).round() as u8;
            bgra[p + 1] = (g * 255.0).round() as u8;
            bgra[p + 2] = (r * 255.0).round() as u8;
            bgra[p + 3] = 255;
        }
    }
    let mut out = vec![114.0 / 255.0; 3 * dst * dst];
    let scale = (dst as f32 / src_w as f32).min(dst as f32 / src_h as f32);
    let rw = ((src_w as f32 * scale).round() as usize).clamp(1, dst);
    let rh = ((src_h as f32 * scale).round() as usize).clamp(1, dst);
    let left = (dst - rw) / 2;
    let top = (dst - rh) / 2;
    for dy in 0..rh {
        for dx in 0..rw {
            let sx = (dx as f32 + 0.5) * src_w as f32 / rw as f32 - 0.5;
            let sy = (dy as f32 + 0.5) * src_h as f32 / rh as f32 - 0.5;
            // 边缘钳制：fx 先于钳制计算（负 sx 时 fx=sx-floor(sx)∈[0,1)，邻居钳到 0）
            let fx = sx - sx.floor();
            let fy = sy - sy.floor();
            let sx0 = sx.floor().max(0.0) as usize;
            let sy0 = sy.floor().max(0.0) as usize;
            let sx1 = (sx0 + 1).min(src_w - 1);
            let sy1 = (sy0 + 1).min(src_h - 1);
            let idx = (dy + top) * dst + dx + left;
            for ch in 0..3 {
                let byte = [2usize, 1, 0][ch]; // NCHW ch: 0=R 1=G 2=B；BGRA 内 R=+2 G=+1 B=+0
                let v00 = bgra[(sy0 * src_w + sx0) * 4 + byte] as f32 / 255.0;
                let v01 = bgra[(sy0 * src_w + sx1) * 4 + byte] as f32 / 255.0;
                let v10 = bgra[(sy1 * src_w + sx0) * 4 + byte] as f32 / 255.0;
                let v11 = bgra[(sy1 * src_w + sx1) * 4 + byte] as f32 / 255.0;
                let top_l = v00 + (v01 - v00) * fx;
                let bot_l = v10 + (v11 - v10) * fx;
                out[ch * dst * dst + idx] = top_l + (bot_l - top_l) * fy;
            }
        }
    }
    (bgra, out)
}

#[test]
#[ignore = "需要真实 NVIDIA GPU（AM_SDK_ORT_DIR 提供 cudart）"]
fn bgra_kernel_matches_cpu_reference() {
    let driver = DriverApi::load().expect("加载 nvcuda");
    let _context = driver.establish_context_driver().expect("driver 上下文");
    let kernel = KernelModule::load_bgra_letterbox(driver.clone()).expect("kernel JIT");

    let (src_w, src_h, dst) = (320usize, 240usize, 64usize);
    let nv12 = gradient_nv12(src_w, src_h);
    let (bgra, reference) = reference_nchw(&nv12, src_w, src_h, dst);
    let scale = (dst as f32 / src_w as f32).min(dst as f32 / src_h as f32);
    let rw = ((src_w as f32 * scale).round() as u32).clamp(1, dst as u32);
    let rh = ((src_h as f32 * scale).round() as u32).clamp(1, dst as u32);

    // BGRA 上传（单 buffer，kernel 拿基地址）
    let mut d_bgra: *mut std::ffi::c_void = std::ptr::null_mut();
    driver
        .check(
            unsafe { (driver.mem_alloc)(&mut d_bgra, bgra.len()) },
            "分配BGRA缓冲",
        )
        .unwrap();
    driver
        .check(
            unsafe { (driver.memcpy_htod)(d_bgra, bgra.as_ptr().cast(), bgra.len()) },
            "上传BGRA",
        )
        .unwrap();

    let out_len = 3 * dst * dst * 4;
    let mut d_out: *mut std::ffi::c_void = std::ptr::null_mut();
    driver
        .check(
            unsafe { (driver.mem_alloc)(&mut d_out, out_len) },
            "分配输出",
        )
        .unwrap();

    kernel
        .launch(
            d_bgra as u64,
            src_w as u32,
            src_h as u32,
            dst as u32,
            dst as u32,
            rw,
            rh,
            d_out as u64,
        )
        .expect("kernel 启动");

    let mut gpu_out = vec![0f32; 3 * dst * dst];
    driver
        .check(
            unsafe { (driver.memcpy_dtoh)(gpu_out.as_mut_ptr().cast(), d_out, out_len) },
            "取回输出",
        )
        .unwrap();

    // 对拍：两路径同域（RGB 双线性）同输入字节，纯 f32 运算序一致——容差
    // 收紧到 f32 单操作级（修复前 YUV 域双路径 2e-3 容差的时代结束）
    let mut max_diff = 0f32;
    let mut bad = 0usize;
    for (i, (g, r)) in gpu_out.iter().zip(reference.iter()).enumerate() {
        let diff = (g - r).abs();
        if diff > max_diff {
            max_diff = diff;
        }
        if diff > 1e-5 {
            bad += 1;
            if bad <= 5 {
                eprintln!("[对拍] idx={i} gpu={g:.6} ref={r:.6}");
            }
        }
    }
    unsafe {
        (driver.mem_free)(d_bgra);
        (driver.mem_free)(d_out);
    }
    assert_eq!(
        bad, 0,
        "GPU/CPU 输出差异超容差（max={max_diff:.7}，{bad} 处）"
    );
}
