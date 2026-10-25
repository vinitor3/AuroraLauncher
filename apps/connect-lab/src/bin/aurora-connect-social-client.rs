//! Explicit two-process local fixture. Pins/context come from its trusted
//! parent runner; no renderer configuration, account token, or password input.
use aurora_connect_lab as lab;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::{BufRead, Read, Write},
    net::{SocketAddr, UdpSocket},
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
type Result<T> = std::result::Result<T, &'static str>;

struct Input {
    rx: mpsc::Receiver<Result<String>>,
    reader: Option<std::thread::JoinHandle<()>>,
}
impl Input {
    fn new() -> Self {
        let (send, rx) = mpsc::channel(8);
        let reader = std::thread::spawn(move || {
            let stdin = std::io::stdin();
            let mut input = stdin.lock();
            loop {
                let mut line = Vec::new();
                let length = match input
                    .by_ref()
                    .take((lab::MAX_SOCIAL_ADMISSION_BYTES + 3) as u64)
                    .read_until(b'\n', &mut line)
                {
                    Ok(n) => n,
                    Err(_) => {
                        let _ = send.blocking_send(Err("input_failed"));
                        break;
                    }
                };
                if length == 0 {
                    break;
                }
                if line.last() == Some(&b'\n') {
                    line.pop();
                }
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                if line.len() > lab::MAX_SOCIAL_ADMISSION_BYTES {
                    let _ = send.blocking_send(Err("input_too_large"));
                    break;
                }
                let line = String::from_utf8(line).map_err(|_| "input_invalid_utf8");
                let failed = line.is_err();
                if send.blocking_send(line).is_err() || failed {
                    break;
                }
            }
        });
        Self {
            rx,
            reader: Some(reader),
        }
    }
    async fn next(&mut self, deadline: Instant) -> Result<String> {
        tokio::select! {biased;
            _=tokio::time::sleep_until(deadline.into())=>Err("preparation_expired"),
            _=tokio::signal::ctrl_c()=>Err("cancelled"),
            value=self.rx.recv()=>{
                let value=value.ok_or("input_eof")??;
                if value=="stop"{Err("stopped")}else{Ok(value)}
            },
        }
    }
}
impl Drop for Input {
    fn drop(&mut self) {
        self.rx.close();
        // One process-scoped reader, never one detached thread per command.
        // EOF from the owned runner lets us join it. On console stop/timeout a
        // blocked OS stdin read ends with this process, after network cleanup.
        if let Some(reader) = self.reader.take()
            && reader.is_finished()
        {
            let _ = reader.join();
        }
    }
}
fn event(value: Value) -> Result<()> {
    let mut output = std::io::stdout().lock();
    writeln!(output, "{value}")
        .and_then(|_| output.flush())
        .map_err(|_| "output_failed")
}
fn parse<T: for<'a> Deserialize<'a>>(line: &str) -> Result<T> {
    if line.len() > lab::MAX_SOCIAL_ADMISSION_BYTES {
        return Err("packet_too_large");
    }
    serde_json::from_str(line).map_err(|_| "invalid_packet")
}
fn public(text: &str) -> Result<lab::EndpointId> {
    let key: lab::EndpointId = text.parse().map_err(|_| "invalid_authority_pin")?;
    if key.to_string() != text {
        return Err("invalid_authority_pin");
    }
    Ok(key)
}
fn valid_uid(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 128
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn valid_uuid(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() == 36
        && b[14] == b'4'
        && matches!(b[19], b'8' | b'9' | b'a' | b'b')
        && b.iter().enumerate().all(|(i, &v)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                v == b'-'
            } else {
                v.is_ascii_digit() || (b'a'..=b'f').contains(&v)
            }
        })
}
fn operation_id() -> String {
    // Independent cryptographic randomness, never bytes of the transport key.
    let mut random = lab::generate_key().to_bytes();
    random[6] = (random[6] & 15) | 64;
    random[8] = (random[8] & 63) | 128;
    let s: String = random[..16].iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &s[..8],
        &s[8..12],
        &s[12..16],
        &s[16..20],
        &s[20..]
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectPacket {
    expect: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HostPacket {
    grant: Value,
    lease: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GuestPacket {
    grant: Value,
    lease: Value,
    invitation: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LeasePacket {
    lease: Value,
}
struct Context {
    key: lab::SecretKey,
    pin: lab::EndpointId,
    challenge: lab::SocialKeyChallenge,
    deadline: Instant,
    source: Option<SocketAddr>,
    expected: Option<lab::SocialGuestExpectation>,
}
async fn prepare(input: &mut Input) -> Result<Context> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let (host, pin, source, session, account) = match args.as_slice() {
        [role, pin, source, session, account] if role == "host" => {
            let address: SocketAddr = source.parse().map_err(|_| "invalid_source")?;
            if address.to_string() != *source {
                return Err("invalid_source");
            }
            lab::social_source_digest(address).map_err(|_| "invalid_source")?;
            (true, public(pin)?, Some(address), session, account)
        }
        [role, pin, session, account] if role == "guest" => {
            (false, public(pin)?, None, session, account)
        }
        _ => return Err("invalid_arguments"),
    };
    if !valid_uuid(session) || !valid_uid(account) {
        return Err("invalid_context");
    }
    let at = Instant::now();
    let deadline = at + Duration::from_secs(60);
    let expiry = lab::unix_millis() + 60_000;
    let key = lab::generate_key();
    let operation = operation_id();
    let role = if host { "host" } else { "guest" };
    event(
        json!({"event":"social_identity","role":role,"publicKey":key.public().to_string(),"operationId":operation,"generation":1,"preparedExpiresAtUnixMs":expiry}),
    )?;
    let expected = if host {
        None
    } else {
        let packet: ExpectPacket = parse(&input.next(deadline).await?)?;
        Some(
            lab::SocialGuestExpectation::parse(&packet.expect.to_string())
                .map_err(|_| "invalid_expectation")?,
        )
    };
    let challenge = lab::SocialKeyChallenge::parse(&input.next(deadline).await?)
        .map_err(|_| "invalid_challenge")?;
    if challenge.role().as_str() != role
        || challenge.operation_id() != operation
        || challenge.generation() != 1
        || challenge.public_key() != key.public().to_string()
        || challenge.account_id() != account
        || challenge.session_id() != session
        || challenge.expires_at_unix_ms() > expiry
        || expected
            .as_ref()
            .is_some_and(|v| v.session_digest() != challenge.session_digest())
        || Instant::now() >= deadline
    {
        return Err("challenge_context");
    }
    let proof = challenge.sign(&key).map_err(|_| "proof_rejected")?;
    event(serde_json::to_value(proof).map_err(|_| "proof_encoding")?)?;
    Ok(Context {
        key,
        pin,
        challenge,
        deadline,
        source,
        expected,
    })
}
fn limits() -> lab::Limits {
    lab::Limits {
        handshake_timeout: Duration::from_secs(2),
        ..lab::Limits::default()
    }
}
fn sockets_free(sockets: Vec<SocketAddr>) -> bool {
    sockets.into_iter().all(|s| UdpSocket::bind(s).is_ok())
}
async fn host(input: &mut Input, context: Context) -> Result<()> {
    let source = context.source.ok_or("invalid_source")?;
    let visibility = input.next(context.deadline).await?;
    let policy = lab::SocialHostPolicy::review(&context.challenge, &visibility, source)
        .map_err(|_| "policy_rejected")?;
    let digest = policy.digest();
    event(
        json!({"event":"social_host_review","policy":policy,"digest":digest,"source":source.to_string()}),
    )?;
    let approval = input.next(context.deadline).await?;
    if approval != format!("approve {digest}") {
        return Err("approval_mismatch");
    }
    let policy = policy
        .approve(&context.key, &digest, context.deadline)
        .map_err(|_| "policy_approval_rejected")?;
    event(json!({"event":"social_host_policy","policy":policy}))?;
    let packet: HostPacket = parse(&input.next(context.deadline).await?)?;
    let grant = lab::VerifiedSocialGrant::for_host(
        &packet.grant.to_string(),
        context.pin,
        &context.challenge,
        &policy,
        source,
        context.deadline,
    )
    .map_err(|_| "grant_rejected")?;
    let mut continuity = lab::SocialContinuity::start(&packet.lease.to_string(), &grant)
        .map_err(|_| "continuity_rejected")?;
    let reservation = UdpSocket::bind("127.0.0.1:0").map_err(|_| "bind_failed")?;
    let bind = reservation.local_addr().map_err(|_| "bind_failed")?;
    let service = lab::Service {
        service_id: "game".into(),
        protocol: lab::Protocol::Tcp,
        endpoint: source,
        revision: 1,
        max_datagram_bytes: 1400,
        required_capability: lab::Capability::ReliableStream,
        udp_return_policy: lab::UdpReturnPolicy::ReplyBudget,
    };
    let invitation = lab::SessionInvitation::issue_loopback(
        &context.key,
        grant.guest_key(),
        grant.session_id(),
        bind,
        std::slice::from_ref(&service),
        lab::SessionDeadlines {
            admission_expires_at_unix_ms: grant.admission_expires_at_unix_ms(),
            session_expires_at_unix_ms: grant.session_expires_at_unix_ms(),
        },
    )
    .map_err(|_| "invitation_rejected")?;
    continuity.check().map_err(|_| "continuity_expired")?;
    grant.check_admission().map_err(|_| "admission_expired")?;
    drop(reservation);
    let starting = lab::Host::start_invited_session(
        lab::HostConfig {
            secret_key: context.key,
            bind_addr: bind,
            session_id: grant.session_id().into(),
            services: vec![service],
            leases: vec![invitation.lease()],
            limits: limits(),
        },
        &invitation,
    );
    tokio::pin!(starting);
    let mut failure = None;
    let mut admitted = false;
    let mut reason = "stop";
    let mut interrupted = false;
    let mut startup_tick = tokio::time::interval(Duration::from_millis(20));
    // Observe the original lease while bind is pending, but retain ownership
    // of the bind future through interruption and close any late endpoint.
    let result = loop {
        tokio::select! { biased;
            _=startup_tick.tick()=>if continuity.check().is_err()||grant.check_admission().is_err(){
                interrupted=true;reason="continuity_expired";break starting.as_mut().await;
            },
            _=tokio::signal::ctrl_c()=>{interrupted=true;reason="cancelled";break starting.as_mut().await;},
            line=input.rx.recv()=>match line {
                Some(Ok(line)) if line=="stop"=>{interrupted=true;break starting.as_mut().await;},
                Some(Ok(line))=>{
                    let update=parse::<LeasePacket>(&line).map_err(|_|"invalid_lease_packet")
                        .and_then(|packet|continuity.update(&packet.lease.to_string()).map_err(|_|"invalid_continuity"));
                    if let Err(error)=update{interrupted=true;failure=Some(error);reason="invalid_continuity";break starting.as_mut().await;}
                },
                Some(Err(error))=>{interrupted=true;failure=Some(error);reason="invalid_input";break starting.as_mut().await;},
                None=>{interrupted=true;reason="eof";break starting.as_mut().await;},
            },
            result=&mut starting=>break result,
        }
    };
    let host = result.map_err(|_| "host_start_failed")?;
    if interrupted||continuity.check().is_err()||grant.check_admission().is_err(){if !interrupted{reason="continuity_expired";}}
    else if event(json!({"event":"social_host_ready","invitation":invitation.encode(),"grantId":grant.grant_id()})).is_err(){failure=Some("output_failed");}
    else{
        let mut tick=tokio::time::interval(Duration::from_millis(20));
        loop{tokio::select!{biased;
            _=tick.tick()=>{
                if continuity.check().is_err(){reason="continuity_expired";break;}
                let admission_current=grant.check_admission().is_ok();
                admitted|=host.invitation_consumed(invitation.invitation_id());
                if !admitted&&!admission_current{reason="admission_expired";break;}
                if admitted&&!host.invitation_active(invitation.invitation_id()){reason="connection_closed";break;}
            },
            _=tokio::signal::ctrl_c()=>{reason="cancelled";break;},
            line=input.rx.recv()=>{match line{
                Some(Ok(line)) if line=="stop"=>break,
                Some(Ok(line))=>{let update=parse::<LeasePacket>(&line).map_err(|_|"invalid_lease_packet")
                    .and_then(|packet|continuity.update(&packet.lease.to_string()).map_err(|_|"invalid_continuity"));
                    if let Err(error)=update{failure=Some(error);reason="invalid_continuity";break;}},
                Some(Err(error))=>{failure=Some(error);reason="invalid_input";break;},
                None=>{reason="eof";break;},
            }},
        }}
    }
    let close_started = Instant::now();
    host.revoke(invitation.invitation_id());
    let sockets = host.bound_sockets();
    host.shutdown().await;
    drop(host);
    let rebound = sockets_free(sockets);
    event(
        json!({"event":"social_host_stopped","reason":reason,"cleanup":rebound,"rebound":rebound,"admitted":admitted,
        "sequence":continuity.sequence(),"cleanupElapsedMs":close_started.elapsed().as_millis()}),
    )?;
    if !rebound {
        return Err("cleanup_failed");
    }
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(())
}
async fn guest(input: &mut Input, context: Context) -> Result<()> {
    let packet: GuestPacket = parse(&input.next(context.deadline).await?)?;
    let expected = context.expected.as_ref().ok_or("missing_expectation")?;
    let grant = lab::VerifiedSocialGrant::for_guest(
        &packet.grant.to_string(),
        context.pin,
        &context.challenge,
        expected,
        context.deadline,
    )
    .map_err(|_| "grant_rejected")?;
    let continuity = lab::SocialContinuity::start(&packet.lease.to_string(), &grant)
        .map_err(|_| "continuity_rejected")?;
    let invitation = lab::SessionInvitation::verify_loopback(
        &packet.invitation,
        grant.host_key(),
        context.key.public(),
    )
    .map_err(|_| "invitation_rejected")?;
    if invitation.session_id() != grant.session_id()
        || invitation.admission_expires_at_unix_ms() != grant.admission_expires_at_unix_ms()
        || invitation.session_expires_at_unix_ms() != grant.session_expires_at_unix_ms()
        || !matches!(invitation.direct_addr(),SocketAddr::V4(v) if v.ip().octets()==[127,0,0,1])
    {
        return Err("invitation_context");
    }
    grant.check_admission().map_err(|_| "admission_expired")?;
    continuity.check().map_err(|_| "continuity_expired")?;
    // Keep ownership of a pending dial through cancellation; do not drop the
    // future that owns its Endpoint. A late success is closed before publishing.
    let connecting = lab::Guest::connect_invited_session(context.key, &invitation, limits());
    tokio::pin!(connecting);
    let mut interrupted = None;
    let mut tick = tokio::time::interval(Duration::from_millis(20));
    let result = loop {
        tokio::select! {biased;
            _=tick.tick()=>if continuity.check().is_err(){interrupted=Some("continuity_expired");break connecting.as_mut().await;},
            _=tokio::signal::ctrl_c()=>{interrupted=Some("cancelled");break connecting.as_mut().await;},
            line=input.rx.recv()=>{interrupted=Some(match line{Some(Ok(line))if line=="stop"=>"stop",None=>"eof",_=>"invalid_input"});break connecting.as_mut().await;},
            result=&mut connecting=>break result,
        }
    };
    let guest = match result {
        Ok(guest) => guest,
        Err(_) => return Err("guest_connect_failed"),
    };
    let sockets = guest.bound_sockets();
    if interrupted.is_some() || continuity.check().is_err() || grant.check_session().is_err() {
        guest.shutdown().await;
        drop(guest);
        let rebound = sockets_free(sockets);
        event(
            json!({"event":"social_guest_stopped","reason":interrupted.unwrap_or("continuity_expired"),"cleanup":rebound,"rebound":rebound}),
        )?;
        return if !rebound {
            Err("cleanup_failed")
        } else if interrupted == Some("invalid_input") {
            Err("invalid_input")
        } else {
            Ok(())
        };
    }
    let bridge = match guest
        .tcp_bridge("game", 1, "127.0.0.1:0".parse().expect("fixed loopback"))
        .await
    {
        Ok(bridge) => bridge,
        Err(_) => {
            guest.shutdown().await;
            drop(guest);
            return Err("bridge_failed");
        }
    };
    let tcp = bridge.local_addr();
    let mut failure = None;
    let reason = if continuity.check().is_err() {
        "continuity_expired"
    } else if grant.check_session().is_err() {
        "session_expired"
    } else if event(
        json!({"event":"social_guest_ready","tcp":tcp.to_string(),"grantId":grant.grant_id()}),
    )
    .is_err()
    {
        failure = Some("output_failed");
        "output_failed"
    } else {
        loop {
            tokio::select! {biased;
                _=tick.tick()=>if grant.check_session().is_err(){break "session_expired";},
                _=guest.connection().closed()=>break "connection_closed",
                _=tokio::signal::ctrl_c()=>break "cancelled",
                line=input.rx.recv()=>match line{
                    Some(Ok(line))if line=="stop"=>break "stop",
                    None=>break "eof",
                    _=>{failure=Some("invalid_input");break "invalid_input";},
                },
            }
        }
    };
    let close_started = Instant::now();
    guest.shutdown().await;
    bridge.shutdown().await;
    drop(bridge);
    drop(guest);
    let rebound = sockets_free(sockets) && std::net::TcpListener::bind(tcp).is_ok();
    event(
        json!({"event":"social_guest_stopped","reason":reason,"cleanup":rebound,"rebound":rebound,"cleanupElapsedMs":close_started.elapsed().as_millis()}),
    )?;
    if !rebound {
        return Err("cleanup_failed");
    }
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(())
}
#[tokio::main]
async fn main() {
    let role = std::env::args()
        .nth(1)
        .filter(|v| matches!(v.as_str(), "host" | "guest"));
    let mut input = Input::new();
    let result = match prepare(&mut input).await {
        Ok(context) if context.source.is_some() => host(&mut input, context).await,
        Ok(context) => guest(&mut input, context).await,
        Err(error) => Err(error),
    };
    drop(input);
    if let Err(code) = result {
        if matches!(
            code,
            "stopped" | "input_eof" | "cancelled" | "preparation_expired"
        ) {
            let name = if role.as_deref() == Some("host") {
                "social_host_stopped"
            } else {
                "social_guest_stopped"
            };
            let reason = match code {
                "stopped" => "stop",
                "input_eof" => "eof",
                other => other,
            };
            let _ = event(json!({"event":name,"reason":reason,"cleanup":true,"rebound":true}));
        } else {
            let _ = event(json!({"event":"social_failed","code":code}));
            std::process::exit(1);
        }
    }
}
