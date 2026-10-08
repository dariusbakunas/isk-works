use super::*;

// --- Cooperative shutdown --------------------------------------------
//
// Each of these drives a refresh to a specific suspension point (a
// request-semaphore wait, a retry backoff sleep, an in-flight fan-out),
// cancels the token, and asserts the refresh returns *without the
// virtual clock advancing* -- and that a cancelled refresh persists
// nothing and issues no further ESI request.

/// A `refresh_type` blocked on the 4-permit request semaphore
/// returns promptly as `Skipped` the moment the token is cancelled --
/// it never acquires a permit and never issues its ESI request.
#[tokio::test]
async fn shutdown_while_waiting_on_semaphore_exits_promptly() {
    let repository = Arc::new(RecordingRepository::default());
    // Park every request that reaches the wire; the test never releases
    // them, so a permit holder stays in flight until cancellation drops
    // its request future.
    let transport = RateLimitProbeTransport::new(0, 64);
    let service = PublicMarketService::new(repository.clone(), transport.clone());
    let workspace_id = WorkspaceId::new();
    let source_id = PriceSourceId::new();
    let cancel = CancellationToken::new();

    // Five concurrent refreshes; only four can hold a request permit, so
    // the fifth is parked on `request_limit.acquire()` inside
    // `regional_market_request` having issued nothing.
    let mut tasks = Vec::new();
    for type_id in 34i64..39 {
        let service = service.clone();
        let cancel = cancel.clone();
        tasks.push(tokio::spawn(async move {
            service
                .refresh_type(
                    workspace_id,
                    source_id,
                    JITA_SCOPE,
                    coverage(MarketCoverageRegistration {
                        type_id,
                        type_name: format!("Item{type_id}"),
                    }),
                    &cancel,
                )
                .await
        }));
    }

    while transport.parked_count() < 4 {
        drain(16).await;
    }
    assert_eq!(
        transport.call_instants().len(),
        4,
        "only the four permit holders reached the wire; the fifth is queued on the semaphore"
    );

    // Shutdown. No virtual time is advanced past here: a prompt exit
    // means the futures resolve on the cancellation alone.
    cancel.cancel();
    drain(64).await;

    let mut outcomes = Vec::new();
    for task in tasks {
        outcomes.push(task.await.unwrap());
    }
    assert!(
        outcomes
            .iter()
            .all(|o| matches!(o, MarketRefreshOutcome::Skipped)),
        "every refresh returns Skipped on shutdown, never Failed: {outcomes:?}"
    );
    assert_eq!(
        transport.call_instants().len(),
        4,
        "the semaphore-queued refresh must never issue its ESI request"
    );
    assert!(repository.completed.lock().unwrap().is_empty());
    assert!(repository.failures.lock().unwrap().is_empty());
}

/// A `refresh_type` parked in the 250ms temporary-failure retry
/// backoff returns immediately on cancel -- no second attempt, treated
/// as `Skipped`, nothing persisted. Proves the retry sleep is
/// `select!`ed against cancellation, not waited out.
#[tokio::test(start_paused = true)]
async fn shutdown_during_retry_or_cooldown_sleep_exits_promptly() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![
        Err(EsiError::TemporaryFailure),
        Ok(response(1, 101, JITA_LOCATION_ID)),
    ]);
    let service = PublicMarketService::new(repository.clone(), transport.clone());
    let cancel = CancellationToken::new();

    let refresh = service.refresh_type(
        WorkspaceId::new(),
        PriceSourceId::new(),
        JITA_SCOPE,
        coverage(MarketCoverageRegistration {
            type_id: 34,
            type_name: "Tritanium".to_string(),
        }),
        &cancel,
    );
    let controller = async {
        drain(64).await;
        assert_eq!(
            transport.calls.load(Ordering::SeqCst),
            1,
            "attempt 1 ran and failed; the loop is now in the 250ms backoff"
        );
        cancel.cancel();
        drain(64).await;
    };
    let (outcome, ()) = tokio::join!(refresh, controller);

    assert!(
        matches!(outcome, MarketRefreshOutcome::Skipped),
        "cancelled mid-backoff -> Skipped, not Failed: {outcome:?}"
    );
    assert_eq!(
        transport.calls.load(Ordering::SeqCst),
        1,
        "no retry attempt after cancel"
    );
    assert!(repository.completed.lock().unwrap().is_empty());
    assert!(
        repository.failures.lock().unwrap().is_empty(),
        "shutdown is not a refresh failure"
    );
}

/// Cancelling mid-fan-out through `refresh_source` -> the parent
/// join completes promptly, every queued `refresh_type` task that had
/// not yet passed the request semaphore issues nothing, and no fake
/// `failed`/`complete` state is written. And a pass started under an
/// already-cancelled token dispatches nothing at all.
#[tokio::test]
async fn shutdown_during_market_fan_out_stops_queued_work() {
    let repository = Arc::new(RecordingRepository::default());
    // Eight due candidates, four request permits.
    *repository.candidates.lock().unwrap() = (34i64..42)
        .map(|type_id| {
            coverage(MarketCoverageRegistration {
                type_id,
                type_name: format!("Item{type_id}"),
            })
        })
        .collect();
    let transport = RateLimitProbeTransport::new(0, 64);
    let service = PublicMarketService::new(repository.clone(), transport.clone());
    let workspace_id = WorkspaceId::new();
    let source_id = PriceSourceId::new();
    let cancel = CancellationToken::new();

    let refresh = {
        let service = service.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move {
            service
                .refresh_source(workspace_id, source_id, &cancel)
                .await
        })
    };

    while transport.parked_count() < 4 {
        drain(16).await;
    }
    assert_eq!(
        transport.call_instants().len(),
        4,
        "only four of the eight fan-out tasks reached the wire"
    );
    assert!(!refresh.is_finished());

    cancel.cancel();
    drain(64).await;

    assert!(
        refresh.is_finished(),
        "the parent join completes promptly once children observe cancellation"
    );
    refresh.await.unwrap().unwrap();
    assert_eq!(
        transport.call_instants().len(),
        4,
        "no queued fan-out task issued an ESI request after cancellation"
    );
    assert!(repository.completed.lock().unwrap().is_empty());
    assert!(
        repository.failures.lock().unwrap().is_empty(),
        "no fake failure persisted for shutdown"
    );
    // Leased rows carry a finite recovery deadline (storage-level
    // recovery: `expired_market_refresh_lease_becomes_due_and_can_be_reclaimed`).
    assert!(!repository.lease_deadlines.lock().unwrap().is_empty());

    // A pass entered under an already-cancelled token spawns nothing.
    let precancelled = CancellationToken::new();
    precancelled.cancel();
    let calls_before = transport.call_instants().len();
    service
        .refresh_source(workspace_id, source_id, &precancelled)
        .await
        .unwrap();
    assert_eq!(
        transport.call_instants().len(),
        calls_before,
        "a pre-cancelled pass dispatches no ESI request"
    );
}

/// Requirement 8: a refresh cancelled after it has leased its coverage
/// row leaves that row `refreshing` -- but with a finite
/// `lease_expires_at`, so `lease_expires_at` recovery reclaims it. No
/// `fail`/`complete`/`revalidate` is written for the shutdown.
#[tokio::test(start_paused = true)]
async fn cancelled_market_refresh_leaves_row_recoverable_not_failed() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![Err(EsiError::TemporaryFailure)]);
    let service = PublicMarketService::new(repository.clone(), transport.clone());
    let cancel = CancellationToken::new();

    let refresh = service.refresh_type(
        WorkspaceId::new(),
        PriceSourceId::new(),
        JITA_SCOPE,
        coverage(MarketCoverageRegistration {
            type_id: 34,
            type_name: "Tritanium".to_string(),
        }),
        &cancel,
    );
    let controller = async {
        drain(64).await;
        cancel.cancel();
        drain(64).await;
    };
    let (outcome, ()) = tokio::join!(refresh, controller);

    assert!(
        matches!(outcome, MarketRefreshOutcome::Skipped),
        "{outcome:?}"
    );
    assert!(
        repository.failures.lock().unwrap().is_empty(),
        "no fake `failed` state persisted for shutdown"
    );
    assert!(repository.completed.lock().unwrap().is_empty());
    assert!(repository.revalidations.lock().unwrap().is_empty());

    let deadlines = repository.lease_deadlines.lock().unwrap();
    assert_eq!(deadlines.len(), 1, "exactly one row was leased");
    assert_eq!(deadlines[0].0, 34);
    assert!(
        deadlines[0].1 > Utc::now(),
        "the lease it holds has a future deadline -> recoverable, not permanently stuck"
    );
}

/// When every attempt fails with a rate-limit error, the persisted
/// `next_refresh_at` is no earlier than *both* `attempted_at +
/// REFRESH_RETRY_DELAY` (the 1-minute floor) and the server-directed
/// `Retry-After` deadline. Uses a `Retry-After` far longer than the
/// floor so the server value is the one that wins.
#[tokio::test(start_paused = true)]
async fn rate_limited_failure_is_not_scheduled_before_server_cooldown() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![
        Err(EsiError::RateLimited {
            retry_after_seconds: Some(3_600),
        }),
        Err(EsiError::RateLimited {
            retry_after_seconds: Some(3_600),
        }),
        Err(EsiError::RateLimited {
            retry_after_seconds: Some(3_600),
        }),
    ]);
    let service = PublicMarketService::new(repository.clone(), transport.clone());
    let mut item = coverage(MarketCoverageRegistration {
        type_id: 34,
        type_name: "Tritanium".to_string(),
    });
    item.observed_at = Some(Utc::now());

    let refresh = service.refresh_type(
        WorkspaceId::new(),
        PriceSourceId::new(),
        JITA_SCOPE,
        item,
        no_cancel(),
    );
    let controller = async {
        // Advance past both Retry-After-sized retry waits (3600s each).
        for _ in 0..4 {
            tokio::time::advance(std::time::Duration::from_secs(3_600)).await;
            drain(64).await;
        }
    };
    let (outcome, ()) = tokio::join!(refresh, controller);

    assert!(matches!(outcome, MarketRefreshOutcome::Failed));
    assert_eq!(transport.calls.load(Ordering::SeqCst), 3);
    // The cached observation is untouched -- no batch was completed.
    assert!(repository.completed.lock().unwrap().is_empty());

    let schedule = repository.fail_schedule.lock().unwrap();
    assert_eq!(schedule.len(), 1);
    let (attempted_at, next_refresh_at) = schedule[0];
    assert!(
        next_refresh_at >= attempted_at + chrono::Duration::minutes(1),
        "at least the fixed REFRESH_RETRY_DELAY floor"
    );
    assert!(
        next_refresh_at >= attempted_at + chrono::Duration::seconds(3_600),
        "at least the server-directed Retry-After cooldown"
    );
}

/// Same contract for ESI's error limit: retries wait out the reported
/// window instead of spinning through their attempts while every ESI
/// call is paused, and the failed row isn't due again before it.
#[tokio::test(start_paused = true)]
async fn error_limited_failure_waits_for_the_error_window() {
    let repository = Arc::new(RecordingRepository::default());
    let error_limited = || {
        Err(EsiError::EsiErrorLimit {
            reset_seconds: Some(3_600),
        })
    };
    let transport = PageTransport::new(vec![error_limited(), error_limited(), error_limited()]);
    let service = PublicMarketService::new(repository.clone(), transport.clone());
    let mut item = coverage(MarketCoverageRegistration {
        type_id: 34,
        type_name: "Tritanium".to_string(),
    });
    item.observed_at = Some(Utc::now());

    let refresh = service.refresh_type(
        WorkspaceId::new(),
        PriceSourceId::new(),
        JITA_SCOPE,
        item,
        no_cancel(),
    );
    let controller = async {
        // Short of one full window, the second attempt must not have run.
        tokio::time::advance(std::time::Duration::from_secs(3_000)).await;
        drain(64).await;
        assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
        for _ in 0..4 {
            tokio::time::advance(std::time::Duration::from_secs(3_600)).await;
            drain(64).await;
        }
    };
    let (outcome, ()) = tokio::join!(refresh, controller);

    assert!(matches!(outcome, MarketRefreshOutcome::Failed));
    assert_eq!(transport.calls.load(Ordering::SeqCst), 3);
    let schedule = repository.fail_schedule.lock().unwrap();
    let (attempted_at, next_refresh_at) = schedule[0];
    assert!(next_refresh_at >= attempted_at + chrono::Duration::seconds(3_600));
}
