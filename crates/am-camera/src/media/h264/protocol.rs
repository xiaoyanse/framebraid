//! H.264 搬运 WS 协议帧（批 1.2）：32B 二进制头+Annex B 载荷；JSON 控制帧。
//! 字段语义对齐 WEB_0815 蓝本（h264_stream_config/preview_reconnect_required）。

/// 二进制帧头长度（字节）。
pub const FRAME_HEADER_LEN: usize = 32;
/// 帧头魔数（4 字节 "H264"，与 11B 心跳 ping 可区分）。
pub const FRAME_MAGIC: [u8; 4] = *b"H264";
/// flags 位：关键帧。
pub const FLAG_KEYFRAME: u32 = 1;

/// 帧头字段（pack/unpack 对偶）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct H264FrameHeader {
    /// bit0=关键帧
    pub flags: u32,
    /// 源配置代次（断流/重连/分辨率变化递增，旧代次数据不混入新流）
    pub generation: u32,
    /// PTS 时间基分母（RTSP H.264 固定 90kHz）
    pub time_base_den: u32,
    /// 源内单调递增序号
    pub seq: u64,
    /// 流时间基 PTS（原样透传）
    pub pts: i64,
}

impl H264FrameHeader {
    pub fn is_keyframe(&self) -> bool {
        self.flags & FLAG_KEYFRAME != 0
    }
}

/// 打包 32B 帧头：magic(4)+flags(4)+generation(4)+time_base_den(4)+seq(8)+pts(8)，小端。
pub fn pack_h264_header(header: &H264FrameHeader) -> [u8; FRAME_HEADER_LEN] {
    let mut out = [0u8; FRAME_HEADER_LEN];
    out[0..4].copy_from_slice(&FRAME_MAGIC);
    out[4..8].copy_from_slice(&header.flags.to_le_bytes());
    out[8..12].copy_from_slice(&header.generation.to_le_bytes());
    out[12..16].copy_from_slice(&header.time_base_den.to_le_bytes());
    out[16..24].copy_from_slice(&header.seq.to_le_bytes());
    out[24..32].copy_from_slice(&header.pts.to_le_bytes());
    out
}

/// 解包帧头（长度不足或魔数不符返回 None）。
pub fn unpack_h264_header(bytes: &[u8]) -> Option<H264FrameHeader> {
    if bytes.len() < FRAME_HEADER_LEN || bytes[0..4] != FRAME_MAGIC {
        return None;
    }
    Some(H264FrameHeader {
        flags: u32::from_le_bytes(bytes[4..8].try_into().ok()?),
        generation: u32::from_le_bytes(bytes[8..12].try_into().ok()?),
        time_base_den: u32::from_le_bytes(bytes[12..16].try_into().ok()?),
        seq: u64::from_le_bytes(bytes[16..24].try_into().ok()?),
        pts: i64::from_le_bytes(bytes[24..32].try_into().ok()?),
    })
}

/// 帧头+载荷打包为单个二进制帧。
pub fn pack_frame(header: &H264FrameHeader, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(FRAME_HEADER_LEN + payload.len());
    out.extend_from_slice(&pack_h264_header(header));
    out.extend_from_slice(payload);
    out
}

/// 单条检测叠层项：ROI 身份+命中标志+归一化扁平轮廓（批③，检测 tick 随流推送）。
#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DetectionOverlayItem {
    pub id: String,
    /// ROI 本 tick 是否命中（false 时前端不绘制）
    pub motion: bool,
    /// 轮廓点列 [x1,y1,x2,y2,...]（归一化 0..1，通常单条闭环）
    pub contours: Vec<Vec<f64>>,
}

/// JSON 控制帧（WS Text 消息；type 字段判别）。
/// 字段名保持 snake_case，批 2 前端按本结构对接。
#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum ControlFrame {
    /// 连接建立/配置变化时首发：解码器配置与源代次
    #[serde(rename = "h264_stream_config")]
    StreamConfig {
        codec: String,
        width: i32,
        height: i32,
        /// 探测失败为 null（可选字段）
        #[serde(skip_serializing_if = "Option::is_none")]
        fps: Option<f64>,
        decode_generation: u32,
    },
    /// 慢客户端重同步信令：服务端已丢帧，客户端收到后续二进制帧从关键帧续播
    #[serde(rename = "preview_reconnect_required")]
    PreviewReconnect {
        reason: String,
        decode_generation: u32,
    },
    /// 检测叠层帧（批③）：低频 JSON 控制帧随视频流推，画面与检测帧时序天然对齐
    #[serde(rename = "detection_overlay")]
    DetectionOverlay {
        detections: Vec<DetectionOverlayItem>,
        /// 对齐的视频帧序号
        source_frame_seq: u64,
        workstation: String,
        task: String,
    },
}

/// 慢客户端重同步原因（语言无关键）。
pub const RESYNC_SLOW_CLIENT: &str = "slow_client";
