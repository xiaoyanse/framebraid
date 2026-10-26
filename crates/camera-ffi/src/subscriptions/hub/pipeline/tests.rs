use super::*;
use am_sdk_support::{context::Context, handles::Managed};

fn job(source: &SharedSource, config: &Arc<Configuration>, seq: u64, key: bool) -> Job {
    Job {
        packet: Arc::new(queue::Payload {
            abi: am_encoded_packet_t {
                codec: AM_CODEC_H264,
                flags: if key { AM_PKT_KEYFRAME } else { 0 },
                seq,
                source_generation: 1,
                config_generation: config.generation,
                ..Default::default()
            },
            bytes: vec![0; 32],
            _charge: source.state.charge(32, false).unwrap(),
        }),
        config: config.clone(),
        received: seq as i64,
        epoch: 0,
    }
}
fn config(source: &SharedSource) -> Arc<Configuration> {
    Arc::new(Configuration {
        stream: Arc::new(Stream {
            info: StreamInfo {
                codec: AM_CODEC_H264,
                width: 64,
                height: 64,
                fps: 30.,
                time_base_num: 1,
                time_base_den: 30,
                extradata: vec![],
            },
            _charge: source.state.charge(0, false).unwrap(),
        }),
        extra: vec![],
        generation: 1,
        _charge: source.state.charge(0, false).unwrap(),
    })
}
#[test]
fn bounded_slow_pixel_work_does_not_block_compressed_and_invalidates_inflight_epoch() {
    let (domain, source, hub, _) = super::super::tests::fixture();
    let context = source.state.ctx.data::<Context>().unwrap();
    {
        let mut policies = context.queues.lock().unwrap();
        policies[AM_Q_ENCODED_STREAM as usize].max_items = 2;
        policies[AM_Q_ENCODED_STREAM as usize].max_bytes = 64;
    }
    let config = config(&source);
    let pipe = Pipeline::new(source.state.clone()).unwrap();
    let encoded = Subscription::new(
        source.state.clone(),
        &hub,
        Kind::Encoded {
            mask: AM_CODEC_H264,
        },
        AM_SUB_PULL,
        0,
        0,
    )
    .unwrap();
    pipe.submit(job(&source, &config, 1, true)).unwrap();
    let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
    std::thread::scope(|scope| {
        let processing = pipe.clone();
        let worker = scope.spawn(move || {
            let in_flight = processing.pop().unwrap().unwrap();
            entered_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            assert!(
                !processing.current(in_flight.epoch),
                "过载前在读帧不可迟到发布"
            );
            let mut recovered = Vec::new();
            while let Some(item) = processing.pop().unwrap() {
                recovered.push(item.packet.abi.seq);
            }
            recovered
        });
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let started = Instant::now();
        for n in 2..=101 {
            let next = job(&source, &config, n, n == 2 || n % 3 == 0);
            encoded.enqueue(Item::Encoded(next.packet.clone()));
            assert!(matches!(encoded.pull(0), Ok(Item::Encoded(p)) if p.abi.seq == n));
            pipe.submit(next).unwrap();
            let data = pipe.data.lock().unwrap();
            assert!(data.queue.len() <= 2 && data.bytes <= 64);
        }
        println!("pixel-isolation {{\"compressed_delivered\":100,\"pixel_worker_blocked\":true,\"producer_us\":{},\"queue_items_limit\":2,\"queue_bytes_limit\":64,\"scope\":\"production queues; controlled barrier, not network FPS\"}}", started.elapsed().as_micros());
        // 最后一批旧参考帧清掉，只留下可重同步的最新随机访问点。
        {
            let mut data = pipe.data.lock().unwrap();
            pipe.invalidate(&mut data).unwrap();
        }
        pipe.submit(job(&source, &config, 102, true)).unwrap();
        release_tx.send(()).unwrap();
        // 不靠睡眠猜已处理，通过队列计数等待实际出队。
        let until = Instant::now() + Duration::from_secs(2);
        while source
            .state
            .metrics
            .decoder_queue_items
            .load(Ordering::Acquire)
            != 0
        {
            assert!(Instant::now() < until);
            std::thread::yield_now();
        }
        pipe.stop();
        assert_eq!(worker.join().unwrap(), vec![102]);
    });
    assert_eq!(
        source
            .state
            .metrics
            .decoder_queue_items
            .load(Ordering::Acquire),
        0
    );
    assert_eq!(
        source
            .state
            .metrics
            .decoder_queue_bytes
            .load(Ordering::Acquire),
        0
    );
    assert!(source.state.metrics.dropped.load(Ordering::Acquire) > 0);
    assert!(source.state.metrics.resyncs.load(Ordering::Acquire) > 0);
    encoded.request_close();
    drop((encoded, config, pipe, hub));
    assert_eq!(source.state.usage_snapshot().unwrap(), (0, 0, 0));
    let key = source.state.ctx_key;
    drop(source);
    domain.close(key, 100).unwrap();
    domain.destroy(key).unwrap();
}

#[test]
fn stopped_pipeline_wakes_waiter_and_repeated_close_does_not_underflow_counts() {
    let (domain, source, hub, _) = super::super::tests::fixture();
    for _ in 0..100 {
        let pipe = Pipeline::new(source.state.clone()).unwrap();
        std::thread::scope(|scope| {
            let waiting = pipe.clone();
            let reader = scope.spawn(move || waiting.pop().unwrap().is_none());
            pipe.stop();
            pipe.stop();
            assert!(reader.join().unwrap());
        });
        assert_eq!(
            source
                .state
                .metrics
                .decoder_queue_items
                .load(Ordering::Acquire),
            0
        );
    }
    drop(hub);
    assert_eq!(source.state.usage_snapshot().unwrap(), (0, 0, 0));
    let key = source.state.ctx_key;
    drop(source);
    domain.close(key, 100).unwrap();
    domain.destroy(key).unwrap();
}

#[test]
fn blocked_pixel_execution_keeps_decoder_permit_and_does_not_spawn_replacement() {
    let (domain, source, mut hub, _) = super::super::tests::fixture();
    source
        .state
        .ctx
        .data::<Context>()
        .unwrap()
        .limits
        .lock()
        .unwrap()
        .max_decoders = 1;
    let block = Arc::new(Block::default());
    Arc::get_mut(&mut hub).unwrap().pixel_block = Some(block.clone());
    let config = config(&source);
    let stamp = SessionStamp {
        revision: 0,
        generation: 1,
        credentials: source.credential_version(),
    };
    for _ in 0..20 {
        block.entered.store(false, Ordering::Release);
        *block.released.lock().unwrap() = false;
        std::thread::scope(|scope| {
            let mut execution = Execution::new(scope, &hub, &source, stamp);
            execution.submit(job(&source, &config, 1, true)).unwrap();
            let until = Instant::now() + Duration::from_secs(2);
            while !block.entered.load(Ordering::Acquire) {
                assert!(Instant::now() < until);
                std::thread::yield_now();
            }
            execution.stop();
            assert!(!execution.handle.as_ref().unwrap().is_finished());
            assert!(matches!(
                source.state.decoder(),
                Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
            ));
            assert_eq!(
                execution.submit(job(&source, &config, 2, true)),
                Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
            );
            *block.released.lock().unwrap() = true;
            block.wake.notify_all();
        });
        // Scope的线程排空屏障之后才允许重新领取额度。
        drop(source.state.decoder().unwrap());
        assert_eq!(source.state.usage_snapshot().unwrap(), (0, 0, 0));
    }
    drop((config, hub));
    let key = source.state.ctx_key;
    drop(source);
    domain.close(key, 100).unwrap();
    domain.destroy(key).unwrap();
}
