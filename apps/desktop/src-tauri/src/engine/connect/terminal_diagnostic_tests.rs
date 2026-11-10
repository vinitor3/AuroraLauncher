use super::*;

#[tokio::test]
async fn connect_terminal_observation_is_allowlisted_once_and_outside_actor_lock() {
    let original = tests::manager();
    let state = Arc::downgrade(&original.state);
    let records = Arc::new(Mutex::new(Vec::new()));
    let received = records.clone();
    let manager = original.with_terminal_recorder(Arc::new(move |record| {
        assert!(state.upgrade().unwrap().try_lock().is_ok());
        received.lock().unwrap().push(record);
    }));
    let request = SocialReserveRequest {
        account_epoch: 0,
        account_id: "private-account-marker".into(),
        session_id: uuid::Uuid::new_v4().to_string(),
        role: "guest".into(),
        game_target: None,
        visibility: None,
    };
    let id = manager
        .social_reserve("main", request)
        .unwrap()
        .operation_id;
    {
        let mut state = manager.state.lock().unwrap();
        let a = state.attempt.as_mut().unwrap();
        a.social.as_mut().unwrap().observation.failure = Some(SocialFailureObservation {
            gate: SocialFailureGate::HostPoll,
            bootstrap_roundtrip_ms: Some(702),
            bootstrap_uncertainty_ms: Some(352),
            host_poll_elapsed_ms: Some(1756),
            host_poll_age_ms: Some(3009),
            session_remaining_ms: 14_086_000,
        });
    }
    let error = ConnectError::SocialFailure(
        SocialFailureStage::SessionContinuity,
        SocialFailureReason::ContinuityLost,
    );
    finish(&manager.state, &id, Some(error));
    finish(&manager.state, &id, Some(error));
    let values = records.lock().unwrap();
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].sequence(), 1);
    let value = serde_json::to_value(&values[0]).unwrap();
    assert!(ConnectTerminalDiagnostic::valid_stored(&value));
    assert_eq!(value["phase"], "failed");
    assert_eq!(value["errorCode"], error.code());
    assert_eq!(value["observation"]["gate"], "hostPoll");
    assert_eq!(value["observation"]["hostPollAgeMs"], 3009);
    let encoded = value.to_string();
    for forbidden in [
        &id,
        "private-account-marker",
        "token",
        "peer",
        "sessionId",
        "sequence",
        "digest",
        "gameTarget",
    ] {
        assert!(
            !encoded.contains(forbidden),
            "forbidden diagnostic field/value"
        );
    }
    for key in ["token", "operationId", "peer", "path"] {
        let mut tampered = value.clone();
        tampered[key] = serde_json::json!("secret");
        assert!(!ConnectTerminalDiagnostic::valid_stored(&tampered));
    }
    let mut tampered = value.clone();
    tampered["errorCode"] = serde_json::json!("raw private upstream error");
    assert!(!ConnectTerminalDiagnostic::valid_stored(&tampered));
    let mut tampered = value;
    tampered["observation"]["hostPollAgeMs"] = serde_json::json!(u64::MAX);
    assert!(!ConnectTerminalDiagnostic::valid_stored(&tampered));
}

#[tokio::test]
async fn connect_terminal_closed_is_not_failure_and_recorder_panic_is_fail_soft() {
    let original = tests::manager();
    let state = original.state.clone();
    let records = Arc::new(Mutex::new(Vec::new()));
    let received = records.clone();
    let manager = original.with_terminal_recorder(Arc::new(move |record| {
        assert!(state.try_lock().is_ok());
        received.lock().unwrap().push(record);
        panic!("controlled diagnostic recorder failure");
    }));
    let id = manager
        .social_reserve(
            "main",
            SocialReserveRequest {
                account_epoch: 0,
                account_id: "fixture".into(),
                session_id: uuid::Uuid::new_v4().to_string(),
                role: "guest".into(),
                game_target: None,
                visibility: None,
            },
        )
        .unwrap()
        .operation_id;
    manager.stop("main", &id).await.unwrap();
    let snapshot = manager.snapshot("main").unwrap();
    assert_eq!(snapshot.phase, "closed");
    assert_eq!(snapshot.cleanup_state, "complete");
    assert!(snapshot.error_code.is_none());
    assert_eq!(manager.state.lock().unwrap().terminal_sequence, 1);
    let records = records.lock().unwrap();
    assert_eq!(records.len(), 1);
    let value = serde_json::to_value(&records[0]).unwrap();
    assert!(ConnectTerminalDiagnostic::valid_stored(&value));
    assert_eq!(value["phase"], "closed");
    assert!(value["errorCode"].is_null() && value["observation"].is_null());
}
