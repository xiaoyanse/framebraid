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
