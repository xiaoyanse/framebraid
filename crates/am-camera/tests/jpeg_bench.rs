//! JPEG 解码全链计时（R6 基准资产）：decode_image 现网路径。
//! AM_SDK_JPEG_BENCH=样本JPEG路径。
use std::time::Instant;

#[test]
#[ignore = "诊断用：AM_SDK_JPEG_BENCH 指向 JPEG 样本"]
fn jpeg_decode_stage_timing() {
    let path = std::env::var("AM_SDK_JPEG_BENCH").expect("AM_SDK_JPEG_BENCH");
    let encoded = std::fs::read(&path).expect("读取样本");
    let rounds = 30;
    let mut total = 0u128;
    let (mut width, mut height) = (0, 0);
    for _ in 0..rounds {
        let t0 = Instant::now();
        let image = am_camera::media::sdk_io::decode_image(encoded.clone(), 16_777_216, true)
            .expect("解码");
        let t1 = Instant::now();
        total += (t1 - t0).as_micros();
        width = image.width;
        height = image.height;
    }
    eprintln!(
        "[bench] decode_image 全链 avg {:.2}ms（{width}x{height}，{rounds} 轮）",
        total as f64 / rounds as f64 / 1000.0,
    );
}
