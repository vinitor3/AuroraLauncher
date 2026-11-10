use super::*;
#[path = "social_poll_schedule_tests.rs"]
mod poll_schedule_tests;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Notify,
};

const POLICY: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const FIRST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SECOND: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn fixture(
    listener: &TcpListener,
) -> (
    Run,
    Http,
    lab::SecretKey,
    lab::SocialKeyChallenge,
    lab::SignedSocialHostPolicy,
) {
    fixture_mode(listener, false)
}

fn fixture_mode(
    listener: &TcpListener,
    private: bool,
) -> (
    Run,
    Http,
    lab::SecretKey,
    lab::SocialKeyChallenge,
    lab::SignedSocialHostPolicy,
) {
    let source = "127.0.0.1:43001".parse().unwrap();
    let authority = lab::generate_key();
    let key = lab::generate_key();
    let now = lab::unix_millis();
    let origin = Box::leak(format!("http://{}", listener.local_addr().unwrap()).into_boxed_str());
    let run = Run {
        state: Arc::new(Mutex::new(State::default())),
        id: uuid::Uuid::new_v4().to_string(),
        generation: 1,
        request: SocialReserveRequest {
            account_epoch: 0,
            account_id: "fixture_host".into(),
            session_id: uuid::Uuid::new_v4().to_string(),
            role: "host".into(),
            game_target: None,
            visibility: Some(if private { "private" } else { "friends" }.into()),
        },
        selection: None,
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        native_voice_source: None,
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        native_voice_local_key: None,
        key,
        authority: SocialLabAuthority {
            pin: authority.public(),
            issuer: "33".repeat(32),
            online_origin: Some(origin),
            private_invitations: private,
            join_policy: "open".into(),
            session_expiry: now + 20_000,
            max_players: 8,
        },
        deadline: Instant::now() + Duration::from_secs(8),
        expiry: now + 20_000,
        clock: None,
        cancel: CancellationToken::new(),
        #[cfg(windows)]
        source_gate: None,
        guard_probe: None,
    };
    let http = Http {
        client: reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap(),
        token: Arc::new(Mutex::new("fixture-token".into())),
        cancel: run.cancel.clone(),
        deadline: run.deadline,
        origin,
        session_id: run.request.session_id.clone(),
        online: true,
    };
    let challenge = lab::SocialKeyChallenge::parse(
        &json!({
            "schemaVersion":1,"purpose":"accountKeyBinding","issuerInstance":run.authority.issuer,
            "challengeId":POLICY,"accountId":run.request.account_id,"operationId":run.id,
            "generation":run.generation,"role":"host","publicKey":run.key.public().to_string(),
            "sessionId":run.request.session_id,"sessionDigest":"44".repeat(32),"issuedAtUnixMs":now,
            "expiresAtUnixMs":now+8_000
        })
        .to_string(),
    )
    .unwrap();
    let review = super::super::review_host_policy(&run, &http, &challenge, source).unwrap();
    let policy = review
        .approve(&run.key, &review.digest(), run.deadline)
        .unwrap();
    (run, http, authority, challenge, policy)
}

fn signed(authority: &lab::SecretKey, domain: &str, fields: &[(&str, Value)]) -> Value {
    let mut material = domain.as_bytes().to_vec();
    let mut body = serde_json::Map::new();
    for (name, value) in fields {
        let field = value
            .as_str()
            .map_or_else(|| value.to_string(), str::to_owned);
        material.extend_from_slice(&(field.len() as u16).to_be_bytes());
        material.extend_from_slice(field.as_bytes());
        body.insert((*name).into(), value.clone());
    }
    let signature = authority
        .sign(&material)
        .to_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    json!({"body":body,"signature":signature})
}

#[tokio::test]
async fn social_private_online_review_is_final_before_consent_and_keeps_capacity_and_expiry() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (mut run, http, _, challenge, public) = fixture(&listener);
    let source = "127.0.0.1:43001".parse().unwrap();
    run.request.visibility = Some("private".into());
    assert!(super::super::review_host_policy(&run, &http, &challenge, source).is_err());
    run.authority.private_invitations = true;
    let policy = super::super::review_host_policy(&run, &http, &challenge, source).unwrap();
    assert_eq!(serde_json::to_value(&policy).unwrap()["schemaVersion"], 6);
    assert_eq!(
        serde_json::to_value(&policy).unwrap()["visibility"],
        "private"
    );
    assert_eq!(policy.expires_at_unix_ms(), run.authority.session_expiry);
    assert_eq!(policy.max_guests(), 8);
    assert!(policy
        .approve(&run.key, &public.body().digest(), run.deadline)
        .is_err());
    policy
        .approve(&run.key, &policy.digest(), run.deadline)
        .unwrap();
    run.authority.max_players = 2;
    assert_eq!(
        super::super::review_host_policy(&run, &http, &challenge, source)
            .unwrap()
            .max_guests(),
        2
    );
    run.authority.join_policy = "approval".into();
    assert!(super::super::review_host_policy(&run, &http, &challenge, source).is_err());
    run.authority.join_policy = "open".into();
    run.authority.session_expiry = lab::unix_millis() + 14_400_001;
    assert!(super::super::review_host_policy(&run, &http, &challenge, source).is_err());
    run.authority.online_origin = None;
    let local = super::super::review_host_policy(&run, &http, &challenge, source).unwrap();
    assert_eq!(local.expires_at_unix_ms(), run.expiry);
    assert_eq!(local.max_guests(), 1);
}

fn initial_delivery(inbox: &PollInbox<'_>, authority: &lab::SecretKey, id: &str) -> Value {
    let run = inbox.run;
    let now = lab::unix_millis();
    let private = run.request.visibility.as_deref() == Some("private");
    let mut fields = vec![
        ("schemaVersion", json!(if private { 6 } else { 2 })),
        ("purpose", json!("socialAdmission")),
        ("issuerInstance", json!(run.authority.issuer)),
        ("grantId", json!(id)),
        ("policyId", json!(POLICY)),
        ("policyDigest", json!(inbox.policy.body().digest())),
        ("sessionId", json!(run.request.session_id)),
        ("sessionDigest", json!(inbox.challenge.session_digest())),
        ("hostAccountId", json!(run.request.account_id)),
        ("hostPublicKey", json!(run.key.public().to_string())),
        ("hostOperationId", json!(run.id)),
        ("hostGeneration", json!(run.generation)),
        (
            "guestAccountId",
            json!(if id == FIRST {
                "first_guest"
            } else {
                "second_guest"
            }),
        ),
        (
            "guestPublicKey",
            json!(lab::generate_key().public().to_string()),
        ),
        ("guestOperationId", json!(uuid::Uuid::new_v4().to_string())),
        ("guestGeneration", json!(1)),
        ("issuedAtUnixMs", json!(now)),
        ("admissionExpiresAtUnixMs", json!(now + 7_000)),
        ("sessionExpiresAtUnixMs", json!(run.expiry)),
    ];
    if private {
        fields.push(("authorizationId", json!("cd".repeat(32))));
        fields.push(("authorizationRevision", json!(1)));
    }
    let grant = signed(authority, "aurora/connect/social-admission/v1\0", &fields);
    let initial = lease(authority, run, inbox.policy, id, 1);
    json!({"grant":grant,"initialLease":initial,"lease":initial})
}

fn lease(
    authority: &lab::SecretKey,
    run: &Run,
    policy: &lab::SignedSocialHostPolicy,
    id: &str,
    sequence: u64,
) -> Value {
    let now = lab::unix_millis();
    signed(
        authority,
        "aurora/connect/social-continuity/v1\0",
        &[
            ("schemaVersion", json!(1)),
            ("purpose", json!("socialContinuity")),
            ("issuerInstance", json!(run.authority.issuer)),
            ("grantId", json!(id)),
            ("policyDigest", json!(policy.body().digest())),
            ("sequence", json!(sequence)),
            ("issuedAtUnixMs", json!(now)),
            ("expiresAtUnixMs", json!(now + 2_500)),
        ],
    )
}

async fn receive_pulse(listener: &TcpListener) -> TcpStream {
    let (mut socket, _) = listener.accept().await.unwrap();
    let mut bytes = Vec::new();
    let expected = json!({"policyId":POLICY}).to_string();
    loop {
        let mut chunk = [0; 512];
        let count = socket.read(&mut chunk).await.unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&chunk[..count]);
        assert!(bytes.len() < 4096);
        let wire = std::str::from_utf8(&bytes).unwrap();
        if let Some((header, body)) = wire.split_once("\r\n\r\n") {
            if body.len() >= expected.len() {
                assert!(header.starts_with("POST /v1/social/host/poll "));
                assert_eq!(body, expected);
                return socket;
            }
        }
    }
}

async fn respond(socket: &mut TcpStream, status: u16, value: Value) {
    let body = value.to_string();
    let reply = format!(
        "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    socket.write_all(reply.as_bytes()).await.unwrap();
}

#[tokio::test]
async fn social_startup_polls_retain_original_pending_delivery_without_reopening_removed_peer() {
    exercise_startup_polls(false).await;
}

#[tokio::test]
async fn social_private_online_polls_preserve_initial_lease_and_removed_peer_tombstone() {
    exercise_startup_polls(true).await;
}

async fn exercise_startup_polls(private: bool) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (run, http, authority, challenge, policy) = fixture_mode(&listener, private);
    let mut inbox = PollInbox {
        run: &run,
        challenge: &challenge,
        policy: &policy,
        source: "127.0.0.1:43001".parse().unwrap(),
        maximum: 128,
        pending: HashMap::new(),
    };
    let mut first = initial_delivery(&inbox, &authority, FIRST);
    let mut second = initial_delivery(&inbox, &authority, SECOND);
    let initial_expiry = first["initialLease"]["body"]["expiresAtUnixMs"]
        .as_u64()
        .unwrap();
    let release = Notify::new();
    let serving = async {
        for sequence in 1..=8 {
            let mut socket = receive_pulse(&listener).await;
            if sequence > 1 {
                first["lease"] = lease(&authority, &run, &policy, FIRST, sequence);
                second["lease"] = lease(&authority, &run, &policy, SECOND, sequence);
            }
            let deliveries = if sequence == 2 {
                vec![first.clone()]
            } else {
                vec![first.clone(), second.clone()]
            };
            respond(
                &mut socket,
                200,
                json!({"state":"group","deliveries":deliveries}),
            )
            .await;
            if sequence == 8 {
                release.notify_one();
            }
        }
    };
    let initializing = poll_while_starting(&run, &http, POLICY, &mut inbox, async {
        release.notified().await;
        Ok(())
    });
    let (result, ()) = tokio::time::timeout(Duration::from_secs(7), async {
        tokio::join!(initializing, serving)
    })
    .await
    .unwrap();
    assert_eq!(result, Ok(()));
    assert!(lab::unix_millis() >= initial_expiry);
    let pending = inbox.take(FIRST).expect("initial signed delivery retained");
    assert_eq!(pending.continuity.sequence(), 8);
    assert!(pending.continuity.check().is_ok());
    assert!(inbox.take(SECOND).is_none());
}

#[tokio::test]
async fn social_startup_poll_failure_cancels_and_collects_original_initializer() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (run, http, _, challenge, policy) = fixture(&listener);
    let mut inbox = PollInbox {
        run: &run,
        challenge: &challenge,
        policy: &policy,
        source: "127.0.0.1:43001".parse().unwrap(),
        maximum: 128,
        pending: HashMap::new(),
    };
    let entered = Notify::new();
    let mut collected = false;
    let serving = async {
        let mut socket = receive_pulse(&listener).await;
        entered.notified().await;
        respond(&mut socket, 403, json!({"code":"policy_expired"})).await;
    };
    let initializing = poll_while_starting(&run, &http, POLICY, &mut inbox, async {
        entered.notify_one();
        run.cancel.cancelled().await;
        collected = true;
        Err::<(), _>(ConnectError::Cancelled)
    });
    let (result, ()) = tokio::time::timeout(Duration::from_secs(3), async {
        tokio::join!(initializing, serving)
    })
    .await
    .unwrap();
    assert_eq!(
        result,
        Err(ConnectError::SocialFailure(
            SocialFailureStage::SessionContinuity,
            SocialFailureReason::AuthorityChanged
        ))
    );
    assert!(collected);
    assert!(run.cancel.is_cancelled());
}
