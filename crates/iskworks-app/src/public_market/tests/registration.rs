use super::*;

/// `register_and_refresh` prioritizes exactly the items it was just
/// asked to register -- this is what lets a later, smaller request
/// (single item or a different category) outrank an earlier, larger
/// category's still-draining backlog in `candidates()`'s
/// `priority_requested_at DESC` ordering, rather than the two ties
/// falling back to arbitrary `type_id` ordering.
#[tokio::test]
async fn register_and_refresh_prioritizes_every_item_it_just_registered() {
    // The per-workspace path, which only serves structure scopes
    // (public scopes prioritize app-wide demand instead).
    let repository = Arc::new(structure_recording_repository());
    let transport = PageTransport::new(Vec::new());
    let service = PublicMarketService::new(repository.clone(), transport);

    service
        .register_and_refresh(
            WorkspaceId::new(),
            PriceSourceId::new(),
            vec![
                MarketCoverageRegistration {
                    type_id: 34,
                    type_name: "Tritanium".to_string(),
                },
                MarketCoverageRegistration {
                    type_id: 35,
                    type_name: "Pyerite".to_string(),
                },
            ],
        )
        .await
        .unwrap();

    let calls = repository.prioritize_calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0], vec![34, 35]);
}

/// `register_and_refresh` must return coverage scoped to exactly the
/// items it was asked to register, not whatever else the repository's
/// `register` happens to hand back -- the real Postgres-backed
/// repository returns *every* coverage row for the price source, and
/// `request_market_data_for_group`'s `already_current_count` (routes/
/// market.rs) is computed directly from this return value, so an
/// unrelated row leaking through here silently miscounts it.
#[tokio::test]
async fn register_and_refresh_scopes_its_return_value_to_the_registered_items() {
    let repository = Arc::new(structure_recording_repository());
    *repository.register_extra.lock().unwrap() = vec![coverage(MarketCoverageRegistration {
        type_id: 999,
        type_name: "Unrelated Item Already In Coverage".to_string(),
    })];
    let transport = PageTransport::new(Vec::new());
    let service = PublicMarketService::new(repository.clone(), transport);

    let coverage_returned = service
        .register_and_refresh(
            WorkspaceId::new(),
            PriceSourceId::new(),
            vec![MarketCoverageRegistration {
                type_id: 34,
                type_name: "Tritanium".to_string(),
            }],
        )
        .await
        .unwrap();

    assert_eq!(
        coverage_returned
            .iter()
            .map(|item| item.type_id)
            .collect::<Vec<_>>(),
        vec![34]
    );
}

/// `register_and_refresh_now` awaits the dispatch instead of spawning
/// it (proved by `transport.calls` already reflecting the fetch
/// immediately after the outer `.await` returns, with no extra yield
/// in between -- under `#[tokio::test]`'s single-threaded runtime, a
/// merely-spawned task would not have run yet at this point), and
/// dispatches *only* the caller's own identity-scoped item, never an
/// unrelated item that happens to already be sitting in the same due
/// backlog.
#[tokio::test]
async fn register_and_refresh_now_awaits_and_is_identity_scoped() {
    let repository = Arc::new(RecordingRepository {
        candidates: Mutex::new(vec![
            coverage(MarketCoverageRegistration {
                type_id: 34,
                type_name: "Tritanium".to_string(),
            }),
            coverage(MarketCoverageRegistration {
                type_id: 99,
                type_name: "Unrelated Due Item".to_string(),
            }),
        ]),
        ..RecordingRepository::default()
    });
    let transport = PageTransport::new(vec![Ok(response(1, 101, JITA_LOCATION_ID))]);
    let service = PublicMarketService::new(repository.clone(), transport.clone());

    let coverage_result = service
        .register_and_refresh_now(
            WorkspaceId::new(),
            PriceSourceId::new(),
            vec![MarketCoverageRegistration {
                type_id: 34,
                type_name: "Tritanium".to_string(),
            }],
        )
        .await
        .unwrap();

    assert_eq!(
        transport.calls.load(Ordering::SeqCst),
        1,
        "the fetch must already have happened by the time the await returns"
    );
    assert_eq!(
        coverage_result.len(),
        1,
        "only the requested type_id, never the unrelated due item"
    );
    assert_eq!(coverage_result[0].type_id, 34);
}
