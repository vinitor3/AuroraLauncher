use super::*;
use serde::Serialize;
use std::collections::HashSet;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

#[path = "social_host_poll_probe.rs"]
mod host_poll_probe;

thread_local! {
    // Only the explicit current-thread ignored probe enables these traces.
    static HTTP_DIAGNOSTIC_SAMPLE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

pub(super) fn trace_http(
    route: SocialRoute,
    started: Instant,
    phase: &'static str,
    response: Option<&reqwest::Response>,
    error: Option<&reqwest::Error>,
) {
    let index = HTTP_DIAGNOSTIC_SAMPLE.with(std::cell::Cell::get);
    if index == 0 || route != SocialRoute::Bootstrap {
        return;
    }
    // Never print a reqwest error: its URL/source can contain request data.
    println!(
        "AURORA_BOOTSTRAP_HTTP {}",
        json!({
            "index": index, "route": "bootstrap", "phase": phase,
            "elapsedMs": started.elapsed().as_millis(),
            "status": response.map(|value| value.status().as_u16()),
            "httpVersion": response.map(|value| format!("{:?}", value.version())),
            "addressFamily": response.and_then(reqwest::Response::remote_addr).map(|address| if address.is_ipv4() { "ipv4" } else { "ipv6" }),
            "timeout": error.is_some_and(reqwest::Error::is_timeout),
            "connect": error.is_some_and(reqwest::Error::is_connect),
            "body": error.is_some_and(reqwest::Error::is_body),
            "decode": error.is_some_and(reqwest::Error::is_decode),
        })
    );
}

/// The Node diagnostic owns the test account's temporary advertisement and its
/// compare-and-swap cleanup. It passes the credential to this child only in RAM.
/// This probe calls the unchanged production Http client, not a replica of it.
#[tokio::test(flavor = "current_thread")]
#[ignore = "requires explicitly authorized temporary own advertisement and in-memory Firebase token"]
async fn social_bootstrap_real_http_diagnostic() {
    fn take_env(name: &str) -> String {
        let value = std::env::var(name).expect("missing explicitly supplied diagnostic input");
        std::env::remove_var(name);
        value
    }
    let token = take_env("AURORA_BOOTSTRAP_TOKEN");
    let account = take_env("AURORA_BOOTSTRAP_ACCOUNT");
    let session = take_env("AURORA_BOOTSTRAP_SESSION");
    assert!(
        online_token_destination(&token, &account),
        "wrong diagnostic token destination"
    );
    assert!(
        uuid::Uuid::parse_str(&session).is_ok(),
        "invalid diagnostic session"
    );
    let authority = SocialLabAuthority::from_compiled_online()
        .expect("compile the fixed online origin for this probe");
    assert!(
        authority.online_origin
            == Some("https://aurora-connect-social.aurora-edge-proxy.workers.dev"),
        "wrong compiled diagnostic origin"
    );
    let run = Run {
        state: Arc::new(Mutex::new(State::default())),
        id: uuid::Uuid::new_v4().to_string(),
        generation: 1,
        request: SocialReserveRequest {
            account_epoch: 0,
            account_id: account,
            session_id: session,
            role: "host".into(),
            game_target: None,
            visibility: Some("friends".into()),
        },
        selection: None,
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        native_voice_source: None,
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        native_voice_local_key: None,
        key: lab::generate_key(),
        authority,
        deadline: Instant::now() + Duration::from_secs(60),
        expiry: lab::unix_millis().saturating_add(60_000),
        clock: None,
        cancel: CancellationToken::new(),
        #[cfg(windows)]
        source_gate: None,
        guard_probe: None,
    };
    let http = Http::new(token, &run).expect("diagnostic client construction failed");
    let compare_pool = std::env::var("AURORA_BOOTSTRAP_COMPARE_POOL").as_deref() == Ok("1");
    std::env::remove_var("AURORA_BOOTSTRAP_COMPARE_POOL");
    let mut pooled = http.clone();
    if compare_pool {
        // Diagnostic-only alternative; the production client remains unchanged.
        pooled.client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .pool_max_idle_per_host(1)
            .pool_idle_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(2))
            .build()
            .expect("pooled diagnostic client construction failed");
    }
    let mut original = None;
    struct ResetTrace;
    impl Drop for ResetTrace {
        fn drop(&mut self) {
            HTTP_DIAGNOSTIC_SAMPLE.with(|value| value.set(0));
        }
    }
    let _reset = ResetTrace;
    for index in 1..=if compare_pool { 6 } else { 3 } {
        run.current().expect("original diagnostic attempt expired");
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        HTTP_DIAGNOSTIC_SAMPLE.with(|value| value.set(index));
        let reuse = compare_pool && index % 2 == 0;
        let mode = if reuse {
            "keepAlive1Idle10s"
        } else {
            "productionPool0"
        };
        let sent = Instant::now();
        let result = (if reuse { &pooled } else { &http })
            .post(
                SocialRoute::Bootstrap,
                json!({
                    "operationId": run.id, "generation": run.generation, "nonce": nonce,
                }),
            )
            .await;
        let received = Instant::now();
        let elapsed = received.duration_since(sent);
        let value = match result {
            Ok(value) => value,
            Err(error) => {
                println!(
                    "AURORA_BOOTSTRAP_SAMPLE {}",
                    json!({ "index": index, "mode": mode, "route": "bootstrap", "elapsedMs": elapsed.as_millis(),
                    "outcome": match error {
                        ConnectError::SocialFailure(_, SocialFailureReason::RequestTimedOut) => "http_timeout",
                        ConnectError::SocialFailure(_, SocialFailureReason::ServiceUnavailable) => "transport_unavailable",
                        ConnectError::SocialFailure(_, SocialFailureReason::Authentication) => "authentication_rejected",
                        ConnectError::PlanExpired => "original_attempt_expired",
                        _ => "request_rejected",
                    }, "signatureVerified": false })
                );
                continue;
            }
        };
        assert!(
            value["schemaVersion"] == 1
                && value["accountId"] == run.request.account_id
                && value["sessionId"] == run.request.session_id,
            "bootstrap diagnostic binding rejected"
        );
        let validated = SocialLabAuthority::from_local_process(
            field(&value, "admissionPublicKey").expect("missing pin"),
            field(&value, "issuerInstance").expect("missing issuer"),
        )
        .expect("invalid bootstrap authority");
        let sample = lab::OnlineClock::verify_bootstrap_sample(
            &packet(&value, "clock").expect("missing clock"),
            validated.pin,
            lab::OnlineClockBinding {
                issuer: &validated.issuer,
                account: &run.request.account_id,
                session: &run.request.session_id,
                operation: &run.id,
                generation: run.generation,
                nonce: &nonce,
            },
            sent,
            received,
        );
        let (outcome, expiry) = match sample {
            Ok(clock) => ("accepted", clock.session_expiry()),
            Err(lab::OnlineClockBootstrapError::Uncertain { session_expiry, .. }) => {
                ("clock_uncertain", session_expiry)
            }
            Err(_) => panic!("signed diagnostic sample rejected"),
        };
        let scope = (validated.pin, validated.issuer, expiry);
        assert!(
            value["expiresAtUnixMs"].as_u64() == Some(expiry)
                && original.as_ref().is_none_or(|old| old == &scope),
            "original authority changed"
        );
        original = Some(scope);
        let body = &value["clock"]["body"];
        let processing = body["serverNowUnixMs"].as_u64().unwrap()
            - body["serverReceivedAtUnixMs"].as_u64().unwrap();
        println!(
            "AURORA_BOOTSTRAP_SAMPLE {}",
            json!({ "index": index, "mode": mode, "route": "bootstrap", "elapsedMs": elapsed.as_millis(),
            "serverProcessingMs": processing, "unaccountedMs": elapsed.as_millis().saturating_sub(processing.saturating_sub(1) as u128),
            "outcome": outcome, "signatureVerified": true })
        );
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Reply {
    Cold,
    Cancel,
    Slow,
    ContradictorySlow,
    Timeout,
    Unavailable,
    Disconnect,
    Valid,
    InvalidSignature,
    Expired,
    Denied,
    ForbiddenBroker,
    ForbiddenBackend,
    UnauthorizedForged,
    QuotaForged,
    ChangedExpiry,
    ChangedPin,
    ChangedIssuer,
    ChangedGeneration,
    ChangedSession,
    ChangedNonce,
}

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

async fn bootstrap_request(listener: &TcpListener, session: &str) -> (TcpStream, Value) {
    let (mut socket, _) = listener.accept().await.unwrap();
    let mut received = Vec::new();
    loop {
        let mut chunk = [0; 512];
        let read = socket.read(&mut chunk).await.unwrap();
        assert!(read > 0);
        received.extend_from_slice(&chunk[..read]);
        assert!(received.len() < 4096);
        let wire = std::str::from_utf8(&received).unwrap();
        if let Some((header, body)) = wire.split_once("\r\n\r\n") {
            assert!(header.starts_with("POST /v1/social/bootstrap "));
            assert!(header.lines().any(|line| line.split_once(':').is_some_and(
                |(name, value)| {
                    name.eq_ignore_ascii_case("x-aurora-session") && value.trim() == session
                }
            )));
            let size = header
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            if body.len() == size {
                let request = serde_json::from_str(body).unwrap();
                return (socket, request);
            }
        }
    }
}

async fn exercise(script: &[Reply], expected: Result<(), ConnectError>) {
    exercise_with_expiry(script, expected, 20_000).await;
}

async fn exercise_with_expiry(
    script: &[Reply],
    expected: Result<(), ConnectError>,
    session_remaining_ms: u64,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = Box::leak(format!("http://{}", listener.local_addr().unwrap()).into_boxed_str());
    let authority = lab::generate_key();
    let other_authority = lab::generate_key();
    let issuer = "33".repeat(32);
    let operation = uuid::Uuid::new_v4().to_string();
    let session = uuid::Uuid::new_v4().to_string();
    let generation = 7;
    let account = "bootstrap_fixture";
    let original_deadline = Instant::now() + Duration::from_secs(15);
    let initial_expiry = lab::unix_millis() + 15_000;
    let signed_expiry = lab::unix_millis() + session_remaining_ms;
    let mut run = Run {
        state: Arc::new(Mutex::new(State::default())),
        id: operation.clone(),
        generation,
        request: SocialReserveRequest {
            account_epoch: 0,
            account_id: account.into(),
            session_id: session.clone(),
            role: "host".into(),
            game_target: None,
            visibility: Some("friends".into()),
        },
        selection: None,
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        native_voice_source: None,
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        native_voice_local_key: None,
        key: lab::generate_key(),
        authority: SocialLabAuthority {
            pin: lab::generate_key().public(),
            issuer: String::new(),
            online_origin: Some(origin),
            join_policy: "open".into(),
            session_expiry: 0,
            max_players: 0,
        },
        deadline: original_deadline,
        expiry: initial_expiry,
        clock: None,
        cancel: CancellationToken::new(),
        #[cfg(windows)]
        source_gate: None,
        guard_probe: None,
    };
    let mut http = Http {
        client: reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap(),
        token: Arc::new(Mutex::new("bootstrap-fixture-credential".into())),
        cancel: run.cancel.clone(),
        deadline: original_deadline,
        origin,
        session_id: session.clone(),
        online: true,
    };
    let cancellation = run.cancel.clone();
    let responses = async {
        let mut nonces = HashSet::new();
        for reply in script {
            let (mut socket, request) = bootstrap_request(&listener, &session).await;
            assert_eq!(request.as_object().unwrap().len(), 3);
            assert_eq!(request["operationId"], operation);
            assert_eq!(request["generation"], generation);
            let nonce = request["nonce"].as_str().unwrap();
            assert_eq!(nonce.len(), 32);
            assert!(nonces.insert(nonce.to_owned()));
            if matches!(reply, Reply::Cancel) {
                cancellation.cancel();
                return;
            }
            if matches!(reply, Reply::Cold) {
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
            if matches!(reply, Reply::Disconnect) {
                drop(socket);
                continue;
            }
            if matches!(reply, Reply::Timeout) {
                tokio::time::sleep(Duration::from_millis(6100)).await;
            }
            if matches!(
                reply,
                Reply::Slow | Reply::ContradictorySlow | Reply::InvalidSignature | Reply::Expired
            ) {
                // Simulate delay before DO entry, which the signed server
                // interval cannot deduct from the HTTP round trip.
                tokio::time::sleep(Duration::from_millis(2_100)).await;
            }
            let now = lab::unix_millis();
            let expiry = match reply {
                Reply::Expired => now - 1,
                Reply::ChangedExpiry => signed_expiry + 1,
                _ => signed_expiry,
            };
            let (status, response) = if matches!(reply, Reply::Unavailable) {
                (503, json!({"code":"backend_unavailable"}))
            } else if matches!(reply, Reply::Denied) {
                (403, json!({"code":"session_not_authorized"}))
            } else if matches!(reply, Reply::ForbiddenBroker) {
                (403, json!({"code":"broker_unavailable"}))
            } else if matches!(reply, Reply::ForbiddenBackend) {
                (403, json!({"code":"backend_unavailable"}))
            } else if matches!(reply, Reply::UnauthorizedForged) {
                (401, json!({"code":"broker_unavailable"}))
            } else if matches!(reply, Reply::QuotaForged) {
                (429, json!({"code":"backend_unavailable"}))
            } else {
                let alternate_issuer = "44".repeat(32);
                let alternate_session = uuid::Uuid::new_v4().to_string();
                let alternate_nonce = uuid::Uuid::new_v4().simple().to_string();
                let selected_issuer = if matches!(reply, Reply::ChangedIssuer) {
                    &alternate_issuer
                } else {
                    &issuer
                };
                let selected_authority = if matches!(reply, Reply::ChangedPin) {
                    &other_authority
                } else {
                    &authority
                };
                let body = ClockBody {
                    schema_version: 1,
                    issuer_instance: selected_issuer,
                    account_id: account,
                    session_id: if matches!(reply, Reply::ChangedSession) {
                        &alternate_session
                    } else {
                        &session
                    },
                    operation_id: &operation,
                    generation: generation + u64::from(matches!(reply, Reply::ChangedGeneration)),
                    nonce: if matches!(reply, Reply::ChangedNonce) {
                        &alternate_nonce
                    } else {
                        nonce
                    },
                    server_received_at_unix_ms: now
                        + if matches!(reply, Reply::ContradictorySlow) {
                            5000
                        } else {
                            0
                        },
                    server_now_unix_ms: now
                        + if matches!(reply, Reply::ContradictorySlow) {
                            5000
                        } else {
                            0
                        },
                    session_expires_at_unix_ms: expiry,
                };
                let mut bytes = b"aurora/connect/social-clock/v1\0".to_vec();
                bytes.extend(serde_json::to_vec(&body).unwrap());
                let signature = if matches!(reply, Reply::InvalidSignature) {
                    "00".repeat(64)
                } else {
                    selected_authority
                        .sign(&bytes)
                        .to_bytes()
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>()
                };
                (
                    200,
                    json!({
                        "schemaVersion":1,"accountId":account,"sessionId":session,
                        "admissionPublicKey":selected_authority.public().to_string(),"issuerInstance":selected_issuer,
                        "joinPolicy":"open","maxPlayers":8,"expiresAtUnixMs":expiry,
                        "clock":{"body":body,"signature":signature},
                    }),
                )
            };
            let body = response.to_string();
            let wire = format!("HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            let written = socket.write_all(wire.as_bytes()).await;
            if !matches!(reply, Reply::Timeout) {
                written.unwrap();
            }
        }
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(22), async {
        tokio::join!(bootstrap_clock(&mut run, &mut http), responses)
    })
    .await
    .unwrap();
    assert_eq!(result, expected);
    assert_eq!(run.id, operation);
    assert_eq!(run.generation, generation);
    assert_eq!(run.request.session_id, session);
    assert!(
        tokio::time::timeout(Duration::from_millis(30), listener.accept())
            .await
            .is_err()
    );
    if expected.is_ok() {
        assert!(run.deadline <= original_deadline);
        assert_eq!(http.deadline, run.deadline);
        let clock = run.clock.unwrap();
        assert_eq!(clock.session_expiry(), signed_expiry);
        if script.contains(&Reply::Cold) && script.contains(&Reply::Valid) {
            assert!(clock.bootstrap_uncertainty() < Duration::from_millis(200));
            assert!(clock.bootstrap_round_trip() < Duration::from_millis(200));
        }
    } else {
        assert_eq!(run.deadline, original_deadline);
        assert_eq!(http.deadline, original_deadline);
        assert_eq!(run.expiry, initial_expiry);
        assert!(run.clock.is_none());
    }
}

#[tokio::test]
async fn social_bootstrap_retries_only_twice_without_renewing_the_original_attempt() {
    tokio::join!(
        exercise(&[Reply::Slow, Reply::Slow, Reply::Valid], Ok(())),
        exercise(
            &[Reply::Slow, Reply::Slow, Reply::Slow],
            Err(BOOTSTRAP_CLOCK_UNCERTAIN),
        ),
    );
}

#[tokio::test]
async fn social_bootstrap_rejected_or_expired_signed_samples_do_not_retry() {
    tokio::join!(
        exercise(&[Reply::InvalidSignature], Err(BOOTSTRAP_CLOCK_UNVERIFIED)),
        exercise(&[Reply::Expired], Err(ConnectError::PlanExpired)),
        exercise(
            &[Reply::Denied],
            Err(ConnectError::SocialFailure(
                SocialFailureStage::Bootstrap,
                SocialFailureReason::NotAuthorized,
            ))
        ),
    );
}

#[tokio::test]
async fn social_bootstrap_retry_cannot_replace_the_original_signed_expiry() {
    exercise(
        &[Reply::Slow, Reply::ChangedExpiry],
        Err(BOOTSTRAP_CLOCK_UNVERIFIED),
    )
    .await;
}

#[tokio::test]
async fn social_bootstrap_transient_requests_retry_within_the_same_three_sample_budget() {
    tokio::join!(
        exercise(&[Reply::Timeout, Reply::Valid, Reply::Valid], Ok(())),
        exercise(&[Reply::Unavailable, Reply::Valid, Reply::Valid], Ok(())),
        exercise(&[Reply::Disconnect, Reply::Valid, Reply::Valid], Ok(())),
        exercise(
            &[Reply::Unavailable, Reply::Unavailable, Reply::Unavailable],
            Err(ConnectError::SocialFailure(
                SocialFailureStage::Bootstrap,
                SocialFailureReason::ServiceUnavailable,
            ))
        ),
        exercise(
            &[Reply::Unavailable, Reply::Unavailable, Reply::Timeout],
            Err(ConnectError::SocialFailure(
                SocialFailureStage::Bootstrap,
                SocialFailureReason::RequestTimedOut,
            ))
        ),
        exercise(
            &[Reply::Slow, Reply::Unavailable, Reply::ChangedExpiry],
            Err(BOOTSTRAP_CLOCK_UNVERIFIED)
        ),
        exercise(
            &[Reply::Unavailable, Reply::Denied],
            Err(ConnectError::SocialFailure(
                SocialFailureStage::Bootstrap,
                SocialFailureReason::NotAuthorized,
            ))
        ),
    );
}

#[tokio::test]
async fn social_bootstrap_selects_the_best_sample_before_publishing_one_clock() {
    tokio::join!(
        exercise(&[Reply::Cold, Reply::Valid, Reply::Cold], Ok(())),
        exercise(&[Reply::Valid, Reply::Unavailable, Reply::Timeout], Ok(())),
        exercise(&[Reply::Valid, Reply::Cancel], Err(ConnectError::Cancelled)),
        exercise_with_expiry(
            &[Reply::Valid, Reply::Timeout],
            Err(ConnectError::PlanExpired),
            500,
        ),
    );
}

#[tokio::test]
async fn social_bootstrap_discards_an_earlier_candidate_after_any_authority_rejection() {
    for reply in [
        Reply::ChangedPin,
        Reply::ChangedIssuer,
        Reply::ChangedGeneration,
        Reply::ChangedSession,
        Reply::ChangedNonce,
        Reply::ChangedExpiry,
        Reply::InvalidSignature,
        Reply::ContradictorySlow,
    ] {
        exercise(&[Reply::Valid, reply], Err(BOOTSTRAP_CLOCK_UNVERIFIED)).await;
    }
    exercise(
        &[Reply::Valid, Reply::Expired],
        Err(ConnectError::PlanExpired),
    )
    .await;
    exercise(
        &[Reply::Valid, Reply::Denied],
        Err(ConnectError::SocialFailure(
            SocialFailureStage::Bootstrap,
            SocialFailureReason::NotAuthorized,
        )),
    )
    .await;
}

#[tokio::test]
async fn social_bootstrap_denials_cannot_claim_a_retryable_body() {
    for (reply, reason) in [
        (Reply::ForbiddenBroker, SocialFailureReason::RequestRejected),
        (
            Reply::ForbiddenBackend,
            SocialFailureReason::RequestRejected,
        ),
        (
            Reply::UnauthorizedForged,
            SocialFailureReason::Authentication,
        ),
        (Reply::QuotaForged, SocialFailureReason::Quota),
    ] {
        exercise(
            &[Reply::Valid, reply],
            Err(ConnectError::SocialFailure(
                SocialFailureStage::Bootstrap,
                reason,
            )),
        )
        .await;
    }
}

async fn delayed_request(
    route: SocialRoute,
    response_delay: Duration,
    attempt_remaining: Duration,
    cancel_after: Option<Duration>,
    expected: Result<Value, ConnectError>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = Box::leak(format!("http://{}", listener.local_addr().unwrap()).into_boxed_str());
    let cancel = CancellationToken::new();
    let deadline = Instant::now() + attempt_remaining;
    let http = Http {
        client: reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap(),
        token: Arc::new(Mutex::new("fixture-only".into())),
        cancel: cancel.clone(),
        deadline,
        origin,
        session_id: uuid::Uuid::new_v4().to_string(),
        online: true,
    };
    let work = async {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        let read = socket.read(&mut request).await.unwrap();
        assert!(std::str::from_utf8(&request[..read])
            .unwrap()
            .starts_with(&format!("POST {} ", route.path())));
        if let Some(delay) = cancel_after {
            tokio::time::sleep(delay).await;
            cancel.cancel();
        } else {
            tokio::time::sleep(response_delay).await;
        }
        let _ = socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
            .await;
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(8), async {
        tokio::join!(http.post(route, json!({})), work)
    })
    .await
    .unwrap();
    assert_eq!(result, expected);
    assert_eq!(http.deadline, deadline);
    // Even a timed-out mutation must not be retried behind the review.
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn social_host_registration_allows_slow_single_mutation_but_keeps_other_bounds() {
    use SocialFailureReason::RequestTimedOut;
    tokio::join!(
        delayed_request(
            SocialRoute::HostRegister,
            Duration::from_millis(2300),
            Duration::from_secs(10),
            None,
            Ok(json!({}))
        ),
        delayed_request(
            SocialRoute::HostPoll,
            Duration::from_millis(2300),
            Duration::from_secs(10),
            None,
            Err(ConnectError::SocialFailure(
                SocialFailureStage::SessionContinuity,
                RequestTimedOut
            ))
        ),
        delayed_request(
            SocialRoute::HostRegister,
            Duration::from_millis(6200),
            Duration::from_secs(10),
            None,
            Err(ConnectError::SocialFailure(
                SocialFailureStage::HostRegistration,
                RequestTimedOut
            ))
        ),
        delayed_request(
            SocialRoute::HostRegister,
            Duration::from_millis(150),
            Duration::from_millis(75),
            None,
            Err(ConnectError::PlanExpired)
        ),
        delayed_request(
            SocialRoute::HostRegister,
            Duration::ZERO,
            Duration::from_secs(10),
            Some(Duration::from_millis(50)),
            Err(ConnectError::Cancelled)
        ),
    );
}
