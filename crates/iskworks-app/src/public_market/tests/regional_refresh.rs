use super::*;

#[tokio::test]
async fn refreshes_complete_jita_book_after_all_pages() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![
        Ok(response(3, 101, JITA_LOCATION_ID)),
        Ok(response(3, 102, 60_003_761)),
        Ok(response(3, 103, JITA_LOCATION_ID)),
    ]);
    let service = PublicMarketService::new(repository.clone(), transport.clone());

    service
        .refresh_type(
            WorkspaceId::new(),
            PriceSourceId::new(),
            JITA_SCOPE,
            coverage(MarketCoverageRegistration {
                type_id: 34,
                type_name: "Tritanium".to_string(),
            }),
            no_cancel(),
        )
        .await;

    assert_eq!(transport.calls.load(Ordering::SeqCst), 3);
    let completed = repository.completed.lock().unwrap();
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].orders.len(), 2);
    assert_eq!(completed[0].orders[0].order_id, 101);
    assert_eq!(completed[0].orders[1].order_id, 103);
    assert!(repository.failures.lock().unwrap().is_empty());
}

/// `fetch_type` must use the caller's scope, never a hardcoded Jita
/// region/system/location (otherwise a `PriceSource` configured for any
/// other station would silently fetch Jita's book). Proves a non-Jita scope
/// drives the real ESI request and the completed batch's own
/// region/location.
#[tokio::test]
async fn refreshes_a_non_jita_region_using_its_own_configured_scope() {
    let repository = Arc::new(RecordingRepository {
        scope: RENS_SCOPE,
        ..RecordingRepository::default()
    });
    let transport = PageTransport::new(vec![Ok(response(1, 201, RENS_LOCATION_ID))]);
    let service = PublicMarketService::new(repository.clone(), transport.clone());

    service
        .refresh_type(
            WorkspaceId::new(),
            PriceSourceId::new(),
            RENS_SCOPE,
            coverage(MarketCoverageRegistration {
                type_id: 34,
                type_name: "Tritanium".to_string(),
            }),
            no_cancel(),
        )
        .await;

    assert_eq!(
        transport.requested_regions.lock().unwrap().as_slice(),
        [RENS_REGION_ID]
    );
    let completed = repository.completed.lock().unwrap();
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].region_id, RENS_REGION_ID);
    assert_eq!(completed[0].location_id, RENS_LOCATION_ID);
    assert_eq!(completed[0].orders.len(), 1);
    assert_eq!(completed[0].orders[0].order_id, 201);
    assert!(repository.failures.lock().unwrap().is_empty());
}

/// A region-wide scope (`location_id: None`) keeps every location's
/// orders instead of filtering to one -- the batch itself has no single
/// location, so it's stamped with the `0` sentinel rather than an
/// arbitrary real one.
#[tokio::test]
async fn refreshes_a_region_wide_scope_keeping_every_location() {
    let region_wide_scope = MarketScope {
        region_id: JITA_REGION_ID,
        location_id: None,
    };
    let repository = Arc::new(RecordingRepository {
        scope: region_wide_scope,
        ..RecordingRepository::default()
    });
    let mut two_locations = response(1, 101, JITA_LOCATION_ID);
    two_locations
        .records
        .push(response(1, 103, 60_003_761).records.remove(0));
    let transport = PageTransport::new(vec![Ok(two_locations)]);
    let service = PublicMarketService::new(repository.clone(), transport.clone());

    service
        .refresh_type(
            WorkspaceId::new(),
            PriceSourceId::new(),
            region_wide_scope,
            coverage(MarketCoverageRegistration {
                type_id: 34,
                type_name: "Tritanium".to_string(),
            }),
            no_cancel(),
        )
        .await;

    let completed = repository.completed.lock().unwrap();
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].region_id, JITA_REGION_ID);
    assert_eq!(completed[0].location_id, 0);
    assert_eq!(completed[0].orders.len(), 2);
    assert_eq!(completed[0].orders[0].order_id, 101);
    assert_eq!(completed[0].orders[1].order_id, 103);
    assert!(repository.failures.lock().unwrap().is_empty());
}

#[tokio::test]
async fn skips_order_with_non_positive_duration_but_keeps_valid_siblings() {
    let repository = Arc::new(RecordingRepository::default());
    let mut page = response(1, 101, JITA_LOCATION_ID);
    let mut malformed = page.records[0].clone();
    malformed.order_id = 102;
    malformed.duration_days = 0;
    page.records.push(malformed);
    let transport = PageTransport::new(vec![Ok(page)]);
    let service = PublicMarketService::new(repository.clone(), transport.clone());

    service
        .refresh_type(
            WorkspaceId::new(),
            PriceSourceId::new(),
            JITA_SCOPE,
            coverage(MarketCoverageRegistration {
                type_id: 34,
                type_name: "Tritanium".to_string(),
            }),
            no_cancel(),
        )
        .await;

    let completed = repository.completed.lock().unwrap();
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].orders.len(), 1);
    assert_eq!(completed[0].orders[0].order_id, 101);
    assert!(repository.failures.lock().unwrap().is_empty());
}

#[tokio::test]
async fn pagination_failure_never_completes_partial_book() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![
        Ok(response(2, 101, JITA_LOCATION_ID)),
        Err(EsiError::TemporaryFailure),
        Err(EsiError::TemporaryFailure),
        Err(EsiError::TemporaryFailure),
    ]);
    let service = PublicMarketService::new(repository.clone(), transport);

    service
        .refresh_type(
            WorkspaceId::new(),
            PriceSourceId::new(),
            JITA_SCOPE,
            coverage(MarketCoverageRegistration {
                type_id: 34,
                type_name: "Tritanium".to_string(),
            }),
            no_cancel(),
        )
        .await;

    assert!(repository.completed.lock().unwrap().is_empty());
    assert_eq!(repository.failures.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn commit_failure_records_a_failure_instead_of_hot_looping() {
    let repository = Arc::new(RecordingRepository {
        complete_should_fail: AtomicBool::new(true),
        ..RecordingRepository::default()
    });
    let transport = PageTransport::new(vec![Ok(response(1, 101, JITA_LOCATION_ID))]);
    let service = PublicMarketService::new(repository.clone(), transport);

    let outcome = service
        .refresh_type(
            WorkspaceId::new(),
            PriceSourceId::new(),
            JITA_SCOPE,
            coverage(MarketCoverageRegistration {
                type_id: 34,
                type_name: "Tritanium".to_string(),
            }),
            no_cancel(),
        )
        .await;

    assert!(matches!(outcome, MarketRefreshOutcome::Failed));
    assert!(repository.completed.lock().unwrap().is_empty());
    let failures = repository.failures.lock().unwrap();
    assert_eq!(failures.len(), 1);
    assert!(failures[0].contains("duration_days check constraint violated"));
}

#[tokio::test]
async fn temporary_esi_failures_are_retried_before_marking_coverage_failed() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![
        Err(EsiError::TemporaryFailure),
        Err(EsiError::TemporaryFailure),
        Ok(response(1, 101, JITA_LOCATION_ID)),
    ]);
    let service = PublicMarketService::new(repository.clone(), transport.clone());

    service
        .refresh_type(
            WorkspaceId::new(),
            PriceSourceId::new(),
            JITA_SCOPE,
            coverage(MarketCoverageRegistration {
                type_id: 34,
                type_name: "Tritanium".to_string(),
            }),
            no_cancel(),
        )
        .await;

    assert_eq!(transport.calls.load(Ordering::SeqCst), 3);
    assert_eq!(repository.completed.lock().unwrap().len(), 1);
    assert!(repository.failures.lock().unwrap().is_empty());
}

#[tokio::test]
async fn retry_logs_item_attempt_and_delay() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![
        Err(EsiError::TemporaryFailure),
        Ok(response(1, 101, JITA_LOCATION_ID)),
    ]);
    let service = PublicMarketService::new(repository, transport);
    let source_id = PriceSourceId::new();
    let logs = CapturedLogs::default();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_writer(logs.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    service
        .refresh_type(
            WorkspaceId::new(),
            source_id,
            JITA_SCOPE,
            coverage(MarketCoverageRegistration {
                type_id: 34,
                type_name: "Tritanium".to_string(),
            }),
            no_cancel(),
        )
        .await;

    let output = logs.text();
    assert!(
        output.contains("market refresh retry scheduled"),
        "{output}"
    );
    assert!(
        output.contains(&format!("source_id={}", source_id.0)),
        "{output}"
    );
    assert!(output.contains("item_name=Tritanium"), "{output}");
    assert!(output.contains("type_id=34"), "{output}");
    assert!(output.contains("attempt=1"), "{output}");
    assert!(output.contains("next_attempt=2"), "{output}");
    assert!(output.contains("retry_delay_ms=250"), "{output}");
    assert!(output.contains("retryable=true"), "{output}");
}

#[tokio::test]
async fn terminal_failure_logs_cached_observation_state() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![
        Err(EsiError::TemporaryFailure),
        Err(EsiError::TemporaryFailure),
        Err(EsiError::TemporaryFailure),
    ]);
    let service = PublicMarketService::new(repository, transport);
    let source_id = PriceSourceId::new();
    let mut item = coverage(MarketCoverageRegistration {
        type_id: 34,
        type_name: "Tritanium".to_string(),
    });
    item.observed_at = Some(Utc::now());
    let logs = CapturedLogs::default();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_writer(logs.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    service
        .refresh_type(WorkspaceId::new(), source_id, JITA_SCOPE, item, no_cancel())
        .await;

    let output = logs.text();
    assert!(output.contains("market refresh failed"), "{output}");
    assert!(
        output.contains(&format!("source_id={}", source_id.0)),
        "{output}"
    );
    assert!(output.contains("item_name=Tritanium"), "{output}");
    assert!(output.contains("type_id=34"), "{output}");
    assert!(output.contains("attempts=3"), "{output}");
    assert!(output.contains("cached_observation=true"), "{output}");
}

#[tokio::test]
async fn explicit_source_refresh_logs_aggregate_outcomes() {
    let item = coverage(MarketCoverageRegistration {
        type_id: 34,
        type_name: "Tritanium".to_string(),
    });
    let repository = Arc::new(RecordingRepository {
        candidates: Mutex::new(vec![item.clone(), item]),
        ..RecordingRepository::default()
    });
    let transport = PageTransport::new(vec![Ok(response(1, 101, JITA_LOCATION_ID))]);
    let service = PublicMarketService::new(repository, transport);
    let source_id = PriceSourceId::new();
    let logs = CapturedLogs::default();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_writer(logs.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    service
        .refresh_source(WorkspaceId::new(), source_id, no_cancel())
        .await
        .unwrap();

    let output = logs.text();
    assert!(
        output.contains("market source refresh completed"),
        "{output}"
    );
    assert!(
        output.contains(&format!("source_id={}", source_id.0)),
        "{output}"
    );
    assert!(output.contains("requested=2"), "{output}");
    assert!(output.contains("succeeded=1"), "{output}");
    assert!(output.contains("failed=0"), "{output}");
    assert!(output.contains("skipped=1"), "{output}");
    assert!(output.contains("elapsed_ms="), "{output}");
}

#[tokio::test]
async fn concurrent_refreshes_share_the_repository_lease() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![Ok(response(1, 101, JITA_LOCATION_ID))]);
    let service = PublicMarketService::new(repository.clone(), transport.clone());
    let workspace_id = WorkspaceId::new();
    let source_id = PriceSourceId::new();
    let item = coverage(MarketCoverageRegistration {
        type_id: 34,
        type_name: "Tritanium".to_string(),
    });

    tokio::join!(
        service.refresh_type(
            workspace_id,
            source_id,
            JITA_SCOPE,
            item.clone(),
            no_cancel()
        ),
        service.refresh_type(workspace_id, source_id, JITA_SCOPE, item, no_cancel()),
    );

    assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
    assert_eq!(repository.completed.lock().unwrap().len(), 1);
}
