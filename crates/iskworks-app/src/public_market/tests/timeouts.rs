use super::*;

// --- ESI HTTP timeout bounds -----------------------------------------
//
// `iskworks-esi` bounds every HTTP request (connect + total) and a
// timeout surfaces as `EsiError::TemporaryFailure` -- the same retryable
// contract as a 5xx. These lock down that a bounded-out
// request (a) keeps the 3-attempt / 250-500ms market retry,
// (b) releases its concurrency permit so queued work progresses, and
// (c) is still beaten instantly by cooperative cancellation.

/// A counting TCP black hole: accepts every connection, never answers,
/// never closes. Returns the base URL, an accept counter, and the
/// server task (kept alive by binding the handle).
async fn spawn_counting_blackhole() -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
    use tokio::io::AsyncReadExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&accepted);
    let handle = tokio::spawn(async move {
        let mut held = Vec::new();
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            counter.fetch_add(1, Ordering::SeqCst);
            let mut scratch = vec![0_u8; 1024];
            let _ = socket.read(&mut scratch).await;
            held.push(socket); // never respond, never drop
        }
    });
    (format!("http://{address}"), accepted, handle)
}

/// A persistent ESI timeout (`TemporaryFailure` on every attempt) is
/// retried exactly `MAX_FETCH_ATTEMPTS` times with the unchanged
/// 250ms/500ms backoff, then the coverage row is `Failed` -- no extra
/// retry layer, no changed counts.
#[tokio::test(start_paused = true)]
async fn persistent_esi_timeout_gives_up_after_the_existing_attempt_ceiling() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![
        Err(EsiError::TemporaryFailure),
        Err(EsiError::TemporaryFailure),
        Err(EsiError::TemporaryFailure),
    ]);
    let service = PublicMarketService::new(repository.clone(), transport.clone());

    let refresh = service.refresh_type(
        WorkspaceId::new(),
        PriceSourceId::new(),
        JITA_SCOPE,
        coverage(MarketCoverageRegistration {
            type_id: 34,
            type_name: "Tritanium".to_string(),
        }),
        no_cancel(),
    );
    let controller = async {
        drain(64).await;
        tokio::time::advance(std::time::Duration::from_millis(251)).await;
        drain(64).await;
        tokio::time::advance(std::time::Duration::from_millis(501)).await;
        drain(64).await;
    };
    let (outcome, ()) = tokio::join!(refresh, controller);

    assert!(matches!(outcome, MarketRefreshOutcome::Failed));
    assert_eq!(
        transport.calls.load(Ordering::SeqCst),
        3,
        "MAX_FETCH_ATTEMPTS is unchanged at 3"
    );
    assert_eq!(repository.failures.lock().unwrap().len(), 1);
    assert!(repository.completed.lock().unwrap().is_empty());
}

/// The original production defect: four hung requests occupy all four
/// permits forever. After the fix they hit the request timeout, error,
/// release their permits, and the queued fifth candidate connects.
#[tokio::test(start_paused = true)]
async fn timed_out_market_requests_release_their_semaphore_permits() {
    let (base_url, accepted, _server) = spawn_counting_blackhole().await;
    let repository = Arc::new(RecordingRepository::default());
    *repository.candidates.lock().unwrap() = (34i64..39)
        .map(|type_id| {
            coverage(MarketCoverageRegistration {
                type_id,
                type_name: format!("Item{type_id}"),
            })
        })
        .collect();
    let service = PublicMarketService::new(
        repository.clone(),
        Arc::new(iskworks_esi::HttpEsiTransport::public(base_url)),
    );

    let handle = tokio::spawn({
        let service = service.clone();
        async move {
            service
                .refresh_source(WorkspaceId::new(), PriceSourceId::new(), no_cancel())
                .await
        }
    });

    for _ in 0..200 {
        if accepted.load(Ordering::SeqCst) >= 4 {
            break;
        }
        drain(32).await;
    }
    drain(64).await;
    assert_eq!(
        accepted.load(Ordering::SeqCst),
        4,
        "four permits => four concurrent connections; candidate 5 is queued on the semaphore"
    );

    // The four in-flight requests reach ESI_REQUEST_TIMEOUT (30s),
    // error as TemporaryFailure, and free their permits.
    for _ in 0..40 {
        if accepted.load(Ordering::SeqCst) >= 5 {
            break;
        }
        tokio::time::advance(std::time::Duration::from_secs(5)).await;
        drain(128).await;
    }
    assert!(
        accepted.load(Ordering::SeqCst) >= 5,
        "a timed-out request released its permit and candidate 5 reached the transport"
    );

    handle.abort();
}

/// A request timeout of 30s must never keep cancellation waiting:
/// cancelling ~1s in returns immediately as `Skipped`, no failure
/// persisted. Cooperative cancellation stays the shutdown mechanism.
#[tokio::test(start_paused = true)]
async fn cancellation_returns_before_the_request_timeout_elapses() {
    let (base_url, accepted, _server) = spawn_counting_blackhole().await;
    let repository = Arc::new(RecordingRepository::default());
    let service = PublicMarketService::new(
        repository.clone(),
        Arc::new(iskworks_esi::HttpEsiTransport::public(base_url)),
    );
    let cancel = CancellationToken::new();

    let handle = tokio::spawn({
        let service = service.clone();
        let cancel = cancel.clone();
        async move {
            service
                .refresh_type(
                    WorkspaceId::new(),
                    PriceSourceId::new(),
                    JITA_SCOPE,
                    coverage(MarketCoverageRegistration {
                        type_id: 34,
                        type_name: "Tritanium".to_string(),
                    }),
                    &cancel,
                )
                .await
        }
    });

    for _ in 0..200 {
        if accepted.load(Ordering::SeqCst) >= 1 {
            break;
        }
        drain(32).await;
    }
    assert_eq!(accepted.load(Ordering::SeqCst), 1, "request is in flight");

    tokio::time::advance(std::time::Duration::from_secs(1)).await;
    cancel.cancel();
    drain(128).await;

    assert!(
        handle.is_finished(),
        "cancel returns without waiting out the 30s request timeout"
    );
    assert!(matches!(
        handle.await.unwrap(),
        MarketRefreshOutcome::Skipped
    ));
    assert!(
        repository.failures.lock().unwrap().is_empty(),
        "cancellation is control flow, not a refresh failure"
    );
}

/// Once one regional-market request is rate-limited with `Retry-After:
/// 2`, no *new* regional-market HTTP request may start until the shared
/// 2s cooldown elapses -- while requests already in flight when the 429
/// landed are left alone. Runs six candidates through the real request
/// concurrency of 4 (as concurrent `refresh_type` futures, bypassing
/// `refresh_via_public_path`'s internal `tokio::spawn` so the
/// current-thread test runtime drives them deterministically).
#[tokio::test(start_paused = true)]
async fn rate_limit_cooldown_blocks_queued_market_requests() {
    let repository = Arc::new(RecordingRepository::default());
    // Four concurrent HTTP slots (the real `request_limit` size); park
    // all four initial requests so they are genuinely in flight before
    // request 0 returns its `Retry-After: 2` 429.
    let transport = RateLimitProbeTransport::new(2, 4);
    let service = PublicMarketService::new(repository.clone(), transport.clone());
    let workspace_id = WorkspaceId::new();
    let source_id = PriceSourceId::new();

    let mut tasks = Vec::new();
    for type_id in 34i64..40 {
        let service = service.clone();
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
                    no_cancel(),
                )
                .await
        }));
    }

    // Wait until all four permitted requests are in flight (parked in
    // the transport). Items 5 and 6 are still blocked on the service
    // semaphore and have issued nothing.
    while transport.parked_count() < 4 {
        drain(16).await;
    }
    assert_eq!(
        transport.call_instants().len(),
        4,
        "exactly the four already-in-flight requests have hit the wire"
    );

    // Release them: request 0 -> 429 -> shared cooldown until +2s;
    // requests 1..3 -> ok -> free their permits.
    transport.release_parked();
    drain(64).await;
    assert_eq!(
        transport.call_instants().len(),
        4,
        "the two queued requests must not start during the cooldown"
    );

    // 1ms short of the cooldown: still gated.
    tokio::time::advance(std::time::Duration::from_millis(1_999)).await;
    drain(64).await;
    assert_eq!(
        transport.call_instants().len(),
        4,
        "still gated 1ms before the server cooldown elapses"
    );

    // Cooldown elapses: the poisoned request's retry plus the two
    // queued requests now proceed.
    tokio::time::advance(std::time::Duration::from_millis(2)).await;
    drain(64).await;

    let mut outcomes = Vec::new();
    for task in tasks {
        outcomes.push(task.await.unwrap());
    }
    for outcome in &outcomes {
        assert!(
            matches!(outcome, MarketRefreshOutcome::Succeeded),
            "{outcome:?}"
        );
    }

    let instants = transport.call_instants();
    assert_eq!(
        instants.len(),
        7,
        "4 initial + the poisoned retry + 2 gated queued requests"
    );
    let base = instants[0];
    for instant in &instants {
        let offset = instant.duration_since(base);
        assert!(
            offset <= std::time::Duration::from_millis(1)
                || offset >= std::time::Duration::from_millis(1_999),
            "no request started mid-cooldown (offset {offset:?})"
        );
    }
}
