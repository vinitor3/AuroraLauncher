//! Dev-dependency + debug-only bridge to two real81 ownership boundaries.
//! No closure, provider, authority, enable operation or Endpoint is exposed.
use super::*;
use std::sync::Weak;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeVoiceRetirementStage {
    AfterConnect,
    Completed,
}
struct Inner {
    entered: CancellationToken,
    released: CancellationToken,
    socket: Mutex<Option<SocketAddr>>,
    timed_out: AtomicBool,
}
/// Releasing or dropping this gate permits the SAME actual resource owner to
/// continue. It owns no Host/Endpoint, socket, work guard or fragment payload.
pub struct NativeVoiceRetirementGate {
    inner: Arc<Inner>,
}
impl NativeVoiceRetirementGate {
    pub async fn entered(&self) {
        self.inner.entered.cancelled().await;
    }
    pub fn release(&self) {
        self.inner.released.cancel();
    }
    pub fn socket_addr(&self) -> Option<SocketAddr> {
        *self.inner.socket.lock().unwrap()
    }
    pub fn timed_out(&self) -> bool {
        self.inner.timed_out.load(Ordering::Acquire)
    }
}
impl Drop for NativeVoiceRetirementGate {
    fn drop(&mut self) {
        self.release();
    }
}

#[derive(Default)]
pub(super) struct Slot(Mutex<Option<(NativeVoiceRetirementStage, Weak<Inner>)>>);
impl Slot {
    pub(super) fn install(
        &self,
        stage: NativeVoiceRetirementStage,
    ) -> Result<NativeVoiceRetirementGate> {
        let mut current = self.0.lock().map_err(|_| Error::Configuration)?;
        if current
            .as_ref()
            .is_some_and(|(_, gate)| gate.upgrade().is_some())
        {
            return Err(Error::Configuration);
        }
        let inner = Arc::new(Inner {
            entered: CancellationToken::new(),
            released: CancellationToken::new(),
            socket: Mutex::new(None),
            timed_out: AtomicBool::new(false),
        });
        *current = Some((stage, Arc::downgrade(&inner)));
        Ok(NativeVoiceRetirementGate { inner })
    }
    pub(super) async fn checkpoint(
        &self,
        stage: NativeVoiceRetirementStage,
        socket: Option<SocketAddr>,
    ) {
        let gate = self
            .0
            .lock()
            .unwrap()
            .as_ref()
            .filter(|(expected, _)| *expected == stage)
            .and_then(|(_, gate)| gate.upgrade());
        if let Some(gate) = gate {
            *gate.socket.lock().unwrap() = socket;
            gate.entered.cancel();
            // Same bound as existing81 test gates; never a production timeout
            // or a way to publish cleanup while the actual owner is retained.
            if tokio::time::timeout(Duration::from_secs(8), gate.released.cancelled())
                .await
                .is_err()
            {
                gate.timed_out.store(true, Ordering::Release);
            }
        }
    }
}
impl Host {
    pub fn native_voice_service_disabled_for_test(&self, lease_id: &str) -> Result<bool> {
        let lease = self
            .state
            .leases
            .get(lease_id)
            .ok_or(Error::ServiceDenied)?;
        let (handle, service) = self.state.services.get("aux").ok_or(Error::ServiceDenied)?;
        if service.protocol != Protocol::Udp || service.revision != 1 {
            return Err(Error::ServiceDenied);
        }
        Ok(!lease
            .udp_services
            .get(handle)
            .ok_or(Error::ServiceDenied)?
            .enabled())
    }
    /// Only an existing invited aux/UDP/r1/fragmented grant may be held. This
    /// does not create a service, grant, association or replacement authority.
    pub fn native_voice_retirement_gate_for_test(
        &self,
        lease_id: &str,
        stage: NativeVoiceRetirementStage,
    ) -> Result<NativeVoiceRetirementGate> {
        let lease = self
            .state
            .leases
            .get(lease_id)
            .ok_or(Error::ServiceDenied)?;
        let (handle, service) = self.state.services.get("aux").ok_or(Error::ServiceDenied)?;
        if !matches!(lease.invitation, Some(InvitationAuthority::Session(_)))
            || !lease.value.services.contains("aux")
            || service.protocol != Protocol::Udp
            || service.revision != 1
            || service.required_capability != Capability::FragmentedDatagram
        {
            return Err(Error::ServiceDenied);
        }
        lease
            .udp_services
            .get(handle)
            .ok_or(Error::ServiceDenied)?
            .native_gate(stage)
    }
}
