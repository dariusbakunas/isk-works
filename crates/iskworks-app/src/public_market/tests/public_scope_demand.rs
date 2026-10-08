use super::*;

// --- Public scopes register app-wide demand only ---

fn tritanium_registration() -> Vec<MarketCoverageRegistration> {
    vec![MarketCoverageRegistration {
        type_id: 34,
        type_name: "Tritanium".to_string(),
    }]
}

#[tokio::test]
async fn register_and_refresh_for_a_station_registers_only_app_wide_demand() {
    let repository = Arc::new(RecordingRepository::default());
    let service = PublicMarketService::new(repository.clone(), PageTransport::new(Vec::new()));

    let coverage = service
        .register_and_refresh(
            WorkspaceId::new(),
            PriceSourceId::new(),
            tritanium_registration(),
        )
        .await
        .unwrap();

    assert_eq!(
        *repository.public_demand.lock().unwrap(),
        vec![(JITA_REGION_ID, vec![34], true)]
    );
    assert!(
        repository.prioritize_calls.lock().unwrap().is_empty(),
        "no per-workspace coverage for a public scope"
    );
    assert_eq!(
        coverage.iter().map(|item| item.type_id).collect::<Vec<_>>(),
        vec![34],
        "the app-wide rows are returned"
    );
}

#[tokio::test]
async fn register_and_refresh_now_refreshes_the_app_wide_row_before_returning() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![Ok(response(1, 101, JITA_LOCATION_ID))]);
    let service = PublicMarketService::new(repository.clone(), transport.clone());

    let coverage = service
        .register_and_refresh_now(
            WorkspaceId::new(),
            PriceSourceId::new(),
            tritanium_registration(),
        )
        .await
        .unwrap();

    assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
    assert_eq!(repository.public_completed.lock().unwrap().len(), 1);
    assert!(repository.completed.lock().unwrap().is_empty());
    assert_eq!(
        coverage.iter().map(|item| item.type_id).collect::<Vec<_>>(),
        vec![34]
    );
}

#[tokio::test]
async fn an_api_request_fetches_a_bounded_batch_and_leaves_the_rest_to_the_worker() {
    let repository = Arc::new(RecordingRepository::default());
    let registrations = (1..=40)
        .map(|type_id| MarketCoverageRegistration {
            type_id,
            type_name: format!("Type {type_id}"),
        })
        .collect::<Vec<_>>();
    let transport = PageTransport::new(
        (0..40)
            .map(|_| Ok(response(1, 101, JITA_LOCATION_ID)))
            .collect(),
    );
    let service = PublicMarketService::new(repository.clone(), transport.clone());

    let coverage = service
        .register_and_refresh_now(WorkspaceId::new(), PriceSourceId::new(), registrations)
        .await
        .unwrap();

    assert_eq!(coverage.len(), 40, "every item is registered and returned");
    assert_eq!(
        transport.calls.load(Ordering::SeqCst),
        crate::public_market::API_INLINE_REFRESH_LIMIT,
        "only a bounded batch is fetched inline"
    );
}

#[tokio::test]
async fn a_region_wide_scope_registers_app_wide_demand() {
    let repository = Arc::new(RecordingRepository {
        scope: MarketScope {
            region_id: JITA_REGION_ID,
            location_id: None,
        },
        classification: MarketLocationClassification::Unknown,
        ..RecordingRepository::default()
    });
    let service = PublicMarketService::new(repository.clone(), PageTransport::new(Vec::new()));

    service
        .register_and_refresh(
            WorkspaceId::new(),
            PriceSourceId::new(),
            tritanium_registration(),
        )
        .await
        .unwrap();

    assert_eq!(
        *repository.public_demand.lock().unwrap(),
        vec![(JITA_REGION_ID, vec![34], true)]
    );
}

#[tokio::test]
async fn a_structure_scope_keeps_per_workspace_coverage() {
    let repository = Arc::new(RecordingRepository {
        scope: STRUCTURE_SCOPE,
        classification: MarketLocationClassification::Structure {
            solar_system_id: STRUCTURE_SOLAR_SYSTEM_ID,
        },
        ..RecordingRepository::default()
    });
    let service = PublicMarketService::new(repository.clone(), PageTransport::new(Vec::new()));

    service
        .register_and_refresh(
            WorkspaceId::new(),
            PriceSourceId::new(),
            tritanium_registration(),
        )
        .await
        .unwrap();

    assert!(
        repository.public_demand.lock().unwrap().is_empty(),
        "structure books stay per workspace"
    );
    assert_eq!(*repository.prioritize_calls.lock().unwrap(), vec![vec![34]]);
}

#[tokio::test]
async fn a_public_demand_failure_fails_the_request() {
    let repository = Arc::new(RecordingRepository::default());
    repository
        .public_demand_should_fail
        .store(true, Ordering::SeqCst);
    let service = PublicMarketService::new(repository.clone(), PageTransport::new(Vec::new()));

    assert!(
        service
            .register_and_refresh(
                WorkspaceId::new(),
                PriceSourceId::new(),
                tritanium_registration()
            )
            .await
            .is_err(),
        "app-wide demand is the only path for a public scope now"
    );
}
