use super::*;

fn report(event: &str, detail: Value) {
    println!(
        "AURORA_HOST_POLL {}",
        json!({"event":event,"detail":detail})
    );
}

fn take_env(name: &str) -> String {
    let value = std::env::var(name).expect("missing explicitly supplied diagnostic input");
    std::env::remove_var(name);
    value
}

/// The Node runner owns the temporary advertisement. This deliberately never
/// creates a listener, relay ticket, guest grant, or connected presence.
#[tokio::test(flavor = "current_thread")]
#[ignore = "requires an explicitly authorized temporary own advertisement and in-memory Firebase token"]
async fn social_host_poll_real_http_diagnostic() {
    let token = take_env("AURORA_BOOTSTRAP_TOKEN");
    let account = take_env("AURORA_BOOTSTRAP_ACCOUNT");
    let session = take_env("AURORA_BOOTSTRAP_SESSION");
    assert!(online_token_destination(&token, &account));
    assert!(uuid::Uuid::parse_str(&session).is_ok());
    let authority = SocialLabAuthority::from_compiled_online().expect("compile the online origin");
    assert!(
        authority.online_origin
            == Some("https://aurora-connect-social.aurora-edge-proxy.workers.dev")
    );
    let mut run = Run {
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
    let mut http = Http::new(token, &run).expect("diagnostic client construction failed");
    let mut policy_id = None;
    let mut stage = "bootstrap";
    let result = async {
        bootstrap_clock(&mut run, &mut http).await?;
        let clock = run.clock()?;
        report(
            "clock",
            json!({
                "roundTripMs":clock.bootstrap_round_trip().as_millis(),
                "uncertaintyMs":clock.bootstrap_uncertainty().as_millis(),
                "upperMinusWallMs":i128::from(clock.upper_now())-i128::from(lab::unix_millis()),
            }),
        );
        if run.authority.join_policy != "approval" || run.authority.max_players != 2 {
            return Err(ConnectError::InvalidRequest);
        }
        stage = "challenge";
        let (challenge, proof) = challenge(&run, &http, None).await?;
        stage = "policy";
        let policy = lab::SocialHostPolicy::review_online_voice_with_clock(
            &challenge,
            "friends",
            SocketAddr::from(([127, 0, 0, 1], 43001)),
            "approval",
            run.authority.session_expiry,
            "svc",
            &"5a".repeat(32),
            true,
            run.clock.as_ref(),
        )
        .and_then(|policy| policy.with_runtime_profile("fabric-loader-0.16.14-1.21.1"))
        .and_then(lab::SocialHostPolicy::with_native_voice_transport)
        .and_then(|policy| policy.with_guest_limit(1))
        .map_err(|_| ConnectError::InvitationRejected)?;
        let digest = policy.digest();
        let policy = policy
            .approve_with_clock(&run.key, &digest, run.deadline, run.clock.as_ref())
            .map_err(|_| ConnectError::InvitationRejected)?;
        let expiry = policy.body().expires_at_unix_ms();
        run.deadline = run
            .clock()?
            .deadline_for(expiry)
            .map_err(|_| ConnectError::PlanExpired)?;
        run.expiry = expiry;
        http.deadline = run.deadline;
        let id = policy.body().policy_id().to_owned();
        policy_id = Some(id.clone());
        stage = "register";
        let start = Instant::now();
        let response = http
            .post(
                SocialRoute::HostRegister,
                json!({"proof":proof,"policySigned":policy}),
            )
            .await?;
        if http.reply_field(SocialRoute::HostRegister, &response, "policyId")? != id
            || http.reply_field(SocialRoute::HostRegister, &response, "policyDigest")? != digest
        {
            return Err(http.invalid_reply(SocialRoute::HostRegister));
        }
        report(
            "registered",
            json!({"roundTripMs":start.elapsed().as_millis()}),
        );
        stage = "poll";
        poll(&run, &http, &id).await
    }
    .await;
    if let Err(error) = result {
        report("failure", json!({"stage":stage,"code":error.code()}));
    }
    if let Some(id) = policy_id {
        // A timed-out registration can still commit. Stop only this exact ID.
        http.deadline = Instant::now() + Duration::from_secs(2);
        let stopped = http
            .post(SocialRoute::HostStop, json!({"policyId":id}))
            .await;
        match stopped {
            Ok(body) => report("stop", json!({"acknowledged":body["stopped"]==true})),
            Err(error) => report("stop", json!({"acknowledged":false,"code":error.code()})),
        }
    }
}

async fn poll(run: &Run, http: &Http, policy_id: &str) -> Result<(), ConnectError> {
    let began = Instant::now();
    let end = began + Duration::from_secs(30);
    let clock = run.clock()?;
    let mut cadence = interval(Duration::from_millis(500));
    cadence.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut last_checked: Option<u64> = None;
    let mut index = 0;
    let mut valid = 0;
    loop {
        tokio::select! { biased;
            _ = tokio::time::sleep_until(end.into()) => break,
            _ = cadence.tick() => {},
        }
        index += 1;
        let started = Instant::now();
        let issued = run.server_issue_time();
        let request = http.post(
            SocialRoute::HostPoll,
            json!({"policyId":policy_id,"transport":{
                "observedAtUnixMs":issued,"connectedGrantIds":[],"hostPlayerConnected":false,
            }}),
        );
        let response = request.await;
        let upper = clock.upper_now();
        let prior_age = last_checked.map(|checked| i128::from(upper) - i128::from(checked));
        let prior_fresh = last_checked.map(|checked| {
            clock
                .deadline_for(checked.saturating_add(3000))
                .is_ok_and(|deadline| Instant::now() < deadline)
        });
        let checked = response.as_ref().ok().and_then(|value| {
            (value["state"] == "group" && value["deliveries"].as_array().is_some_and(Vec::is_empty))
                .then(|| value["checkedAtUnixMs"].as_u64())
                .flatten()
        });
        let age = checked.map(|checked| i128::from(upper) - i128::from(checked));
        let code = match (response, checked, age) {
            (Err(error), _, _) => Some(error.code()),
            (_, None, _) => Some("invalid_reply"),
            (_, Some(checked), Some(age))
                if !(0..3000).contains(&age)
                    || last_checked.is_some_and(|previous| checked < previous) =>
            {
                Some("checked_at_not_fresh")
            }
            _ if prior_fresh == Some(false) => Some("previous_freshness_elapsed"),
            _ => None,
        };
        report(
            "poll",
            json!({
                "index":index,"elapsedMs":began.elapsed().as_millis(),"roundTripMs":started.elapsed().as_millis(),
                "checkedAtAgeUpperMs":age,"previousCheckedAtAgeUpperMs":prior_age,
                "observationAgeUpperMs":upper.saturating_sub(issued),
                "freshnessRemainingMs":age.map(|age| (3000-age).clamp(0,3000)),
                "sessionRemainingMs":run.deadline.saturating_duration_since(Instant::now()).as_millis(),
                "code":code,
            }),
        );
        if code.is_some() {
            break;
        }
        last_checked = checked;
        valid += 1;
    }
    report(
        "summary",
        json!({"attempted":index,"fresh":valid,"elapsedMs":began.elapsed().as_millis(),"fullWindow":valid==index && Instant::now()>=end}),
    );
    Ok(())
}
