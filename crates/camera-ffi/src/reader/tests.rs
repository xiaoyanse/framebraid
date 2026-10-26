use super::*;
use am_sdk_support::{abi::*, context::Context};

#[test]
#[ignore = "显式本轮回环RTSP/合成凭据和FFmpeg，仅验证SDK读包预算与回收"]
fn native_loopback_reader_reserves_before_copy_and_transfers_packet_ownership() {
    let url = std::env::var("AM_SDK_LOCAL_RTSP_URL").unwrap();
    assert!(url.starts_with("rtsp://127.0.0.1:") && !url.contains('@'));
    let secret = Secret {
        username: std::env::var("AM_SDK_LOCAL_RTSP_USER").unwrap(),
        password: std::env::var("AM_SDK_LOCAL_RTSP_PASSWORD").unwrap(),
    };
    let api = Arc::new(
        FfmpegApi::load_from(std::path::Path::new(
            &std::env::var("AM_RT_FFMPEG_DIR").unwrap(),
        ))
        .unwrap(),
    );
    let (domain, state) = crate::frames::isolated_test_state();
    let key = state.ctx_key;
    let open = || {
        BudgetedReader::open(
            state.clone(),
            api.clone(),
            &url,
            &secret,
            Arc::new(AtomicBool::new(false)),
            Duration::from_secs(5),
        )
        .unwrap()
    };
    let mut reader = open();
    let initial = state.usage_snapshot().unwrap();
    assert_eq!(initial.0, reader.reader.retained_buffer_bytes() as u64);
    let context = state.ctx.data::<Context>().unwrap();
    let old_limit = context.limits.lock().unwrap().frame_budget_bytes;
    context.limits.lock().unwrap().frame_budget_bytes = initial.0 + 1;
    assert!(matches!(reader.read_packet(), Err(IoError::Resource)));
    assert_eq!(state.usage_snapshot().unwrap(), initial);
    assert!(matches!(reader.read_packet(), Err(IoError::Closed)));
    drop(reader);
    assert_eq!(state.usage_snapshot().unwrap(), (0, 0, 0));
    context.limits.lock().unwrap().frame_budget_bytes = old_limit;
    let mut reader = open();
    let mut held: Option<(Packet, Charge)> = None;
    let mut payload_peak = 0;
    for _ in 0..25 {
        let previous_bytes = held.as_ref().map_or(0, |(_, charge)| charge.bytes());
        let (packet, charge) = reader.read_packet().unwrap().unwrap();
        assert!(packet.data.starts_with(&[0, 0, 0, 1]) || packet.data.starts_with(&[0, 0, 1]));
        assert_eq!(charge.bytes(), packet.data.capacity() as u64);
        assert_eq!(
            state.usage_snapshot().unwrap(),
            (
                reader.reader.retained_buffer_bytes() as u64 + charge.bytes() + previous_bytes,
                0,
                0
            )
        );
        payload_peak = payload_peak.max(charge.bytes());
        drop(held.take());
        held = Some((packet, charge));
    }
    let (packet, charge) = held.take().unwrap();
    let bytes = packet.data.clone();
    drop(reader);
    assert_eq!(state.usage_snapshot().unwrap(), (charge.bytes(), 0, 0));
    assert_eq!(packet.data, bytes);
    drop(packet);
    drop(charge);
    assert_eq!(state.usage_snapshot().unwrap(), (0, 0, 0));
    println!("reader-budget {{\"packets\":25,\"retained_initial_bytes\":{},\"payload_capacity_peak\":{payload_peak},\"after_drop_bytes\":0}}", initial.0);
    drop(state);
    assert_eq!(domain.close(key, 100).unwrap(), AM_LIFECYCLE_CLOSED as u32);
    domain.destroy(key).unwrap();
}
