//! 只在像素worker里短暂等deadline；最终due仍按严格的实际发布时间判定。
use super::*;

pub(super) fn superseded(subscribers: &Snapshot, delivery: &pipeline::Delivery<'_>) -> bool {
    let mut wanted = subscribers
        .iter()
        .filter(|sub| sub.wants_frames())
        .peekable();
    wanted.peek().is_some()
        && wanted.all(|sub| matches!(sub.kind, Kind::Frames { fps, .. } if fps != 0))
        && delivery.pipe.has_newer_backlog(delivery.epoch)
}

fn next(subscribers: &Snapshot, now: Instant, allow_wait: bool) -> Option<Instant> {
    subscribers
        .iter()
        .filter_map(|sub| {
            let Kind::Frames { fps, .. } = sub.kind else {
                return None;
            };
            let at = sub.next_pixels_at(now)?;
            let window = if !allow_wait || fps == 0 {
                Duration::ZERO
            } else {
                Duration::from_nanos(1_000_000_000u64.div_ceil(fps as u64))
                    .min(Duration::from_millis(50))
            };
            if at.saturating_duration_since(now) > window {
                return None;
            }
            // 较慢到期不能抢走较快订阅即将到期的同一帧；同速在窗口内对齐到较晚时刻。
            let rank = if fps == 0 { u64::MAX } else { fps as u64 };
            Some((rank, at))
        })
        .max()
        .map(|(_, at)| at)
}
pub(super) fn ready_soon(subscribers: &Snapshot, now: Instant, allow_wait: bool) -> bool {
    next(subscribers, now, allow_wait).is_some()
}
pub(super) fn wait_due(
    subscribers: &Snapshot,
    delivery: &pipeline::Delivery<'_>,
) -> std::result::Result<bool, IoError> {
    let Some(at) = next(subscribers, Instant::now(), true) else {
        return Ok(false);
    };
    delivery
        .pipe
        .wait_until(delivery.epoch, at)
        .map_err(|_| IoError::Resource)
}

#[cfg(test)]
mod tests {
    use super::*;
    use am_sdk_support::handles::Managed;
    #[test]
    fn deadline_queries_preserve_strict_boundaries_and_never_spend_an_early_opportunity() {
        let (domain, source, hub, _) = super::super::tests::fixture();
        let now = hub.now();
        for fps in [1, 5, 10, 30, 60, 1_000_000_001, u32::MAX] {
            let sub = super::super::tests::sub(&source, &hub, fps, AM_SUB_PULL);
            assert_eq!(sub.next_pixels_at(now), Some(now));
            assert!(sub.due(now));
            let gap = Duration::from_nanos(1_000_000_000u64.div_ceil(fps as u64));
            let before = now + gap - Duration::from_nanos(1);
            assert!(!sub.due(before));
            assert_eq!(sub.next_pixels_at(before), Some(now + gap));
            assert!(ready_soon(
                &Snapshot::from_iter([sub.clone()]),
                before,
                true
            ));
            assert!(!sub.needs_pixels(before));
            assert!(sub.due(now + gap));
            assert!(!sub.due(now + gap));
            sub.reset();
            assert!(sub.needs_pixels(now + gap));
            sub.request_close();
            assert_eq!(sub.next_pixels_at(now), None);
        }
        let unlimited = super::super::tests::sub(&source, &hub, 0, AM_SUB_PULL);
        assert!(unlimited.due(now));
        assert!(unlimited.due(now));
        assert_eq!(unlimited.next_pixels_at(now), Some(now));
        let push = super::super::tests::sub(&source, &hub, 10, AM_SUB_PUSH);
        assert_eq!(push.next_pixels_at(now), None);
        assert!(!ready_soon(&Snapshot::from_iter([push.clone()]), now, true));
        drop((unlimited, push, hub));
        let key = source.state.ctx_key;
        drop(source);
        domain.close(key, 100).unwrap();
        domain.destroy(key).unwrap();
    }
    #[test]
    fn fast_deadline_wins_slow_due_and_equal_rates_align_only_within_the_bounded_window() {
        let (domain, source, hub, _) = super::super::tests::fixture();
        let now = hub.now();
        let slow = super::super::tests::sub(&source, &hub, 5, AM_SUB_PULL);
        let fast = super::super::tests::sub(&source, &hub, 30, AM_SUB_PULL);
        assert!(slow.due(now - Duration::from_millis(210)));
        assert!(fast.due(now - Duration::from_millis(20)));
        let deadline = now - Duration::from_millis(20) + Duration::from_nanos(33_333_334);
        assert_eq!(
            next(
                &Snapshot::from_iter([slow.clone(), fast.clone()]),
                now,
                true
            ),
            Some(deadline)
        );
        assert_eq!(
            next(
                &Snapshot::from_iter([slow.clone(), fast.clone()]),
                now,
                false
            ),
            Some(now)
        );
        let twin = super::super::tests::sub(&source, &hub, 30, AM_SUB_PULL);
        assert!(twin.due(now - Duration::from_millis(15)));
        assert_eq!(
            next(
                &Snapshot::from_iter([fast.clone(), twin.clone()]),
                now,
                true
            ),
            twin.next_pixels_at(now)
        );
        let later = super::super::tests::sub(&source, &hub, 5, AM_SUB_PULL);
        assert!(later.due(now));
        assert_eq!(
            next(
                &Snapshot::from_iter([slow.clone(), later.clone()]),
                now,
                true
            ),
            Some(now)
        );
        let unlimited = super::super::tests::sub(&source, &hub, 0, AM_SUB_PULL);
        assert_eq!(
            next(
                &Snapshot::from_iter([fast.clone(), unlimited.clone()]),
                now,
                true
            ),
            Some(now)
        );
        drop((slow, fast, twin, later, unlimited, hub));
        let key = source.state.ctx_key;
        drop(source);
        domain.close(key, 100).unwrap();
        domain.destroy(key).unwrap();
    }
}
