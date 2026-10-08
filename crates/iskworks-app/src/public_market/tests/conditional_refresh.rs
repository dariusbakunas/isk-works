use super::*;

// --- ETag / If-None-Match conditional regional-market refresh ---------

/// A due coverage item whose previous book is small enough to be
/// single-page and carries a persisted ETag.
fn conditional_item(
    type_id: i64,
    name: &str,
    prior_etag: Option<&str>,
    order_count: u64,
) -> MarketCoverageItem {
    MarketCoverageItem {
        refresh_state: MarketRefreshState::Current,
        observed_at: Some(Utc::now() - chrono::Duration::minutes(10)),
        prior_etag: prior_etag.map(str::to_string),
        order_count,
        ..coverage(MarketCoverageRegistration {
            type_id,
            type_name: name.to_string(),
        })
    }
}

#[tokio::test]
async fn conditional_request_sends_persisted_etag() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![Ok(response(1, 101, JITA_LOCATION_ID))]);
    let service = PublicMarketService::new(repository.clone(), transport.clone());

    service
        .refresh_type(
            WorkspaceId::new(),
            PriceSourceId::new(),
            JITA_SCOPE,
            conditional_item(34, "Tritanium", Some("abc"), 3),
            no_cancel(),
        )
        .await;

    assert_eq!(transport.requested_etags(), vec![Some("abc".to_string())]);
}

#[tokio::test]
async fn not_modified_reuses_current_order_book() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![Ok(not_modified_page(Some(1)))]);
    let service = PublicMarketService::new(repository.clone(), transport);

    let outcome = service
        .refresh_type(
            WorkspaceId::new(),
            PriceSourceId::new(),
            JITA_SCOPE,
            conditional_item(34, "Tritanium", Some("abc"), 3),
            no_cancel(),
        )
        .await;

    assert!(matches!(outcome, MarketRefreshOutcome::Revalidated));
    // No new batch was completed and no failure recorded -- the batch
    // at last_completed_batch_id stays authoritative.
    assert!(repository.completed.lock().unwrap().is_empty());
    assert!(repository.failures.lock().unwrap().is_empty());
    let revalidations = repository.revalidations.lock().unwrap();
    assert_eq!(revalidations.len(), 1);
    assert_eq!(revalidations[0].0, 34);
    assert_eq!(
        revalidations[0].3,
        Some("2026-07-25T12:05:00Z".parse().unwrap()),
        "the 304's Expires bounds the next fetch"
    );
}

#[tokio::test]
async fn not_modified_counts_as_successful_refresh() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![Ok(not_modified_page(None))]);
    let service = PublicMarketService::new(repository.clone(), transport);

    let before = Utc::now();
    let outcome = service
        .refresh_type(
            WorkspaceId::new(),
            PriceSourceId::new(),
            JITA_SCOPE,
            conditional_item(34, "Tritanium", Some("abc"), 3),
            no_cancel(),
        )
        .await;
    let after = Utc::now();

    assert!(matches!(outcome, MarketRefreshOutcome::Revalidated));
    assert!(repository.failures.lock().unwrap().is_empty());
    let revalidations = repository.revalidations.lock().unwrap();
    assert_eq!(revalidations.len(), 1);
    let (_, _claim, next_refresh_at, _) = revalidations[0];
    // Advanced by the service refresh interval (300s for `new`).
    assert!(next_refresh_at >= before + chrono::Duration::seconds(299));
    assert!(next_refresh_at <= after + chrono::Duration::seconds(301));
}

#[tokio::test]
async fn not_modified_does_not_empty_the_book() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![Ok(not_modified_page(Some(1)))]);
    let service = PublicMarketService::new(repository.clone(), transport);

    service
        .refresh_type(
            WorkspaceId::new(),
            PriceSourceId::new(),
            JITA_SCOPE,
            conditional_item(34, "Tritanium", Some("abc"), 3),
            no_cancel(),
        )
        .await;

    // A 304 must never push an (empty) batch through `complete` -- that
    // would replace the still-authoritative previous order book.
    assert!(repository.completed.lock().unwrap().is_empty());
    assert_eq!(repository.revalidations.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn two_hundred_with_empty_orders_replaces_book_with_empty_book() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![Ok(EsiResponse {
        records: Vec::new(),
        not_modified: false,
        metadata: EsiResponseMetadata {
            pages: Some(1),
            etag: Some("empty-book".to_string()),
            ..EsiResponseMetadata::default()
        },
    })]);
    let service = PublicMarketService::new(repository.clone(), transport);

    let outcome = service
        .refresh_type(
            WorkspaceId::new(),
            PriceSourceId::new(),
            JITA_SCOPE,
            conditional_item(34, "Tritanium", Some("abc"), 3),
            no_cancel(),
        )
        .await;

    // A genuinely empty 200 is new market state: a new completed batch
    // with zero orders, NOT a revalidation.
    assert!(matches!(outcome, MarketRefreshOutcome::Succeeded));
    assert!(repository.revalidations.lock().unwrap().is_empty());
    let completed = repository.completed.lock().unwrap();
    assert_eq!(completed.len(), 1);
    assert!(completed[0].orders.is_empty());
}

#[tokio::test]
async fn two_hundred_with_changed_etag_persists_a_new_observation() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![Ok(response(1, 101, JITA_LOCATION_ID))]);
    let service = PublicMarketService::new(repository.clone(), transport.clone());

    let outcome = service
        .refresh_type(
            WorkspaceId::new(),
            PriceSourceId::new(),
            JITA_SCOPE,
            conditional_item(34, "Tritanium", Some("old"), 3),
            no_cancel(),
        )
        .await;

    assert!(matches!(outcome, MarketRefreshOutcome::Succeeded));
    assert_eq!(transport.requested_etags(), vec![Some("old".to_string())]);
    assert!(repository.revalidations.lock().unwrap().is_empty());
    let completed = repository.completed.lock().unwrap();
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].orders.len(), 1);
    assert_eq!(completed[0].etag.as_deref(), Some("page-101"));
}

#[tokio::test(start_paused = true)]
async fn rate_limited_conditional_request_still_honors_shared_cooldown() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![
        Err(EsiError::RateLimited {
            retry_after_seconds: Some(2),
        }),
        Ok(response(1, 101, JITA_LOCATION_ID)),
    ]);
    let service = PublicMarketService::new(repository.clone(), transport.clone());

    let refresh = service.refresh_type(
        WorkspaceId::new(),
        PriceSourceId::new(),
        JITA_SCOPE,
        conditional_item(34, "Tritanium", Some("abc"), 3),
        no_cancel(),
    );
    let controller = async {
        drain(64).await;
        assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
        // Retry must wait the full Retry-After, not the 250ms local
        // backoff -- identical to an unconditional 429.
        tokio::time::advance(std::time::Duration::from_millis(1_999)).await;
        drain(64).await;
        assert_eq!(
            transport.calls.load(Ordering::SeqCst),
            1,
            "conditional 429 still gated by the shared regional cooldown"
        );
        tokio::time::advance(std::time::Duration::from_millis(2)).await;
        drain(64).await;
    };
    let (outcome, ()) = tokio::join!(refresh, controller);

    assert!(matches!(outcome, MarketRefreshOutcome::Succeeded));
    assert_eq!(transport.calls.load(Ordering::SeqCst), 2);
    // Both the initial attempt and the retry are conditional.
    assert_eq!(
        transport.requested_etags(),
        vec![Some("abc".to_string()), Some("abc".to_string())]
    );
}

#[tokio::test]
async fn conditional_fetch_skipped_near_pagination_boundary() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![Ok(response(1, 101, JITA_LOCATION_ID))]);
    let service = PublicMarketService::new(repository.clone(), transport.clone());

    service
        .refresh_type(
            WorkspaceId::new(),
            PriceSourceId::new(),
            JITA_SCOPE,
            conditional_item(34, "Tritanium", Some("abc"), 900),
            no_cancel(),
        )
        .await;

    // order_count == CONDITIONAL_REFRESH_MAX_ORDERS is NOT eligible.
    assert_eq!(transport.requested_etags(), vec![None]);
}

#[tokio::test]
async fn conditional_fetch_skipped_when_prior_book_is_multi_page() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![Ok(response(1, 101, JITA_LOCATION_ID))]);
    let service = PublicMarketService::new(repository.clone(), transport.clone());

    service
        .refresh_type(
            WorkspaceId::new(),
            PriceSourceId::new(),
            JITA_SCOPE,
            conditional_item(34, "Tritanium", Some("abc"), 1_500),
            no_cancel(),
        )
        .await;

    assert_eq!(transport.requested_etags(), vec![None]);
}

#[tokio::test]
async fn not_modified_with_multi_page_metadata_falls_back_to_full_fetch() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![
        Ok(not_modified_page(Some(2))),
        Ok(response(2, 101, JITA_LOCATION_ID)),
        Ok(response(2, 103, JITA_LOCATION_ID)),
    ]);
    let service = PublicMarketService::new(repository.clone(), transport.clone());
    let logs = CapturedLogs::default();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_writer(logs.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let outcome = service
        .refresh_type(
            WorkspaceId::new(),
            PriceSourceId::new(),
            JITA_SCOPE,
            conditional_item(34, "Tritanium", Some("abc"), 3),
            no_cancel(),
        )
        .await;

    assert!(matches!(outcome, MarketRefreshOutcome::Succeeded));
    // page 1 conditional -> 304 (pages=2) -> page 1 unconditional -> page 2.
    assert_eq!(
        transport.requested_etags(),
        vec![Some("abc".to_string()), None, None]
    );
    assert!(repository.revalidations.lock().unwrap().is_empty());
    assert_eq!(repository.completed.lock().unwrap().len(), 1);
    assert!(logs.text().contains("multiple pages"), "{}", logs.text());
}

#[tokio::test]
async fn null_prior_etag_sends_no_conditional_header() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![Ok(response(1, 101, JITA_LOCATION_ID))]);
    let service = PublicMarketService::new(repository.clone(), transport.clone());

    let outcome = service
        .refresh_type(
            WorkspaceId::new(),
            PriceSourceId::new(),
            JITA_SCOPE,
            conditional_item(34, "Tritanium", None, 3),
            no_cancel(),
        )
        .await;

    assert!(matches!(outcome, MarketRefreshOutcome::Succeeded));
    assert_eq!(transport.requested_etags(), vec![None]);
}
