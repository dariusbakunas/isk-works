use super::*;

fn confirmed_resolution(
    connection_id: ConnectedCharacterId,
    first_page: EsiResponse<MarketOrderObservation>,
) -> MarketAccessResolution {
    MarketAccessResolution::Confirmed {
        connection_id,
        character_name: "Valka".to_string(),
        access_token: "token".to_string(),
        first_page,
    }
}

#[tokio::test]
async fn structure_scope_single_fetch_serves_every_due_candidate() {
    let candidates = vec![
        coverage(MarketCoverageRegistration {
            type_id: 34,
            type_name: "Tritanium".to_string(),
        }),
        coverage(MarketCoverageRegistration {
            type_id: 35,
            type_name: "Pyerite".to_string(),
        }),
    ];
    let repository = Arc::new(StructureRepository::new(
        MarketLocationClassification::Structure {
            solar_system_id: STRUCTURE_SOLAR_SYSTEM_ID,
        },
        candidates,
    ));
    let connection_id = ConnectedCharacterId::new();
    let first_page = structure_page(1, vec![structure_order(1, 34), structure_order(2, 35)]);
    let market_access =
        FakeMarketAccess::confirmed(vec![confirmed_resolution(connection_id, first_page)]);
    let transport = PageTransport::new(Vec::new());
    let service = PublicMarketService::new(repository.clone(), transport.clone())
        .with_market_access(Some(market_access.clone()));

    service
        .refresh_source(WorkspaceId::new(), PriceSourceId::new(), no_cancel())
        .await
        .unwrap();

    assert_eq!(market_access.calls.load(Ordering::SeqCst), 1);
    // Page 1 came from `first_page` -- with `pages: Some(1)` there is no
    // page 2, so the transport's own `structure_market_orders` is never
    // called at all. This is the "no redundant fetch" property: total
    // structure HTTP calls made anywhere == metadata.pages - 1 (the
    // pages resolution didn't already fetch), not +1 for a re-fetch of
    // page 1.
    assert_eq!(transport.structure_calls.load(Ordering::SeqCst), 0);
    let completed = repository.completed.lock().unwrap();
    assert_eq!(completed.len(), 2);
    assert!(repository.failures.lock().unwrap().is_empty());
    assert_eq!(*repository.remembered.lock().unwrap(), vec![connection_id]);
}

#[tokio::test]
async fn structure_scope_fetches_remaining_pages_without_refetching_page_one() {
    let candidates = vec![coverage(MarketCoverageRegistration {
        type_id: 34,
        type_name: "Tritanium".to_string(),
    })];
    let repository = Arc::new(StructureRepository::new(
        MarketLocationClassification::Structure {
            solar_system_id: STRUCTURE_SOLAR_SYSTEM_ID,
        },
        candidates,
    ));
    let connection_id = ConnectedCharacterId::new();
    let first_page = structure_page(2, vec![structure_order(1, 34)]);
    let market_access =
        FakeMarketAccess::confirmed(vec![confirmed_resolution(connection_id, first_page)]);
    let transport = PageTransport::with_structure_pages(vec![Ok(structure_page(
        2,
        vec![structure_order(2, 34)],
    ))]);
    let service = PublicMarketService::new(repository.clone(), transport.clone())
        .with_market_access(Some(market_access));

    service
        .refresh_source(WorkspaceId::new(), PriceSourceId::new(), no_cancel())
        .await
        .unwrap();

    // Exactly one call -- for page 2. Page 1 was already in hand from
    // resolution and must never be re-requested.
    assert_eq!(transport.structure_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        transport.requested_structures.lock().unwrap().as_slice(),
        [(STRUCTURE_LOCATION_ID, STRUCTURE_SOLAR_SYSTEM_ID, 2)]
    );
    let completed = repository.completed.lock().unwrap();
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].orders.len(), 2);
}

#[tokio::test]
async fn structure_access_denied_mid_fetch_clears_and_retries_once() {
    let candidates = vec![coverage(MarketCoverageRegistration {
        type_id: 34,
        type_name: "Tritanium".to_string(),
    })];
    let repository = Arc::new(StructureRepository::new(
        MarketLocationClassification::Structure {
            solar_system_id: STRUCTURE_SOLAR_SYSTEM_ID,
        },
        candidates,
    ));
    let stale_connection = ConnectedCharacterId::new();
    let fresh_connection = ConnectedCharacterId::new();
    *repository.preferred_connection.lock().unwrap() = Some(stale_connection);
    let market_access = FakeMarketAccess::confirmed(vec![
        confirmed_resolution(
            stale_connection,
            structure_page(2, vec![structure_order(1, 34)]),
        ),
        confirmed_resolution(
            fresh_connection,
            structure_page(1, vec![structure_order(2, 34)]),
        ),
    ]);
    // First structure_market_orders call (page 2 of the stale
    // connection's book) comes back denied; the retry's resolution has
    // only 1 page, so no second transport call happens.
    let transport = PageTransport::with_structure_pages(vec![Err(EsiError::AccessDenied)]);
    let service = PublicMarketService::new(repository.clone(), transport.clone())
        .with_market_access(Some(market_access.clone()));

    service
        .refresh_source(WorkspaceId::new(), PriceSourceId::new(), no_cancel())
        .await
        .unwrap();

    assert_eq!(market_access.calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        market_access.seen_preferred.lock().unwrap().as_slice(),
        [Some(stale_connection), None]
    );
    assert_eq!(repository.cleared.load(Ordering::SeqCst), 1);
    assert_eq!(
        *repository.remembered.lock().unwrap(),
        vec![fresh_connection]
    );
    let completed = repository.completed.lock().unwrap();
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].orders[0].order_id, 2);
    assert!(repository.failures.lock().unwrap().is_empty());
}

#[tokio::test]
async fn structure_access_denied_twice_fails_every_claimed_candidate() {
    let candidates = vec![
        coverage(MarketCoverageRegistration {
            type_id: 34,
            type_name: "Tritanium".to_string(),
        }),
        coverage(MarketCoverageRegistration {
            type_id: 35,
            type_name: "Pyerite".to_string(),
        }),
    ];
    let repository = Arc::new(StructureRepository::new(
        MarketLocationClassification::Structure {
            solar_system_id: STRUCTURE_SOLAR_SYSTEM_ID,
        },
        candidates,
    ));
    let connection_id = ConnectedCharacterId::new();
    let market_access = FakeMarketAccess::confirmed(vec![
        confirmed_resolution(
            connection_id,
            structure_page(2, vec![structure_order(1, 34)]),
        ),
        confirmed_resolution(
            connection_id,
            structure_page(2, vec![structure_order(2, 34)]),
        ),
    ]);
    let transport = PageTransport::with_structure_pages(vec![
        Err(EsiError::AccessDenied),
        Err(EsiError::AccessDenied),
    ]);
    let service = PublicMarketService::new(repository.clone(), transport)
        .with_market_access(Some(market_access.clone()));

    service
        .refresh_source(WorkspaceId::new(), PriceSourceId::new(), no_cancel())
        .await
        .unwrap();

    assert_eq!(market_access.calls.load(Ordering::SeqCst), 2);
    assert_eq!(repository.cleared.load(Ordering::SeqCst), 2);
    assert!(repository.completed.lock().unwrap().is_empty());
    let failures = repository.failures.lock().unwrap();
    assert_eq!(failures.len(), 2);
    assert!(failures
        .iter()
        .all(|(_, message)| message.contains("denied")));
}

#[tokio::test]
async fn structure_no_eligible_character_fails_claimed_with_zero_transport_calls() {
    let candidates = vec![coverage(MarketCoverageRegistration {
        type_id: 34,
        type_name: "Tritanium".to_string(),
    })];
    let repository = Arc::new(StructureRepository::new(
        MarketLocationClassification::Structure {
            solar_system_id: STRUCTURE_SOLAR_SYSTEM_ID,
        },
        candidates,
    ));
    let market_access =
        FakeMarketAccess::confirmed(vec![MarketAccessResolution::NoEligibleCharacter]);
    let transport = PageTransport::new(Vec::new());
    let service = PublicMarketService::new(repository.clone(), transport.clone())
        .with_market_access(Some(market_access));

    service
        .refresh_source(WorkspaceId::new(), PriceSourceId::new(), no_cancel())
        .await
        .unwrap();

    assert_eq!(transport.structure_calls.load(Ordering::SeqCst), 0);
    assert_eq!(transport.calls.load(Ordering::SeqCst), 0);
    assert!(repository.completed.lock().unwrap().is_empty());
    let failures = repository.failures.lock().unwrap();
    assert_eq!(failures.len(), 1);
    assert!(failures[0].1.contains("no connected character"));
}

#[tokio::test]
async fn unknown_location_fails_claimed_with_zero_esi_calls_of_either_kind() {
    let candidates = vec![coverage(MarketCoverageRegistration {
        type_id: 34,
        type_name: "Tritanium".to_string(),
    })];
    let repository = Arc::new(StructureRepository::new(
        MarketLocationClassification::Unknown,
        candidates,
    ));
    let transport = PageTransport::new(Vec::new());
    let service = PublicMarketService::new(repository.clone(), transport.clone());

    service
        .refresh_source(WorkspaceId::new(), PriceSourceId::new(), no_cancel())
        .await
        .unwrap();

    assert_eq!(transport.calls.load(Ordering::SeqCst), 0);
    assert_eq!(transport.structure_calls.load(Ordering::SeqCst), 0);
    assert!(repository.completed.lock().unwrap().is_empty());
    let failures = repository.failures.lock().unwrap();
    assert_eq!(failures.len(), 1);
    assert!(failures[0].1.contains("not a known NPC station"));
}

#[tokio::test]
async fn structure_dispatch_with_no_market_access_configured_fails_claimed() {
    let candidates = vec![coverage(MarketCoverageRegistration {
        type_id: 34,
        type_name: "Tritanium".to_string(),
    })];
    let repository = Arc::new(StructureRepository::new(
        MarketLocationClassification::Structure {
            solar_system_id: STRUCTURE_SOLAR_SYSTEM_ID,
        },
        candidates,
    ));
    let transport = PageTransport::new(Vec::new());
    // No `.with_market_access(...)` -- mirrors an environment where EVE
    // SSO isn't configured.
    let service = PublicMarketService::new(repository.clone(), transport);

    service
        .refresh_source(WorkspaceId::new(), PriceSourceId::new(), no_cancel())
        .await
        .unwrap();

    assert!(repository.completed.lock().unwrap().is_empty());
    let failures = repository.failures.lock().unwrap();
    assert_eq!(failures.len(), 1);
    assert!(failures[0].1.contains("ESI is not configured"));
}
