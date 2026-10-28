//! NV12 GPU kernel 对拍测试（M2b 3.4）：手写 PTX kernel vs CPU 参考实现。
//! 真实 GPU，无需 D3D11 纹理（kernel 输入是 device buffer，等同 D2D 拷贝后形态）。
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

/// CPU 参考：NV12→RGB（BT.601 limited float）+ letterbox bilinear → NCHW。
/// 坐标规则与 crates/vision/src/preprocess.rs 完全一致。
fn reference_nchw(nv12: &[u8], src_w: usize, src_h: usize, dst: usize) -> Vec<f32> {
    let mut out = vec![114.0 / 255.0; 3 * dst * dst];
    let scale = (dst as f32 / src_w as f32).min(dst as f32 / src_h as f32);
    let rw = ((src_w as f32 * scale).round() as usize).clamp(1, dst);
    let rh = ((src_h as f32 * scale).round() as usize).clamp(1, dst);
    let left = (dst - rw) / 2;
    let top = (dst - rh) / 2;
    // NV12→RGB 全图
    let mut rgb = vec![0f32; src_w * src_h * 3];
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
            let p = (y * src_w + x) * 3;
            rgb[p] = r;
            rgb[p + 1] = g;
            rgb[p + 2] = b;
        }
    }
    // bilinear resize（与 kernel 同坐标：sx=(dx+0.5)*src/rw-0.5）
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
                let v00 = rgb[(sy0 * src_w + sx0) * 3 + ch];
                let v01 = rgb[(sy0 * src_w + sx1) * 3 + ch];
                let v10 = rgb[(sy1 * src_w + sx0) * 3 + ch];
                let v11 = rgb[(sy1 * src_w + sx1) * 3 + ch];
                let top_l = v00 + (v01 - v00) * fx;
                let bot_l = v10 + (v11 - v10) * fx;
                out[ch * dst * dst + idx] = top_l + (bot_l - top_l) * fy;
            }
        }
    }
    out
}

#[test]
#[ignore = "需要真实 NVIDIA GPU（AM_SDK_ORT_DIR 提供 cudart）"]
fn nv12_kernel_matches_cpu_reference() {
    let driver = DriverApi::load().expect("加载 nvcuda");
    let _context = driver.establish_context_driver().expect("driver 上下文");
    let kernel = KernelModule::load_nv12_letterbox(driver.clone()).expect("kernel JIT");

    let (src_w, src_h, dst) = (320usize, 240usize, 64usize);
    let nv12 = gradient_nv12(src_w, src_h);
    let scale = (dst as f32 / src_w as f32).min(dst as f32 / src_h as f32);
    let rw = ((src_w as f32 * scale).round() as u32).clamp(1, dst as u32);
    let rh = ((src_h as f32 * scale).round() as u32).clamp(1, dst as u32);

    // NV12 上传（Y 与 UV 连续同 buffer，kernel 拿两个基地址）
    let mut d_nv12: *mut std::ffi::c_void = std::ptr::null_mut();
    driver
        .check(
            unsafe { (driver.mem_alloc)(&mut d_nv12, nv12.len()) },
            "分配NV12缓冲",
        )
        .unwrap();
    driver
        .check(
            unsafe { (driver.memcpy_htod)(d_nv12, nv12.as_ptr().cast(), nv12.len()) },
            "上传NV12",
        )
        .unwrap();
    let uv_offset = (src_w * src_h) as u64;

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
            d_nv12 as u64,
            d_nv12 as u64 + uv_offset,
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

    // 对拍
    let reference = reference_nchw(&nv12, src_w, src_h, dst);
    let mut max_diff = 0f32;
    let mut bad = 0usize;
    for (i, (g, r)) in gpu_out.iter().zip(reference.iter()).enumerate() {
        let diff = (g - r).abs();
        if diff > max_diff {
            max_diff = diff;
        }
        if diff > 2e-3 {
            bad += 1;
            if bad <= 5 {
                eprintln!("[对拍] idx={i} gpu={g:.5} ref={r:.5}");
            }
        }
    }
    unsafe {
        (driver.mem_free)(d_nv12);
        (driver.mem_free)(d_out);
    }
    assert_eq!(
        bad, 0,
        "GPU/CPU 输出差异超容差（max={max_diff:.5}，{bad} 处）"
    );
}
