//! Native voice monitoring inside the original social online actor. No process,
//! source endpoint, observation result or voice permission comes from a DTO.
use super::*;
use crate::engine::connect_voice::dedicated::{
    DedicatedError, NativeVoiceHostBinding, VoiceRuntimeError, VoiceRuntimeSample,
};
use std::collections::HashSet;
use tokio::task::{JoinHandle, JoinSet};

const WINDOW: Duration = Duration::from_secs(2);
const PERIOD: Duration = Duration::from_secs(1);
enum Sample {
    Panicked,
    Voice(std::result::Result<VoiceRuntimeSample, VoiceRuntimeError>),
    Tcp(std::result::Result<(), DedicatedError>),
}
struct Query {
    task: JoinHandle<Sample>,
    window: Instant,
    voice: bool,
}
pub(super) struct VoiceMonitor {
    binding: NativeVoiceHostBinding,
    query: Option<Query>,
    next: Instant,
    terminal: bool,
    failed: bool,
    leases: HashSet<String>,
    retirements: JoinSet<lab::Result<()>>,
}
impl VoiceMonitor {
    pub(super) fn new(binding: NativeVoiceHostBinding) -> Self {
        Self {
            binding,
            query: None,
            next: Instant::now(),
            terminal: false,
            failed: false,
            leases: HashSet::new(),
            retirements: JoinSet::new(),
        }
    }
    pub(super) fn source(&self) -> &NativeVoiceHostBinding {
        &self.binding
    }
    pub(super) fn state(&self) -> &'static str {
        if self.failed {
            "failed"
        } else if !self.terminal {
            "enabled"
        } else if self.retirements.is_empty() {
            "retired"
        } else {
            "retiring"
        }
    }
    /// Called before the invitation can be delivered. A retired native source
    /// can admit game TCP, but never gives a new lease live aux authority.
    pub(super) async fn register_lease(
        &mut self,
        host: &Arc<lab::Host>,
        invitation: &lab::SessionInvitation,
    ) -> Result<(), ConnectError> {
        let id = invitation.invitation_id().to_owned();
        if self.leases.len() >= 128 || !self.leases.insert(id.clone()) {
            return Err(ConnectError::InvitationRejected);
        }
        if self.terminal {
            host.disable_udp_service(&id, "aux", 1)
                .await
                .map_err(|_| ConnectError::Internal)?;
        }
        Ok(())
    }
    fn retire(&mut self, host: &Option<Arc<lab::Host>>) {
        if self.terminal {
            return;
        }
        self.terminal = true;
        if let Some(host) = host {
            // All original leases are driven independently, including revoked,
            // unadmitted and disconnected entries. No first-owner drain delays
            // the next lease's real81 gate.
            for id in &self.leases {
                let host = host.clone();
                let id = id.clone();
                self.retirements
                    .spawn(async move { host.disable_udp_service(&id, "aux", 1).await });
            }
        }
    }
    fn collect_retirements(&mut self) {
        while let Some(result) = self.retirements.try_join_next() {
            if !matches!(result, Ok(Ok(()))) {
                self.failed = true;
            }
        }
    }
    pub(super) async fn tick(
        &mut self,
        run: &Run,
        host: &Option<Arc<lab::Host>>,
    ) -> Result<(), ConnectError> {
        run.current()?;
        self.binding
            .check_current()
            .map_err(|_| ConnectError::SourceChanged)?;
        self.collect_retirements();
        if self.failed {
            return Err(ConnectError::Internal);
        }
        if self
            .query
            .as_ref()
            .is_some_and(|query| query.voice && Instant::now() >= query.window)
        {
            self.retire(host);
        }
        if self
            .query
            .as_ref()
            .is_some_and(|query| query.task.is_finished())
        {
            let query = self.query.take().unwrap();
            let late = Instant::now() >= query.window;
            let result = query.task.await;
            if query.voice {
                if !self.terminal
                    && (late
                        || !matches!(result, Ok(Sample::Voice(Ok(sample))) if self.binding.accepts(&sample)))
                {
                    self.retire(host);
                }
            } else {
                match result {
                    Ok(Sample::Tcp(result)) => tcp_result(result)?,
                    _ => return Err(ConnectError::Internal),
                }
            }
        }
        if host.is_some() && self.query.is_none() && Instant::now() >= self.next {
            self.next = Instant::now() + PERIOD;
            let window = Instant::now() + WINDOW;
            if !self.terminal {
                match self.binding.reserve_voice() {
                    Ok(query) => {
                        self.query = Some(Query {
                            task: tokio::task::spawn_blocking(move || {
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    Sample::Voice(query.execute())
                                }))
                                .unwrap_or(Sample::Panicked)
                            }),
                            window,
                            voice: true,
                        })
                    }
                    Err(_) => self.retire(host),
                }
            } else {
                match self.binding.reserve_tcp() {
                    Ok(query) => {
                        self.query = Some(Query {
                            task: tokio::task::spawn_blocking(move || {
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    Sample::Tcp(query.execute())
                                }))
                                .unwrap_or(Sample::Panicked)
                            }),
                            window,
                            voice: false,
                        })
                    }
                    Err(error) => tcp_result(Err(error))?,
                }
            }
        }
        Ok(())
    }
    /// Transport is closed by the enclosing actor before this wait. No timeout
    /// or cancelled UI waiter may turn either original owner into completion.
    pub(super) async fn finish(&mut self) {
        if let Some(query) = self.query.take() {
            let _ = query.task.await;
        }
        while let Some(result) = self.retirements.join_next().await {
            if !matches!(result, Ok(Ok(()))) {
                self.failed = true;
            }
        }
    }
}
fn tcp_result(result: std::result::Result<(), DedicatedError>) -> Result<(), ConnectError> {
    use crate::engine::{
        connect_discovery::DiscoveryError as D, connect_owned_run::OwnedRunError as O,
    };
    match result {
        Ok(())
        | Err(
            DedicatedError::Busy
            | DedicatedError::NotReady
            | DedicatedError::Source(O::Busy | O::Discovery(D::Busy)),
        ) => Ok(()),
        Err(
            DedicatedError::Closed
            | DedicatedError::Cancelled
            | DedicatedError::Source(
                O::Closed
                | O::Cancelled
                | O::Deadline
                | O::Discovery(
                    D::ProcessChanged
                    | D::ListenerChanged
                    | D::Unavailable
                    | D::Limit
                    | D::StatusFailed,
                ),
            ),
        ) => Err(ConnectError::SourceChanged),
        _ => Err(ConnectError::Internal),
    }
}
/// Before review and immediately before/after the first bind. A late or cancelled
/// observation is discarded, but its real84 thread is collected by this same
/// enclosing Attempt before it can release the source slot.
pub(in crate::engine::connect::live::social) async fn probe(
    run: &Run,
    binding: &NativeVoiceHostBinding,
    host: Option<&Arc<lab::Host>>,
) -> Result<(), ConnectError> {
    run.current()?;
    binding
        .online_voice_policy()
        .map_err(|_| ConnectError::SourceChanged)?;
    let query = binding
        .reserve_voice()
        .map_err(|_| ConnectError::SourceChanged)?;
    let deadline = Instant::now() + WINDOW;
    let mut task = tokio::task::spawn_blocking(move || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| query.execute()))
    });
    let mut tick = interval(Duration::from_millis(20));
    let result = loop {
        tokio::select! { biased;
            _ = run.cancel.cancelled() => break None,
            _ = tokio::time::sleep_until(deadline.into()) => break None,
            _ = tick.tick() => if run.current().is_err() || binding.check_current().is_err() { break None; },
            value = &mut task => break Some(value),
        }
    };
    let Some(result) = result else {
        if let Some(host) = host {
            host.shutdown().await;
        }
        let _ = task.await;
        return Err(ConnectError::SourceChanged);
    };
    if Instant::now() >= deadline {
        return Err(ConnectError::SourceChanged);
    }
    run.current()?;
    binding
        .check_current()
        .map_err(|_| ConnectError::SourceChanged)?;
    match result {
        Ok(Ok(Ok(sample))) if binding.accepts(&sample) => Ok(()),
        _ => Err(ConnectError::SourceChanged),
    }
}

/// Exact service grant shared by the policy2 SVC and Plasmo profiles. Payloads
/// remain opaque; the application's complete datagram is capped at 4096 bytes.
pub(super) fn service(binding: &NativeVoiceHostBinding) -> lab::Service {
    lab::Service {
        service_id: "aux".into(),
        protocol: lab::Protocol::Udp,
        endpoint: binding.targets().1,
        revision: 1,
        max_datagram_bytes: 4096,
        required_capability: lab::Capability::FragmentedDatagram,
        udp_return_policy: lab::UdpReturnPolicy::AuthorizedDownstream {
            bytes_per_second: 16384,
            datagrams_per_second: 20,
        },
    }
}

#[derive(Default)]
pub(super) struct LocalPlayer {
    guest: Option<Arc<lab::Guest>>,
    tcp: Option<Arc<lab::TcpBridge>>,
    authority: Option<Arc<ready_source::HostPlayerLaunchAuthority>>,
    seen_poll: bool,
    udp: Option<lab::UdpBridge>,
    sockets: Vec<SocketAddr>,
    tcp_address: Option<SocketAddr>,
}
/// An interrupted initializer is still collected. The caller records every
/// returned resource before returning the interruption, so outer cleanup owns it.
async fn initialize<T>(
    run: &Run,
    host: &Arc<lab::Host>,
    monitor: &mut VoiceMonitor,
    future: impl std::future::Future<Output = lab::Result<T>>,
) -> (Option<ConnectError>, lab::Result<T>) {
    tokio::pin!(future);
    let mut tick = interval(Duration::from_millis(20));
    let error = loop {
        tokio::select! { biased;
            _ = run.cancel.cancelled() => break ConnectError::Cancelled,
            _ = tick.tick() => if let Err(error) = monitor.tick(run, &Some(host.clone())).await { break error; },
            result = &mut future => return (None, result),
        }
    };
    host.shutdown().await;
    (Some(error), future.await)
}
impl LocalPlayer {
    #[allow(clippy::too_many_arguments)] // Original policy, source and ticket remain distinct authorities.
    pub(super) async fn start(
        &mut self,
        run: &Run,
        host: &Arc<lab::Host>,
        monitor: &mut VoiceMonitor,
        key: &lab::SecretKey,
        invitation: &lab::SessionInvitation,
        policy: &lab::SignedSocialHostPolicy,
        network_ticket: &lab::OnlineNetworkTicket,
    ) -> Result<(), ConnectError> {
        let ticket = host
            .local_player_ticket(invitation, key)
            .map_err(|_| ConnectError::InvitationRejected)?;
        let (interrupted, result) = initialize(
            run,
            host,
            monitor,
            lab::Guest::connect_local_authorized(key.clone(), invitation, limits(), &ticket),
        )
        .await;
        self.guest = Some(Arc::new(result.map_err(|_| ConnectError::TransportFailed)?));
        self.sockets = self.guest.as_ref().unwrap().bound_sockets();
        if let Some(error) = interrupted {
            return Err(error);
        }
        run.current()?;
        let (interrupted, result) = initialize(
            run,
            host,
            monitor,
            self.guest
                .as_ref()
                .unwrap()
                .tcp_bridge("game", 1, "127.0.0.1:0".parse().unwrap()),
        )
        .await;
        self.tcp = Some(Arc::new(result.map_err(|_| ConnectError::PortUnavailable)?));
        self.tcp_address = Some(self.tcp.as_ref().unwrap().local_addr());
        if let Some(error) = interrupted {
            return Err(error);
        }
        run.current()?;
        let (interrupted, result) = initialize(
            run,
            host,
            monitor,
            self.guest.as_ref().unwrap().udp_bridge_fragmented(
                "aux",
                1,
                "127.0.0.1:35554".parse().unwrap(),
            ),
        )
        .await;
        self.udp = Some(result.map_err(|_| ConnectError::PortUnavailable)?);
        self.sockets.push(self.udp.as_ref().unwrap().local_addr());
        if let Some(error) = interrupted {
            return Err(error);
        }
        run.current()?;
        if self
            .udp
            .as_ref()
            .unwrap()
            .max_payload()
            .is_none_or(|size| size < 4096)
        {
            return Err(ConnectError::TransportFailed);
        }
        let authority = ready_source::HostPlayerLaunchAuthority::new(
            run,
            policy,
            monitor.source().clone(),
            host,
            invitation,
            network_ticket,
        )?;
        let source = ready_game::GuestSource::social_host(
            self.guest.as_ref().unwrap(),
            self.tcp.as_ref().unwrap(),
            &authority,
            run.deadline,
            &run.cancel,
        );
        self.authority = Some(authority);
        update(&run.state, &run.id, |a| {
            a.guest_source = Some(source);
            a.session_deadline = Some(run.deadline);
            a.snapshot.expires_at_ms = Some(policy.body().expires_at_unix_ms().min(run.expiry));
            a.snapshot.tcp_address = self.tcp_address.map(|address| address.to_string());
            a.snapshot.transport_state = "authenticated";
            if let Some(social) = &mut a.snapshot.social {
                social.host_player_voice_state = Some("enabled");
            }
        });
        Ok(())
    }
    pub(super) fn refresh_poll(&mut self, run: &Run, response: &Value) -> Result<(), ConnectError> {
        let Some(authority) = &self.authority else {
            return Ok(());
        };
        let checked = response["checkedAtUnixMs"]
            .as_u64()
            .ok_or(ConnectError::InvitationRejected)?;
        authority.refresh_host_poll(checked)?;
        self.seen_poll = true;
        update(&run.state, &run.id, |a| {
            if let Some(social) = &mut a.snapshot.social {
                social.ready_game_launch_available = Some(true);
            }
        });
        Ok(())
    }
    pub(super) fn invalidate(&self, run: &Run) {
        if let Some(authority) = &self.authority {
            authority.close();
        }
        update(&run.state, &run.id, |a| {
            a.guest_source = None;
            ready_game::cancel(a);
            if let Some(social) = &mut a.snapshot.social {
                social.ready_game_launch_available = None;
            }
        });
    }
    pub(super) async fn tick(&mut self, run: &Run) -> Result<(), ConnectError> {
        if self.seen_poll
            && self
                .authority
                .as_ref()
                .is_some_and(|authority| authority.with_current(|| Ok(())).is_err())
        {
            return Err(ConnectError::PlanExpired);
        }

        if self
            .guest
            .as_ref()
            .is_some_and(|guest| guest.connection().close_reason().is_some())
        {
            return Err(ConnectError::TransportFailed);
        }
        if self.udp.as_ref().is_some_and(|bridge| {
            bridge.is_closed() || bridge.max_payload().is_none_or(|size| size < 4096)
        }) {
            if let Some(bridge) = self.udp.take() {
                bridge.shutdown().await;
            }
            update(&run.state, &run.id, |a| {
                if let Some(social) = &mut a.snapshot.social {
                    social.host_player_voice_state = Some("retired");
                }
            });
        }
        Ok(())
    }
    pub(super) async fn shutdown(&mut self, run: &Run) -> Result<(), ConnectError> {
        self.invalidate(run);
        if let Some(guest) = self.guest.take() {
            guest.shutdown().await;
        }
        if let Some(bridge) = self.udp.take() {
            bridge.shutdown().await;
        }
        if let Some(bridge) = self.tcp.take() {
            bridge.shutdown().await;
        }
        update(&run.state, &run.id, |a| {
            a.snapshot.tcp_address = None;
            if let Some(social) = &mut a.snapshot.social {
                if social.host_player == Some(true) {
                    social.host_player_voice_state = Some("closed");
                }
            }
        });
        release(&self.sockets, self.tcp_address.take()).await
    }
}

#[cfg(all(test, debug_assertions))]
#[path = "social_online_voice_tests.rs"]
mod tests;
