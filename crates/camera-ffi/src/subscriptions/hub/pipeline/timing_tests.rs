use super::*;

#[test]
#[ignore = "本机真实等待计时；固定样本记录超时粒度，不作跨平台时延保证"]
fn quantify_sampling_wait_overshoot_without_global_timer_changes() {
    let (domain, source, hub, _) = super::super::tests::fixture();
    let pipe = Pipeline::new(source.state.clone()).unwrap();
    for micros in [1000, 5000, 20000] {
        let mut old = Vec::new();
        let mut current = Vec::new();
        for round in 0..12 {
            for sliced in if round % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                let start = Instant::now();
                let deadline = start + Duration::from_micros(micros);
                if sliced {
                    assert!(pipe.wait_until(1, deadline).unwrap());
                } else {
                    let mutex = Mutex::new(());
                    let wake = Condvar::new();
                    let mut guard = mutex.lock().unwrap();
                    loop {
                        let now = Instant::now();
                        if now >= deadline {
                            break;
                        }
                        guard = wake.wait_timeout(guard, deadline - now).unwrap().0;
                    }
                }
                let elapsed = start.elapsed().as_micros();
                assert!(elapsed >= micros as u128);
                if sliced {
                    current.push(elapsed);
                } else {
                    old.push(elapsed);
                }
            }
        }
        println!("sampling-wait {{\"requested_us\":{micros},\"condvar_us\":{old:?},\"sliced_sleep_us\":{current:?},\"scope\":\"same process actual Windows timing; no busy spin or global settings\"}}");
    }
    pipe.stop();
    drop((pipe, hub));
    assert_eq!(source.state.usage_snapshot().unwrap(), (0, 0, 0));
    let key = source.state.ctx_key;
    drop(source);
    domain.close(key, 100).unwrap();
    domain.destroy(key).unwrap();
}
