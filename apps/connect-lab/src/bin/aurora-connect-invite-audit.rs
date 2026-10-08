//! Opt-in invitation audit, separate from the existing lab CLI and desktop.
//! Private keys and the approved invitation remain in this process's RAM only.
use aurora_connect_lab as lab;
use serde_json::json;
use std::{
    io::{BufRead, Write},
    net::SocketAddr,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

type Result<T = ()> = std::result::Result<T, &'static str>;
const LINE_LIMIT: usize = lab::MAX_INVITATION_BYTES + 5; // BOM and CRLF framing.
const COMMAND_LIMIT: usize = 32;

#[derive(Clone, Copy)]
enum Mode {
    Loopback,
    ExternalIpv6,
    ExternalIpv6Interactive,
}
impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Loopback => "loopback",
            Self::ExternalIpv6 | Self::ExternalIpv6Interactive => "externalIpv6",
        }
    }
    fn preparation_timeout(self) -> Duration {
        Duration::from_secs(match self {
            Self::ExternalIpv6Interactive => 600,
            Self::Loopback | Self::ExternalIpv6 => 60,
        })
    }
    fn address(self, address: SocketAddr) -> Result {
        match self {
            Self::Loopback
                if address.ip().is_loopback()
                    && address.port() != 0
                    && !matches!(address,SocketAddr::V6(v6) if v6.scope_id()!=0 || v6.flowinfo()!=0) =>
            {
                Ok(())
            }
            Self::ExternalIpv6 | Self::ExternalIpv6Interactive => {
                lab::validate_external_invitation_address(address)
                    .map_err(|_| "invalid_configuration")
            }
            _ => Err("invalid_configuration"),
        }
    }
    fn verify(
        self,
        text: &str,
        host: lab::EndpointId,
        guest: lab::EndpointId,
    ) -> lab::Result<lab::Invitation> {
        match self {
            Self::Loopback => lab::Invitation::verify(text, host, guest),
            Self::ExternalIpv6 | Self::ExternalIpv6Interactive => {
                lab::Invitation::verify_external_ipv6(text, host, guest)
            }
        }
    }
    async fn connect(
        self,
        key: lab::SecretKey,
        invitation: &lab::Invitation,
    ) -> lab::Result<lab::Guest> {
        match self {
            Self::Loopback => {
                lab::Guest::connect_invited(key, invitation, lab::Limits::default()).await
            }
            Self::ExternalIpv6 | Self::ExternalIpv6Interactive => {
                lab::Guest::connect_invited_external_ipv6(key, invitation, lab::Limits::default())
                    .await
            }
        }
    }
}
fn event(value: serde_json::Value) {
    println!("{value}");
    let _ = std::io::stdout().flush();
}
fn native(error: lab::Error) -> &'static str {
    match error {
        lab::Error::Configuration => "invalid_configuration",
        lab::Error::Transport => "transport_failed",
        lab::Error::Timeout => "timed_out",
        lab::Error::Unauthorized => "unauthorized",
        lab::Error::Expired => "expired_or_revoked",
        lab::Error::ServiceDenied => "service_not_authorized",
        lab::Error::CapabilityUnavailable => "capability_unavailable",
        lab::Error::StaleRevision => "stale_service_revision",
        lab::Error::Protocol => "invalid_protocol",
        lab::Error::Replay => "replayed_request",
        lab::Error::Limit => "resource_limit",
        lab::Error::Destination => "destination_unavailable",
        lab::Error::DatagramTooLarge => "datagram_too_large",
        lab::Error::Closed => "closed",
        lab::Error::Io => "local_io_failed",
    }
}
fn frame(mut bytes: Vec<u8>) -> Result<String> {
    if bytes.is_empty() || bytes.len() > LINE_LIMIT || bytes.last() != Some(&b'\n') {
        return Err("invalid_input");
    }
    bytes.pop();
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    let mut line = String::from_utf8(bytes).map_err(|_| "invalid_input")?;
    if line.starts_with('\u{feff}') {
        line.remove(0);
    }
    if line.len() > lab::MAX_INVITATION_BYTES || line.contains(['\r', '\n', '\0']) {
        return Err("invalid_input");
    }
    Ok(line)
}
struct Input {
    lines: mpsc::Receiver<Result<String>>,
}
impl Input {
    fn new() -> Self {
        let (send, lines) = mpsc::channel(1);
        // Exactly one bounded reader, never a Tokio blocking task. A terminal
        // read cannot be cancelled on all platforms; process exit ends it.
        std::thread::spawn(move || {
            let mut stdin = std::io::stdin().lock();
            loop {
                let mut bytes = Vec::new();
                let count = std::io::Read::take(&mut stdin, (LINE_LIMIT + 1) as u64)
                    .read_until(b'\n', &mut bytes);
                match count {
                    Ok(0) => break,
                    Err(_) => {
                        let _ = send.blocking_send(Err("local_io_failed"));
                        break;
                    }
                    _ => {}
                }
                let result = frame(bytes);
                let invalid = result.is_err();
                if send.blocking_send(result).is_err() || invalid {
                    break;
                }
            }
        });
        Self { lines }
    }
    async fn prompt(&mut self, limit: Instant) -> Result<String> {
        // A buffered line cannot win a select race against an expired prompt.
        // Recheck after wake-up as well, before interpreting any input.
        if Instant::now() >= limit {
            return Err("timed_out");
        }
        let result = tokio::select! {
            line=self.lines.recv()=>line.ok_or("input_closed")?,
            _=tokio::time::sleep_until(limit.into())=>Err("timed_out"),
            _=tokio::signal::ctrl_c()=>Err("cancelled"),
        };
        if Instant::now() >= limit {
            return Err("timed_out");
        }
        result
    }
}
fn prompt_deadline(mode: Mode) -> Instant {
    Instant::now() + mode.preparation_timeout()
}
fn invitation_deadline(invitation: &lab::Invitation) -> Result<Instant> {
    let remaining = invitation
        .expires_at_unix_ms()
        .saturating_sub(lab::unix_millis());
    if remaining == 0 || remaining > 600_000 {
        return Err("expired_or_revoked");
    }
    Ok(Instant::now() + Duration::from_millis(remaining))
}
fn peer(text: &str) -> Result<lab::EndpointId> {
    if text.len() != 64
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("invalid_configuration");
    }
    text.parse().map_err(|_| "invalid_configuration")
}
fn listener(text: &str) -> Result<SocketAddr> {
    let addr: SocketAddr = text.parse().map_err(|_| "invalid_configuration")?;
    if addr.to_string() != text
        || !addr.ip().is_loopback()
        || matches!(addr,SocketAddr::V6(v6) if v6.scope_id()!=0 || v6.flowinfo()!=0)
    {
        return Err("invalid_configuration");
    }
    Ok(addr)
}
fn game_only(invitation: &lab::Invitation) -> Result {
    let lease = invitation.lease();
    if lease.services.len() != 1
        || !lease.services.contains("game")
        || !game_projection(&invitation.preview())
    {
        return Err("service_not_authorized");
    }
    Ok(())
}
fn game_projection(preview: &serde_json::Value) -> bool {
    preview["services"].as_array().is_some_and(|services| {
        services.len() == 1
            && services[0]["serviceId"] == "game"
            && services[0]["protocol"] == "tcp"
            && services[0]["revision"] == 1
            && services[0]["requiredCapability"] == "reliableStream"
    })
}
async fn approve(
    input: &mut Input,
    invitation: &lab::Invitation,
    mode: Mode,
    role: &str,
    host: lab::EndpointId,
    guest: lab::EndpointId,
    addresses: (Option<SocketAddr>, Option<SocketAddr>),
) -> Result {
    let limit = prompt_deadline(mode).min(invitation_deadline(invitation)?);
    let input_expires = (lab::unix_millis() + mode.preparation_timeout().as_millis() as u64)
        .min(invitation.expires_at_unix_ms());
    // This intentional local review is not a shareable diagnostic event. It
    // omits the invitation nonce/session and all private-key material.
    event(
        json!({"event":"audit_review","role":role,"mode":mode.name(),"digest":invitation.digest(),
        "hostPublicId":host.to_string(),"guestPublicId":guest.to_string(),"directAddress":invitation.direct_addr().to_string(),
        "gameTarget":addresses.0.map(|a|a.to_string()),"localTcpBind":addresses.1.map(|a|a.to_string()),"services":["game"],"singleAdmission":true,
        "expiresAtMs":invitation.expires_at_unix_ms(),"inputExpiresAtMs":input_expires,
        "preparationTimeoutSeconds":mode.preparation_timeout().as_secs(),"next":"approve <digest>"}),
    );
    if input.prompt(limit).await? != format!("approve {}", invitation.digest()) {
        return Err("approval_mismatch");
    }
    Ok(())
}
fn rebind_udp(addresses: &[SocketAddr]) -> bool {
    let held: Vec<_> = addresses.iter().map(std::net::UdpSocket::bind).collect();
    held.iter().all(std::result::Result::is_ok)
}
async fn host(args: &[String], mode: Mode) -> Result {
    if args.len() != 2 {
        return Err("invalid_arguments");
    }
    let plan = lab::load_plan(&args[0], Some(&args[1])).map_err(|e| e.code())?;
    if plan.seconds > 600
        || plan.services.len() != 1
        || plan.services[0].service_id != "game"
        || plan.services[0].protocol != lab::Protocol::Tcp
    {
        return Err("invalid_configuration");
    }
    mode.address(plan.bind)?;
    let mut input = Input::new();
    let key = lab::generate_key();
    let host_id = key.public();
    let input_deadline = prompt_deadline(mode);
    let seconds = mode.preparation_timeout().as_secs();
    event(
        json!({"event":"audit_host_identity","peer":host_id.to_string(),
        "preparationTimeoutSeconds":seconds,"inputExpiresAtMs":lab::unix_millis()+seconds*1000,
        "next":format!("guest public ID within {seconds} seconds")}),
    );
    let guest = peer(&input.prompt(input_deadline).await?)?;
    let expires = lab::unix_millis() + plan.seconds * 1000;
    let session = host_id.to_string();
    let invitation = match mode {
        Mode::Loopback => {
            lab::Invitation::issue(&key, guest, &session, plan.bind, &plan.services, expires)
        }
        Mode::ExternalIpv6 | Mode::ExternalIpv6Interactive => lab::Invitation::issue_external_ipv6(
            &key,
            guest,
            &session,
            plan.bind,
            &plan.services,
            expires,
        ),
    }
    .map_err(native)?;
    let deadline = invitation_deadline(&invitation)?;
    approve(
        &mut input,
        &invitation,
        mode,
        "host",
        host_id,
        guest,
        (Some(plan.services[0].endpoint), None),
    )
    .await?;
    let lease_id = invitation.lease().lease_id;
    let config = lab::HostConfig {
        secret_key: key,
        bind_addr: plan.bind,
        session_id: session,
        services: plan.services,
        leases: vec![invitation.lease()],
        limits: lab::Limits::default(),
    };
    let host = match mode {
        Mode::Loopback => lab::Host::start_invited(config, &invitation).await,
        Mode::ExternalIpv6 | Mode::ExternalIpv6Interactive => {
            lab::Host::start_invited_external_ipv6(config, &invitation).await
        }
    }
    .map_err(native)?;
    let bound = host.bound_sockets();
    event(
        json!({"event":"audit_invitation","invitation":invitation.encode(),"digest":invitation.digest()}),
    );
    event(json!({"event":"audit_host_ready","relay":false,"next":"stats | revoke | stop"}));
    let mut revoked = false;
    let mut reason = "stop";
    let result:Result=async {
        for index in 0..COMMAND_LIMIT {
            let command=tokio::select! {
                line=input.lines.recv()=>match line {Some(value)=>value?,None=>{reason="input_closed";break;}},
                _=tokio::time::sleep_until(deadline.into())=>{reason="lease_expired";break;},
                _=tokio::signal::ctrl_c()=>{reason="cancelled";break;},
            };
            if command=="stop" {break;}
            if command=="revoke" && !revoked {
                revoked=host.revoke(&lease_id);
                if !revoked {return Err("revocation_failed");}
                event(json!({"event":"audit_revoked","endpointRetained":true,"metrics":host.metrics()}));
            } else if command=="stats" {event(json!({"event":"audit_host_stats","revoked":revoked,"metrics":host.metrics()}));}
            else {return Err("invalid_command");}
            if index+1==COMMAND_LIMIT {reason="command_limit";return Err("command_limit");}
        }
        Ok(())
    }.await;
    host.revoke(&lease_id);
    host.shutdown().await;
    let metrics = host.metrics();
    drop(host);
    let rebound = rebind_udp(&bound);
    event(
        json!({"event":"audit_host_stopped","reason":reason,"rebound":rebound,"metrics":metrics}),
    );
    if !rebound {
        return Err("cleanup_failed");
    }
    result
}
struct Active {
    guest: lab::Guest,
    bridge: lab::TcpBridge,
    transport: Vec<SocketAddr>,
    listener: SocketAddr,
}
impl Active {
    async fn close(self) -> Result {
        let Self {
            guest,
            bridge,
            transport,
            listener,
        } = self;
        guest.shutdown().await;
        bridge.shutdown().await;
        drop(bridge);
        drop(guest);
        let tcp = std::net::TcpListener::bind(listener);
        let udp = rebind_udp(&transport);
        let rebound = tcp.is_ok() && udp;
        event(
            json!({"event":"audit_guest_disconnected","rebound":rebound,"transportSockets":transport.len(),"tcpListeners":1}),
        );
        if rebound {
            Ok(())
        } else {
            Err("cleanup_failed")
        }
    }
}
async fn bare_cleanup(guest: lab::Guest) -> Result {
    let bound = guest.bound_sockets();
    guest.shutdown().await;
    drop(guest);
    if rebind_udp(&bound) {
        Ok(())
    } else {
        Err("cleanup_failed")
    }
}
// Cancellation never drops the native dial future. Await its bounded result,
// then close any late success before allowing a bridge or ready event.
async fn attempt(
    input: &mut Input,
    mode: Mode,
    key: lab::SecretKey,
    invitation: &lab::Invitation,
    deadline: Instant,
) -> Result<Option<lab::Result<lab::Guest>>> {
    let connect = mode.connect(key, invitation);
    tokio::pin!(connect);
    let cancellation = tokio::select! {
        result=&mut connect=>return Ok(Some(result)),
        line=input.lines.recv()=>match line {None=>Ok(()),Some(Ok(line)) if line=="stop"=>Ok(()),_=>Err("invalid_command")},
        _=tokio::signal::ctrl_c()=>Ok(()),
        _=tokio::time::sleep_until(deadline.into())=>Err("expired_or_revoked"),
    };
    if let Ok(guest) = connect.await {
        bare_cleanup(guest).await?;
    }
    cancellation?;
    Ok(None)
}
async fn guest(args: &[String], mode: Mode) -> Result {
    if args.len() != 2 {
        return Err("invalid_arguments");
    }
    let host_id = peer(&args[0])?;
    let bind = listener(&args[1])?;
    let key = lab::generate_key();
    let guest_id = key.public();
    let mut input = Input::new();
    let input_deadline = prompt_deadline(mode);
    let seconds = mode.preparation_timeout().as_secs();
    event(
        json!({"event":"audit_guest_identity","peer":guest_id.to_string(),
        "preparationTimeoutSeconds":seconds,"inputExpiresAtMs":lab::unix_millis()+seconds*1000,
        "next":format!("invitation within {seconds} seconds")}),
    );
    let text = input.prompt(input_deadline).await?;
    let invitation = mode.verify(&text, host_id, guest_id).map_err(native)?;
    drop(text);
    game_only(&invitation)?;
    let deadline = invitation_deadline(&invitation)?;
    approve(
        &mut input,
        &invitation,
        mode,
        "guest",
        host_id,
        guest_id,
        (None, Some(bind)),
    )
    .await?;
    let connected = attempt(&mut input, mode, key.clone(), &invitation, deadline).await?;
    let Some(connected) = connected else {
        event(
            json!({"event":"audit_guest_stopped","reason":"cancelled_before_ready","sameIdentity":true}),
        );
        return Ok(());
    };
    let guest = connected.map_err(native)?;
    if Instant::now() >= deadline {
        bare_cleanup(guest).await?;
        return Err("expired_or_revoked");
    }
    let bridge = match guest.tcp_bridge("game", 1, bind).await {
        Ok(value) => value,
        Err(error) => {
            bare_cleanup(guest).await?;
            return Err(native(error));
        }
    };
    let local = bridge.local_addr();
    let transport = guest.bound_sockets();
    let mut active = Some(Active {
        guest,
        bridge,
        transport,
        listener: local,
    });
    event(
        json!({"event":"audit_guest_ready","tcp":local.to_string(),"sameIdentity":true,"next":"disconnect | retry-replay | retry-revoked | stop"}),
    );
    let mut reason = "stop";
    let mut commands = 0usize;
    let mut retries = 0usize;
    let result:Result=async {
        loop {
            // The extra observation window does not extend the lease. No
            // new dial is permitted after its original deadline.
            let closure=async {if let Some(value)=&active {value.guest.connection().closed().await;}else{std::future::pending::<()>().await;}};
            let command=tokio::select! {
                line=input.lines.recv()=>match line {Some(value)=>Some(value?),None=>{reason="input_closed";break;}},
                _=closure=>None,
                _=tokio::time::sleep_until((deadline+Duration::from_secs(3)).into())=>{
                    if active.is_some() {return Err("expiry_close_not_observed");}
                    reason="lease_expired";break;
                },
                _=tokio::signal::ctrl_c()=>{reason="cancelled";break;},
            };
            if command.is_none() {
                let closed_at=lab::unix_millis();
                if let Some(value)=active.take() {value.close().await?;}
                let expired=closed_at>=invitation.expires_at_unix_ms();
                event(json!({"event":"audit_connection_closed","deadlineReached":expired,"sameIdentity":true}));
                if expired {
                    let local_check=mode.verify(&invitation.encode(),host_id,guest_id);
                    if !matches!(local_check,Err(lab::Error::Expired)) {return Err("expiry_verification_failed");}
                    event(json!({"event":"audit_expiration_checked","code":"expired_or_revoked","source":"local_verification_after_observed_close"}));
                    reason="lease_expired";break;
                }
                continue;
            }
            let command=command.ok_or("invalid_command")?;commands+=1;
            if commands>COMMAND_LIMIT {return Err("command_limit");}
            if command=="stop" {break;}
            if command=="disconnect" {
                let value=active.take().ok_or("not_connected")?;value.close().await?;continue;
            }
            let expected=match command.as_str() {"retry-replay"=>lab::Error::Replay,"retry-revoked"=>lab::Error::Expired,_=>return Err("invalid_command")};
            if active.is_some() || retries>=4 {return Err("retry_not_permitted");}
            // A still-valid signed invitation is required before a negative
            // remote trial. Local expiry must never masquerade as revocation.
            mode.verify(&invitation.encode(),host_id,guest_id).map_err(native)?;
            if deadline.saturating_duration_since(Instant::now())<Duration::from_secs(1)
                || invitation.expires_at_unix_ms().saturating_sub(lab::unix_millis())<1000 {return Err("trial_too_close_to_expiry");}
            retries+=1;
            let tried=attempt(&mut input,mode,key.clone(),&invitation,deadline).await?;
            let Some(tried)=tried else {reason="cancelled";break;};
            let observed=match tried {Ok(unexpected)=>{bare_cleanup(unexpected).await?;return Err("unexpected_admission");},Err(error)=>error};
            // If expiry raced the dial, its refusal cannot prove revocation.
            if deadline.saturating_duration_since(Instant::now())<Duration::from_secs(1)
                || invitation.expires_at_unix_ms().saturating_sub(lab::unix_millis())<1000 {return Err("trial_crossed_expiry");}
            event(json!({"event":"audit_retry_result","case":if expected==lab::Error::Replay{"replay"}else{"revoked"},"code":native(observed),"passed":observed==expected,"sameIdentity":true,"source":"native_guest_error","requiresHostDenialCounter":true}));
            if observed!=expected {return Err("unexpected_rejection");}
        }
        Ok(())
    }.await;
    let cleanup = if let Some(value) = active.take() {
        value.close().await
    } else {
        Ok(())
    };
    event(
        json!({"event":"audit_guest_stopped","reason":reason,"sameIdentity":true,"retries":retries,"cleanup":cleanup.is_ok()}),
    );
    cleanup?;
    result
}
async fn run() -> Result {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("host") => host(&args[1..], Mode::Loopback).await,
        Some("host-external-ipv6") => host(&args[1..], Mode::ExternalIpv6).await,
        Some("host-external-ipv6-interactive") => {
            host(&args[1..], Mode::ExternalIpv6Interactive).await
        }
        Some("guest") => guest(&args[1..], Mode::Loopback).await,
        Some("guest-external-ipv6") => guest(&args[1..], Mode::ExternalIpv6).await,
        Some("guest-external-ipv6-interactive") => {
            guest(&args[1..], Mode::ExternalIpv6Interactive).await
        }
        _ => Err("invalid_arguments"),
    }
}
#[tokio::main]
async fn main() {
    if let Err(code) = run().await {
        event(json!({"event":"audit_failed","code":code}));
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interactive_preparation_preserves_default_authentication_and_lease_limits() {
        assert_eq!(
            Mode::Loopback.preparation_timeout(),
            Duration::from_secs(60)
        );
        assert_eq!(
            Mode::ExternalIpv6.preparation_timeout(),
            Duration::from_secs(60)
        );
        assert_eq!(
            Mode::ExternalIpv6Interactive.preparation_timeout(),
            Duration::from_secs(600)
        );
        assert_eq!(
            lab::Limits::default().handshake_timeout,
            Duration::from_secs(10)
        );
        assert_eq!(lab::MAX_INVITATION_LIFETIME, Duration::from_secs(600));
        assert!(
            Mode::ExternalIpv6Interactive
                .address("127.0.0.1:42000".parse().unwrap())
                .is_err()
        );
    }
    #[tokio::test]
    async fn expired_prompt_refuses_buffered_input_without_consuming_it() {
        let (send, lines) = mpsc::channel(1);
        send.send(Ok("approve fixture".into())).await.unwrap();
        let mut input = Input { lines };
        assert_eq!(input.prompt(Instant::now()).await, Err("timed_out"));
        assert_eq!(input.lines.try_recv().unwrap().unwrap(), "approve fixture");
        let deadline = Instant::now() + Duration::from_millis(5);
        assert_eq!(input.prompt(deadline).await, Err("timed_out"));
        send.send(Ok("stop".into())).await.unwrap();
        assert_eq!(input.prompt(deadline).await, Err("timed_out"));
        assert_eq!(input.lines.try_recv().unwrap().unwrap(), "stop");
    }
    #[test]
    fn game_projection_requires_the_single_exact_tcp_contract() {
        let valid = json!({"services":[{"serviceId":"game","protocol":"tcp","revision":1,"requiredCapability":"reliableStream"}]});
        assert!(game_projection(&valid));
        for (field, value) in [
            ("serviceId", json!("aux")),
            ("protocol", json!("udp")),
            ("revision", json!(2)),
            ("requiredCapability", json!("datagram")),
        ] {
            let mut changed = valid.clone();
            changed["services"][0][field] = value;
            assert!(
                !game_projection(&changed),
                "different service grant refused"
            );
        }
        let mut extra = valid.clone();
        extra["services"]
            .as_array_mut()
            .expect("fixture array")
            .push(valid["services"][0].clone());
        assert!(!game_projection(&extra));
        assert!(!game_projection(&json!({"services":[]})));
    }
    #[test]
    fn bounded_input_and_explicit_modes_reject_before_io() {
        assert!(frame(b"stop\r\n".to_vec()).is_ok());
        assert!(frame("\u{feff}stop\n".as_bytes().to_vec()).is_ok());
        for bytes in [
            vec![b'a'; LINE_LIMIT + 1],
            b"stop".to_vec(),
            b"stop\0\n".to_vec(),
            vec![0xff, b'\n'],
        ] {
            assert!(frame(bytes).is_err());
        }
        assert!(
            Mode::Loopback
                .address("[2003::1]:42000".parse().unwrap())
                .is_err()
        );
        assert!(
            Mode::ExternalIpv6
                .address("127.0.0.1:42000".parse().unwrap())
                .is_err()
        );
        assert!(listener("0.0.0.0:0").is_err());
        assert!(listener("127.0.0.1:0").is_ok());
        assert!(peer(&"A".repeat(64)).is_err());
    }
}
