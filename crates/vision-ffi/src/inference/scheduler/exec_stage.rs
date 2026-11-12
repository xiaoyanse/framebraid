//! B1.2 执行错峰（executor 层执行槽）+ 批 E 执行期补账与失败收口的聚拢模块。
use super::*;
use std::time::Duration;

/// 执行槽守卫：持有期间执行期份额在账（补账后至 Job 收尾归还），drop 释放
/// 并唤醒等槽者。等待只在多 Model 并发竞争时发生；单 Model 受 busy 互斥
/// 约束至多一个 Job 在执行，acquire 即时成功，行为与无槽版本一致。
pub(super) struct ExecSlotGuard {
    engine: Arc<Engine>,
}
impl Drop for ExecSlotGuard {
    fn drop(&mut self) {
        let mut data = self
            .engine
            .data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        data.exec_slot = false;
        drop(data);
        self.engine.wake.notify_all();
    }
}

pub(super) fn acquire(engine: &Arc<Engine>) -> Result<ExecSlotGuard> {
    let mut data = lock(&engine.data)?;
    while data.exec_slot {
        if engine.stop.load(Ordering::Acquire) {
            return Err(AM_ERR_SDK_HANDLE_CLOSED);
        }
        // 轮询上限兜底丢唤醒；release/stop 均会 notify_all 正常即时唤醒。
        data = engine
            .wake
            .wait_timeout(data, Duration::from_millis(50))
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .0;
    }
    data.exec_slot = true;
    Ok(ExecSlotGuard {
        engine: engine.clone(),
    })
}

/// execute 的槽入口：Ok(Some)=持槽、Ok(None)=零执行份额批不进互斥窗口
/// （GPU 直传 exec=0、一次性全额入账路径）、Err=stop 期等槽批或补账失败批
/// 已按既有执行期预算失败语义整笔收口（逐项回执+在途归还，不静默绕过
/// 护栏、不留下部分入账），调用方直接返回。
pub(super) fn enter(
    entry: &Arc<Entry>,
    input: &Payload,
    id: u64,
    admissions: &mut Vec<Admission>,
    charges: &[usize],
) -> Result<Option<ExecSlotGuard>> {
    if admissions.len() != charges.len() {
        abort_job(entry, input, id, AM_ERR_SDK_INTERNAL_ERROR);
        return Err(AM_ERR_SDK_INTERNAL_ERROR);
    }
    if !charges.iter().any(|&charge| charge > 0) {
        return Ok(None);
    }
    let Some(engine) = admissions.first().map(|a| a.engine.clone()) else {
        abort_job(entry, input, id, AM_ERR_SDK_INTERNAL_ERROR);
        return Err(AM_ERR_SDK_INTERNAL_ERROR);
    };
    let guard = match acquire(&engine) {
        Ok(guard) => guard,
        Err(code) => {
            abort_job(entry, input, id, code);
            return Err(code);
        }
    };
    match stage_charges(entry, admissions, charges) {
        Ok(()) => Ok(Some(guard)),
        Err(code) => {
            drop(guard);
            abort_job(entry, input, id, code);
            Err(code)
        }
    }
}

/// 整笔收口：逐项回执+在途/并行计数归还，_admissions 由 Job drop 归还账目。
fn abort_job(entry: &Arc<Entry>, input: &Payload, id: u64, code: i32) {
    let ids: Vec<u64> = match input {
        Payload::Single(_) => vec![id],
        Payload::Native { ids, .. } => ids.clone(),
    };
    if let Ok(model) = entry.data::<ModelHandle>() {
        for request in &ids {
            super::execution::finish(model, *request, Err(code));
            model.inflight.fetch_sub(1, Ordering::AcqRel);
        }
        model.state.running.fetch_sub(1, Ordering::AcqRel);
        model.busy.store(false, Ordering::Release);
        model.state.changed.notify_all();
    }
}

/// 批 E：逐项执行期补账——挂起期准入之上追加同源账目预留（项数已在准入
/// 计入，仅补字节）；run_batch 的同源核心组批预算检查以补账后合计为基
/// （Admission::totals 含补账守卫，与分期前同一求和口径）。锁序与准入
/// 一致（limits→queues→engine.data→usage），worker 此时不持其他锁。
fn stage_charges(
    entry: &Arc<Entry>,
    admissions: &mut Vec<Admission>,
    charges: &[usize],
) -> Result<()> {
    if admissions.len() != charges.len() {
        return Err(AM_ERR_SDK_INTERNAL_ERROR);
    }
    let model = entry.data::<ModelHandle>()?;
    let ctx = model.state.context()?;
    let context = ctx.data::<Context>()?;
    let limits = *lock(&context.limits)?;
    let queues = lock(&context.queues)?;
    let mut staged = Vec::new();
    for (admission, extra) in admissions.iter().zip(charges) {
        if *extra == 0 {
            continue;
        }
        // Admission.class 为预算类（0=实时/1=离线）；队列策略按 AM_Q 索引
        // （2/3）取对应类——与 admission::submit 传入的 policy 同一来源。
        let policy_class = if admission.class == 1 {
            AM_Q_OFFLINE_BATCH
        } else {
            AM_Q_REALTIME_INFER
        } as usize;
        staged.push(budget::topup(
            admission,
            *extra,
            &limits,
            &queues[policy_class],
        )?);
    }
    admissions.extend(staged);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> Arc<Engine> {
        let budget = ThreadBudget {
            parallel: 2,
            total: 4,
            slots: 64,
        };
        Arc::new(Engine {
            data: Mutex::new(Data::new(budget).unwrap()),
            wake: Condvar::new(),
            budget,
            stop: AtomicBool::new(false),
            threads: Mutex::default(),
        })
    }

    #[test]
    fn slot_flags_busy_and_releases_on_drop() {
        let e = engine();
        {
            let guard = acquire(&e).unwrap();
            assert!(e.data.lock().unwrap().exec_slot);
            drop(guard);
        }
        assert!(!e.data.lock().unwrap().exec_slot);
    }

    /// 错峰核心语义：B 的 acquire 在 A 持槽期间挂起（挂起份额由账目测试覆
    /// 盖），A 释放后 B 获得槽继续补账执行。
    #[test]
    fn blocked_waiter_proceeds_after_holder_releases() {
        let e = engine();
        let holder = acquire(&e).unwrap();
        let waiter_engine = e.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let waiter = std::thread::spawn(move || {
            let guard = acquire(&waiter_engine).unwrap();
            tx.send(()).unwrap();
            drop(guard);
        });
        assert!(rx.recv_timeout(Duration::from_millis(150)).is_err());
        drop(holder);
        rx.recv_timeout(Duration::from_secs(2))
            .expect("槽释放后等槽者应获得槽");
        waiter.join().unwrap();
        assert!(!e.data.lock().unwrap().exec_slot);
    }

    /// stop 期等槽者按关闭类错误退出且不清持槽者的标志；持槽者随后释放。
    #[test]
    fn stop_errors_blocked_waiter_without_clearing_holder() {
        let e = engine();
        let _holder = acquire(&e).unwrap();
        e.stop.store(true, Ordering::Release);
        assert!(matches!(acquire(&e), Err(AM_ERR_SDK_HANDLE_CLOSED)));
        assert!(e.data.lock().unwrap().exec_slot);
        drop(_holder);
        assert!(!e.data.lock().unwrap().exec_slot);
    }
}
