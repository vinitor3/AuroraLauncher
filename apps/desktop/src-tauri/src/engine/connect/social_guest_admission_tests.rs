//! Signed original-authority tests, with the real guest caller stopped at a
//! rejecting relay-ticket endpoint. No Java, fixed game port or UDP is opened.
use super::*;
use serde::Serialize;
use std::sync::atomic::AtomicUsize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClockBody<'a> {
    schema_version: u8,
    issuer_instance: &'a str,
    account_id: &'a str,
    session_id: &'a str,
    operation_id: &'a str,
    generation: u64,
    nonce: &'a str,
    server_received_at_unix_ms: u64,
    server_now_unix_ms: u64,
    session_expires_at_unix_ms: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TicketBody<'a> {
    schema_version: u8,
    session_id: &'a str,
    issuer_instance: &'a str,
    peer_id: String,
    host_peer_id: String,
    account_id: &'a str,
    generation: u64,
    issued_at_unix_ms: u64,
    expires_at_unix_ms: u64,
    direct_addresses: Vec<String>,
}

fn hex_signature(key: &lab::SecretKey, bytes: &[u8]) -> String {
    key.sign(bytes)
        .to_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn sign(key: &lab::SecretKey, domain: &[u8], body: Value, names: &[&str]) -> String {
    let mut bytes = domain.to_vec();
    for name in names {
        let field = body[*name]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| body[*name].to_string());
        bytes.extend_from_slice(&(field.len() as u16).to_be_bytes());
        bytes.extend_from_slice(field.as_bytes());
    }
    json!({"body":body,"signature":hex_signature(key,&bytes)}).to_string()
}

struct Fixture {
    manager: Manager,
    run: Run,
    authority: lab::SecretKey,
    host: lab::SecretKey,
    context: lab::VerifiedSocialJoinContext,
    challenge: lab::SocialKeyChallenge,
    encoded: String,
    issued: u64,
}
impl Fixture {
    fn new(context_ms: u64, attempt_ms: u64) -> Self {
        let authority = lab::generate_key();
        let host = lab::generate_key();
        let now = lab::unix_millis();
        let manager = Manager::with_social_lab(SocialLabAuthority {
            pin: authority.public(),
            issuer: "33".repeat(32),
            online_origin: Some("https://guest-review-fixture.invalid"),
            join_policy: "open".into(),
            session_expiry: now + 60_000,
            max_players: 8,
        });
        let reservation = manager
            .social_reserve(
                "main",
                SocialReserveRequest {
                    account_epoch: 0,
                    account_id: "guestReviewFixture".into(),
                    session_id: uuid::Uuid::new_v4().to_string(),
                    role: "guest".into(),
                    game_target: None,
                    visibility: None,
                },
            )
            .unwrap();
        let mut run = {
            let state = manager.state.lock().unwrap();
            let attempt = state.attempt.as_ref().unwrap();
            let control = attempt.social.as_ref().unwrap();
            Run {
                state: manager.state.clone(),
                id: reservation.operation_id,
                generation: reservation.generation,
                request: control.request.clone(),
                selection: None,
                #[cfg(all(windows, feature = "connect-voice-lab"))]
                native_voice_source: None,
                #[cfg(all(windows, feature = "connect-voice-lab"))]
                native_voice_local_key: None,
                key: control.key.clone().unwrap(),
                authority: manager.social_authority.clone().unwrap(),
                deadline: attempt
                    .prepared_deadline
                    .min(Instant::now() + Duration::from_millis(attempt_ms)),
                expiry: now + 60_000,
                clock: None,
                cancel: attempt.cancel.clone(),
                #[cfg(windows)]
                source_gate: None,
                guard_probe: None,
            }
        };
        let nonce = "ab".repeat(16);
        let sent = Instant::now();
        let body = ClockBody {
            schema_version: 1,
            issuer_instance: &run.authority.issuer,
            account_id: &run.request.account_id,
            session_id: &run.request.session_id,
            operation_id: &run.id,
            generation: run.generation,
            nonce: &nonce,
            server_received_at_unix_ms: now,
            server_now_unix_ms: now,
            session_expires_at_unix_ms: run.expiry,
        };
        let mut bytes = b"aurora/connect/social-clock/v1\0".to_vec();
        bytes.extend(serde_json::to_vec(&body).unwrap());
        let encoded_clock =
            json!({"body":body,"signature":hex_signature(&authority,&bytes)}).to_string();
        let clock = lab::OnlineClock::verify_bootstrap(
            &encoded_clock,
            authority.public(),
            lab::OnlineClockBinding {
                issuer: &run.authority.issuer,
                account: &run.request.account_id,
                session: &run.request.session_id,
                operation: &run.id,
                generation: run.generation,
                nonce: &nonce,
            },
            sent,
            Instant::now(),
        )
        .unwrap();
        let challenge = lab::SocialKeyChallenge::parse_with_clock(&json!({
            "schemaVersion":1,"purpose":"accountKeyBinding","issuerInstance":run.authority.issuer,
            "challengeId":"11".repeat(32),"accountId":run.request.account_id,"operationId":run.id,
            "generation":run.generation,"role":"guest","publicKey":run.key.public().to_string(),
            "sessionId":run.request.session_id,"sessionDigest":"44".repeat(32),
            "issuedAtUnixMs":now,"expiresAtUnixMs":now+9_000,
        }).to_string(), &clock).unwrap();
        let host_operation = uuid::Uuid::new_v4().to_string();
        let context = sign(
            &authority,
            b"aurora/connect/social-join-context/v1\0",
            json!({
                "schemaVersion":1,"purpose":"socialJoinContext","issuerInstance":run.authority.issuer,
                "recipientAccountId":run.request.account_id,"sessionId":run.request.session_id,"sessionDigest":"44".repeat(32),
                "hostAccountId":"hostReviewFixture","hostPublicKey":host.public().to_string(),"hostOperationId":host_operation,
                "hostGeneration":1,"policyId":"55".repeat(32),"policyDigest":"66".repeat(32),
                "issuedAtUnixMs":now,"expiresAtUnixMs":now+context_ms,
            }),
            &[
                "schemaVersion",
                "purpose",
                "issuerInstance",
                "recipientAccountId",
                "sessionId",
                "sessionDigest",
                "hostAccountId",
                "hostPublicKey",
                "hostOperationId",
                "hostGeneration",
                "policyId",
                "policyDigest",
                "issuedAtUnixMs",
                "expiresAtUnixMs",
            ],
        );
        let context = lab::VerifiedSocialJoinContext::verify_with_clock(
            &context,
            authority.public(),
            &run.request.account_id,
            &run.request.session_id,
            run.deadline,
            Some(&clock),
        )
        .unwrap();
        let encoded = sign(
            &authority,
            b"aurora/connect/social-admission/v1\0",
            json!({
                "schemaVersion":5,"purpose":"socialAdmission","issuerInstance":run.authority.issuer,
                "grantId":"77".repeat(32),"policyId":"55".repeat(32),"policyDigest":"66".repeat(32),
                "sessionId":run.request.session_id,"sessionDigest":"44".repeat(32),
                "hostAccountId":"hostReviewFixture","hostPublicKey":host.public().to_string(),"hostOperationId":host_operation,"hostGeneration":1,
                "guestAccountId":run.request.account_id,"guestPublicKey":run.key.public().to_string(),"guestOperationId":run.id,"guestGeneration":run.generation,
                "issuedAtUnixMs":now,"admissionExpiresAtUnixMs":now+30_000,"sessionExpiresAtUnixMs":run.expiry,
                "voiceProfile":"svc","voiceSourceDigest":"88".repeat(32),"hostPlayer":true,
                "runtimeProfile":"fabric-loader-0.16.14-1.21.1","voiceTransportProfile":"nativeVoice8V1",
            }),
            &[
                "schemaVersion",
                "purpose",
                "issuerInstance",
                "grantId",
                "policyId",
                "policyDigest",
                "sessionId",
                "sessionDigest",
                "hostAccountId",
                "hostPublicKey",
                "hostOperationId",
                "hostGeneration",
                "guestAccountId",
                "guestPublicKey",
                "guestOperationId",
                "guestGeneration",
                "issuedAtUnixMs",
                "admissionExpiresAtUnixMs",
                "sessionExpiresAtUnixMs",
                "voiceProfile",
                "voiceSourceDigest",
                "hostPlayer",
                "runtimeProfile",
                "voiceTransportProfile",
            ],
        );
        run.clock = Some(clock);
        Self {
            manager,
            run,
            authority,
            host,
            context,
            challenge,
            encoded,
            issued: now,
        }
    }
    fn mint(&self) -> (lab::VerifiedSocialGrant, NativeVoiceAdmission) {
        let (grant, native) =
            verify_guest_admission(&self.run, &self.context, &self.challenge, &self.encoded)
                .unwrap();
        (grant, native.unwrap())
    }
    fn lease(&self, duration: u64) -> String {
        sign(
            &self.authority,
            b"aurora/connect/social-continuity/v1\0",
            json!({
                "schemaVersion":1,"purpose":"socialContinuity","issuerInstance":self.run.authority.issuer,
                "grantId":"77".repeat(32),"policyDigest":"66".repeat(32),"sequence":1,
                "issuedAtUnixMs":self.issued,"expiresAtUnixMs":self.issued+duration,
            }),
            &[
                "schemaVersion",
                "purpose",
                "issuerInstance",
                "grantId",
                "policyDigest",
                "sequence",
                "issuedAtUnixMs",
                "expiresAtUnixMs",
            ],
        )
    }
    fn snapshot(&self) -> ConnectSnapshot {
        self.manager
            .state
            .lock()
            .unwrap()
            .attempt
            .as_ref()
            .unwrap()
            .snapshot
            .clone()
    }
}

#[tokio::test]
async fn native_voice_admission_consumes_only_fresh_context_and_keeps_original_caps() {
    let fixture = Fixture::new(150, 60_000);
    let (grant, native) = fixture.mint();
    let deadline = native.deadline;
    tokio::time::sleep(Duration::from_millis(180)).await;
    assert!(fixture.context.check_current().is_err());
    assert!(GuestIngress::Legacy(&fixture.context)
        .check_current()
        .is_err());
    assert!(verify_guest_admission(
        &fixture.run,
        &fixture.context,
        &fixture.challenge,
        &fixture.encoded
    )
    .is_err());
    assert!(native.check().is_ok());
    assert!(grant.check_admission().is_ok());
    assert_eq!(native.deadline, deadline);
    fixture.manager.state.lock().unwrap().social_epoch += 1;
    assert!(native.check().is_err());

    let fixture = Fixture::new(5_000, 150);
    let (_, native) = fixture.mint();
    assert!(native.deadline <= fixture.run.deadline);
    tokio::time::sleep(Duration::from_millis(180)).await;
    assert!(
        native.check().is_err(),
        "grant expiry must not extend the original attempt"
    );
    let fixture = Fixture::new(5_000, 60_000);
    let (_, native) = fixture.mint();
    fixture.run.cancel.cancel();
    assert_eq!(native.check(), Err(ConnectError::Cancelled));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    ApproveAfterContext,
    WrongDigest,
    Cancel,
    Epoch,
    Timeout,
    Continuity,
    Heartbeat,
    Downgrade,
}

async fn requests(listener: tokio::net::TcpListener, delivery: Value, count: Arc<AtomicUsize>) {
    loop {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut received = Vec::new();
        let route = loop {
            let mut chunk = [0; 512];
            let read = socket.read(&mut chunk).await.unwrap();
            assert!(read > 0);
            received.extend_from_slice(&chunk[..read]);
            assert!(received.len() < 8192);
            let text = std::str::from_utf8(&received).unwrap();
            if let Some((headers, body)) = text.split_once("\r\n\r\n") {
                let size = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                if body.len() == size {
                    break headers.lines().next().unwrap().to_owned();
                }
            }
        };
        let index = count.fetch_add(1, Ordering::SeqCst);
        let (code, body) = if index == 0 {
            assert!(route.starts_with("POST /v1/social/guest/receive "));
            (200, delivery.to_string())
        } else {
            assert_eq!(index, 1, "no retry or additional transport authorization");
            assert!(route.starts_with("POST /v1/social/relay/ticket "));
            (403, json!({"code":"fixture_reject_transport"}).to_string())
        };
        socket.write_all(format!("HTTP/1.1 {code} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    }
}

async fn exercise_review(outcome: Outcome) {
    let mut fixture = Fixture::new(
        if outcome == Outcome::ApproveAfterContext {
            250
        } else {
            5_000
        },
        if outcome == Outcome::Timeout {
            500
        } else {
            60_000
        },
    );
    let (grant, native) = fixture.mint();
    fixture.run.deadline = grant.session_deadline();
    let ingress = GuestIngress::Native(&native);
    let services = vec![
        game("127.0.0.1:31111".parse().unwrap()),
        lab::NativeVoiceBudget::service("127.0.0.1:31112".parse().unwrap()),
    ];
    let deadlines = lab::SessionDeadlines {
        admission_expires_at_unix_ms: grant.admission_expires_at_unix_ms(),
        session_expires_at_unix_ms: grant.session_expires_at_unix_ms(),
    };
    let invitation = lab::SessionInvitation::issue_online_native_voice_with_clock(
        &fixture.host,
        fixture.run.key.public(),
        &fixture.run.request.session_id,
        &services,
        deadlines,
        fixture.run.clock.as_ref().unwrap(),
    )
    .unwrap();
    let encoded = if outcome == Outcome::Downgrade {
        // An authentic older invitation is still a downgrade from this signed grant.
        lab::SessionInvitation::issue_online(
            &fixture.host,
            fixture.run.key.public(),
            &fixture.run.request.session_id,
            &[game("127.0.0.1:31111".parse().unwrap())],
            deadlines,
        )
        .unwrap()
        .encode()
    } else {
        invitation.encode()
    };
    let lease = fixture.lease(if outcome == Outcome::Continuity {
        500
    } else {
        3_000
    });
    let continuity = lab::SocialContinuity::start(&lease, &grant).unwrap();
    let authority =
        ready_source::LaunchAuthority::new(&fixture.run, grant.clone(), continuity).unwrap();
    let (_sender, receiver) = tokio::sync::watch::channel(lease);
    let mut heartbeat = tokio::spawn(std::future::pending::<Result<(), ConnectError>>());
    let abort_heartbeat = heartbeat.abort_handle();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = Box::leak(format!("http://{}", listener.local_addr().unwrap()).into_boxed_str());
    let count = Arc::new(AtomicUsize::new(0));
    let server = tokio::spawn(requests(
        listener,
        json!({"state":"ready","invitation":encoded}),
        count.clone(),
    ));
    let http = Http {
        client: reqwest::Client::builder().no_proxy().build().unwrap(),
        token: Arc::new(Mutex::new("fixture-credential".into())),
        cancel: fixture.run.cancel.clone(),
        deadline: fixture.run.deadline,
        origin,
        session_id: fixture.run.request.session_id.clone(),
        online: true,
    };
    let caller = guest_admitted(
        &fixture.run,
        &http,
        &ingress,
        &grant,
        receiver,
        &mut heartbeat,
        &authority,
    );
    let control = async {
        if outcome == Outcome::Downgrade {
            return;
        }
        let review = loop {
            let snapshot = fixture.snapshot();
            if snapshot
                .social
                .as_ref()
                .is_some_and(|s| s.stage == "guestReview")
            {
                break snapshot;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        };
        let digest = review.social.as_ref().unwrap().digest.as_ref().unwrap();
        assert_ne!(
            digest,
            &invitation.digest(),
            "review also binds the original signed grant and attempt"
        );
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "no relay ticket before consent"
        );
        assert!(review.tcp_address.is_none() && review.udp.is_none());
        let original_deadline = fixture
            .manager
            .state
            .lock()
            .unwrap()
            .attempt
            .as_ref()
            .unwrap()
            .prepared_deadline;
        assert!(original_deadline <= native.deadline);
        match outcome {
            Outcome::ApproveAfterContext => {
                tokio::time::sleep(Duration::from_millis(300)).await;
                assert!(fixture.context.check_current().is_err());
                assert!(native.check().is_ok());
                fixture
                    .manager
                    .social_confirm_guest("main", &fixture.run.id, review.generation, digest)
                    .unwrap();
                assert!(fixture
                    .manager
                    .social_confirm_guest("main", &fixture.run.id, review.generation, digest)
                    .is_err());
                assert_eq!(
                    fixture
                        .manager
                        .state
                        .lock()
                        .unwrap()
                        .attempt
                        .as_ref()
                        .unwrap()
                        .prepared_deadline,
                    original_deadline
                );
            }
            Outcome::WrongDigest => {
                assert_eq!(
                    fixture
                        .manager
                        .social_confirm_guest(
                            "main",
                            &fixture.run.id,
                            review.generation,
                            &"00".repeat(32)
                        )
                        .unwrap_err(),
                    ConnectError::WrongDigest
                );
                assert!(fixture
                    .manager
                    .social_confirm_guest("other", &fixture.run.id, review.generation, digest)
                    .is_err());
                assert!(fixture
                    .manager
                    .social_confirm_guest("main", &fixture.run.id, review.generation + 1, digest)
                    .is_err());
                assert_eq!(count.load(Ordering::SeqCst), 1);
                fixture.run.cancel.cancel();
            }
            Outcome::Cancel => fixture.run.cancel.cancel(),
            Outcome::Epoch => fixture.manager.state.lock().unwrap().social_epoch += 1,
            Outcome::Timeout | Outcome::Continuity => {}
            Outcome::Heartbeat => abort_heartbeat.abort(),
            Outcome::Downgrade => unreachable!(),
        }
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(4), async {
        tokio::join!(caller, control)
    })
    .await
    .unwrap();
    assert!(
        result.is_err(),
        "{outcome:?}: ticket endpoint deliberately rejects after consent"
    );
    assert_eq!(
        count.load(Ordering::SeqCst),
        if outcome == Outcome::ApproveAfterContext {
            2
        } else {
            1
        },
        "{outcome:?}"
    );
    let final_snapshot = fixture.snapshot();
    assert!(final_snapshot.tcp_address.is_none() && final_snapshot.udp.is_none());
    assert!(final_snapshot.social.unwrap().digest.is_none());
    heartbeat.abort();
    let _ = heartbeat.await;
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn native_voice_guest_review_requires_original_consent_before_any_transport() {
    for outcome in [
        Outcome::ApproveAfterContext,
        Outcome::WrongDigest,
        Outcome::Cancel,
        Outcome::Epoch,
        Outcome::Timeout,
        Outcome::Continuity,
        Outcome::Heartbeat,
        Outcome::Downgrade,
    ] {
        exercise_review(outcome).await;
    }
}

#[tokio::test]
async fn native_voice_after_ticket_gate_prevents_polling_udp_on_cancel_epoch_expiry_or_revoke() {
    for mode in 0..5 {
        let mut fixture = Fixture::new(5_000, if mode == 3 { 150 } else { 60_000 });
        let (grant, native) = fixture.mint();
        fixture.run.deadline = grant.session_deadline();
        let ingress = GuestIngress::Native(&native);
        let invitation = lab::SessionInvitation::issue_online_native_voice_with_clock(
            &fixture.host,
            fixture.run.key.public(),
            &fixture.run.request.session_id,
            &[
                game("127.0.0.1:31111".parse().unwrap()),
                lab::NativeVoiceBudget::service("127.0.0.1:31112".parse().unwrap()),
            ],
            lab::SessionDeadlines {
                admission_expires_at_unix_ms: grant.admission_expires_at_unix_ms(),
                session_expires_at_unix_ms: grant.session_expires_at_unix_ms(),
            },
            fixture.run.clock.as_ref().unwrap(),
        )
        .unwrap();
        let continuity = lab::SocialContinuity::start(&fixture.lease(3_000), &grant).unwrap();
        let authority =
            ready_source::LaunchAuthority::new(&fixture.run, grant.clone(), continuity).unwrap();
        let body = TicketBody {
            schema_version: 1,
            session_id: &fixture.run.request.session_id,
            issuer_instance: &fixture.run.authority.issuer,
            peer_id: fixture.run.key.public().to_string(),
            host_peer_id: fixture.host.public().to_string(),
            account_id: &fixture.run.request.account_id,
            generation: fixture.run.generation,
            issued_at_unix_ms: fixture.issued,
            expires_at_unix_ms: fixture.run.expiry,
            direct_addresses: vec![],
        };
        let mut bytes = b"aurora/connect/relay-ticket/v1\0".to_vec();
        bytes.extend(serde_json::to_vec(&body).unwrap());
        let ticket = lab::OnlineNetworkTicket::for_local_test(
            &json!({"body":body,"signature":hex_signature(&fixture.authority,&bytes)}).to_string(),
            fixture.authority.public(),
            &fixture.run.authority.issuer,
            &fixture.run.request.account_id,
            &fixture.run.request.session_id,
            fixture.run.key.public(),
            fixture.host.public(),
            "http://127.0.0.1:9",
        )
        .unwrap();
        // The ticket has already returned and verified. Change only original
        // admission authority before the connection future receives its first poll.
        match mode {
            1 => fixture.run.cancel.cancel(),
            2 => fixture.manager.state.lock().unwrap().social_epoch += 1,
            3 => tokio::time::sleep(Duration::from_millis(180)).await,
            4 => authority.close(),
            _ => (),
        }
        let polls = AtomicUsize::new(0);
        let connection = async {
            polls.fetch_add(1, Ordering::SeqCst);
            ticket.check_current().unwrap();
            let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
            Ok(socket.local_addr().unwrap())
        };
        let result = after_ticket(
            &fixture.run,
            &ingress,
            &grant,
            &invitation,
            &authority,
            connection,
        )
        .await;
        assert_eq!(result.is_ok(), mode == 0, "mode {mode}");
        assert_eq!(
            polls.load(Ordering::SeqCst),
            usize::from(mode == 0),
            "mode {mode}: rejected authority must not even poll the UDP opener"
        );
    }
}
