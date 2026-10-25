//! 搬运会话单测（批 1.6，真装配）：GOP 缓存边界/慢客户端重同步/本地 H.264 源
//! 全链路（打开→探测→包分发→订阅）与 EOF 重开代次递增。
//! probe_source.mp4 为 MP4/AVCC 源，同时锁定 AVCC→Annex B 转换与关键帧参数集补齐。
use super::annexb::{self, NAL_IDR, NAL_PPS, NAL_SLICE, NAL_SPS};
use super::fanout::{ClientFan, GopCache, CLIENT_QUEUE_MAX};
use super::protocol;
use super::session::{DenyReason, H264Hub, H264Message, H264Packet, GOP_CACHE_MAX_BYTES};
use crate::media::testkit::wait_until;
use std::sync::Arc;
use std::time::Duration;

/// 测试包构造：非关键帧默认 16B。
fn packet(seq: u64, keyframe: bool) -> Arc<H264Packet> {
    let mut data = Vec::new();
    data.extend_from_slice(&annexb::START_CODE4);
    data.push(if keyframe { NAL_IDR } else { NAL_SLICE });
    data.extend(std::iter::repeat_n(0x55, 11));
    Arc::new(H264Packet {
        seq,
        pts: seq as i64 * 3600,
        keyframe,
        generation: 1,
        data: data.into(),
    })
}

/// GOP 缓存：无关键帧不缓存；关键帧开组；字节超限整组弃置。
#[test]
fn gop_cache_bounds_and_keyframe_start() {
    let mut gop = GopCache::new(64);
    gop.push(&packet(1, false));
    assert!(gop.snapshot().is_empty(), "无关键帧起点不缓存");
    gop.push(&packet(2, true));
    gop.push(&packet(3, false));
    assert_eq!(gop.snapshot().len(), 2);
    // 关键帧开新组
    gop.push(&packet(4, true));
    assert_eq!(gop.snapshot().len(), 1);
    // 超字节上限（每包 16B，上限 64B）：整组弃置
    for seq in 5..12 {
        gop.push(&packet(seq, false));
    }
    assert!(gop.snapshot().is_empty(), "超上限应弃置整组等下个关键帧");
}

/// 慢客户端：队列满丢旧+重同步信令一次+非关键帧丢弃+关键帧续发恢复。
#[test]
fn slow_client_overflow_resync_then_keyframe_resume() {
    let fan = Arc::new(ClientFan::new());
    // 填满队列（初始不在等待关键帧状态）
    for seq in 1..=CLIENT_QUEUE_MAX as u64 {
        fan.deliver(H264Message::Packet(packet(seq, seq == 1)));
    }
    // 溢出一帧非关键包：丢最旧+信令一次+转入等关键帧
    fan.deliver(H264Message::Packet(packet(257, false)));
    {
        let queue = fan.queue.lock_for_test();
        let resync_count = queue
            .iter()
            .filter(|m| matches!(m, H264Message::ResyncRequired { .. }))
            .count();
        assert_eq!(resync_count, 1, "重同步信令应恰一次");
        assert!(
            !queue
                .iter()
                .any(|m| matches!(m, H264Message::Packet(p) if p.seq == 1)),
            "溢出应丢最旧（seq=1 被移除）"
        );
        assert!(
            queue
                .iter()
                .any(|m| matches!(m, H264Message::Packet(p) if p.seq == 2)),
            "仅最旧一帧被移除（seq=2 保留）"
        );
        assert!(!queue
            .iter()
            .any(|m| matches!(m, H264Message::Packet(p) if p.seq == 257)));
    }
    // 等待关键帧期间非关键包直接丢弃
    fan.deliver(H264Message::Packet(packet(258, false)));
    assert!(!fan
        .queue
        .lock_for_test()
        .iter()
        .any(|m| matches!(m, H264Message::Packet(p) if p.seq == 258)));
    // 关键帧到达：入队续发，状态清除
    fan.deliver(H264Message::Packet(packet(259, true)));
    {
        let queue = fan.queue.lock_for_test();
        assert!(queue
            .iter()
            .any(|m| matches!(m, H264Message::Packet(p) if p.seq == 259)));
    }
    // 再次溢出会再发信令（去重标记已随关键帧清除）
    let overflow_base = 300u64;
    for seq in overflow_base..overflow_base + CLIENT_QUEUE_MAX as u64 + 2 {
        fan.deliver(H264Message::Packet(packet(seq, false)));
    }
    assert_eq!(
        fan.queue
            .lock_for_test()
            .iter()
            .filter(|m| matches!(m, H264Message::ResyncRequired { .. }))
            .count(),
        2,
        "第二轮溢出应再次信令"
    );
}

/// 本地 MP4（AVCC）源真装配：订阅→配置帧→Annex B 包流（关键帧带 SPS/PPS、
/// 序号单调、PTS 递增、访问单元边界=一包）。
#[tokio::test]
async fn hub_streams_local_h264_file_with_parameter_sets() {
    let sample = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/media/testdata/probe_source.mp4");
    if !sample.is_file() {
        eprintln!("[搬运] 本地样本缺失，跳过");
        return;
    }
    let path = sample.to_string_lossy().into_owned();
    let hub = H264Hub::new(Arc::new(move |_| Some(path.clone())), GOP_CACHE_MAX_BYTES);
    let url = "rtsp://relay.test/main";
    assert!(
        matches!(hub.subscribe(url), Err(DenyReason::NotReady)),
        "首订应未就绪（会话刚拉起），探测与配置在异步线程"
    );
    assert!(
        wait_until(Duration::from_secs(10), || hub.is_ready(url)),
        "会话应在超时内就绪（含 FFmpeg 打开+探测）"
    );
    let subscription = hub.subscribe(url).expect("就绪后订阅应成功");
    // 首个消息必为配置帧
    match subscription.recv().await.expect("应收到配置帧") {
        H264Message::Config(config) => {
            assert_eq!(config.generation, 1, "首开代次为 1");
            assert_eq!(config.width, 640);
            assert_eq!(config.height, 360);
            assert_eq!(config.fps, Some(25.0));
        }
        other => panic!("首个消息应为配置帧: {other:?}"),
    }
    // 包流：Annex B 化（AVCC 源）+关键帧参数集补齐+序号单调+PTS 透传（含
    // B 帧流解码序 PTS 往返，前端解码器负责重排）+头打包往返。文件循环重开
    // （EOF→退避→代次递增）属预期，Config 广播跳过。
    let mut last_seq = 0u64;
    let mut seen_keyframe_with_sps = false;
    let mut examined = 0usize;
    let wall_deadline = std::time::Instant::now() + Duration::from_secs(60);
    while examined < 25 {
        assert!(
            std::time::Instant::now() < wall_deadline,
            "60s 内未收齐 25 包（当前 {examined}）"
        );
        let message = tokio::time::timeout(Duration::from_secs(30), subscription.recv())
            .await
            .expect("recv 不应超过 30s")
            .expect("订阅应存活");
        let H264Message::Packet(p) = message else {
            continue;
        };
        examined += 1;
        assert!(annexb::starts_with_start_code(&p.data), "应为 Annex B");
        assert!(p.seq > last_seq, "序号单调（跨代次持续递增）");
        assert!(p.pts != i64::MIN, "PTS 不应为 NOPTS");
        last_seq = p.seq;
        if p.keyframe {
            assert!(
                annexb::contains_nal_type(&p.data, NAL_SPS)
                    && annexb::contains_nal_type(&p.data, NAL_PPS),
                "关键帧应携带参数集（avcC 全局头补齐）"
            );
            seen_keyframe_with_sps = true;
        }
        // 帧头协议对偶（端点将按此打包）
        let header = protocol::H264FrameHeader {
            flags: if p.keyframe {
                protocol::FLAG_KEYFRAME
            } else {
                0
            },
            generation: p.generation,
            time_base_den: super::session::TIME_BASE_DEN,
            seq: p.seq,
            pts: p.pts,
        };
        let frame = protocol::pack_frame(&header, &p.data);
        let unpacked = protocol::unpack_h264_header(&frame).expect("帧头解包");
        assert_eq!(unpacked.seq, p.seq);
        assert_eq!(unpacked.is_keyframe(), p.keyframe);
        assert_eq!(&frame[protocol::FRAME_HEADER_LEN..], &p.data[..]);
    }
    assert!(seen_keyframe_with_sps, "样本内应有关键帧");
}

/// EOF 重开：文件播完退避重连，成功再开递增源代次并广播新配置。
#[tokio::test]
async fn generation_increments_after_eof_reopen() {
    let sample = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/media/testdata/probe_source.mp4");
    if !sample.is_file() {
        eprintln!("[搬运] 本地样本缺失，跳过");
        return;
    }
    let path = sample.to_string_lossy().into_owned();
    let hub = H264Hub::new(Arc::new(move |_| Some(path.clone())), GOP_CACHE_MAX_BYTES);
    let url = "rtsp://relay.test/reopen";
    assert!(matches!(hub.subscribe(url), Err(DenyReason::NotReady)));
    assert!(wait_until(Duration::from_secs(10), || hub.is_ready(url)));
    let subscription = hub.subscribe(url).expect("订阅应成功");
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    let mut max_generation = 0u32;
    loop {
        if std::time::Instant::now() > deadline {
            panic!("EOF 重开应在期限内递增代次（当前 max={max_generation}）");
        }
        let message = tokio::time::timeout(Duration::from_secs(5), subscription.recv())
            .await
            .expect("recv 不应超过 5s")
            .expect("订阅应存活");
        match message {
            H264Message::Config(config) => {
                max_generation = max_generation.max(config.generation);
                if config.generation >= 2 {
                    break; // 旧代次数据不混入：配置先于新代次包
                }
            }
            H264Message::Packet(p) => {
                assert_eq!(p.generation, 1, "代次 2 配置前不应出现旧代次混流");
            }
            other => panic!("不应收到: {other:?}"),
        }
    }
    drop(subscription);
}

/// 检测叠层注入（批③）：无会话即丢——不得为 overlay 建会话/拉流/积压。
#[test]
fn overlay_publish_without_session_drops_silently() {
    let hub = H264Hub::new(Arc::new(|_| None), GOP_CACHE_MAX_BYTES);
    let url = "rtsp://overlay.test/none";
    hub.publish_overlay(url, Arc::from(r#"{"type":"detection_overlay"}"#));
    assert!(
        !hub.is_ready(url),
        "无订阅者路径不得建会话（否则 overlay 会拉起无人观看的流）"
    );
}

/// 检测叠层注入（批③）：多客户端同享同帧；队列满客户端丢 overlay 不挤占视频帧。
#[test]
fn overlay_publish_fans_out_and_drops_on_full_queue() {
    let hub = H264Hub::new(Arc::new(|_| None), GOP_CACHE_MAX_BYTES);
    let url = "rtsp://overlay.test/main";
    let queues = hub.inject_clients(url, 2);
    // 客户端 2 队列先填满（模拟慢消费者）
    {
        let mut full = queues[1].lock_for_test();
        for seq in 1..=CLIENT_QUEUE_MAX as u64 {
            full.push_back(H264Message::Packet(packet(seq, false)));
        }
    }
    let json: Arc<str> = Arc::from(r#"{"type":"detection_overlay","detections":[]}"#);
    hub.publish_overlay(url, json.clone());
    // 客户端 1：收到同一条 overlay 帧
    assert!(
        queues[0]
            .lock_for_test()
            .iter()
            .any(|m| matches!(m, H264Message::Overlay(s) if s.as_ref() == json.as_ref())),
        "空闲客户端应收 overlay"
    );
    // 客户端 2：队列满即丢，长度不变（未挤掉视频帧）
    let second = queues[1].lock_for_test();
    assert_eq!(second.len(), CLIENT_QUEUE_MAX, "满队列长度不得变化");
    assert!(
        !second.iter().any(|m| matches!(m, H264Message::Overlay(_))),
        "满队列应丢 overlay 不丢视频帧"
    );
}
