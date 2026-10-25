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
