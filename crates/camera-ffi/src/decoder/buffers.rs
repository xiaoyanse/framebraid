//! 输入槽与时间关联表固定预留，空闲容量也占控制额度。
use super::*;
use crate::frames::ControlCharge;

pub(super) const TIMING_ITEMS: usize = 256;
pub(super) struct Buffers {
    pub queue: VecDeque<Packet>,
    pub timings: VecDeque<Timing>,
    _metadata: ControlCharge,
}
impl Buffers {
    fn bytes(items: usize) -> Result<usize> {
        if items == 0 || items > SDK_DECODER_INPUT_ITEMS as usize {
            return Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE);
        }
        Ok(size_of::<Self>() + items * size_of::<Packet>() + TIMING_ITEMS * size_of::<Timing>())
    }
    pub(super) fn new(state: &Arc<CameraState>, items: usize) -> Result<Self> {
        let metadata = state.control(Self::bytes(items)?, false)?;
        let mut buffers = Self {
            queue: VecDeque::new(),
            timings: VecDeque::new(),
            _metadata: metadata,
        };
        buffers
            .queue
            .try_reserve_exact(items)
            .map_err(|_| AM_ERR_SDK_OUT_OF_MEMORY)?;
        buffers
            .timings
            .try_reserve_exact(TIMING_ITEMS)
            .map_err(|_| AM_ERR_SDK_OUT_OF_MEMORY)?;
        if buffers.queue.capacity() > items || buffers.timings.capacity() > TIMING_ITEMS {
            return Err(AM_ERR_SDK_RESOURCE_EXHAUSTED);
        }
        Ok(buffers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_rejects_before_allocating_and_idle_slots_remain_charged() {
        let (_domain, state) = crate::frames::isolated_test_state();
        let context = state.ctx.data::<Context>().unwrap();
        let required = Buffers::bytes(64).unwrap();
        context.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_bytes = required as u64 - 1;
        assert!(matches!(
            Buffers::new(&state, 64),
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
        ));
        assert_eq!(state.control_usage_snapshot().unwrap(), (0, 0));
        context.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_bytes = required as u64;
        let mut buffers = Buffers::new(&state, 64).unwrap();
        assert_eq!(state.control_usage_snapshot().unwrap(), (1, required));
        let capacity = (buffers.queue.capacity(), buffers.timings.capacity());
        for index in 0..64 {
            buffers.queue.push_back(Packet {
                bytes: Vec::new(),
                valid: 0,
                pts: index,
                tag: index,
                received: index,
                _charge: state.charge(0, false).unwrap(),
            });
        }
        for index in 0..TIMING_ITEMS {
            buffers.timings.push_back(Timing {
                tag: index as i64,
                pts: index as i64,
                received: index as i64,
                config: 1,
                delivered: false,
            });
        }
        assert_eq!(
            (buffers.queue.capacity(), buffers.timings.capacity()),
            capacity
        );
        buffers.queue.clear();
        buffers.timings.clear();
        assert_eq!(state.control_usage_snapshot().unwrap(), (1, required));
        drop(buffers);
        assert_eq!(state.control_usage_snapshot().unwrap(), (0, 0));
        println!(
            "decoder-metadata bytes={required} packet={} timing={} capacities={capacity:?}",
            size_of::<Packet>(),
            size_of::<Timing>()
        );
    }

    #[test]
    fn rebuilding_keeps_old_storage_until_the_new_context_is_ready() {
        let (_domain, state) = crate::frames::isolated_test_state();
        let context = state.ctx.data::<Context>().unwrap();
        let required = Buffers::bytes(1).unwrap();
        context.queues.lock().unwrap()[AM_Q_CONTROL as usize].max_bytes = required as u64;
        let old = Buffers::new(&state, 1).unwrap();
        assert!(matches!(
            Buffers::new(&state, 1),
            Err(AM_ERR_SDK_RESOURCE_EXHAUSTED)
        ));
        assert_eq!(old.queue.capacity(), 1);
        assert_eq!(state.control_usage_snapshot().unwrap(), (1, required));
        drop(old);
        let rebuilt = Buffers::new(&state, 1).unwrap();
        assert_eq!(state.control_usage_snapshot().unwrap(), (1, required));
        drop(rebuilt);
        assert_eq!(state.control_usage_snapshot().unwrap(), (0, 0));
        assert!(matches!(
            Buffers::new(&state, 0),
            Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE)
        ));
        assert!(matches!(
            Buffers::new(&state, 65),
            Err(AM_ERR_SDK_LIMIT_OUT_OF_RANGE)
        ));
    }
}
