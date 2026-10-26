use super::*;

#[test]
fn timestamp_storage_stays_within_the_reserved_bytes_after_many_rollovers() {
    let (_domain, source, hub, _) = super::super::super::tests::fixture();
    let before = source.state.usage_snapshot().unwrap().0;
    let stamp = SessionStamp {
        revision: 0,
        generation: 0,
        credentials: 0,
    };
    let permit = Arc::new(source.state.decoder().unwrap());
    let mut worker = Worker::new(&hub, &source, stamp, permit).unwrap();
    for pts in 0..10_000 {
        Worker::remember(&mut worker.times, pts, 20_000 + pts);
    }
    let reserved = source.state.usage_snapshot().unwrap().0 - before;
    let bytes = worker.times.capacity() * std::mem::size_of::<(i64, i64)>();
    println!(
        "rtsp-times len={} capacity={} allocated={bytes} reserved={reserved}",
        worker.times.len(),
        worker.times.capacity()
    );
    assert_eq!(worker.times.len(), 128);
    assert_eq!(worker.times.front(), Some(&(9872, 29872)));
    assert_eq!(worker.times.back(), Some(&(9999, 29999)));
    assert!(bytes as u64 <= reserved);
    drop(worker);
    assert_eq!(source.state.usage_snapshot().unwrap().0, before);
}
