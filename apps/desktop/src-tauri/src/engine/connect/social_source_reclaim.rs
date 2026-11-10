//! A source can be reviewed again only after the exact previous social worker
//! has completed and its JoinHandle has been consumed. This grants no network
//! authority and cannot stop an active attempt to make the condition true.
use super::*;

fn original<'a>(
    state: &'a mut State,
    receipt: &SocialNativeSourceReceipt,
) -> Result<&'a mut Attempt, ConnectError> {
    if state.shutting_down || state.social_epoch != receipt.epoch {
        return Err(ConnectError::StaleOperation);
    }
    let attempt = selected(state, &receipt.owner, &receipt.operation)?;
    let control = attempt
        .social
        .as_ref()
        .ok_or(ConnectError::StaleOperation)?;
    if !control.online
        || control.reservation_generation != receipt.generation
        || control.request.role != "host"
        || control.request.account_epoch != receipt.epoch
        || control.request.account_id != receipt.account
        || control.request.session_id != receipt.session
        || control.native_voice_claim.as_deref() != Some(receipt.claim.as_str())
    {
        return Err(ConnectError::StaleOperation);
    }
    ready_game::refresh_cleanup(attempt);
    if !terminal(attempt) {
        return Err(ConnectError::Busy);
    }
    Ok(attempt)
}

impl Manager {
    pub(in crate::engine::connect) fn can_reclaim_native_source(
        &self,
        receipt: &SocialNativeSourceReceipt,
    ) -> bool {
        lock(&self.state).is_ok_and(|mut state| original(&mut state, receipt).is_ok())
    }

    pub(in crate::engine::connect) async fn join_reclaim_native_source(
        &self,
        receipt: &SocialNativeSourceReceipt,
    ) -> Result<(), ConnectError> {
        let mut state = lock(&self.state)?;
        let attempt = original(&mut state, receipt)?;
        // Never transfer task ownership to a cancelable IPC waiter. The shared
        // Stop helper consumes only a finished result while State stays locked.
        if reap_finished_worker(attempt).is_some_and(|joined| joined.is_err()) {
            return Err(ConnectError::CleanupFailed);
        }
        if attempt.worker.is_some() {
            return Err(ConnectError::Busy);
        }
        Ok(())
    }

    pub(in crate::engine::connect) fn with_reclaim_native_source<T>(
        &self,
        receipt: &SocialNativeSourceReceipt,
        work: impl FnOnce() -> Result<T, ConnectError>,
    ) -> Result<T, ConnectError> {
        let mut state = lock(&self.state)?;
        if original(&mut state, receipt)?.worker.is_some() {
            return Err(ConnectError::Busy);
        }
        // Preserve Connect -> Dedicated Shared lock order through the final
        // generation rotation, so account transition or a new Attempt cannot race it.
        work()
    }
}

#[cfg(test)]
impl ConnectManager {
    pub(crate) async fn test_completed_native_source(
        binding: crate::engine::connect_voice::dedicated::NativeVoiceHostBinding,
    ) -> (Self, SocialNativeSourceReceipt) {
        for panic_on_completion in [true, false] {
            let mut authority = SocialLabAuthority::from_local_process(
                &lab::generate_key().public().to_string(),
                &"52".repeat(32),
            )
            .unwrap();
            authority.online_origin = Some("https://social.invalid");
            let manager = Self {
                live: Manager::with_social_lab(authority),
            };
            let (reservation, receipt) = manager
                .social_reserve_native_voice_recorded(
                    "main",
                    SocialDiscoveredReserveRequest {
                        account_epoch: 0,
                        account_id: "native-source-fixture".into(),
                        session_id: uuid::Uuid::new_v4().to_string(),
                        visibility: "friends".into(),
                    },
                    binding.clone(),
                    false,
                )
                .unwrap();
            assert!(
                !manager.can_reclaim_native_source(&receipt),
                "active reservation cannot be reclaimed"
            );
            assert_eq!(
                manager.with_reclaim_native_source(&receipt, || Ok(())),
                Err(ConnectError::Busy)
            );
            // No proof, policy, connection or source IO is created by this fixture.
            // It exercises the real completed-worker gate with the original source claim.
            let (release, held) = oneshot::channel();
            update(&manager.live.state, &reservation.operation_id, |attempt| {
                attempt.worker = Some(tokio::spawn(async move {
                    let _ = held.await;
                    assert!(!panic_on_completion, "owned failed-worker fixture");
                }));
            });
            finish(
                &manager.live.state,
                &reservation.operation_id,
                Some(ConnectError::Cancelled),
            );
            assert!(
                !manager.can_reclaim_native_source(&receipt),
                "unfinished worker remains owned"
            );
            {
                // Drop the first Stop waiter while its original worker is still
                // held. A later waiter must still find that same task in State.
                let mut stop =
                    std::pin::pin!(manager.live.stop(&receipt.owner, &receipt.operation));
                let mut context = std::task::Context::from_waker(std::task::Waker::noop());
                assert!(std::future::Future::poll(stop.as_mut(), &mut context).is_pending());
            }
            assert!(lock(&manager.live.state)
                .unwrap()
                .attempt
                .as_ref()
                .unwrap()
                .worker
                .is_some());
            release.send(()).unwrap();
            loop {
                if manager.can_reclaim_native_source(&receipt) {
                    break;
                }
                tokio::task::yield_now().await;
            }
            assert_eq!(
                manager.with_reclaim_native_source(&receipt, || Ok(())),
                Err(ConnectError::Busy),
                "finished is not joined"
            );
            // Exhaust the task budget: plain JoinHandle.await would return
            // Pending even though this owned task has already completed.
            for _ in 0..256 {
                if !tokio::task::coop::has_budget_remaining() {
                    break;
                }
                tokio::task::consume_budget().await;
            }
            assert!(!tokio::task::coop::has_budget_remaining());
            {
                let mut stop =
                    std::pin::pin!(manager.live.stop(&receipt.owner, &receipt.operation));
                let mut context = std::task::Context::from_waker(std::task::Waker::noop());
                let result = std::future::Future::poll(stop.as_mut(), &mut context);
                assert!(
                    matches!(result, std::task::Poll::Ready(Ok(_))),
                    "completed Stop must not transfer ownership across a yield"
                );
            }
            assert!(lock(&manager.live.state)
                .unwrap()
                .attempt
                .as_ref()
                .unwrap()
                .worker
                .is_none());
            if panic_on_completion {
                assert!(!manager.can_reclaim_native_source(&receipt));
                assert!(manager
                    .with_reclaim_native_source(&receipt, || Ok(()))
                    .is_err());
                assert_eq!(
                    lock(&manager.live.state)
                        .unwrap()
                        .attempt
                        .as_ref()
                        .unwrap()
                        .snapshot
                        .cleanup_state,
                    "failed"
                );
                continue;
            }
            // Also exercise the dedicated review join path with its own handle.
            update(&manager.live.state, &receipt.operation, |attempt| {
                attempt.worker = Some(tokio::spawn(async {}));
            });
            while !manager.can_reclaim_native_source(&receipt) {
                tokio::task::yield_now().await;
            }
            for _ in 0..256 {
                if !tokio::task::coop::has_budget_remaining() {
                    break;
                }
                tokio::task::consume_budget().await;
            }
            assert!(!tokio::task::coop::has_budget_remaining());
            {
                let mut join = std::pin::pin!(manager.join_reclaim_native_source(&receipt));
                let mut context = std::task::Context::from_waker(std::task::Waker::noop());
                assert!(
                    matches!(
                        std::future::Future::poll(join.as_mut(), &mut context),
                        std::task::Poll::Ready(Ok(()))
                    ),
                    "completed review join must not yield task ownership"
                );
            }
            assert_eq!(
                manager.with_reclaim_native_source(&receipt, || Ok(())),
                Ok(())
            );
            for change in 0..7 {
                let mut wrong = receipt.clone();
                match change {
                    0 => wrong.epoch += 1,
                    1 => wrong.generation += 1,
                    2 => wrong.operation = uuid::Uuid::new_v4().to_string(),
                    3 => wrong.session = uuid::Uuid::new_v4().to_string(),
                    4 => wrong.claim = "00".repeat(32),
                    5 => wrong.account = "different-account".into(),
                    _ => wrong.owner = "different-window".into(),
                }
                assert!(manager
                    .with_reclaim_native_source(&wrong, || Ok(()))
                    .is_err());
            }
            return (manager, receipt);
        }
        unreachable!("successful completion fixture must return")
    }
}
