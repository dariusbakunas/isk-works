use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use iskworks_api::{build_router, AppState};
use iskworks_core::{
    apply_inventory_event, InventoryBalance, InventoryError, InventoryEvent, InventoryEventId,
    InventoryHistory, InventoryItemKey, InventoryPosting, InventoryRepository, Money, NewWorkspace,
    OwnerId, WorkspaceId, WorkspaceState,
};
use iskworks_sde::{
    ActiveSde, BlueprintSearchResult, ManufacturingRecipe, SdeError, SdeInventoryTypeMetadata,
    SdeReadRepository, TypeSearchResult,
};
use rust_decimal::Decimal;
use std::str::FromStr;
use tower::ServiceExt;
use uuid::Uuid;

mod support;
use support::inventory::EmptyInventoryRepository;
use support::workspace::{configured_workspace, ConfiguredWorkspaceRepository};

struct TwoItemInventoryRepository;

#[async_trait]
impl InventoryRepository for TwoItemInventoryRepository {
    async fn list_balances(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
    ) -> Result<Vec<InventoryBalance>, InventoryError> {
        let mut tritanium = InventoryBalance::empty(
            InventoryItemKey {
                workspace_id,
                owner_id,
                type_id: 34,
            },
            "Tritanium".to_string(),
        );
        tritanium.quantity = 100;
        Ok(vec![
            tritanium,
            InventoryBalance::empty(
                InventoryItemKey {
                    workspace_id,
                    owner_id,
                    type_id: 5_876,
                },
                "Rifter".to_string(),
            ),
        ])
    }

    async fn get_history(
        &self,
        key: &InventoryItemKey,
    ) -> Result<InventoryHistory, InventoryError> {
        Ok(InventoryHistory {
            balance: InventoryBalance::empty(key.clone(), "Fixture".to_string()),
            events: Vec::new(),
        })
    }

    async fn post(&self, _posting: InventoryPosting) -> Result<InventoryHistory, InventoryError> {
        Err(InventoryError::RevisionConflict)
    }

    async fn reverse_latest(
        &self,
        _key: &InventoryItemKey,
        _event_id: InventoryEventId,
        _expected_revision: u64,
        _reason: String,
    ) -> Result<InventoryHistory, InventoryError> {
        Err(InventoryError::EventNotFound)
    }

    async fn rebuild(
        &self,
        _workspace_id: WorkspaceId,
        _owner_id: OwnerId,
    ) -> Result<Vec<InventoryBalance>, InventoryError> {
        Ok(Vec::new())
    }
}

/// A real, stateful fake: `post` actually applies the event via the same
/// `apply_inventory_event` production logic and persists the result, so
/// multi-step scenarios (e.g. two postings against the same item) behave
/// like the real Postgres repository instead of always failing or ignoring
/// state.
#[derive(Clone, Default)]
struct StatefulInventoryRepository {
    items: Arc<Mutex<HashMap<i64, InventoryHistory>>>,
    /// Per-item `get_history` reads -- the list must never issue one per row.
    history_calls: Arc<std::sync::atomic::AtomicUsize>,
}

impl StatefulInventoryRepository {
    fn seed(&self, balance: InventoryBalance) {
        self.items.lock().unwrap().insert(
            balance.key.type_id,
            InventoryHistory {
                balance,
                events: Vec::new(),
            },
        );
    }
}

#[async_trait]
impl InventoryRepository for StatefulInventoryRepository {
    async fn list_balances(
        &self,
        _workspace_id: WorkspaceId,
        _owner_id: OwnerId,
    ) -> Result<Vec<InventoryBalance>, InventoryError> {
        Ok(self
            .items
            .lock()
            .unwrap()
            .values()
            .map(|history| history.balance.clone())
            .collect())
    }

    async fn get_history(
        &self,
        key: &InventoryItemKey,
    ) -> Result<InventoryHistory, InventoryError> {
        self.history_calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.items
            .lock()
            .unwrap()
            .get(&key.type_id)
            .cloned()
            .ok_or(InventoryError::ItemNotFound)
    }

    async fn list_events_by_type(
        &self,
        _workspace_id: WorkspaceId,
        _owner_id: OwnerId,
        type_ids: &[i64],
    ) -> Result<BTreeMap<i64, Vec<InventoryEvent>>, InventoryError> {
        let items = self.items.lock().unwrap();
        Ok(type_ids
            .iter()
            .filter_map(|type_id| {
                items
                    .get(type_id)
                    .filter(|history| !history.events.is_empty())
                    .map(|history| (*type_id, history.events.clone()))
            })
            .collect())
    }

    async fn post(&self, posting: InventoryPosting) -> Result<InventoryHistory, InventoryError> {
        let mut items = self.items.lock().unwrap();
        let existing = items.get(&posting.key.type_id).cloned();
        let current = existing
            .as_ref()
            .map(|history| history.balance.clone())
            .unwrap_or_else(|| {
                InventoryBalance::empty(posting.key.clone(), posting.type_name.clone())
            });
        let resulting = apply_inventory_event(&current, &posting)?;
        let mut events = existing.map(|history| history.events).unwrap_or_default();
        let sequence = events.len() as u64 + 1;
        events.push(InventoryEvent {
            id: posting.id,
            key: posting.key.clone(),
            type_name: posting.type_name.clone(),
            kind: posting.kind,
            quantity_delta: posting.quantity_delta,
            total_cost_delta: posting.total_cost_delta,
            unit_cost: posting.unit_cost,
            cost_quality: posting.cost_quality,
            source_reference: posting.source_reference,
            note: posting.note,
            effective_at: posting.effective_at,
            recorded_at: posting.recorded_at,
            sequence,
            reverses_event_id: posting.reverses_event_id,
            reversed_by_event_id: None,
            resulting_balance: resulting.clone(),
        });
        let history = InventoryHistory {
            balance: resulting,
            events,
        };
        items.insert(posting.key.type_id, history.clone());
        Ok(history)
    }

    async fn reverse_latest(
        &self,
        _key: &InventoryItemKey,
        _event_id: InventoryEventId,
        _expected_revision: u64,
        _reason: String,
    ) -> Result<InventoryHistory, InventoryError> {
        Err(InventoryError::EventNotFound)
    }

    async fn rebuild(
        &self,
        _workspace_id: WorkspaceId,
        _owner_id: OwnerId,
    ) -> Result<Vec<InventoryBalance>, InventoryError> {
        Ok(Vec::new())
    }
}

#[derive(Clone, Default)]
struct RecordingSdeRepository {
    calls: Arc<Mutex<Vec<Vec<i64>>>>,
}

#[async_trait]
impl SdeReadRepository for RecordingSdeRepository {
    async fn active_sde(&self) -> Result<Option<ActiveSde>, SdeError> {
        Ok(None)
    }

    async fn search_manufacturing_blueprints(
        &self,
        _query: &str,
        _limit: u32,
    ) -> Result<Vec<BlueprintSearchResult>, SdeError> {
        Ok(Vec::new())
    }

    async fn manufacturing_recipe(
        &self,
        _blueprint_type_id: i64,
    ) -> Result<Option<ManufacturingRecipe>, SdeError> {
        Ok(None)
    }

    async fn search_types(
        &self,
        _query: &str,
        _limit: u32,
    ) -> Result<Vec<TypeSearchResult>, SdeError> {
        Ok(Vec::new())
    }

    async fn inventory_type_metadata(
        &self,
        type_ids: &[i64],
    ) -> Result<BTreeMap<i64, SdeInventoryTypeMetadata>, SdeError> {
        self.calls.lock().unwrap().push(type_ids.to_vec());
        let mut metadata = BTreeMap::new();
        if type_ids.contains(&34) {
            metadata.insert(
                34,
                SdeInventoryTypeMetadata {
                    group_name: Some("Mineral".to_string()),
                    packaged_volume_m3: Some(Decimal::new(1, 2)),
                },
            );
        }
        Ok(metadata)
    }

    async fn type_names(&self, type_ids: &[i64]) -> Result<BTreeMap<i64, String>, SdeError> {
        let mut names = BTreeMap::new();
        if type_ids.contains(&39) {
            names.insert(39, "Zydrine".to_string());
        }
        Ok(names)
    }
}

/// Serves fixed order-book contents for any requested `MarketScope`,
/// regardless of `type_ids` -- good enough for asserting the default-scope
/// pricing path resolves at all and reads the right scope, not for
/// exercising the real scope-merge query itself (that's `#[ignore]`d
/// Postgres-backed coverage in `iskworks-storage`). Every other
/// `MarketRepository` method keeps the trait's own default (empty/no-op)
/// body -- none of them are exercised by the Inventory route.
#[derive(Clone, Default)]
struct FixtureMarketRepository {
    orders: BTreeMap<i64, Vec<iskworks_core::MarketOrderView>>,
    scope_calls: Arc<Mutex<Vec<iskworks_core::MarketScope>>>,
}

#[async_trait]
impl iskworks_core::MarketRepository for FixtureMarketRepository {
    async fn resolve_type_name(
        &self,
        _: i64,
    ) -> Result<Option<String>, iskworks_core::MarketError> {
        unimplemented!("not exercised by inventory route tests")
    }
    async fn location_names(
        &self,
        _: WorkspaceId,
        _: &[i64],
    ) -> Result<BTreeMap<i64, String>, iskworks_core::MarketError> {
        unimplemented!("not exercised by inventory route tests")
    }
    async fn save_location_names(
        &self,
        _: WorkspaceId,
        _: Vec<iskworks_core::ResolvedMarketLocation>,
    ) -> Result<(), iskworks_core::MarketError> {
        unimplemented!("not exercised by inventory route tests")
    }
    async fn imported_file_checksums(
        &self,
        _: WorkspaceId,
        _: &[String],
    ) -> Result<std::collections::BTreeSet<String>, iskworks_core::MarketError> {
        unimplemented!("not exercised by inventory route tests")
    }
    async fn commit_import(
        &self,
        _: WorkspaceId,
        _: Vec<iskworks_core::ResolvedMarketExport>,
        _: u64,
        _: Vec<String>,
    ) -> Result<iskworks_core::MarketImportBatch, iskworks_core::MarketError> {
        unimplemented!("not exercised by inventory route tests")
    }
    async fn list_imports(
        &self,
        _: WorkspaceId,
    ) -> Result<Vec<iskworks_core::MarketImportBatch>, iskworks_core::MarketError> {
        unimplemented!("not exercised by inventory route tests")
    }
    async fn get_import(
        &self,
        _: WorkspaceId,
        _: iskworks_core::MarketImportBatchId,
    ) -> Result<iskworks_core::MarketImportBatch, iskworks_core::MarketError> {
        unimplemented!("not exercised by inventory route tests")
    }
    async fn get_order_book(
        &self,
        _: WorkspaceId,
        _: i64,
        _: i64,
        _: Option<iskworks_core::MarketImportBatchId>,
    ) -> Result<iskworks_core::MarketOrderBook, iskworks_core::MarketError> {
        unimplemented!("not exercised by inventory route tests")
    }
    async fn get_market_price_source(
        &self,
        _: WorkspaceId,
        _: iskworks_core::PriceSourceId,
    ) -> Result<iskworks_core::MarketPriceSource, iskworks_core::MarketError> {
        unimplemented!("not exercised by inventory route tests")
    }

    async fn scoped_order_books(
        &self,
        _workspace_id: WorkspaceId,
        scope: iskworks_core::MarketScope,
        type_ids: &[i64],
    ) -> Result<BTreeMap<i64, Vec<iskworks_core::MarketOrderView>>, iskworks_core::MarketError>
    {
        self.scope_calls.lock().unwrap().push(scope);
        Ok(self
            .orders
            .iter()
            .filter(|(type_id, _)| type_ids.contains(type_id))
            .map(|(type_id, orders)| (*type_id, orders.clone()))
            .collect())
    }
}

fn buy_order(
    type_id: i64,
    price: &str,
    volume: u64,
    observed_at: chrono::DateTime<chrono::Utc>,
) -> iskworks_core::MarketOrderView {
    iskworks_core::MarketOrderView {
        observation_id: None,
        import_batch_id: None,
        imported_file_id: None,
        order_id: 1,
        type_id,
        type_name: "Fixture".to_string(),
        side: iskworks_core::MarketOrderSide::Buy,
        price: money(price),
        remaining_volume: volume,
        entered_volume: volume,
        minimum_volume: 1,
        order_range: 32767,
        issued_at: observed_at,
        duration_days: 90,
        observed_at,
        revalidated_at: None,
        location_id: 60_003_760,
        solar_system_id: 30_000_142,
        region_id: 10_000_002,
        jumps: 0,
    }
}

fn app() -> axum::Router {
    build_router(
        AppState::new(Arc::new(configured_workspace("Industry")))
            .with_inventory_repository(Arc::new(EmptyInventoryRepository)),
    )
}

fn posting_json(quality: &str, unit_cost: Option<&str>) -> String {
    serde_json::json!({
        "typeId": 34,
        "typeName": "Tritanium",
        "quantity": 100,
        "unitCost": unit_cost,
        "costQuality": quality,
        "sourceReference": "Fixture",
        "note": "",
        "effectiveAt": "2026-07-25T12:00:00Z",
        "expectedRevision": 0,
        "acknowledgeZeroCost": false
    })
    .to_string()
}

#[tokio::test]
async fn inventory_list_enriches_group_names_in_one_batch() {
    let sde = RecordingSdeRepository::default();
    let app = build_router(
        AppState::new(Arc::new(configured_workspace("Industry")))
            .with_inventory_repository(Arc::new(TwoItemInventoryRepository))
            .with_sde_repository(Arc::new(sde.clone())),
    );
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/inventory")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body[0]["groupName"], "Mineral");
    assert_eq!(body[0]["packagedVolumeM3"], "0.01");
    assert_eq!(body[0]["totalVolumeM3"], "1.00");
    assert!(body[1]["groupName"].is_null());
    assert!(body[1]["packagedVolumeM3"].is_null());
    assert!(body[1]["totalVolumeM3"].is_null());

    let calls = sde.calls.lock().unwrap();
    assert_eq!(calls.len(), 1, "expected a single batched lookup call");
    let mut requested_ids = calls[0].clone();
    requested_ids.sort_unstable();
    assert_eq!(requested_ids, vec![34, 5_876]);
}

#[tokio::test]
async fn inventory_list_has_an_explicit_empty_array() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/api/inventory")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(body.as_ref(), b"[]");
}

#[tokio::test]
async fn inventory_detail_has_an_empty_reservations_array_with_no_production_repository() {
    let (app, repository) = app_with_stateful_inventory();
    repository.seed(InventoryBalance {
        key: InventoryItemKey {
            workspace_id: WorkspaceId(Uuid::nil()),
            owner_id: OwnerId(Uuid::nil()),
            type_id: 34,
        },
        type_name: "Tritanium".to_string(),
        quantity: 100,
        total_historical_cost: money("100.0000"),
        average_unit_cost: Some(money("1.0000")),
        revision: 1,
        last_activity_at: None,
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/inventory/34")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["reservations"], serde_json::json!([]));
}

#[tokio::test]
async fn inventory_detail_resolves_reservations_to_their_order_and_ticket() {
    let (app, repository) = app_with_stateful_inventory_and_reservations(vec![
        iskworks_core::InventoryReservation {
            allocation_id: iskworks_core::order::InventoryAllocationId(Uuid::nil()),
            quantity: 400,
            created_at: chrono::Utc::now(),
            source: iskworks_core::InventoryReservationSource::Order {
                order_id: iskworks_core::order::OrderId(Uuid::nil()),
                display_name: "Manufacture Ishtar".to_string(),
                status: iskworks_core::OrderReservationStatus::InProgress,
            },
        },
        iskworks_core::InventoryReservation {
            allocation_id: iskworks_core::order::InventoryAllocationId(Uuid::nil()),
            quantity: 800,
            created_at: chrono::Utc::now(),
            source: iskworks_core::InventoryReservationSource::Ticket {
                ticket_id: iskworks_core::order::TicketId(Uuid::nil()),
                display_id: "ISK-1852".to_string(),
                status: iskworks_core::order::TicketStatus::Todo,
            },
        },
    ]);
    repository.seed(InventoryBalance {
        key: InventoryItemKey {
            workspace_id: WorkspaceId(Uuid::nil()),
            owner_id: OwnerId(Uuid::nil()),
            type_id: 34,
        },
        type_name: "Tritanium".to_string(),
        quantity: 1_200,
        total_historical_cost: money("1200.0000"),
        average_unit_cost: Some(money("1.0000")),
        revision: 1,
        last_activity_at: None,
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/inventory/34")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();

    assert_eq!(body["reservedQuantity"], 1_200);
    assert_eq!(body["reservations"].as_array().unwrap().len(), 2);
    assert_eq!(body["reservations"][0]["quantity"], 400);
    assert_eq!(body["reservations"][0]["source"]["kind"], "order");
    assert_eq!(
        body["reservations"][0]["source"]["displayName"],
        "Manufacture Ishtar"
    );
    assert_eq!(body["reservations"][0]["source"]["status"], "inProgress");
    assert_eq!(body["reservations"][1]["quantity"], 800);
    assert_eq!(body["reservations"][1]["source"]["kind"], "ticket");
    assert_eq!(body["reservations"][1]["source"]["displayId"], "ISK-1852");
    assert_eq!(body["reservations"][1]["source"]["status"], "todo");
}

#[tokio::test]
async fn opening_preview_returns_exact_strings_and_revision() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/inventory/opening-balance/preview")
                .header("content-type", "application/json")
                .body(Body::from(posting_json("estimated", Some("4.2500"))))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["posting"]["totalCostDelta"], "425.0000");
    assert_eq!(body["resulting"]["averageUnitCost"], "4.2500");
    assert_eq!(body["resulting"]["revision"], 1);
}

/// `CostInputQuality` has no `Unknown` variant -- the invariant that
/// every positive accounted quantity has a cost basis is enforced by
/// the wire format itself, not a runtime acknowledgement check. Sending
/// `"unknown"` fails to deserialize into the enum at all, before any
/// application code runs.
#[tokio::test]
async fn an_unknown_cost_quality_string_is_rejected_by_deserialization() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/inventory/opening-balance/preview")
                .header("content-type", "application/json")
                .body(Body::from(posting_json("unknown", None)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn stale_posting_returns_structured_conflict() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/inventory/opening-balance")
                .header("content-type", "application/json")
                .body(Body::from(posting_json("known", Some("10.0000"))))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["error"]["code"], "revision_conflict");
}

#[tokio::test]
async fn invalid_event_id_is_not_found() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/inventory/34/events/{}/reverse",
                    Uuid::new_v4()
                ))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"expectedRevision":1,"reason":"Wrong"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

fn money(value: &str) -> Money {
    Money(Decimal::from_str(value).unwrap())
}

fn app_with_stateful_inventory() -> (axum::Router, StatefulInventoryRepository) {
    let repository = StatefulInventoryRepository::default();
    let router = build_router(
        AppState::new(Arc::new(configured_workspace("Industry")))
            .with_inventory_repository(Arc::new(repository.clone())),
    );
    (router, repository)
}

/// Returns whatever `reservations`/`observations` it's given for every
/// call, regardless of the requested type -- good enough for asserting
/// the route wires these fields through, not for exercising the storage
/// query itself (that's `#[ignore]`d Postgres-backed coverage in
/// `iskworks-storage`).
#[derive(Default)]
struct FixtureProductionRepository {
    reservations: Vec<iskworks_core::InventoryReservation>,
    observations: BTreeMap<i64, iskworks_core::EsiObservation>,
    holdings: BTreeMap<i64, iskworks_core::EsiHoldings>,
    /// Calls to the per-type `reserved_quantity` read.
    reserved_calls: Arc<std::sync::atomic::AtomicUsize>,
}

#[async_trait]
impl iskworks_core::ProductionRepository for FixtureProductionRepository {
    async fn coverage(
        &self,
        _workspace_id: WorkspaceId,
        _build_id: iskworks_core::BuildId,
    ) -> Result<iskworks_core::BuildCoverageReport, iskworks_core::ProductionError> {
        unimplemented!("not exercised by inventory route tests")
    }

    async fn reserved_quantity(
        &self,
        _workspace_id: WorkspaceId,
        _owner_id: OwnerId,
        _type_id: i64,
    ) -> Result<u64, iskworks_core::ProductionError> {
        self.reserved_calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(self.reservations.iter().map(|item| item.quantity).sum())
    }

    async fn reserved_quantities(
        &self,
        _workspace_id: WorkspaceId,
        _owner_id: OwnerId,
        type_ids: &[i64],
    ) -> Result<BTreeMap<i64, u64>, iskworks_core::ProductionError> {
        let reserved = self.reservations.iter().map(|item| item.quantity).sum();
        Ok(type_ids
            .iter()
            .map(|type_id| (*type_id, reserved))
            .collect())
    }

    async fn list_reservations(
        &self,
        _workspace_id: WorkspaceId,
        _owner_id: OwnerId,
        _type_id: i64,
    ) -> Result<Vec<iskworks_core::InventoryReservation>, iskworks_core::ProductionError> {
        Ok(self.reservations.clone())
    }

    async fn list_esi_observations(
        &self,
        _workspace_id: WorkspaceId,
        _owner_id: OwnerId,
    ) -> Result<BTreeMap<i64, iskworks_core::EsiObservation>, iskworks_core::ProductionError> {
        Ok(self.observations.clone())
    }

    async fn esi_holdings(
        &self,
        _workspace_id: WorkspaceId,
        _owner_id: OwnerId,
        type_id: i64,
    ) -> Result<iskworks_core::EsiHoldings, iskworks_core::ProductionError> {
        Ok(self
            .holdings
            .get(&type_id)
            .cloned()
            .unwrap_or(iskworks_core::EsiHoldings {
                type_id,
                observed_quantity: 0,
                ignored_quantity: 0,
                included_quantity: 0,
                observed_at: None,
                contributors: Vec::new(),
            }))
    }

    async fn set_esi_holding_reconciliation_inclusion(
        &self,
        _workspace_id: WorkspaceId,
        _owner_id: OwnerId,
        command: iskworks_core::SetEsiHoldingReconciliationInclusion,
    ) -> Result<iskworks_core::EsiHoldings, iskworks_core::ProductionError> {
        let mut holdings = self
            .esi_holdings(_workspace_id, _owner_id, command.type_id)
            .await?;
        let row = holdings
            .contributors
            .iter_mut()
            .find(|row| {
                row.eve_character_id == command.eve_character_id
                    && row.location_id == command.effective_location_id
            })
            .ok_or_else(|| {
                iskworks_core::ProductionError::Persistence("stale contributor".into())
            })?;
        row.ignored_for_reconciliation = !command.included;
        holdings.ignored_quantity = if command.included { 0 } else { row.quantity };
        holdings.included_quantity = holdings.observed_quantity - holdings.ignored_quantity;
        Ok(holdings)
    }
}

fn app_with_stateful_inventory_and_reservations(
    reservations: Vec<iskworks_core::InventoryReservation>,
) -> (axum::Router, StatefulInventoryRepository) {
    let repository = StatefulInventoryRepository::default();
    let router = build_router(
        AppState::new(Arc::new(configured_workspace("Industry")))
            .with_inventory_repository(Arc::new(repository.clone()))
            .with_production_repository(Arc::new(FixtureProductionRepository {
                reservations,
                ..Default::default()
            })),
    );
    (router, repository)
}

fn app_with_stateful_inventory_and_esi_observations(
    observations: BTreeMap<i64, iskworks_core::EsiObservation>,
) -> (axum::Router, StatefulInventoryRepository) {
    let repository = StatefulInventoryRepository::default();
    let router = build_router(
        AppState::new(Arc::new(configured_workspace("Industry")))
            .with_sde_repository(Arc::new(RecordingSdeRepository::default()))
            .with_inventory_repository(Arc::new(repository.clone()))
            .with_production_repository(Arc::new(FixtureProductionRepository {
                observations,
                ..Default::default()
            })),
    );
    (router, repository)
}

#[tokio::test]
async fn esi_fields_are_absent_not_zero_when_no_observation_exists() {
    let (app, repository) = app_with_stateful_inventory_and_esi_observations(BTreeMap::new());
    repository.seed(InventoryBalance {
        key: InventoryItemKey {
            workspace_id: WorkspaceId(Uuid::nil()),
            owner_id: OwnerId(Uuid::nil()),
            type_id: 34,
        },
        type_name: "Tritanium".to_string(),
        quantity: 100,
        total_historical_cost: money("100.0000"),
        average_unit_cost: Some(money("1.0000")),
        revision: 1,
        last_activity_at: None,
    });

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/inventory")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let list_body: serde_json::Value = serde_json::from_slice(
        &to_bytes(list_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(list_body.as_array().unwrap().len(), 1);
    assert!(list_body[0]["esiObservedQuantity"].is_null());
    assert!(list_body[0]["esiObservedAt"].is_null());
    assert!(list_body[0]["reconciliationDifference"].is_null());

    let detail_response = app
        .oneshot(
            Request::builder()
                .uri("/api/inventory/34")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let detail_body: serde_json::Value = serde_json::from_slice(
        &to_bytes(detail_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(detail_body["esiObservedQuantity"].is_null());
}

#[tokio::test]
async fn esi_observation_surfaces_a_positive_discrepancy_on_an_existing_item() {
    let observed_at = "2026-08-23T12:00:00Z";
    let (app, repository) = app_with_stateful_inventory_and_esi_observations(BTreeMap::from([(
        34,
        iskworks_core::EsiObservation {
            quantity: 140,
            ignored_quantity: 0,
            included_quantity: 140,
            observed_at: observed_at.parse().unwrap(),
        },
    )]));
    repository.seed(InventoryBalance {
        key: InventoryItemKey {
            workspace_id: WorkspaceId(Uuid::nil()),
            owner_id: OwnerId(Uuid::nil()),
            type_id: 34,
        },
        type_name: "Tritanium".to_string(),
        quantity: 100,
        total_historical_cost: money("100.0000"),
        average_unit_cost: Some(money("1.0000")),
        revision: 1,
        last_activity_at: None,
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/inventory/34")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["esiObservedQuantity"], 140);
    assert_eq!(body["ignoredEsiQuantity"], 0);
    assert_eq!(body["includedEsiQuantity"], 140);
    assert_eq!(body["reconciliationDifference"], 40);
    assert_eq!(body["esiObservedAt"], observed_at);
}

#[tokio::test]
async fn the_default_tracked_list_excludes_a_type_with_no_inventory_balance() {
    let (app, _repository) = app_with_stateful_inventory_and_esi_observations(BTreeMap::from([(
        39,
        iskworks_core::EsiObservation {
            quantity: 50,
            ignored_quantity: 0,
            included_quantity: 50,
            observed_at: "2026-08-23T12:00:00Z".parse().unwrap(),
        },
    )]));

    let list_response = app
        .oneshot(
            Request::builder()
                .uri("/api/inventory")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);
    let list_body: serde_json::Value = serde_json::from_slice(
        &to_bytes(list_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        list_body.as_array().unwrap().len(),
        0,
        "an ESI-only type must not appear in the default (tracked) list"
    );
}

#[tokio::test]
async fn the_untracked_scoped_list_returns_only_a_type_esi_observes_with_no_inventory_balance() {
    let (app, _repository) = app_with_stateful_inventory_and_esi_observations(BTreeMap::from([(
        39,
        iskworks_core::EsiObservation {
            quantity: 50,
            ignored_quantity: 0,
            included_quantity: 50,
            observed_at: "2026-08-23T12:00:00Z".parse().unwrap(),
        },
    )]));

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/inventory?scope=untracked")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);
    let list_body: serde_json::Value = serde_json::from_slice(
        &to_bytes(list_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(list_body.as_array().unwrap().len(), 1);
    assert_eq!(list_body[0]["balance"]["typeName"], "Zydrine");
    assert_eq!(list_body[0]["balance"]["quantity"], 0);
    assert_eq!(list_body[0]["esiObservedQuantity"], 50);
    assert_eq!(list_body[0]["ignoredEsiQuantity"], 0);
    assert_eq!(list_body[0]["includedEsiQuantity"], 50);
    assert_eq!(list_body[0]["reconciliationDifference"], 50);

    // Selecting it (?item=39 deep-link) must not 404 just because Inventory
    // never accounted for it -- it's a real "new item observed by ESI" row,
    // and the detail endpoint doesn't take a scope: it's reachable however
    // the caller got to type 39 (untracked list, or a stored deep-link).
    let detail_response = app
        .oneshot(
            Request::builder()
                .uri("/api/inventory/39")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(detail_response.status(), StatusCode::OK);
    let detail_body: serde_json::Value = serde_json::from_slice(
        &to_bytes(detail_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(detail_body["balance"]["typeName"], "Zydrine");
    assert_eq!(detail_body["balance"]["quantity"], 0);
    assert_eq!(detail_body["esiObservedQuantity"], 50);
    assert_eq!(detail_body["events"], serde_json::json!([]));
}

#[tokio::test]
async fn the_untracked_scoped_list_excludes_a_type_that_already_has_a_balance() {
    let (app, repository) = app_with_stateful_inventory_and_esi_observations(BTreeMap::from([(
        34,
        iskworks_core::EsiObservation {
            quantity: 100,
            ignored_quantity: 0,
            included_quantity: 100,
            observed_at: "2026-08-23T12:00:00Z".parse().unwrap(),
        },
    )]));
    repository.seed(InventoryBalance {
        key: InventoryItemKey {
            workspace_id: WorkspaceId(Uuid::nil()),
            owner_id: OwnerId(Uuid::nil()),
            type_id: 34,
        },
        type_name: "Tritanium".to_string(),
        quantity: 100,
        total_historical_cost: money("100.0000"),
        average_unit_cost: Some(money("1.0000")),
        revision: 1,
        last_activity_at: None,
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/inventory?scope=untracked")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(
        body.as_array().unwrap().len(),
        0,
        "a type that already has an accounting balance must not appear in the untracked list, matched or not"
    );
}

#[tokio::test]
async fn a_type_with_neither_a_balance_nor_an_esi_observation_is_still_not_found() {
    let (app, _repository) = app_with_stateful_inventory_and_esi_observations(BTreeMap::new());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/inventory/99")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn inventory_export_returns_current_balances_and_skips_empty_items() {
    let (app, repository) = app_with_stateful_inventory();
    repository.seed(InventoryBalance {
        key: InventoryItemKey {
            workspace_id: WorkspaceId(Uuid::nil()),
            owner_id: OwnerId(Uuid::nil()),
            type_id: 34,
        },
        type_name: "Tritanium".to_string(),
        quantity: 100,
        total_historical_cost: money("425.0000"),
        average_unit_cost: Some(money("4.2500")),
        revision: 1,
        last_activity_at: None,
    });
    repository.seed(InventoryBalance::empty(
        InventoryItemKey {
            workspace_id: WorkspaceId(Uuid::nil()),
            owner_id: OwnerId(Uuid::nil()),
            type_id: 5_876,
        },
        "Rifter".to_string(),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/inventory/export")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    let items = body["items"].as_array().unwrap();
    assert_eq!(
        items.len(),
        1,
        "the zero-quantity Rifter item should be skipped"
    );
    assert_eq!(items[0]["typeId"], 34);
    assert_eq!(items[0]["typeName"], "Tritanium");
    assert_eq!(items[0]["quantity"], 100);
    assert_eq!(items[0]["averageUnitCost"], "4.2500");
}

fn adjustment_json(quantity_delta: i64, unit_cost: Option<&str>, expected_revision: u64) -> String {
    serde_json::json!({
        "typeId": 34,
        "typeName": "Tritanium",
        "quantityDelta": quantity_delta,
        "unitCost": unit_cost,
        "sourceReference": "Physical inventory correction",
        "note": "",
        "expectedRevision": expected_revision,
    })
    .to_string()
}

#[tokio::test]
async fn positive_adjustment_preview_defaults_to_the_current_weighted_average() {
    let (app, repository) = app_with_stateful_inventory();
    repository.seed(InventoryBalance {
        key: InventoryItemKey {
            workspace_id: WorkspaceId(Uuid::nil()),
            owner_id: OwnerId(Uuid::nil()),
            type_id: 34,
        },
        type_name: "Tritanium".to_string(),
        quantity: 100,
        total_historical_cost: money("425.0000"),
        average_unit_cost: Some(money("4.2500")),
        revision: 1,
        last_activity_at: None,
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/inventory/adjustments/preview")
                .header("content-type", "application/json")
                .body(Body::from(adjustment_json(50, None, 1)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["posting"]["kind"], "adjustment");
    assert_eq!(body["posting"]["unitCost"], "4.2500");
    assert_eq!(body["posting"]["totalCostDelta"], "212.5000");
    assert_eq!(body["posting"]["costQuality"], "known");
    assert_eq!(body["resulting"]["quantity"], 150);
    assert_eq!(body["resulting"]["averageUnitCost"], "4.2500");
}

#[tokio::test]
async fn positive_adjustment_on_a_brand_new_item_without_a_unit_cost_is_rejected() {
    let response = app_with_stateful_inventory()
        .0
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/inventory/adjustments/preview")
                .header("content-type", "application/json")
                .body(Body::from(adjustment_json(50, None, 0)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn negative_adjustment_cannot_carry_a_unit_cost() {
    let (app, repository) = app_with_stateful_inventory();
    repository.seed(InventoryBalance {
        key: InventoryItemKey {
            workspace_id: WorkspaceId(Uuid::nil()),
            owner_id: OwnerId(Uuid::nil()),
            type_id: 34,
        },
        type_name: "Tritanium".to_string(),
        quantity: 100,
        total_historical_cost: money("425.0000"),
        average_unit_cost: Some(money("4.2500")),
        revision: 1,
        last_activity_at: None,
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/inventory/adjustments/preview")
                .header("content-type", "application/json")
                .body(Body::from(adjustment_json(-10, Some("4.2500"), 1)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn negative_adjustment_posts_and_reduces_the_carrying_cost_at_the_weighted_average() {
    let (app, repository) = app_with_stateful_inventory();
    repository.seed(InventoryBalance {
        key: InventoryItemKey {
            workspace_id: WorkspaceId(Uuid::nil()),
            owner_id: OwnerId(Uuid::nil()),
            type_id: 34,
        },
        type_name: "Tritanium".to_string(),
        quantity: 100,
        total_historical_cost: money("1000.0000"),
        average_unit_cost: Some(money("10.0000")),
        revision: 1,
        last_activity_at: None,
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/inventory/adjustments")
                .header("content-type", "application/json")
                .body(Body::from(adjustment_json(-40, None, 1)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["balance"]["quantity"], 60);
    assert_eq!(body["balance"]["totalHistoricalCost"], "600.0000");

    let history = repository
        .get_history(&InventoryItemKey {
            workspace_id: WorkspaceId(Uuid::nil()),
            owner_id: OwnerId(Uuid::nil()),
            type_id: 34,
        })
        .await
        .unwrap();
    assert_eq!(history.events.len(), 1);
    assert_eq!(
        history.events[0].kind,
        iskworks_core::InventoryEventKind::Adjustment
    );
    assert_eq!(
        history.events[0].source_reference,
        "Physical inventory correction"
    );
}

#[tokio::test]
async fn stale_adjustment_returns_structured_conflict() {
    let (app, repository) = app_with_stateful_inventory();
    repository.seed(InventoryBalance {
        key: InventoryItemKey {
            workspace_id: WorkspaceId(Uuid::nil()),
            owner_id: OwnerId(Uuid::nil()),
            type_id: 34,
        },
        type_name: "Tritanium".to_string(),
        quantity: 100,
        total_historical_cost: money("1000.0000"),
        average_unit_cost: Some(money("10.0000")),
        revision: 1,
        last_activity_at: None,
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/inventory/adjustments")
                .header("content-type", "application/json")
                .body(Body::from(adjustment_json(10, Some("10.0000"), 0)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["error"]["code"], "revision_conflict");
}

fn import_item_json(
    type_id: i64,
    quantity: u64,
    average_unit_cost: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "typeId": type_id,
        "typeName": "Fixture Item",
        "quantity": quantity,
        "averageUnitCost": average_unit_cost,
    })
}

#[tokio::test]
async fn inventory_import_reconstructs_a_single_costed_item() {
    let (app, repository) = app_with_stateful_inventory();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/inventory/import")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "items": [import_item_json(34, 100, Some("4.2500"))] })
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["results"][0]["typeId"], 34);
    assert_eq!(body["results"][0]["imported"], true);

    let history = repository
        .get_history(&InventoryItemKey {
            workspace_id: WorkspaceId(Uuid::nil()),
            owner_id: OwnerId(Uuid::nil()),
            type_id: 34,
        })
        .await
        .unwrap();
    assert_eq!(history.events.len(), 1);
    assert_eq!(history.balance.quantity, 100);
    assert_eq!(history.balance.average_unit_cost, Some(money("4.2500")));
    assert_eq!(history.balance.revision, 1);
}

/// Every positive accounted quantity requires a cost basis -- an import
/// item with no `averageUnitCost` is rejected rather than silently
/// imported as unknown-cost.
#[tokio::test]
async fn inventory_import_without_a_unit_cost_is_rejected() {
    let (app, repository) = app_with_stateful_inventory();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/inventory/import")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "items": [import_item_json(34, 100, None)] }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["results"][0]["imported"], false);
    assert!(body["results"][0]["message"]
        .as_str()
        .unwrap()
        .contains("cost"));

    let history = repository
        .get_history(&InventoryItemKey {
            workspace_id: WorkspaceId(Uuid::nil()),
            owner_id: OwnerId(Uuid::nil()),
            type_id: 34,
        })
        .await;
    assert!(matches!(history, Err(InventoryError::ItemNotFound)));
}

#[tokio::test]
async fn inventory_import_reports_per_item_failures_without_aborting_the_batch() {
    let (app, repository) = app_with_stateful_inventory();
    repository.seed(InventoryBalance {
        key: InventoryItemKey {
            workspace_id: WorkspaceId(Uuid::nil()),
            owner_id: OwnerId(Uuid::nil()),
            type_id: 34,
        },
        type_name: "Tritanium".to_string(),
        quantity: 50,
        total_historical_cost: money("500.0000"),
        average_unit_cost: Some(money("10.0000")),
        revision: 1,
        last_activity_at: None,
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/inventory/import")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "items": [
                            import_item_json(34, 100, Some("4.2500")),
                            import_item_json(35, 200, Some("1.0000")),
                        ]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    let results = body["results"].as_array().unwrap();
    assert_eq!(results[0]["typeId"], 34);
    assert_eq!(results[0]["imported"], false);
    assert!(
        results[0]["message"].as_str().unwrap().contains("changed"),
        "blind import always assumes expectedRevision 0, so a pre-existing item should fail with a revision conflict, got: {}",
        results[0]["message"]
    );
    assert_eq!(results[1]["typeId"], 35);
    assert_eq!(results[1]["imported"], true);

    let untouched = repository
        .get_history(&InventoryItemKey {
            workspace_id: WorkspaceId(Uuid::nil()),
            owner_id: OwnerId(Uuid::nil()),
            type_id: 34,
        })
        .await
        .unwrap();
    assert_eq!(
        untouched.balance.quantity, 50,
        "the conflicting item must be left untouched"
    );
}

fn app_with_stateful_inventory_and_esi_holdings(
    holdings: BTreeMap<i64, iskworks_core::EsiHoldings>,
) -> (axum::Router, StatefulInventoryRepository) {
    let repository = StatefulInventoryRepository::default();
    let router = build_router(
        AppState::new(Arc::new(configured_workspace("Industry")))
            .with_inventory_repository(Arc::new(repository.clone()))
            .with_production_repository(Arc::new(FixtureProductionRepository {
                holdings,
                ..Default::default()
            })),
    );
    (router, repository)
}

#[tokio::test]
async fn esi_holdings_route_returns_the_fixture_contributors_for_the_requested_type() {
    let observed_at = "2026-08-23T09:00:00Z";
    let (app, _repository) = app_with_stateful_inventory_and_esi_holdings(BTreeMap::from([(
        34,
        iskworks_core::EsiHoldings {
            type_id: 34,
            observed_quantity: 125_000,
            ignored_quantity: 0,
            included_quantity: 125_000,
            observed_at: Some(observed_at.parse().unwrap()),
            contributors: vec![
                iskworks_core::EsiHoldingContributor {
                    connection_id: Uuid::nil(),
                    eve_character_id: 1_001,
                    character_name: "Corvin".to_string(),
                    location_id: 60_003_760,
                    location_name: Some("Jita IV - Moon 4 - CNAP".to_string()),
                    location_flag: "Hangar".to_string(),
                    quantity: 80_000,
                    ignored_for_reconciliation: false,
                },
                iskworks_core::EsiHoldingContributor {
                    connection_id: Uuid::nil(),
                    eve_character_id: 1_002,
                    character_name: "Hauler Alt".to_string(),
                    location_id: 1_000_000_000_002,
                    location_name: None,
                    location_flag: "Hangar".to_string(),
                    quantity: 45_000,
                    ignored_for_reconciliation: false,
                },
            ],
        },
    )]));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/inventory/34/esi-holdings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["typeId"], 34);
    assert_eq!(body["observedQuantity"], 125_000);
    assert_eq!(body["observedAt"], observed_at);
    let contributors = body["contributors"].as_array().unwrap();
    assert_eq!(contributors.len(), 2);
    assert_eq!(contributors[0]["characterName"], "Corvin");
    assert_eq!(contributors[0]["locationName"], "Jita IV - Moon 4 - CNAP");
    assert_eq!(contributors[1]["characterName"], "Hauler Alt");
    assert!(contributors[1]["locationName"].is_null());
    let total: i64 = contributors
        .iter()
        .map(|contributor| contributor["quantity"].as_i64().unwrap())
        .sum();
    assert_eq!(
        total, 125_000,
        "contributor quantities must sum to exactly the displayed observedQuantity"
    );
}

#[tokio::test]
async fn esi_holdings_route_returns_an_empty_result_not_an_error_for_an_unobserved_type() {
    let (app, _repository) = app_with_stateful_inventory_and_esi_holdings(BTreeMap::new());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/inventory/99999/esi-holdings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["typeId"], 99999);
    assert_eq!(body["observedQuantity"], 0);
    assert!(body["observedAt"].is_null());
    assert_eq!(body["contributors"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn reconciliation_inclusion_route_returns_refreshed_holdings() {
    let (app, _) = app_with_stateful_inventory_and_esi_holdings(BTreeMap::from([(
        34,
        iskworks_core::EsiHoldings {
            type_id: 34,
            observed_quantity: 100,
            ignored_quantity: 0,
            included_quantity: 100,
            observed_at: None,
            contributors: vec![iskworks_core::EsiHoldingContributor {
                connection_id: Uuid::nil(),
                eve_character_id: 9_101,
                character_name: "Valka".into(),
                location_id: 60_014_708,
                location_name: Some("Odebeinn".into()),
                location_flag: "Hangar".into(),
                quantity: 100,
                ignored_for_reconciliation: false,
            }],
        },
    )]));
    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/inventory/34/esi-holdings/reconciliation-inclusion")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"eveCharacterId":9101,"effectiveLocationId":60014708,"included":false}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

// ─── Default-MarketScope Inventory pricing ────────────────────────────────

/// With no `?priceSourceId=` selected and no market repository configured
/// at all, pricing must stay silently unavailable (the pre-existing
/// `Unavailable` fallback) rather than erroring -- most of this file's
/// other tests rely on exactly this, since none of them wire up
/// `with_market_repository`.
#[tokio::test]
async fn inventory_list_has_no_pricing_when_no_market_repository_is_configured() {
    let (app, repository) = app_with_stateful_inventory();
    repository.seed(InventoryBalance {
        key: InventoryItemKey {
            workspace_id: WorkspaceId(Uuid::nil()),
            owner_id: OwnerId(Uuid::nil()),
            type_id: 34,
        },
        type_name: "Tritanium".to_string(),
        quantity: 100,
        total_historical_cost: money("100.0000"),
        average_unit_cost: Some(money("1.0000")),
        revision: 1,
        last_activity_at: None,
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/inventory")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert!(body[0]["currentPrice"].is_null());
    assert!(body[0]["priceSourceId"].is_null());
    assert!(body[0]["marketRegionId"].is_null());
}

/// No `?priceSourceId=` selected, market repository configured, workspace
/// has no `default_market_region_id` set -- prices against
/// `DEFAULT_MARKET_SCOPE` (Jita 4-4) and reports that scope back on the
/// response, with no `PriceSource` identity at all.
#[tokio::test]
async fn inventory_list_prices_against_jita_when_no_workspace_default_scope_is_set() {
    let inventory = StatefulInventoryRepository::default();
    inventory.seed(InventoryBalance {
        key: InventoryItemKey {
            workspace_id: WorkspaceId(Uuid::nil()),
            owner_id: OwnerId(Uuid::nil()),
            type_id: 34,
        },
        type_name: "Tritanium".to_string(),
        quantity: 100,
        total_historical_cost: money("100.0000"),
        average_unit_cost: Some(money("1.0000")),
        revision: 1,
        last_activity_at: None,
    });
    let observed_at = chrono::Utc::now();
    let market = FixtureMarketRepository {
        orders: BTreeMap::from([(34, vec![buy_order(34, "5.5000", 500, observed_at)])]),
        scope_calls: Arc::new(Mutex::new(Vec::new())),
    };
    let scope_calls = market.scope_calls.clone();
    let app = build_router(
        AppState::new(Arc::new(configured_workspace("Industry")))
            .with_inventory_repository(Arc::new(inventory))
            .with_market_repository(Arc::new(market)),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/inventory")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body[0]["currentPrice"], "5.5000");
    assert!(body[0]["priceSourceId"].is_null());
    assert!(body[0]["priceSourceName"].is_null());
    assert_eq!(body[0]["marketRegionId"], 10_000_002);
    assert_eq!(body[0]["marketLocationId"], 60_003_760);

    assert_eq!(
        scope_calls.lock().unwrap().as_slice(),
        [iskworks_core::DEFAULT_MARKET_SCOPE]
    );
}

/// The workspace's own configured default scope wins over
/// `DEFAULT_MARKET_SCOPE` -- proves `Workspace::default_market_scope()` is
/// actually read, not just hardcoded to Jita everywhere.
#[tokio::test]
async fn inventory_list_honors_a_configured_workspace_default_scope_over_jita() {
    let mut workspace = NewWorkspace::manual("Industry".to_string());
    workspace.workspace.default_market_region_id = Some(10_000_043);
    workspace.workspace.default_market_location_id = Some(60_008_494);
    let inventory = StatefulInventoryRepository::default();
    inventory.seed(InventoryBalance {
        key: InventoryItemKey {
            workspace_id: WorkspaceId(Uuid::nil()),
            owner_id: OwnerId(Uuid::nil()),
            type_id: 34,
        },
        type_name: "Tritanium".to_string(),
        quantity: 100,
        total_historical_cost: money("100.0000"),
        average_unit_cost: Some(money("1.0000")),
        revision: 1,
        last_activity_at: None,
    });
    let observed_at = chrono::Utc::now();
    let market = FixtureMarketRepository {
        orders: BTreeMap::from([(34, vec![buy_order(34, "6.0000", 500, observed_at)])]),
        scope_calls: Arc::new(Mutex::new(Vec::new())),
    };
    let scope_calls = market.scope_calls.clone();
    let app = build_router(
        AppState::new(Arc::new(ConfiguredWorkspaceRepository {
            state: WorkspaceState::configured(workspace.workspace, workspace.owner),
        }))
        .with_inventory_repository(Arc::new(inventory))
        .with_market_repository(Arc::new(market)),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/inventory")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body[0]["currentPrice"], "6.0000");
    assert_eq!(body[0]["marketRegionId"], 10_000_043);
    assert_eq!(body[0]["marketLocationId"], 60_008_494);
    assert_eq!(
        scope_calls.lock().unwrap().as_slice(),
        [iskworks_core::MarketScope {
            region_id: 10_000_043,
            location_id: Some(60_008_494),
        }]
    );
}

fn opening_json(
    type_id: i64,
    type_name: &str,
    quantity: u64,
    quality: &str,
    unit_cost: Option<&str>,
    expected_revision: u64,
) -> String {
    serde_json::json!({
        "typeId": type_id,
        "typeName": type_name,
        "quantity": quantity,
        "unitCost": unit_cost,
        "costQuality": quality,
        "sourceReference": "Fixture",
        "note": "",
        "effectiveAt": "2026-07-25T12:00:00Z",
        "expectedRevision": expected_revision,
        "acknowledgeZeroCost": quality == "zeroCost"
    })
    .to_string()
}

async fn post_inventory(app: &axum::Router, path: &str, body: String) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(
        status,
        StatusCode::CREATED,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
}

/// The default-scope tracked list for N items -- events of every cost
/// quality, a reservation, an ESI observation, full/partial/no market
/// depth -- pinned field-for-field, while the route reads history,
/// reservations and order books once per *request*, never once per row.
#[tokio::test]
async fn inventory_list_reads_a_constant_number_of_times_regardless_of_item_count() {
    let inventory = StatefulInventoryRepository::default();
    let history_calls = inventory.history_calls.clone();
    let listing = inventory.clone();
    let observed_at: chrono::DateTime<chrono::Utc> = "2026-01-01T00:00:00Z".parse().unwrap();
    let market = FixtureMarketRepository {
        orders: BTreeMap::from([
            (34, vec![buy_order(34, "5.5000", 500, observed_at)]),
            (35, vec![buy_order(35, "9.2500", 50, observed_at)]),
        ]),
        scope_calls: Arc::new(Mutex::new(Vec::new())),
    };
    let scope_calls = market.scope_calls.clone();
    let production = FixtureProductionRepository {
        reservations: vec![iskworks_core::InventoryReservation {
            allocation_id: iskworks_core::order::InventoryAllocationId(Uuid::nil()),
            quantity: 30,
            created_at: observed_at,
            source: iskworks_core::InventoryReservationSource::Order {
                order_id: iskworks_core::order::OrderId(Uuid::nil()),
                display_name: "Manufacture Ishtar".to_string(),
                status: iskworks_core::OrderReservationStatus::InProgress,
            },
        }],
        observations: BTreeMap::from([(
            35,
            iskworks_core::EsiObservation {
                quantity: 140,
                ignored_quantity: 0,
                included_quantity: 140,
                observed_at,
            },
        )]),
        ..Default::default()
    };
    let reserved_calls = production.reserved_calls.clone();
    let app = build_router(
        AppState::new(Arc::new(configured_workspace("Industry")))
            .with_sde_repository(Arc::new(RecordingSdeRepository::default()))
            .with_inventory_repository(Arc::new(inventory))
            .with_production_repository(Arc::new(production))
            .with_market_repository(Arc::new(market)),
    );
    let opening = "/api/inventory/opening-balance";
    let purchase = "/api/inventory/purchases";
    post_inventory(
        &app,
        opening,
        opening_json(34, "Tritanium", 100, "known", Some("3.5000"), 0),
    )
    .await;
    post_inventory(
        &app,
        opening,
        opening_json(35, "Pyerite", 100, "estimated", Some("4.2500"), 0),
    )
    .await;
    post_inventory(
        &app,
        purchase,
        opening_json(35, "Pyerite", 20, "zeroCost", None, 1),
    )
    .await;
    post_inventory(
        &app,
        opening,
        opening_json(36, "Mexallon", 7, "zeroCost", None, 0),
    )
    .await;
    history_calls.store(0, std::sync::atomic::Ordering::Relaxed);
    reserved_calls.store(0, std::sync::atomic::Ordering::Relaxed);
    scope_calls.lock().unwrap().clear();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/inventory")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    // Rows come back in `list_balances` order.
    let listed: Vec<i64> = listing
        .list_balances(WorkspaceId(Uuid::nil()), OwnerId(Uuid::nil()))
        .await
        .unwrap()
        .iter()
        .map(|balance| balance.key.type_id)
        .collect();
    let returned: Vec<i64> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["balance"]["key"]["typeId"].as_i64().unwrap())
        .collect();
    assert_eq!(returned, listed);
    let items = body.as_array_mut().unwrap();
    for item in items.iter_mut() {
        // Wall-clock recording time and generated ids, not route output.
        item["balance"]["lastActivityAt"] = serde_json::Value::Null;
        item["balance"]["key"]["ownerId"] = serde_json::Value::Null;
        item["balance"]["key"]["workspaceId"] = serde_json::Value::Null;
    }
    items.sort_by_key(|item| item["balance"]["key"]["typeId"].as_i64());
    let expected: serde_json::Value = serde_json::from_str(INVENTORY_LIST_GOLDEN).unwrap();
    assert_eq!(body, expected);

    assert_eq!(
        history_calls.load(std::sync::atomic::Ordering::Relaxed),
        0,
        "no per-row history read"
    );
    assert_eq!(
        reserved_calls.load(std::sync::atomic::Ordering::Relaxed),
        0,
        "no per-row reserved_quantity read"
    );
    assert_eq!(scope_calls.lock().unwrap().len(), 1, "one order-book read");
}

/// `GET /api/inventory`'s exact output for the fixture above, captured
/// from the per-row implementation before the reads were batched.
const INVENTORY_LIST_GOLDEN: &str = r#"[
  {
    "availableQuantity": 70,
    "balance": {
      "averageUnitCost": "3.5000",
      "key": {
        "ownerId": null,
        "typeId": 34,
        "workspaceId": null
      },
      "lastActivityAt": null,
      "quantity": 100,
      "revision": 1,
      "totalHistoricalCost": "350.0000",
      "typeName": "Tritanium"
    },
    "costQuality": "known",
    "currentPrice": "5.5000",
    "currentValue": "550.0000",
    "esiObservedAt": null,
    "esiObservedQuantity": null,
    "groupName": "Mineral",
    "historicalComparisonComplete": true,
    "historicalDifference": "200.0000",
    "ignoredEsiQuantity": null,
    "includedEsiQuantity": null,
    "marketLocationId": 60003760,
    "marketRegionId": 10000002,
    "packagedVolumeM3": "0.01",
    "priceSourceId": null,
    "priceSourceName": null,
    "priceSourceUpdatedAt": "2026-01-01T00:00:00Z",
    "reconciliationDifference": null,
    "reservedQuantity": 30,
    "totalVolumeM3": "1.00",
    "warnings": [
      "The selected market observations are stale; current value may no longer reflect the market."
    ]
  },
  {
    "availableQuantity": 90,
    "balance": {
      "averageUnitCost": "3.5417",
      "key": {
        "ownerId": null,
        "typeId": 35,
        "workspaceId": null
      },
      "lastActivityAt": null,
      "quantity": 120,
      "revision": 2,
      "totalHistoricalCost": "425.0000",
      "typeName": "Pyerite"
    },
    "costQuality": "estimated",
    "currentPrice": "9.2500",
    "currentValue": "1110.0000",
    "esiObservedAt": "2026-01-01T00:00:00Z",
    "esiObservedQuantity": 140,
    "groupName": null,
    "historicalComparisonComplete": true,
    "historicalDifference": "685.0000",
    "ignoredEsiQuantity": 0,
    "includedEsiQuantity": 140,
    "marketLocationId": 60003760,
    "marketRegionId": 10000002,
    "packagedVolumeM3": null,
    "priceSourceId": null,
    "priceSourceName": null,
    "priceSourceUpdatedAt": "2026-01-01T00:00:00Z",
    "reconciliationDifference": 20,
    "reservedQuantity": 30,
    "totalVolumeM3": null,
    "warnings": [
      "Part of this balance was recorded at an estimated cost because no actual cost was entered, so its average cost is approximate.",
      "This inventory includes quantity explicitly recorded at zero cost. Future accounting profit may appear unusually high.",
      "The selected market observations are stale; current value may no longer reflect the market."
    ]
  },
  {
    "availableQuantity": -23,
    "balance": {
      "averageUnitCost": "0.0000",
      "key": {
        "ownerId": null,
        "typeId": 36,
        "workspaceId": null
      },
      "lastActivityAt": null,
      "quantity": 7,
      "revision": 1,
      "totalHistoricalCost": "0.0000",
      "typeName": "Mexallon"
    },
    "costQuality": "zeroCost",
    "currentPrice": null,
    "currentValue": null,
    "esiObservedAt": null,
    "esiObservedQuantity": null,
    "groupName": null,
    "historicalComparisonComplete": true,
    "historicalDifference": null,
    "ignoredEsiQuantity": null,
    "includedEsiQuantity": null,
    "marketLocationId": 60003760,
    "marketRegionId": 10000002,
    "packagedVolumeM3": null,
    "priceSourceId": null,
    "priceSourceName": null,
    "priceSourceUpdatedAt": null,
    "reconciliationDifference": null,
    "reservedQuantity": 30,
    "totalVolumeM3": null,
    "warnings": [
      "This inventory includes quantity explicitly recorded at zero cost. Future accounting profit may appear unusually high.",
      "No market orders are available for this item in the default market scope."
    ]
  }
]"#;
