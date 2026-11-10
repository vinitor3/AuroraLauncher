//! Real schema5 Host-player/ReadyGame gate driven by the actual policy2
//! dedicated fixture. No Java, Minecraft login or audio is inferred.
use super::*;
use serde::Serialize;
use uuid::Uuid;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TicketBody {
    schema_version: u8,
    session_id: String,
    issuer_instance: String,
    peer_id: String,
    host_peer_id: String,
    account_id: String,
    generation: u64,
    issued_at_unix_ms: u64,
    expires_at_unix_ms: u64,
    direct_addresses: Vec<String>,
}
fn signed_ticket(
    run: &Run,
    authority: &lab::SecretKey,
    origin: &str,
    peer: lab::EndpointId,
) -> lab::OnlineNetworkTicket {
    let body = TicketBody {
        schema_version: 1,
        session_id: run.request.session_id.clone(),
        issuer_instance: run.authority.issuer.clone(),
        peer_id: peer.to_string(),
        host_peer_id: run.key.public().to_string(),
        account_id: run.request.account_id.clone(),
        generation: run.generation,
        issued_at_unix_ms: lab::unix_millis(),
        expires_at_unix_ms: run.expiry,
        direct_addresses: vec![],
    };
    let mut bytes = b"aurora/connect/relay-ticket/v1\0".to_vec();
    bytes.extend(serde_json::to_vec(&body).unwrap());
    let signature = authority
        .sign(&bytes)
        .to_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    lab::OnlineNetworkTicket::for_local_test(
        &json!({"body":body,"signature":signature}).to_string(),
        authority.public(),
        &run.authority.issuer,
        &run.request.account_id,
        &run.request.session_id,
        peer,
        run.key.public(),
        origin,
    )
    .unwrap()
}
async fn bounded<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(8), future)
        .await
        .expect("native_social_voice_fixture_deadline")
}
impl ConnectManager {
    pub(crate) async fn exercise_social_host_player_test(
        binding: NativeVoiceHostBinding,
        mode: u8,
        private: bool,
        stop_source: impl FnOnce(),
    ) {
        let authority = lab::generate_key();
        let manager = ConnectManager::with_social_lab(SocialLabAuthority {
            pin: authority.public(),
            issuer: "33".repeat(32),
            online_origin: Some("https://native-fixture.invalid"),
            private_invitations: private,
            join_policy: "open".into(),
            session_expiry: lab::unix_millis() + 60_000,
            max_players: 8,
        });
        let reservation = manager
            .social_reserve_native_voice(
                "main",
                SocialDiscoveredReserveRequest {
                    account_epoch: 0,
                    account_id: "native_host_fixture".into(),
                    session_id: Uuid::new_v4().to_string(),
                    visibility: if private { "private" } else { "friends" }.into(),
                },
                binding.clone(),
                true,
            )
            .unwrap();
        let mut run = {
            let state = manager.live.state.lock().unwrap();
            let attempt = state.attempt.as_ref().unwrap();
            let control = attempt.social.as_ref().unwrap();
            Run {
                state: manager.live.state.clone(),
                id: reservation.operation_id.clone(),
                generation: reservation.generation,
                request: control.request.clone(),
                selection: None,
                native_voice_source: Some(binding.clone()),
                native_voice_local_key: control.native_voice_local_key.clone(),
                key: control.key.clone().unwrap(),
                authority: manager.live.social_authority.clone().unwrap(),
                clock: None,
                deadline: binding.deadline(),
                expiry: lab::unix_millis()
                    + binding
                        .deadline()
                        .saturating_duration_since(Instant::now())
                        .as_millis() as u64,
                cancel: attempt.cancel.clone(),
                source_gate: None,
                guard_probe: None,
            }
        };
        let now = lab::unix_millis();
        let challenge = lab::SocialKeyChallenge::parse(&json!({"schemaVersion":1,"purpose":"accountKeyBinding","issuerInstance":run.authority.issuer,
            "challengeId":"11".repeat(32),"accountId":run.request.account_id,"operationId":run.id,"generation":run.generation,"role":"host",
            "publicKey":run.key.public().to_string(),"sessionId":run.request.session_id,"sessionDigest":"44".repeat(32),
            "issuedAtUnixMs":now,"expiresAtUnixMs":now+50_000}).to_string()).unwrap();
        let http = Http {
            client: reqwest::Client::new(),
            token: Arc::new(Mutex::new("fixture-token".into())),
            cancel: run.cancel.clone(),
            deadline: run.deadline,
            origin: run.authority.online_origin.unwrap(),
            session_id: run.request.session_id.clone(),
            online: true,
        };
        let policy =
            super::super::super::review_host_policy(&run, &http, &challenge, binding.targets().0)
                .unwrap();
        assert_eq!(
            serde_json::to_value(&policy).unwrap()["schemaVersion"],
            if private { 7 } else { 5 }
        );
        assert_eq!(policy.max_guests(), 7);
        assert!(policy.expires_at_unix_ms() <= run.authority.session_expiry);
        run.expiry = run.expiry.min(policy.expires_at_unix_ms());
        let policy = policy
            .approve(&run.key, &policy.digest(), run.deadline)
            .unwrap();
        let remote_keys = [lab::generate_key(), lab::generate_key()];
        let relay = lab::LocalRelayFixture::start_native_group(
            [
                run.key.public(),
                remote_keys[0].public(),
                remote_keys[1].public(),
            ],
            lab::LocalRelayProfile::Normal,
        )
        .await
        .unwrap();
        let origin = format!("http://{}", relay.bound_addr());
        let ticket = signed_ticket(&run, &authority, &origin, run.key.public());
        let mut group = Group::new(&run);
        let invitation = lab::SessionInvitation::issue_online_native_voice(
            &run.key,
            run.native_voice_local_key.as_ref().unwrap().public(),
            &run.request.session_id,
            &group.services(binding.targets().0).unwrap(),
            lab::SessionDeadlines {
                admission_expires_at_unix_ms: run.expiry.min(lab::unix_millis() + 60_000),
                session_expires_at_unix_ms: run.expiry,
            },
        )
        .unwrap();
        let mut host = None;
        let mut sockets = Vec::new();
        let mut local = LocalPlayer::default();
        bounded(start_host(
            &run,
            &TargetGuard::Manual,
            binding.targets().0,
            &mut group,
            &mut host,
            &mut sockets,
            &invitation,
            &ticket,
        ))
        .await
        .unwrap();
        bounded(local.start(
            &run,
            host.as_ref().unwrap(),
            group.voice.as_mut().unwrap(),
            run.native_voice_local_key.as_ref().unwrap(),
            &invitation,
            &policy,
            &ticket,
        ))
        .await
        .unwrap();
        run.stage("waitingGuest", "hostListening");
        let original = local.authority.as_ref().unwrap().clone();
        let mut observed_failure = None;
        assert!(
            original.with_current(|| Ok(())).is_err(),
            "no Host poll cannot permit a spawn"
        );
        local
            .refresh_poll(&run, &json!({"checkedAtUnixMs":lab::unix_millis()}))
            .unwrap();
        assert!(original.with_current(|| Ok(())).is_ok());
        let runtime = binding.online_runtime_profile().unwrap();
        manager
            .authorize_ready_copy_install("main", &run.id, runtime)
            .unwrap();
        let foreign = crate::engine::connect_ready_profile::ReadyProfile::ALL
            .into_iter()
            .find(|profile| *profile != runtime)
            .unwrap();
        assert_eq!(
            manager.authorize_ready_copy_install("main", &run.id, foreign),
            Err(ReadyGameLaunchError::UnsupportedRuntime),
        );
        if mode >= 4 {
            monitor_remote_leases(
                &run,
                &authority,
                &challenge,
                &policy,
                &origin,
                &remote_keys,
                &mut group,
                &host,
                mode == 5,
            )
            .await;
            assert!(
                group
                    .voice
                    .as_ref()
                    .unwrap()
                    .query
                    .as_ref()
                    .is_some_and(|query| !query.task.is_finished()),
                "real query is still owned while peers are revoked"
            );
            assert!(local
                .guest
                .as_ref()
                .unwrap()
                .connection()
                .close_reason()
                .is_none());
            stop_source(); // In this case release the real blocked QueryJob, not its owner.
        } else {
            let backend = super::super::super::ready_fixture::Fixture::new("spawn").unwrap();
            let prepared = manager
                .live
                .prepare_ready_game(
                    "main",
                    super::super::super::ready_fixture::Fixture::request(&run.id),
                    Arc::new(backend.clone()),
                )
                .unwrap();
            let reviewed = bounded(async {
                loop {
                    let value = manager.live.ready_game_snapshot("main", &run.id).unwrap();
                    if value.phase == "reviewed" {
                        break value;
                    }
                    assert!(
                        value.phase != "failed",
                        "native Host player preparation failed"
                    );
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await;
            let review = reviewed.review.unwrap();
            assert!(review.voice.is_some());
            manager
                .live
                .confirm_ready_game(
                    "main",
                    &run.id,
                    prepared.plan_id.as_ref().unwrap(),
                    &review.digest,
                )
                .unwrap();
            bounded(async {
                while backend.snapshot()["spawnEntered"] != true {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await;
            match mode {
                0 => (),
                1 => stop_source(),
                2 => {
                    manager.live.state.lock().unwrap().social_epoch += 1;
                    assert_eq!(
                        manager.authorize_ready_copy_install("main", &run.id, runtime),
                        Err(ReadyGameLaunchError::Unavailable),
                    );
                }
                3 => {
                    run.observe_host_poll(Duration::from_millis(2000), None);
                    let error = local
                        .refresh_poll(
                            &run,
                            &json!({"checkedAtUnixMs":lab::unix_millis().saturating_sub(3001)}),
                        )
                        .unwrap_err();
                    assert_eq!(
                        error,
                        ConnectError::SocialFailure(
                            SocialFailureStage::SessionContinuity,
                            SocialFailureReason::ContinuityLost
                        )
                    );
                    assert_eq!(local.tick(&run).await, Err(error));
                    assert!(
                        run.deadline > Instant::now(),
                        "the original session has not expired"
                    );
                    observed_failure = Some(error);
                }
                _ => panic!("closed fixture mode"),
            }
            backend.release();
            let ended = bounded(async {
                loop {
                    let value = manager.live.ready_game_snapshot("main", &run.id).unwrap();
                    if matches!(value.phase, "started" | "failed" | "cancelled" | "expired") {
                        break value;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await;
            assert_eq!(backend.snapshot()["spawns"], if mode == 0 { 1 } else { 0 });
            if mode == 0 {
                assert_eq!(ended.phase, "started");
            } else {
                assert_ne!(ended.phase, "started");
            }
            if mode == 2 {
                assert_eq!(backend.snapshot()["epochRejectedBeforeSpawn"], true);
            }
            backend.cleanup();
        }
        local.invalidate(&run);
        host.as_ref().unwrap().shutdown().await;
        local.shutdown(&run).await.unwrap();
        group.voice.as_mut().unwrap().finish().await;
        drop(host.take());
        relay.shutdown().await.unwrap();
        release(&sockets, None).await.unwrap();
        if let Some(error) = observed_failure {
            super::super::super::super::finish(&run.state, &run.id, Some(error));
            let snapshot = manager.snapshot("main").unwrap();
            assert_eq!(snapshot.phase, "failed");
            assert_eq!(snapshot.error_code, Some("connect_social_continuity_lost"));
            let social = snapshot.social.unwrap();
            assert_eq!(
                social.failure_stage,
                Some(SocialFailureStage::SessionContinuity)
            );
            let observed = social.failure_observation.unwrap();
            assert_eq!(observed.gate, SocialFailureGate::HostPoll);
            assert_eq!(observed.host_poll_elapsed_ms, Some(2000));
            assert!(observed.session_remaining_ms > 0);
        }
        manager.shutdown_all().await;
    }
}

fn sign_fields(authority: &lab::SecretKey, domain: &str, body: Value, names: &[&str]) -> String {
    let mut bytes = domain.as_bytes().to_vec();
    for name in names {
        let field = body[name]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| body[name].to_string());
        bytes.extend_from_slice(&(field.len() as u16).to_be_bytes());
        bytes.extend(field.as_bytes());
    }
    let signature = authority
        .sign(&bytes)
        .to_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    json!({"body":body,"signature":signature}).to_string()
}
#[allow(clippy::too_many_arguments)]
async fn monitor_remote_leases(
    run: &Run,
    authority: &lab::SecretKey,
    challenge: &lab::SocialKeyChallenge,
    policy: &lab::SignedSocialHostPolicy,
    origin: &str,
    keys: &[lab::SecretKey; 2],
    group: &mut Group,
    host: &Option<Arc<lab::Host>>,
    kick: bool,
) {
    let mut guests = Vec::new();
    let mut records = Vec::new();
    for (index, key) in keys.iter().enumerate() {
        let now = lab::unix_millis();
        let id = if index == 0 {
            "55".repeat(32)
        } else {
            "66".repeat(32)
        };
        let (profile, digest) = group
            .voice
            .as_ref()
            .unwrap()
            .source()
            .online_voice_policy()
            .unwrap();
        let private = run.request.visibility.as_deref() == Some("private");
        let mut body = json!({"schemaVersion":if private { 7 } else { 5 },"purpose":"socialAdmission","issuerInstance":run.authority.issuer,
            "grantId":id,"policyId":challenge.challenge_id(),"policyDigest":policy.body().digest(),"sessionId":run.request.session_id,
            "sessionDigest":challenge.session_digest(),"hostAccountId":run.request.account_id,"hostPublicKey":run.key.public().to_string(),
            "hostOperationId":run.id,"hostGeneration":run.generation,"guestAccountId":format!("remote_{index}"),"guestPublicKey":key.public().to_string(),
            "guestOperationId":Uuid::new_v4().to_string(),"guestGeneration":1,"issuedAtUnixMs":now,"admissionExpiresAtUnixMs":now+9000,
            "sessionExpiresAtUnixMs":run.expiry,"voiceProfile":profile,"voiceSourceDigest":digest,"hostPlayer":true,
            "runtimeProfile":policy.body().runtime_profile().unwrap(),"voiceTransportProfile":"nativeVoice8V1"});
        let mut names = vec![
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
        ];
        if private {
            body["authorizationId"] = json!(if index == 0 {
                "aa".repeat(32)
            } else {
                "bb".repeat(32)
            });
            body["authorizationRevision"] = json!(3);
            names.extend(["authorizationId", "authorizationRevision"]);
        }
        let encoded = sign_fields(
            authority,
            "aurora/connect/social-admission/v1\0",
            body,
            &names,
        );
        let grant = lab::VerifiedSocialGrant::for_host(
            &encoded,
            authority.public(),
            challenge,
            policy,
            group.voice.as_ref().unwrap().source().targets().0,
            run.deadline,
        )
        .unwrap();
        let invitation = lab::SessionInvitation::issue_online_native_voice(
            &run.key,
            key.public(),
            &run.request.session_id,
            &group
                .services(group.voice.as_ref().unwrap().source().targets().0)
                .unwrap(),
            lab::SessionDeadlines {
                admission_expires_at_unix_ms: grant.admission_expires_at_unix_ms(),
                session_expires_at_unix_ms: grant.session_expires_at_unix_ms(),
            },
        )
        .unwrap();
        host.as_ref().unwrap().admit_online(&invitation).unwrap();
        group
            .voice
            .as_mut()
            .unwrap()
            .register_lease(host.as_ref().unwrap(), &invitation)
            .await
            .unwrap();
        let ticket = signed_ticket(run, authority, origin, key.public());
        let guest = bounded(lab::Guest::connect_invited_online_native_voice(
            key.clone(),
            &invitation,
            native_voice_limits(),
            &ticket,
        ))
        .await
        .unwrap();
        let stream = bounded(guest.open_tcp("game", 1)).await.unwrap();
        drop(stream);
        guests.push(guest);
        records.push((id, grant, invitation));
    }
    for (index, (id, grant, invitation)) in records.into_iter().enumerate() {
        let now = lab::unix_millis();
        let body = json!({"schemaVersion":1,"purpose":"socialContinuity","issuerInstance":run.authority.issuer,"grantId":id,
            "policyDigest":policy.body().digest(),"sequence":1,"issuedAtUnixMs":now,"expiresAtUnixMs":now+if index==0 && !kick {300} else {3000}});
        let packet = sign_fields(
            authority,
            "aurora/connect/social-continuity/v1\0",
            body,
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
        );
        let continuity = lab::SocialContinuity::start(&packet, &grant).unwrap();
        group.participants.insert(
            id,
            Participant {
                grant,
                continuity,
                invitation,
                admitted: true,
                terminal: false,
            },
        );
    }
    if kick {
        run.state
            .lock()
            .unwrap()
            .attempt
            .as_mut()
            .unwrap()
            .social
            .as_mut()
            .unwrap()
            .revocations
            .insert("55".repeat(32));
    }
    let stalled = "77".repeat(32);
    let admission_expires_at_unix_ms = lab::unix_millis() + 25;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let candidate = lab::SessionInvitation::issue_online_native_voice(
        &run.key,
        lab::generate_key().public(),
        &run.request.session_id,
        &group
            .services(group.voice.as_ref().unwrap().source().targets().0)
            .unwrap(),
        lab::SessionDeadlines {
            admission_expires_at_unix_ms,
            session_expires_at_unix_ms: run.expiry,
        },
    )
    .map_err(|_| ConnectError::InvitationRejected);
    assert!(group
        .accept_invitation(&stalled, candidate)
        .unwrap()
        .is_none());
    assert!(group.rejected.contains(&stalled));
    assert!(bounded(guests[1].open_tcp("game", 1)).await.is_ok());
    let end = Instant::now() + Duration::from_millis(2200);
    while Instant::now() < end {
        group.monitor(run, host).await.unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(group.participants[&"55".repeat(32)].terminal);
    assert!(!group.participants[&"66".repeat(32)].terminal);
    assert!(guests[0].connection().close_reason().is_some());
    assert!(guests[1].connection().close_reason().is_none());
    assert!(matches!(
        group.voice.as_ref().unwrap().state(),
        "retired" | "retiring"
    ));
    let stream = bounded(guests[1].open_tcp("game", 1)).await.unwrap();
    drop(stream);
    assert!(matches!(
        bounded(guests[1].open_udp_fragmented("aux", 1)).await,
        Err(lab::Error::ServiceDenied)
    ));
    for guest in guests {
        guest.shutdown().await;
    }
}
