use super::*;
use std::collections::VecDeque;
struct Cached {
    url: String,
    credential: [u64; 2],
    value: Observation,
    _charge: ControlCharge,
}
#[derive(Default)]
struct Entries {
    rows: VecDeque<Cached>,
    storage: Option<ControlCharge>,
}
impl Entries {
    fn discard(&mut self) {
        self.rows = VecDeque::new();
        self.storage = None;
    }
    fn prepare(&mut self, state: &Arc<CameraState>, maximum: usize) -> Result<()> {
        if self.rows.capacity() >= maximum {
            return Ok(());
        }
        let charge = state.metadata(maximum * size_of::<Cached>())?;
        let mut rows = VecDeque::new();
        rows.try_reserve_exact(maximum)
            .map_err(|_| AM_ERR_SDK_RESOURCE_EXHAUSTED)?;
        if rows.capacity() != maximum {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        rows.extend(self.rows.drain(..));
        self.rows = rows;
        self.storage = Some(charge);
        Ok(())
    }
}
pub(super) struct Cache {
    seeds: [RandomState; 2],
    entries: Mutex<Entries>,
    pub(super) generation: AtomicU64,
}
impl Cache {
    pub(super) fn new() -> Self {
        Self {
            seeds: [RandomState::new(), RandomState::new()],
            entries: Mutex::default(),
            generation: AtomicU64::new(0),
        }
    }
    pub(super) fn identity(&self, secret: &Secret) -> [u64; 2] {
        self.seeds.each_ref().map(|seed| {
            let mut h = seed.build_hasher();
            secret.username.hash(&mut h);
            secret.password.hash(&mut h);
            h.finish()
        })
    }
    pub(super) fn clear(&self, discard_storage: bool) -> Result<()> {
        let mut entries = self.entries.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        if discard_storage {
            entries.discard();
        } else {
            entries.rows.clear();
        }
        Ok(())
    }
    pub(super) fn get(&self, url: &str, credential: [u64; 2]) -> Result<Option<Observation>> {
        let mut entries = self.entries.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        entries
            .rows
            .retain(|entry| Instant::now() < entry.value.expires);
        Ok(entries
            .rows
            .iter()
            .find(|entry| entry.url == url && entry.credential == credential)
            .map(|entry| entry.value.clone()))
    }
    pub(super) fn put(
        &self,
        state: &Arc<CameraState>,
        generation: u64,
        url: &str,
        credential: [u64; 2],
        value: Observation,
    ) -> Result<()> {
        let limit = state.limits()?.control_max_items as usize;
        let queue_limit = state
            .ctx
            .data::<Context>()?
            .queues
            .lock()
            .map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?[5]
            .max_items as usize;
        let maximum = CACHE_ITEMS.min(limit.min(queue_limit) / 2);
        let mut entries = self.entries.lock().map_err(|_| AM_ERR_SDK_INTERNAL_ERROR)?;
        if self.generation.load(Ordering::Acquire) != generation {
            return Ok(());
        }
        entries.rows.retain(|entry| {
            Instant::now() < entry.value.expires
                && !(entry.url == url && entry.credential == credential)
        });
        let cost = url.len() + size_of::<Cached>();
        while !entries.rows.is_empty()
            && (entries.rows.len() >= maximum
                || entries
                    .rows
                    .iter()
                    .map(|e| e.url.capacity() + size_of::<Cached>())
                    .sum::<usize>()
                    + cost
                    > CACHE_BYTES)
        {
            entries.rows.pop_front();
        }
        if maximum == 0 || cost > CACHE_BYTES {
            return Ok(());
        }
        // 可选缓存不能挤掉已接受的探测结果。
        if entries.prepare(state, maximum).is_err() {
            return Ok(());
        }
        let Ok(charge) = state.control(url.len(), false) else {
            return Ok(());
        };
        let mut owned = String::new();
        if owned.try_reserve_exact(url.len()).is_err() || owned.capacity() != url.len() {
            return Ok(());
        }
        owned.push_str(url);
        entries.rows.push_back(Cached {
            url: owned,
            credential,
            value,
            _charge: charge,
        });
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_empty_storage_stays_charged_until_discarded() {
        let (_domain, state) = crate::frames::isolated_test_state();
        let cache = Cache::new();
        cache
            .put(
                &state,
                0,
                "unsupported:test",
                [0; 2],
                Observation::failed(AM_PROBE_UNSUPPORTED),
            )
            .unwrap();
        let full = state.control_usage_snapshot().unwrap();
        assert_eq!(full.0, 1);
        cache.clear(false).unwrap();
        let empty = state.control_usage_snapshot().unwrap();
        assert_eq!(empty.0, 0);
        assert_eq!(full.1 - empty.1, "unsupported:test".len());
        assert_eq!(empty.1, 16 * size_of::<Cached>());
        println!(
            "probe_cache_full_bytes={}; retained_empty_bytes={}; slots=16",
            full.1, empty.1
        );
        cache.clear(true).unwrap();
        assert_eq!(state.control_usage_snapshot().unwrap(), (0, 0));
    }
    #[test]
    fn cache_admission_is_optional_and_stale_generation_cannot_repopulate() {
        let (_domain, state) = crate::frames::isolated_test_state();
        state.ctx.data::<Context>().unwrap().queues.lock().unwrap()[5].max_bytes = 1;
        let cache = Cache::new();
        cache
            .put(
                &state,
                0,
                "unsupported:test",
                [0; 2],
                Observation::failed(AM_PROBE_UNSUPPORTED),
            )
            .unwrap();
        assert_eq!(state.control_usage_snapshot().unwrap(), (0, 0));
        assert!(cache.get("unsupported:test", [0; 2]).unwrap().is_none());
        cache.generation.store(1, Ordering::Release);
        cache
            .put(
                &state,
                0,
                "unsupported:test",
                [0; 2],
                Observation::failed(AM_PROBE_UNSUPPORTED),
            )
            .unwrap();
        assert_eq!(cache.entries.lock().unwrap().rows.capacity(), 0);
    }
    #[test]
    fn expiration_drops_url_but_keeps_storage_until_real_discard() {
        let (_domain, state) = crate::frames::isolated_test_state();
        let cache = Cache::new();
        let mut value = Observation::failed(AM_PROBE_UNSUPPORTED);
        value.expires = Instant::now() - Duration::from_secs(1);
        cache
            .put(&state, 0, "unsupported:expired", [0; 2], value)
            .unwrap();
        assert!(cache.get("unsupported:expired", [0; 2]).unwrap().is_none());
        assert_eq!(
            state.control_usage_snapshot().unwrap(),
            (0, 16 * size_of::<Cached>())
        );
        assert_eq!(cache.entries.lock().unwrap().rows.capacity(), 16);
        cache.clear(true).unwrap();
        assert_eq!(state.control_usage_snapshot().unwrap(), (0, 0));
    }
}
