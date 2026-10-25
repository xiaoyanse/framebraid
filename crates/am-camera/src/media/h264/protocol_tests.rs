//! 搬运协议帧单测（批 1.6）：32B 头打包/解包对偶、关键帧标志、控制帧 JSON。

use super::protocol::*;

#[test]
fn frame_header_roundtrip_and_layout() {
    let header = H264FrameHeader {
        flags: FLAG_KEYFRAME,
        generation: 7,
        time_base_den: 90_000,
        seq: 0x0102_0304_0506_0708,
        pts: -12345,
    };
    let packed = pack_h264_header(&header);
    assert_eq!(packed.len(), FRAME_HEADER_LEN);
    assert_eq!(&packed[0..4], b"H264");
    let unpacked = unpack_h264_header(&packed).expect("合法头应解包");
    assert_eq!(unpacked, header);
    assert!(unpacked.is_keyframe());
    // 非关键帧标志位
    let non_key = H264FrameHeader { flags: 0, ..header };
    assert!(!unpack_h264_header(&pack_h264_header(&non_key))
        .expect("解包")
        .is_keyframe());
}

#[test]
fn frame_header_rejects_short_or_wrong_magic() {
    assert!(unpack_h264_header(&[0u8; 16]).is_none());
    let mut packed = pack_h264_header(&H264FrameHeader {
        flags: 0,
        generation: 1,
        time_base_den: 90_000,
        seq: 1,
        pts: 0,
    });
    packed[0] = b'X'; // 魔数破坏
    assert!(unpack_h264_header(&packed).is_none());
}

#[test]
fn pack_frame_concatenates_header_and_payload() {
    let header = H264FrameHeader {
        flags: FLAG_KEYFRAME,
        generation: 2,
        time_base_den: 90_000,
        seq: 42,
        pts: 90000,
    };
    let payload = [0u8, 0, 0, 1, NAL_MARKER, 1, 2];
    let frame = pack_frame(&header, &payload);
    assert_eq!(frame.len(), FRAME_HEADER_LEN + payload.len());
    assert_eq!(&frame[FRAME_HEADER_LEN..], &payload);
    assert_eq!(
        unpack_h264_header(&frame).expect("解包").seq,
        42,
        "整帧前缀应可解出头"
    );
}

/// 控制帧 JSON：type 判别 + snake_case 字段（批 2 前端对接契约锚点）。
#[test]
fn control_frames_json_roundtrip() {
    let config = ControlFrame::StreamConfig {
        codec: "h264".into(),
        width: 1920,
        height: 1080,
        fps: Some(25.0),
        decode_generation: 3,
    };
    let json = serde_json::to_string(&config).expect("序列化");
    assert!(
        json.contains(r#""type":"h264_stream_config""#),
        "实际: {json}"
    );
    assert!(json.contains(r#""decode_generation":3"#), "实际: {json}");
    let parsed: ControlFrame = serde_json::from_str(&json).expect("反序列化");
    assert_eq!(parsed, config);
    // fps 探测失败时省略
    let no_fps = ControlFrame::StreamConfig {
        codec: "h264".into(),
        width: 640,
        height: 360,
        fps: None,
        decode_generation: 1,
    };
    let json = serde_json::to_string(&no_fps).expect("序列化");
    assert!(!json.contains("fps"));
    let resync = ControlFrame::PreviewReconnect {
        reason: RESYNC_SLOW_CLIENT.into(),
        decode_generation: 3,
    };
    let json = serde_json::to_string(&resync).expect("序列化");
    assert!(json.contains(r#""type":"preview_reconnect_required""#));
    assert_eq!(
        serde_json::from_str::<ControlFrame>(&json).expect("反序列化"),
        resync
    );
}

/// 检测叠层控制帧 JSON（批③）：type 判别 + ROI id/motion/扁平轮廓 + 帧序号归属。
#[test]
fn detection_overlay_control_frame_json() {
    let frame = ControlFrame::DetectionOverlay {
        detections: vec![
            DetectionOverlayItem {
                id: "roi-1".into(),
                motion: true,
                contours: vec![vec![0.1, 0.1, 0.4, 0.1, 0.4, 0.4, 0.1, 0.4]],
            },
            DetectionOverlayItem {
                id: "roi-2".into(),
                motion: false,
                contours: vec![vec![0.5, 0.5, 0.6, 0.5, 0.6, 0.6]],
            },
        ],
        source_frame_seq: 42,
        workstation: "ws-1".into(),
        task: "task-1".into(),
    };
    let json = serde_json::to_string(&frame).expect("序列化");
    assert!(
        json.contains(r#""type":"detection_overlay""#),
        "实际: {json}"
    );
    assert!(
        json.contains(r#""contours":[[0.1,0.1,0.4,0.1,0.4,0.4,0.1,0.4]]"#),
        "轮廓应扁平序列化: {json}"
    );
    assert!(json.contains(r#""source_frame_seq":42"#), "实际: {json}");
    assert!(json.contains(r#""motion":true"#), "实际: {json}");
    assert_eq!(
        serde_json::from_str::<ControlFrame>(&json).expect("反序列化"),
        frame
    );
    // 空轮廓项允许（前端宽容解析：无有效轮廓的项跳过）
    let empty = ControlFrame::DetectionOverlay {
        detections: vec![],
        source_frame_seq: 1,
        workstation: String::new(),
        task: String::new(),
    };
    let json = serde_json::to_string(&empty).expect("序列化");
    assert_eq!(
        serde_json::from_str::<ControlFrame>(&json).expect("反序列化"),
        empty
    );
}

const NAL_MARKER: u8 = 5;
