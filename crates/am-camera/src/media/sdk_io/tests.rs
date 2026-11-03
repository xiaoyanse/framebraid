use super::*;
use crate::credentials::Secret;
use std::{
    io::{Cursor, Read, Write},
    net::TcpListener,
    time::Duration,
};

fn png() -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        3,
        2,
        image::Rgba([8, 32, 96, 255]),
    ))
    .write_to(&mut bytes, image::ImageFormat::Png)
    .unwrap();
    bytes.into_inner()
}

#[test]
fn decode_preserves_encoded_and_rejects_pixel_bomb() {
    let bytes = png();
    let image = decode_image(bytes.clone(), 6, true).unwrap();
    assert_eq!(image.encoded, bytes);
    assert_eq!((image.width, image.height), (3, 2));
    assert_eq!(image.rgba.unwrap().len(), 24);
    assert_eq!(decode_image(bytes, 5, true).err(), Some(IoError::Resource));
}

#[test]
fn jpeg_exif_orientation_keeps_original_bytes_and_rotates_rgba() {
    let mut cursor = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(3, 2)
        .write_to(&mut cursor, image::ImageFormat::Jpeg)
        .unwrap();
    let original = cursor.into_inner();
    let exif = [
        b'E', b'x', b'i', b'f', 0, 0, b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0, 0x12, 1, 3, 0, 1, 0, 0,
        0, 6, 0, 0, 0, 0, 0, 0, 0,
    ];
    let mut encoded = vec![0xff, 0xd8, 0xff, 0xe1, 0, 34];
    encoded.extend_from_slice(&exif);
    encoded.extend_from_slice(&original[2..]);
    let image = decode_image(encoded.clone(), 6, true).unwrap();
    assert_eq!(image.encoded, encoded);
    assert_eq!((image.width, image.height), (2, 3));
    assert_eq!(image.rgba.unwrap().len(), 24);
}

#[test]
fn local_digest_capture_preserves_original_and_query_uri() {
    let bytes = png();
    for algorithm in ["MD5", "SHA-256"] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let expected = bytes.clone();
        let server = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            for index in 0..2 {
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(std::time::Instant::now() < deadline);
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("{error}"),
                    }
                };
                // Windows接受的连接可能继承listener非阻塞状态；夹具按阻塞逐字节读取HTTP头。
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    assert_eq!(stream.read(&mut byte).unwrap(), 1);
                    request.push(byte[0]);
                    assert!(request.len() < 16 * 1024);
                }
                let request = String::from_utf8(request).unwrap();
                if index == 0 {
                    assert!(!request.to_ascii_lowercase().contains("authorization:"));
                    write!(stream, "HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Digest realm=\"test\", nonce=\"fixed\", qop=\"auth,auth-int\", algorithm={algorithm}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                } else {
                    assert!(request.contains("uri=\"/picture?channel=101\""));
                    assert!(request.contains(&format!("algorithm={algorithm}")));
                    assert!(!request.contains("sample-password"));
                    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", expected.len()).unwrap();
                    stream.write_all(&expected).unwrap();
                }
            }
        });
        let image = capture(
            &format!("http://127.0.0.1:{port}/picture?channel=101"),
            &Secret {
                username: "sample-user".into(),
                password: "sample-password".into(),
            },
            Duration::from_secs(3),
            4096,
            16,
            true,
        )
        .unwrap();
        assert_eq!(image.encoded, bytes);
        assert_eq!((image.width, image.height), (3, 2));
        server.join().unwrap();
    }
}

#[test]
fn oversized_http_body_rejected_before_decode() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = [0; 4096];
        let count = stream.read(&mut request).unwrap();
        assert!(count > 0);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 99999\r\nConnection: close\r\n\r\n")
            .unwrap();
    });
    let result = capture(
        &format!("http://127.0.0.1:{port}/picture"),
        &Secret {
            username: String::new(),
            password: String::new(),
        },
        Duration::from_secs(2),
        1024,
        16,
        false,
    );
    assert_eq!(result.err(), Some(IoError::Resource));
    server.join().unwrap();
}

#[test]
fn rtsp_authority_accepts_separate_credentials() {
    let mut url = reqwest::Url::parse("rtsp://127.0.0.1:8554/test").unwrap();
    assert!(url.set_username("sdk-transport-user").is_ok());
    assert!(url.set_password(Some("sdk-transport-secret")).is_ok());
    assert_eq!(url.host_str(), Some("127.0.0.1"));
}

#[test]
#[ignore = "显式设置 AM_RT_FFMPEG_DIR 与 AM_SDK_FILE_SOURCE_SAMPLE（真实 MP4）"]
fn reader_file_mode_reads_first_packet() {
    let ffmpeg = std::env::var("AM_RT_FFMPEG_DIR").unwrap();
    let sample = std::env::var("AM_SDK_FILE_SOURCE_SAMPLE").unwrap();
    let api = std::sync::Arc::new(
        crate::media::ffmpeg_lib::FfmpegApi::load_from(std::path::Path::new(&ffmpeg)).unwrap(),
    );
    let secret = Secret {
        username: String::new(),
        password: String::new(),
    };
    let mut reader = super::rtsp::Reader::open_with_budget(
        api,
        &sample,
        &secret,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        Duration::from_secs(5),
        |_| Ok(()),
        true,
    )
    .unwrap();
    assert_eq!(reader.info().codec, 1, "expect h264");
    assert!(reader.info().width > 0, "probe should fill dimensions");
    let mut packets = 0;
    loop {
        match reader.read_packet_with_budget(|_| Ok(())).unwrap() {
            Some(packet) => {
                if packets < 3 {
                    let types: Vec<u8> = packet
                        .data
                        .windows(4)
                        .enumerate()
                        .filter(|(_, w)| *w == [0u8, 0, 0, 1])
                        .map(|(i, _)| packet.data[i + 4] & 31)
                        .collect();
                    eprintln!(
                        "packet#{} key={} len={} types={:?}",
                        packets,
                        packet.keyframe,
                        packet.data.len(),
                        types
                    );
                }
                packets += 1;
            }
            None => break,
        }
    }
    eprintln!(
        "reader file mode: {} packets, extradata {} bytes",
        packets,
        reader.info().extradata.len()
    );
    assert!(packets > 0, "file must yield packets before EOF");
}

#[test]
#[ignore = "显式设置 AM_RT_FFMPEG_DIR 与 AM_SDK_SEEK_SAMPLE（真实多 GOP MP4）"]
fn reader_seek_moves_to_keyframe() {
    let ffmpeg = std::env::var("AM_RT_FFMPEG_DIR").unwrap();
    let sample = std::env::var("AM_SDK_SEEK_SAMPLE").unwrap();
    let api = std::sync::Arc::new(
        crate::media::ffmpeg_lib::FfmpegApi::load_from(std::path::Path::new(&ffmpeg)).unwrap(),
    );
    let secret = Secret {
        username: String::new(),
        password: String::new(),
    };
    let mut reader = super::rtsp::Reader::open_with_budget(
        api,
        &sample,
        &secret,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        Duration::from_secs(5),
        |_| Ok(()),
        true,
    )
    .unwrap();
    assert!((3900..=4100).contains(&reader.duration_ms()));
    let num = i64::from(reader.info().time_base_num);
    let den = i64::from(reader.info().time_base_den);
    let pts_ms = |pts: i64| pts * 1000 * num / den;
    let has_idr = |data: &[u8]| {
        data.windows(5)
            .any(|w| w[..4] == [0, 0, 0, 1] && w[4] & 31 == 5)
    };
    reader.seek(1500, true).unwrap();
    let packet = reader.read_packet().unwrap().unwrap();
    assert!(has_idr(&packet.data), "backward packet must contain an IDR");
    assert!(
        packet.keyframe && (1000..=1499).contains(&pts_ms(packet.pts)),
        "backward pts={}",
        pts_ms(packet.pts)
    );
    reader.seek(1500, false).unwrap();
    let packet = reader.read_packet().unwrap().unwrap();
    assert!(has_idr(&packet.data), "forward packet must contain an IDR");
    assert!(
        packet.keyframe && (1501..=2000).contains(&pts_ms(packet.pts)),
        "forward pts={}",
        pts_ms(packet.pts)
    );
    reader.seek(0, true).unwrap();
    let mut previous = i64::MIN;
    for index in 0..5 {
        let packet = reader.read_packet().unwrap().unwrap();
        let pts = pts_ms(packet.pts);
        if index == 0 {
            assert!(packet.keyframe && (0..=99).contains(&pts));
            assert!(has_idr(&packet.data), "first packet must contain an IDR");
        }
        assert!(pts >= previous, "pts {pts} follows {previous}");
        previous = pts;
    }
}
