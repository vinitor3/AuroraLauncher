//! Causal test-only pause of the original actor, never a replacement authority.
//! The independent real heartbeat keeps validating fresh signed watch packets.
use super::*;

struct Published {
    authority: Arc<LaunchAuthority>,
    leases: tokio::sync::watch::Receiver<String>,
    guest: Weak<lab::Guest>,
    bridge: Weak<lab::TcpBridge>,
    sequence: u64,
    initial_callback_calls: usize,
}
#[derive(Default)]
pub(in crate::engine::connect::live::social) struct Pause {
    published: Mutex<Option<Published>>,
    released: CancellationToken,
}
impl Pause {
    pub(in crate::engine::connect::live::social) fn release(&self) {
        self.released.cancel();
    }
    pub(in crate::engine::connect::live::social) async fn hold(
        &self,
        authority: &Arc<LaunchAuthority>,
        leases: tokio::sync::watch::Receiver<String>,
        guest: &Arc<lab::Guest>,
        bridge: &Arc<lab::TcpBridge>,
        cancel: &CancellationToken,
    ) -> Result<(), ConnectError> {
        let initial_callback_calls = authority
            .with_current(|| Ok(1usize))
            .map_err(|_| ConnectError::PlanExpired)?;
        let sequence = authority.sequence()?;
        *self.published.lock().map_err(|_| ConnectError::Internal)? = Some(Published {
            authority: authority.clone(),
            leases,
            guest: Arc::downgrade(guest),
            bridge: Arc::downgrade(bridge),
            sequence,
            initial_callback_calls,
        });
        tokio::select! { biased;
            _=cancel.cancelled()=>Err(ConnectError::Cancelled),
            _=self.released.cancelled()=>Ok(()),
            _=tokio::time::sleep(Duration::from_secs(10))=>Err(ConnectError::PlanExpired),
        }
    }
    pub(in crate::engine::connect::live::social) async fn inspect_expiry(
        &self,
    ) -> Result<Value, ConnectError> {
        let deadline = Instant::now() + Duration::from_secs(6);
        let (authority, mut leases, guest, bridge, original_sequence, initial_callback_calls) = loop {
            let published = {
                let state = self.published.lock().map_err(|_| ConnectError::Internal)?;
                state.as_ref().map(|value| {
                    (
                        value.authority.clone(),
                        value.leases.clone(),
                        value.guest.clone(),
                        value.bridge.clone(),
                        value.sequence,
                        value.initial_callback_calls,
                    )
                })
            };
            if let Some(value) = published {
                break value;
            }
            if self.released.is_cancelled() || Instant::now() >= deadline {
                return Err(ConnectError::PlanExpired);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        loop {
            if self.released.is_cancelled() || Instant::now() >= deadline {
                return Err(ConnectError::PlanExpired);
            }
            let expired = {
                let current = authority
                    .current
                    .lock()
                    .map_err(|_| ConnectError::Internal)?;
                if current.terminal || current.continuity.sequence() != original_sequence {
                    return Err(ConnectError::Internal);
                }
                current.continuity.check().is_err()
            };
            if expired {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        // Only the existing heartbeat writes this watch, AFTER its own original
        // SocialContinuity has verified signature, grant, sequence and clocks.
        let late = leases.borrow_and_update().clone();
        let packet: Value = serde_json::from_str(&late).map_err(|_| ConnectError::Internal)?;
        let newer = packet["body"]["sequence"]
            .as_u64()
            .is_some_and(|n| n > original_sequence);
        let fresh = packet["body"]["expiresAtUnixMs"]
            .as_u64()
            .is_some_and(|end| end > lab::unix_millis() + 100);
        let grant_current = authority.grant.check_session().is_ok();
        let invitation_current = authority
            .current
            .lock()
            .map_err(|_| ConnectError::Internal)?
            .invitation
            .as_ref()
            .is_some_and(|i| i.check_session_current().is_ok());
        let original_cancelled = authority.cancel.is_cancelled();
        let guest_alive = guest
            .upgrade()
            .is_some_and(|guest| guest.connection().close_reason().is_none());
        let bridge_alive = bridge.upgrade().is_some_and(|bridge| !bridge.is_closed());
        if !newer
            || !fresh
            || !grant_current
            || !invitation_current
            || original_cancelled
            || !guest_alive
            || !bridge_alive
        {
            return Err(ConnectError::Internal);
        }
        let mut callbacks = 0;
        let expired_rejected = authority
            .with_current(|| {
                callbacks += 1;
                Ok(())
            })
            .is_err();
        let late_update_rejected = authority.update(&late).is_err();
        let terminal_rejected = authority
            .with_current(|| {
                callbacks += 1;
                Ok(())
            })
            .is_err();
        let current = authority
            .current
            .lock()
            .map_err(|_| ConnectError::Internal)?;
        Ok(
            json!({"initialCallbackCalls":initial_callback_calls,"originalLeaseExpired":expired_rejected,"grantCurrent":grant_current,"invitationCurrent":invitation_current,
            "originalCancelled":original_cancelled,"guestAlive":guest_alive,"bridgeAlive":bridge_alive,
            "newerValidatedLease":newer,"lateLeaseCurrent":fresh,"lateUpdateRejected":late_update_rejected,
            "terminalRejected":terminal_rejected,"callbackCalls":callbacks,
            "sequenceUnchanged":current.continuity.sequence()==original_sequence,"terminal":current.terminal}),
        )
    }
}
impl Drop for Pause {
    fn drop(&mut self) {
        self.release();
    }
}
