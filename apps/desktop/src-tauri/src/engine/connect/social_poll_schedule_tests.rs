//! Exercise the actor's real poll wait/monitor, with no Java or game endpoint.
use super::*;

async fn wait(
    run: &Run,
    tick: &mut tokio::time::Interval,
    querying: impl Future<Output = Result<Value, ConnectError>>,
) -> Result<Option<Value>, ConnectError> {
    let mut group = Group::new(run);
    #[cfg(all(windows, feature = "connect-voice-lab"))]
    let mut local = voice::LocalPlayer::default();
    wait_host_poll(
        run,
        &mut group,
        &None,
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        &mut local,
        tick,
        querying,
        Instant::now(),
    )
    .await
}

fn overdue_tick() -> (tokio::time::Interval, tokio::time::Instant) {
    let due = tokio::time::Instant::now() - Duration::from_secs(2);
    let mut tick = maintenance_interval();
    assert_eq!(
        tick.missed_tick_behavior(),
        tokio::time::MissedTickBehavior::Skip
    );
    tick.reset_at(due);
    (tick, due)
}

#[tokio::test(flavor = "current_thread")]
async fn social_host_poll_ready_response_preempts_overdue_maintenance() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (run, _, _, challenge, policy) = fixture(&listener);
    let (mut tick, due) = overdue_tick();
    let reply = json!({"state":"group","checkedAtUnixMs":lab::unix_millis(),"deliveries":[]});
    let observed = wait(&run, &mut tick, std::future::ready(Ok(reply.clone())))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(observed, reply);
    // No old tick was consumed before the available reply. Under the old
    // biased order this yields a later timestamp, after catch-up maintenance.
    assert_eq!(tick.tick().await, due);
    // Skip retains one current validation and schedules the next in the future.
    assert!(tick.tick().await > due + Duration::from_secs(1));
    let mut inbox = PollInbox {
        run: &run,
        challenge: &challenge,
        policy: &policy,
        source: "127.0.0.1:43001".parse().unwrap(),
        maximum: 8,
        pending: HashMap::new(),
    };
    inbox.observe(&observed).unwrap();
    assert!(inbox.pending.is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn social_host_poll_reply_cannot_bypass_original_expiry_or_cancel() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (mut run, _, _, _, _) = fixture(&listener);
    run.deadline = Instant::now() - Duration::from_millis(1);
    let (mut tick, due) = overdue_tick();
    let reply = json!({"state":"group","checkedAtUnixMs":lab::unix_millis(),"deliveries":[]});
    assert_eq!(
        wait(&run, &mut tick, std::future::ready(Ok(reply))).await,
        Err(ConnectError::PlanExpired)
    );
    assert_eq!(tick.tick().await, due);

    // Cancellation wins even over an already ready error/reply. It remains
    // normal Stop (None), not a new failure diagnostic for the enclosing actor.
    run.cancel.cancel();
    let queried = std::cell::Cell::new(false);
    assert_eq!(
        wait(&run, &mut tick, async {
            queried.set(true);
            Err(ConnectError::Internal)
        })
        .await,
        Ok(None)
    );
    assert!(!queried.get());
}

#[tokio::test(flavor = "current_thread")]
async fn social_host_poll_pending_reply_keeps_monitor_and_cancellation_live() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (mut run, _, _, _, _) = fixture(&listener);
    run.deadline = Instant::now() - Duration::from_millis(1);
    let (mut tick, _) = overdue_tick();
    assert_eq!(
        wait(&run, &mut tick, std::future::pending()).await,
        Err(ConnectError::PlanExpired)
    );
    run.cancel.cancel();
    assert_eq!(
        wait(&run, &mut tick, std::future::pending()).await,
        Ok(None)
    );
}
