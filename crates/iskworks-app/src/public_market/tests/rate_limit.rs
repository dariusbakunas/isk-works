use super::*;

// --- ESI regional-market 429 rate-limit correctness -------------------
//
// Characterization tests for the shared `RegionalMarketCooldown` and
// `Retry-After`-aware retry delay. Each uses `start_paused` so the
// virtual clock, `RegionalMarketCooldown`'s `tokio::time::Instant`
// deadlines, and the retry `sleep`s all advance together and
// deterministically.

/// A 429 with `Retry-After: 1` must delay the next attempt by the full
/// server-directed second, never the 250ms first-attempt local
/// backoff. Pre-fix, `fetch_type_with_retry` used `FETCH_RETRY_DELAY *
/// attempt` unconditionally and attempt 2 fired ~750ms early.
///
/// Driven with `tokio::join!` of the refresh future and a controller
/// that advances the paused clock -- no `tokio::spawn`, so progress is
/// deterministic on the current-thread test runtime.
#[tokio::test(start_paused = true)]
async fn retry_after_is_honored_over_local_backoff() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![
        Err(EsiError::RateLimited {
            retry_after_seconds: Some(1),
        }),
        Ok(response(1, 101, JITA_LOCATION_ID)),
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
        assert_eq!(
            transport.calls.load(Ordering::SeqCst),
            1,
            "attempt 1 fired and was rate-limited"
        );

        // 1ms short of the server's Retry-After: attempt 2 must still be
        // held (the 250ms local backoff elapsed long ago).
        tokio::time::advance(std::time::Duration::from_millis(999)).await;
        drain(64).await;
        assert_eq!(
            transport.calls.load(Ordering::SeqCst),
            1,
            "attempt 2 must not fire before the full 1s Retry-After"
        );

        tokio::time::advance(std::time::Duration::from_millis(2)).await;
        drain(64).await;
    };
    let (outcome, ()) = tokio::join!(refresh, controller);

    assert!(matches!(outcome, MarketRefreshOutcome::Succeeded));
    assert_eq!(transport.calls.load(Ordering::SeqCst), 2);
    assert_eq!(repository.completed.lock().unwrap().len(), 1);
    assert!(repository.failures.lock().unwrap().is_empty());
}

/// Errors with no server timing hint keep the existing 250ms -> 500ms
/// exponential backoff untouched.
#[tokio::test(start_paused = true)]
async fn temporary_failure_keeps_existing_exponential_backoff() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![
        Err(EsiError::TemporaryFailure),
        Err(EsiError::TemporaryFailure),
        Ok(response(1, 101, JITA_LOCATION_ID)),
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
        assert_eq!(transport.calls.load(Ordering::SeqCst), 1);

        // Attempt 2 after exactly 250ms, not before.
        tokio::time::advance(std::time::Duration::from_millis(249)).await;
        drain(64).await;
        assert_eq!(
            transport.calls.load(Ordering::SeqCst),
            1,
            "still within the 250ms first backoff"
        );
        tokio::time::advance(std::time::Duration::from_millis(2)).await;
        drain(64).await;
        assert_eq!(
            transport.calls.load(Ordering::SeqCst),
            2,
            "attempt 2 at 250ms"
        );

        // Attempt 3 after a further 500ms, not before.
        tokio::time::advance(std::time::Duration::from_millis(499)).await;
        drain(64).await;
        assert_eq!(
            transport.calls.load(Ordering::SeqCst),
            2,
            "still within the 500ms second backoff"
        );
        tokio::time::advance(std::time::Duration::from_millis(2)).await;
        drain(64).await;
    };
    let (outcome, ()) = tokio::join!(refresh, controller);

    assert!(matches!(outcome, MarketRefreshOutcome::Succeeded));
    assert_eq!(
        transport.calls.load(Ordering::SeqCst),
        3,
        "attempt 3 at 500ms"
    );
    assert_eq!(repository.completed.lock().unwrap().len(), 1);
    assert!(repository.failures.lock().unwrap().is_empty());
}
