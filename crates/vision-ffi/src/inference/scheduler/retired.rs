use super::super::records::Records;
use super::*;
use std::sync::TryLockError;

// 只检查仍在执行器队列中的Job；已经被原生执行读取的输入不会进入此路径。
pub(super) fn check(job: &Job) -> Option<bool> {
    let model = job.model.data::<ModelHandle>().ok()?;
    if model.stopping.load(Ordering::Acquire) {
        return Some(true);
    }
    let ids = match &job.input {
        Payload::Single(_) => std::slice::from_ref(&job.id),
        Payload::Native { ids, .. } => ids,
    };
    check_records(&model.state.records, ids)
}

fn check_records(records: &Mutex<Records>, ids: &[u64]) -> Option<bool> {
    match records.try_lock() {
        Ok(records) => Some(ids.iter().all(|id| {
            records
                .get(id)
                .is_none_or(|r| r.canceled || r.state >= AM_REQ_DONE as u32)
        })),
        Err(TryLockError::WouldBlock) => None,
        Err(TryLockError::Poisoned(_)) => Some(false),
    }
}

pub(super) fn release(job: Job) {
    let Job {
        model: entry,
        input,
        id,
        _admission,
        ..
    } = job;
    let Ok(model) = entry.data::<ModelHandle>() else {
        return;
    };
    // 不持域锁或records锁释放输入；外部拥有者的归还不能反向锁死准入。
    let count = match input {
        Payload::Single(input) => {
            drop(input);
            drop(_admission);
            execution::finish(model, id, Err(AM_ERR_SDK_HANDLE_CLOSED));
            1
        }
        Payload::Native { inputs, ids } => {
            let count = ids.len();
            drop(inputs);
            drop(_admission);
            for id in ids {
                execution::finish(model, id, Err(AM_ERR_SDK_HANDLE_CLOSED));
            }
            count
        }
    };
    model.inflight.fetch_sub(count, Ordering::AcqRel);
    // 未执行的清理不触碰busy/running，那里可能仍有另一项正在读取输入。
    model.state.changed.notify_all();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(state: u32, canceled: bool) -> Record {
        Record {
            model: 1,
            slot: 1,
            state,
            code: 0,
            created: 0,
            started: 0,
            finished: 0,
            expires: i64::MAX,
            result: None,
            canceled,
            offline: false,
        }
    }

    #[test]
    fn mixed_native_batch_preserves_all_inputs_until_live_item_completes() {
        let records = Mutex::new(Records::isolated([
            (1, record(AM_REQ_PENDING as u32, true)),
            (2, record(AM_REQ_PENDING as u32, false)),
        ]));
        assert_eq!(check_records(&records, &[1, 2]), Some(false));
        records.lock().unwrap().get_mut(&2).unwrap().canceled = true;
        assert_eq!(check_records(&records, &[1, 2]), Some(true));
    }

    #[test]
    fn replaced_or_expired_records_do_not_hold_unused_admission() {
        let records = Mutex::new(Records::isolated([(
            1,
            record(AM_REQ_REPLACED as u32, false),
        )]));
        assert_eq!(check_records(&records, &[1, 2]), Some(true));
        records
            .lock()
            .unwrap()
            .insert(2, record(AM_REQ_RUNNING as u32, false));
        assert_eq!(check_records(&records, &[1, 2]), Some(false));
    }

    #[test]
    fn records_contention_never_blocks_while_executor_lock_is_held() {
        let records = Mutex::new(Records::default());
        let _guard = records.lock().unwrap();
        assert_eq!(check_records(&records, &[1]), None);
    }
}
