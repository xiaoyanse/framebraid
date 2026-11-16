use super::*;
#[test]
fn padded_bgr_and_letterbox_are_mapped() {
    let bytes = [0, 0, 255, 0, 255, 0, 99, 99];
    let pixels = Pixels {
        width: 2,
        height: 1,
        format: 2,
        planes: vec![&bytes],
        strides: vec![8],
        color_matrix: 0,
        color_range: 0,
    };
    let (tensor, m) = prepare(&pixels, "images", Preprocess::profile(2, false), 1024).unwrap();
    assert_eq!(m.top, 0);
    assert_eq!(tensor.floats().unwrap()[0], 1.0);
    assert_eq!(tensor.floats().unwrap()[3], 114.0 / 255.0);
    assert!(prepare(&pixels, "images", Preprocess::profile(640, false), 12).is_err());
}

fn mapping(value: &ImageTransform) -> [u32; 10] {
    [
        value.width,
        value.height,
        value.dst_w,
        value.dst_h,
        value.scale_x.to_bits(),
        value.scale_y.to_bits(),
        value.crop_x as u32,
        value.crop_y as u32,
        value.left,
        value.top,
    ]
}

fn compare(pixels: &Pixels<'_>, config: Preprocess, budget: usize) {
    let (actual, transform) = prepare(pixels, "images", config, budget).unwrap();
    let (expected, old_transform) =
        super::reference::prepare(pixels, "images", config, budget).unwrap();
    assert_eq!(actual.bytes(), expected.bytes());
    assert_eq!(actual.info.shape, expected.info.shape);
    assert_eq!(actual.info.name, expected.info.name);
    assert_eq!(mapping(&transform), mapping(&old_transform));
    assert_eq!(actual.bytes().as_ptr() as usize % 8, 0);
}

#[test]
fn direct_storage_matches_frozen_algorithm_for_all_pixel_and_preprocess_modes() {
    for format in 0..=3 {
        let channels = if format == 0 { 4 } else { 3 };
        let stride = if format == 3 { 7 } else { 4 * channels + 5 };
        let plane: Vec<_> = (0..stride * 2)
            .map(|n| ((n * 67 + 3) % 256) as u8)
            .collect();
        let uv = [41, 212, 150, 93, 99, 99];
        for matrix in 0..if format == 3 { 3 } else { 1 } {
            for range in 0..if format == 3 { 2 } else { 1 } {
                let image = Pixels {
                    width: 4,
                    height: 2,
                    format,
                    planes: if format == 3 {
                        vec![&plane, &uv]
                    } else {
                        vec![&plane]
                    },
                    strides: if format == 3 {
                        vec![stride, 6]
                    } else {
                        vec![stride]
                    },
                    color_matrix: matrix,
                    color_range: range,
                };
                for (width, height) in [(5, 7), (8, 4), (1, 1)] {
                    for resize in 0..=2 {
                        for normalize in 0..=2 {
                            for nhwc in [false, true] {
                                for bgr in [false, true] {
                                    let config = Preprocess {
                                        width,
                                        height,
                                        resize,
                                        normalize,
                                        nhwc,
                                        bgr,
                                        fill: 83.125,
                                        mean: [0.123, 0.456, 0.789],
                                        std: [0.2, 0.7, 1.1],
                                    };
                                    compare(&image, config, width as usize * height as usize * 12);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn rejected_inputs_keep_errors_and_do_not_allocate_a_large_destination() {
    let bytes = [50u8; 24];
    let image = Pixels {
        width: 4,
        height: 2,
        format: 1,
        planes: vec![&bytes],
        strides: vec![12],
        color_matrix: 0,
        color_range: 0,
    };
    let valid = Preprocess::profile(4, false);
    let cases = [
        Preprocess { width: 0, ..valid },
        Preprocess {
            height: 8193,
            ..valid
        },
        Preprocess { resize: 3, ..valid },
        Preprocess {
            normalize: 3,
            ..valid
        },
        Preprocess {
            fill: f32::NAN,
            ..valid
        },
        Preprocess {
            mean: [f32::INFINITY; 3],
            ..valid
        },
        Preprocess {
            std: [0.0; 3],
            ..valid
        },
    ];
    for config in cases {
        let actual = prepare(&image, "images", config, 1024).err().unwrap();
        let expected = super::reference::prepare(&image, "images", config, 1024)
            .err()
            .unwrap();
        assert_eq!(actual.0, expected.0);
    }
    assert_eq!(
        prepare(&image, "images", valid, 4 * 4 * 12 - 1)
            .err()
            .unwrap()
            .0,
        "vision.imageTooLarge"
    );
    assert!(prepare(&image, "images", valid, 4 * 4 * 12).is_ok());
}

#[test]
fn mutable_float_view_rechecks_changed_metadata_before_casting() {
    let mut data = TensorData::new(
        TensorInfo {
            name: "images".into(),
            dtype: 4,
            shape: vec![3],
        },
        &[1, 2, 3],
    )
    .unwrap();
    assert_eq!(data.floats_mut().err().unwrap().0, "vision.dtypeMismatch");
    data.info.dtype = 0;
    data.info.shape = vec![1];
    assert_eq!(
        data.floats_mut().err().unwrap().0,
        "vision.inputBytesMismatch"
    );
    let mut scalar = TensorData::new(
        TensorInfo {
            name: "scalar".into(),
            dtype: 0,
            shape: vec![],
        },
        &1f32.to_ne_bytes(),
    )
    .unwrap();
    scalar.floats_mut().unwrap()[0] = 2.0;
    assert_eq!(scalar.floats().unwrap(), &[2.0]);
    scalar.info.shape = vec![2];
    assert_eq!(
        scalar.floats_mut().err().unwrap().0,
        "vision.inputBytesMismatch"
    );
}

#[test]
fn parallel_prepares_own_independent_storage() {
    let workers: Vec<_> = (0..4)
        .map(|index| {
            std::thread::spawn(move || {
                let bytes = vec![index * 50u8; 24];
                let image = Pixels {
                    width: 4,
                    height: 2,
                    format: 1,
                    planes: vec![&bytes],
                    strides: vec![12],
                    color_matrix: 0,
                    color_range: 0,
                };
                let (held, _) =
                    prepare(&image, "images", Preprocess::profile(4, false), 192).unwrap();
                let original = held.bytes().to_vec();
                for _ in 0..20 {
                    let other = Preprocess {
                        nhwc: true,
                        bgr: true,
                        ..Preprocess::profile(4, false)
                    };
                    compare(&image, other, 192);
                }
                assert_eq!(held.bytes(), original);
                original
            })
        })
        .collect();
    let outputs: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    assert!(outputs.windows(2).all(|pair| pair[0] != pair[1]));
}

// sdk-prepopt-20261116 分段计时探针（侦察未决 4）：镜像新旧循环的相位结构实测
// 坐标/rgb/归一化与写出份额，验证或修正静态归因。相位变体是测试本地复刻，生产
// 正确性仍由上面的冻结 oracle 把关。手动跑：
// cargo test -p am-vision --lib segment_probe -- --ignored --nocapture
#[test]
#[ignore = "分段计时探针：仅手动取证用"]
fn segment_probe_attributes_prepare_costs() {
    use std::hint::black_box;
    use std::time::Instant;
    fn nv12(w: usize, h: usize) -> (Vec<u8>, Vec<u8>) {
        let y = (0..w * h).map(|i| ((i * 37 + 11) % 256) as u8).collect();
        let uv = (0..w * h / 2)
            .map(|i| ((i * 53 + 29) % 256) as u8)
            .collect();
        (y, uv)
    }
    fn median(samples: &[f64]) -> f64 {
        let mut sorted = samples.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        sorted[sorted.len() / 2]
    }
    fn bench(label: &str, rounds: usize, mut body: impl FnMut()) -> f64 {
        for _ in 0..3 {
            body();
        }
        let mut ms = Vec::with_capacity(rounds);
        for _ in 0..rounds {
            let start = Instant::now();
            body();
            ms.push(start.elapsed().as_secs_f64() * 1e3);
        }
        let value = median(&ms);
        println!("{label:<44} {value:8.3} ms");
        value
    }
    for (w, h) in [(640usize, 640usize), (1920, 1080)] {
        println!("== NV12 {w}x{h} -> 640 letterbox/NCHW/normalize=1（BT.709 有限范围）");
        let (y, uv) = nv12(w, h);
        let pixels = Pixels {
            width: w as u32,
            height: h as u32,
            format: 3,
            planes: vec![&y, &uv],
            strides: vec![w, w],
            color_matrix: 1,
            color_range: 0,
        };
        let config = Preprocess::profile(640, false);
        let budget = 64 * 1024 * 1024;
        // 旧实现整段（冻结参考，A）。
        let old_full = bench("A 旧参考 prepare 全段", 12, || {
            black_box(super::reference::prepare(&pixels, "images", config, budget).unwrap());
        });
        // 旧循环相位复刻：仅坐标链+填充分支（生产式 (v+0.5)/scale-0.5，crop=0）。
        let (m_w, m_h, left, top) = (w as f32, h as f32, 0u32, 0u32);
        let scale_of = || (640.0 / m_w).min(640.0 / m_h);
        let old_coords = bench("B 旧循环坐标链（差值=坐标份额）", 12, || {
            let (sx, sy) = (scale_of(), scale_of());
            let scale = sx.min(sy);
            let (sw, sh) = (
                ((m_w * scale).round() as u32).max(1),
                ((m_h * scale).round() as u32).max(1),
            );
            let mut sink = 0f32;
            for y in 0..640u32 {
                for x in 0..640u32 {
                    if x < left || y < top || x >= left + sw || y >= top + sh {
                        sink += 1.0;
                        continue;
                    }
                    let xx = ((x as f32 + 0.5) / sx - 0.5).clamp(0.0, m_w - 1.0);
                    let yy = ((y as f32 + 0.5) / sy - 0.5).clamp(0.0, m_h - 1.0);
                    let (ix, iy) = (xx as usize, yy as usize);
                    sink += xx - ix as f32 + (yy - iy as f32);
                }
            }
            black_box(sink);
        });
        // 旧循环相位复刻：坐标 + 4 tap rgb + 混合（无归一化/无写出）。
        let old_rgb = bench(
            "C 旧循环坐标+rgb×4+混合（差值=rgb份额）",
            12,
            || {
                let (sx, sy) = (scale_of(), scale_of());
                let scale = sx.min(sy);
                let (sw, sh) = (
                    ((m_w * scale).round() as u32).max(1),
                    ((m_h * scale).round() as u32).max(1),
                );
                let mut sink = 0f32;
                for y in 0..640u32 {
                    for x in 0..640u32 {
                        if x < left || y < top || x >= left + sw || y >= top + sh {
                            continue;
                        }
                        let xx = ((x as f32 + 0.5) / sx - 0.5).clamp(0.0, m_w - 1.0);
                        let yy = ((y as f32 + 0.5) / sy - 0.5).clamp(0.0, m_h - 1.0);
                        let (ix, iy) = (xx as usize, yy as usize);
                        let (fx, fy) = (xx - ix as f32, yy - iy as f32);
                        let a = pixels.rgb(ix, iy);
                        let b = pixels.rgb((ix + 1).min(w - 1), iy);
                        let c = pixels.rgb(ix, (iy + 1).min(h - 1));
                        let d = pixels.rgb((ix + 1).min(w - 1), (iy + 1).min(h - 1));
                        sink += (a[0] * (1.0 - fx) + b[0] * fx) * (1.0 - fy)
                            + (c[0] * (1.0 - fx) + d[0] * fx) * fy;
                    }
                }
                black_box(sink);
            },
        );
        // 新实现整段（生产，D）与相位：仅列常数、列常数+行采样到 raw（无归一化/写出）。
        let new_full = bench("D 新 prepare 全段", 12, || {
            black_box(prepare(&pixels, "images", config, budget).unwrap());
        });
        let new_cols = bench(
            "E 新列常数预计算（差值=坐标摊余）",
            12,
            || {
                let scale = scale_of();
                let sw = ((m_w * scale).round() as u32).max(1);
                let mut sink = 0f32;
                let cols: Vec<f32> = (0..640u32)
                    .map(|x| {
                        if x >= sw {
                            return 0.0;
                        }
                        let xx = ((x as f32 + 0.5) / scale - 0.5).clamp(0.0, m_w - 1.0);
                        let ix = xx as usize;
                        let fx = xx - ix as f32;
                        sink += fx;
                        fx
                    })
                    .collect();
                black_box((&cols, sink));
            },
        );
        let new_sample = bench(
            "F 新列常数+行采样到raw（差值=rgb/写出份额）",
            12,
            || {
                let scale = scale_of();
                let (sw, sh) = (
                    ((m_w * scale).round() as u32).max(1),
                    ((m_h * scale).round() as u32).max(1),
                );
                let cols: Vec<(f32, usize, usize)> = (0..640u32)
                    .filter(|x| *x < sw)
                    .map(|x| {
                        let xx = ((x as f32 + 0.5) / scale - 0.5).clamp(0.0, m_w - 1.0);
                        let ix = xx as usize;
                        (xx - ix as f32, ix, (ix + 1).min(w - 1))
                    })
                    .collect();
                let mut raw = vec![0f32; 3 * 640];
                for y in 0..640usize {
                    if y >= sh as usize {
                        continue;
                    }
                    let yy = ((y as f32 + 0.5) / scale - 0.5).clamp(0.0, m_h - 1.0);
                    let iy = yy as usize;
                    let y1 = (iy + 1).min(h - 1);
                    let fy = yy - iy as f32;
                    for (x, (fx, ix, x1)) in cols.iter().enumerate() {
                        let rgb = if *fx == 0.0 && fy == 0.0 {
                            pixels.rgb(*ix, iy)
                        } else {
                            let a = pixels.rgb(*ix, iy);
                            let b = pixels.rgb(*x1, iy);
                            let c = pixels.rgb(*ix, y1);
                            let d = pixels.rgb(*x1, y1);
                            let (wx, wy) = (1.0 - *fx, 1.0 - fy);
                            [
                                (a[0] * wx + b[0] * fx) * wy + (c[0] * wx + d[0] * fx) * fy,
                                (a[1] * wx + b[1] * fx) * wy + (c[1] * wx + d[1] * fx) * fy,
                                (a[2] * wx + b[2] * fx) * wy + (c[2] * wx + d[2] * fx) * fy,
                            ]
                        };
                        raw[x] = rgb[0];
                        raw[640 + x] = rgb[1];
                        raw[1280 + x] = rgb[2];
                    }
                    black_box(&raw);
                }
            },
        );
        let rgb_share = old_rgb - old_coords;
        let coord_share = old_coords;
        let store_share = old_full - old_rgb;
        println!(
            "旧归因：坐标 {coord_share:.3}ms | rgb×4+混合 {rgb_share:.3}ms | 归一化+写出 {store_share:.3}ms（合计 {old_full:.3}）"
        );
        println!(
            "新结构：列常数 {new_cols:.3}ms | 采样+行写出 {new_sample:.3}ms | 全段 {new_full:.3}ms（D-F≈归一化+终写 {:.3}）",
            new_full - new_sample
        );
    }
}
