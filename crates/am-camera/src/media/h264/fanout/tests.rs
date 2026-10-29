use super::*;
use std::time::Duration;

const EXPECTED_BYTES: usize = 8 * 1024 * 1024;

fn config() -> Arc<StreamConfig> {
    Arc::new(StreamConfig {
        width: 1920,
        height: 1080,
        fps: Some(30.0),
        generation: 1,
    })
}
fn packet(seq: u64, bytes: usize) -> Arc<H264Packet> {
    Arc::new(H264Packet {
        seq,
        pts: seq as i64,
        generation: 1,
        keyframe: true,
        data: vec![seq as u8; bytes].into(),
    })
}
fn payload(queue: &VecDeque<H264Message>) -> usize {
    queue
        .iter()
        .map(|m| match m {
            H264Message::Packet(p) => p.data.len(),
            H264Message::Overlay(s) | H264Message::Closed(_, s) => s.len(),
            _ => 0,
        })
        .sum()
}

#[test]
fn byte_pressure_never_exceeds_payload_ceiling() {
    let fan = ClientFan::new();
    let mut peak = 0;
    for seq in 1..=6 {
        fan.deliver(H264Message::Packet(packet(seq, 2 * 1024 * 1024)));
        peak = peak.max(payload(&fan.queue.lock()));
    }
    println!("R3_FANOUT payload_peak={peak} max={EXPECTED_BYTES}");
    assert!(peak <= EXPECTED_BYTES);
}

#[test]
fn seed_includes_configuration_in_item_budget() {
    let fan = ClientFan::new();
    fan.seed(
        &config(),
        (0..CLIENT_QUEUE_MAX)
            .map(|i| packet(i as u64, 16))
            .collect(),
    );
    let queue = fan.queue.lock();
    println!(
        "R3_FANOUT seed_items={} max={CLIENT_QUEUE_MAX}",
        queue.len()
    );
    assert!(queue.len() <= CLIENT_QUEUE_MAX);
    assert!(matches!(queue.front(), Some(H264Message::Config(_))));
}

#[test]
fn gop_item_budget_rejects_zero_payload_growth() {
    let mut gop = GopCache::new(4 * 1024 * 1024);
    for seq in 0..1000 {
        let mut item = packet(seq, 0);
        Arc::get_mut(&mut item).unwrap().keyframe = seq == 0;
        gop.push(&item);
    }
    let count = gop.snapshot().len();
    println!(
        "R3_FANOUT empty_gop_items={count} max={}",
        CLIENT_QUEUE_MAX - 1
    );
    assert!(count < CLIENT_QUEUE_MAX);
}

#[tokio::test]
async fn closed_is_single_terminal_and_late_producers_cannot_revive_it() {
    let fan = ClientFan::new();
    fan.deliver(H264Message::Packet(packet(1, 16)));
    for _ in 0..100 {
        fan.queue
            .close(H264Message::Closed(1008, Arc::from("closed")));
    }
    fan.deliver_config(config());
    fan.deliver_overlay(Arc::from("late"));
    fan.deliver(H264Message::Packet(packet(2, 16)));
    let count = fan.queue.lock().len();
    println!("R3_FANOUT post_close_items={count} expected=1");
    assert_eq!(count, 1);
    assert!(matches!(
        fan.queue.recv().await,
        Some(H264Message::Closed(1008, _))
    ));
    assert!(fan.queue.recv().await.is_none());
}

#[tokio::test]
async fn close_between_empty_check_and_sleep_is_not_lost() {
    let queue = Arc::new(ClientQueue::new());
    let weak = Arc::downgrade(&queue);
    *queue.before_wait.lock().unwrap() = Some(Box::new(move || {
        weak.upgrade()
            .unwrap()
            .close(H264Message::Closed(1008, Arc::from("closed")));
    }));
    let value = tokio::time::timeout(Duration::from_millis(100), queue.recv()).await;
    assert!(matches!(value, Ok(Some(H264Message::Closed(1008, _)))));
    assert!(queue.recv().await.is_none());
}

#[test]
fn full_valid_seed_and_tiny_gop_both_respect_item_budget() {
    let mut gop = GopCache::new(4 * 1024 * 1024);
    for seq in 0..35 {
        let mut item = packet(seq, 1);
        Arc::get_mut(&mut item).unwrap().keyframe = seq == 0;
        gop.push(&item);
    }
    assert_eq!(gop.snapshot().len(), 35);
    let fan = ClientFan::new();
    fan.seed(&config(), gop.snapshot());
    assert_eq!(fan.queue.lock().len(), 1);
    let mut fitting = gop.snapshot();
    fitting.pop();
    fan.seed(&config(), fitting);
    assert_eq!(fan.queue.lock().len(), CLIENT_QUEUE_MAX - 1);
    let mut overflow = packet(36, 1);
    Arc::get_mut(&mut overflow).unwrap().keyframe = false;
    gop.push(&overflow);
    assert!(gop.snapshot().is_empty());
}

#[tokio::test]
async fn slow_subscriber_stops_without_starving_fast_subscriber_or_old_owner() {
    let (slow, fast) = (ClientFan::new(), ClientFan::new());
    for fan in [&slow, &fast] {
        fan.seed(&config(), Vec::new());
    }
    assert!(matches!(
        fast.queue.recv().await,
        Some(H264Message::Config(_))
    ));
    let mut held = None;
    for seq in 1..=200 {
        let mut p = packet(seq, 16);
        Arc::get_mut(&mut p).unwrap().keyframe = seq % 10 == 1;
        slow.deliver(H264Message::Packet(p.clone()));
        fast.deliver(H264Message::Packet(p));
        let next = tokio::time::timeout(Duration::from_millis(100), fast.queue.recv())
            .await
            .unwrap();
        let Some(H264Message::Packet(p)) = next else {
            panic!("健康订阅不应收到重同步或终态")
        };
        assert_eq!(p.seq, seq);
        if seq == 1 {
            held = Some(p);
        }
    }
    assert_eq!(held.unwrap().data.as_ref(), &[1; 16]);
    assert_eq!(slow.queue.lock().len(), 1);
    assert!(matches!(
        slow.queue.recv().await,
        Some(H264Message::Closed(1011, _))
    ));
    assert!(slow.queue.recv().await.is_none());
    println!("R3_FANOUT healthy_delivery=200/200 slow_terminal=1 held_frame_unchanged=true");
}

#[test]
fn overlays_are_latest_only_and_stay_within_reserved_budget() {
    let fan = ClientFan::new();
    fan.deliver(H264Message::Packet(packet(
        1,
        EXPECTED_BYTES - OVERLAY_MAX_BYTES,
    )));
    for _ in 0..100 {
        fan.deliver_overlay(Arc::from("xy"));
    }
    assert_eq!(fan.queue.lock().len(), 2);
    let q = fan.queue.lock();
    assert!(payload(&q) <= EXPECTED_BYTES);
    assert_eq!(q.len(), 2);
    assert!(q.iter().any(|m| matches!(m, H264Message::Overlay(_))));
}

#[tokio::test]
async fn full_video_queue_delivers_latest_overlay_without_starving_video() {
    let fan = ClientFan::new();
    for seq in 0..CLIENT_QUEUE_MAX - 1 {
        fan.deliver(H264Message::Packet(packet(seq as u64, 1)));
    }
    fan.deliver_overlay(Arc::from("old"));
    fan.deliver_overlay(Arc::from("new"));
    assert!(matches!(fan.queue.recv().await, Some(H264Message::Overlay(s)) if s.as_ref() == "new"));
    fan.deliver_overlay(Arc::from("newer"));
    assert!(matches!(
        fan.queue.recv().await,
        Some(H264Message::Packet(_))
    ));
    assert!(
        matches!(fan.queue.recv().await, Some(H264Message::Overlay(s)) if s.as_ref() == "newer")
    );
    assert!(fan.queue.lock().len() <= CLIENT_QUEUE_MAX);
}

#[tokio::test]
async fn oversized_overlay_and_generation_changes_are_bounded() {
    let fan = ClientFan::new();
    fan.deliver_overlay(Arc::from("x".repeat(OVERLAY_MAX_BYTES)));
    assert!(
        matches!(fan.queue.recv().await, Some(H264Message::Overlay(s)) if s.len() == OVERLAY_MAX_BYTES)
    );
    fan.deliver_overlay(Arc::from("x".repeat(OVERLAY_MAX_BYTES + 1)));
    assert!(fan.queue.lock().is_empty());
    fan.deliver_overlay(Arc::from("old"));
    fan.deliver_config(config());
    fan.deliver_overlay(Arc::from("new"));
    assert!(matches!(
        fan.queue.recv().await,
        Some(H264Message::Config(_))
    ));
    assert!(matches!(fan.queue.recv().await, Some(H264Message::Overlay(s)) if s.as_ref() == "new"));
    fan.queue
        .close(H264Message::Closed(1000, Arc::from("closed")));
    fan.deliver_overlay(Arc::from("late"));
    assert!(matches!(
        fan.queue.recv().await,
        Some(H264Message::Closed(_, _))
    ));
}

#[tokio::test]
async fn resync_clears_old_overlay_and_precedes_new_overlay() {
    let fan = ClientFan::new();
    fan.deliver_overlay(Arc::from("old"));
    for seq in 0..CLIENT_QUEUE_MAX {
        let mut frame = packet(seq as u64, 1);
        Arc::get_mut(&mut frame).unwrap().keyframe = false;
        fan.deliver(H264Message::Packet(frame));
    }
    fan.deliver_overlay(Arc::from("new"));
    assert!(matches!(
        fan.queue.recv().await,
        Some(H264Message::ResyncRequired { generation: 1, .. })
    ));
    assert!(matches!(fan.queue.recv().await, Some(H264Message::Overlay(s)) if s.as_ref() == "new"));
}

#[tokio::test]
async fn oversized_packet_closes_only_once_with_budget_reason() {
    let fan = ClientFan::new();
    fan.deliver(H264Message::Packet(packet(1, EXPECTED_BYTES + 1)));
    fan.deliver(H264Message::Packet(packet(2, 16)));
    assert_eq!(fan.queue.lock().len(), 1);
    assert!(
        matches!(fan.queue.recv().await, Some(H264Message::Closed(1009, reason)) if reason.as_ref() == "media.packetTooLarge")
    );
    assert!(fan.queue.recv().await.is_none());
}

#[tokio::test]
async fn close_broadcast_wakes_all_receivers_with_exactly_one_terminal() {
    let queue = Arc::new(ClientQueue::new());
    let first = queue.clone();
    let second = queue.clone();
    let first = tokio::spawn(async move { first.recv().await });
    let second = tokio::spawn(async move { second.recv().await });
    tokio::task::yield_now().await;
    queue.close(H264Message::Closed(1008, Arc::from("closed")));
    let values = tokio::time::timeout(Duration::from_millis(100), async {
        [first.await.unwrap(), second.await.unwrap()]
    })
    .await
    .unwrap();
    assert_eq!(
        values
            .iter()
            .filter(|x| matches!(x, Some(H264Message::Closed(1008, _))))
            .count(),
        1
    );
    assert_eq!(values.iter().filter(|x| x.is_none()).count(), 1);
}

#[test]
fn concurrent_publish_close_and_reconfiguration_never_revive_terminal() {
    let fan = Arc::new(ClientFan::new());
    let start = Arc::new(std::sync::Barrier::new(8));
    let mut workers = Vec::new();
    for index in 0..8 {
        let (fan, start) = (fan.clone(), start.clone());
        workers.push(std::thread::spawn(move || {
            start.wait();
            for seq in 0..100 {
                match index % 4 {
                    0 => fan.deliver(H264Message::Packet(packet(seq, 64))),
                    1 => fan.deliver_config(config()),
                    2 => fan.deliver_overlay(Arc::from("overlay")),
                    _ => fan
                        .queue
                        .close(H264Message::Closed(1008, Arc::from("closed"))),
                }
            }
        }));
    }
    for thread in workers {
        thread.join().unwrap();
    }
    let q = fan.queue.lock();
    assert_eq!(q.len(), 1);
    assert!(matches!(q.front(), Some(H264Message::Closed(_, _))));
    assert!(payload(&q) <= EXPECTED_BYTES);
}
