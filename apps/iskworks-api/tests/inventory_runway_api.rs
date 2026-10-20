use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use chrono::{TimeZone, Utc};
use iskworks_api::{build_router, AppState};
use iskworks_core::order::OrderId;
use iskworks_core::{
    InventoryError, InventoryRunwayRepository, MaterialCategory, Money, OwnerId, RunwayEpic,
    RunwayFacts, RunwayRequirement, RunwayStockPool, RunwayTypeFact, WorkspaceId,
};
use tower::ServiceExt;
use uuid::Uuid;

mod support;
use support::workspace::configured_workspace;

const OWNER: OwnerId = OwnerId(Uuid::from_u128(1));
const EPIC: OrderId = OrderId(Uuid::from_u128(2));

#[derive(Default)]
struct FakeRunway {
    calls: Mutex<u32>,
}

#[async_trait]
impl InventoryRunwayRepository for FakeRunway {
    async fn runway_facts(
        &self,
        _workspace_id: WorkspaceId,
    ) -> Result<RunwayFacts, InventoryError> {
        *self.calls.lock().unwrap() += 1;
        let requirement = |type_id, required, unit: &str| RunwayRequirement {
            order_id: EPIC,
            type_id,
            required,
            reserved: 0,
            consumed: 0,
            unit_cost: Some(Money::parse(unit).unwrap()),
        };
        Ok(RunwayFacts {
            epics: vec![RunwayEpic {
                order_id: EPIC,
                owner_id: OWNER,
                display_name: "Revelation x1".into(),
                created_at: Utc.with_ymd_and_hms(2026, 10, 5, 0, 0, 0).unwrap(),
            }],
            requirements: vec![
                requirement(11399, 13_767, "16700"),
                requirement(16650, 100, "1"),
            ],
            event_consumption: vec![],
            pools: vec![RunwayStockPool {
                owner_id: OWNER,
                type_id: 16650,
                on_hand: 100,
                active_reserved: 0,
            }],
            types: vec![
                RunwayTypeFact {
                    type_id: 11399,
                    name: "Morphite".into(),
                    category: MaterialCategory::Minerals,
                },
                RunwayTypeFact {
                    type_id: 16650,
                    name: "Promethium".into(),
                    category: MaterialCategory::MoonMaterials,
                },
            ],
        })
    }
}

fn app(fake: Arc<FakeRunway>) -> axum::Router {
    build_router(
        AppState::new(Arc::new(configured_workspace("Runway Test")))
            .with_inventory_runway_repository(fake),
    )
}

async fn get(app: axum::Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

#[tokio::test]
async fn runway_reports_shortfall_in_camel_case() {
    let (status, body) = get(app(Arc::default()), "/api/inventory/runway").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["kpis"]["materialsShort"], 1);
    assert_eq!(body["kpis"]["materialsTotal"], 2);
    assert_eq!(body["kpis"]["shortfallValue"], "229908900.0000");
    assert_eq!(body["kpis"]["epicsBlocked"], 1);
    assert_eq!(body["epics"][0]["displayName"], "Revelation x1");
    assert_eq!(body["materials"][0]["name"], "Promethium");
    assert_eq!(body["materials"][0]["coveragePercent"], 100.0);
    assert_eq!(body["materials"][1]["category"], "Minerals");
    assert_eq!(body["materials"][1]["short"], 13_767);
    assert_eq!(body["materials"][1]["epics"][0]["outstanding"], 13_767);
    assert_eq!(body["missingPriceCount"], 0);
}

#[tokio::test]
async fn runway_category_filter_scopes_the_report() {
    let (status, body) = get(
        app(Arc::default()),
        "/api/inventory/runway?category=Moon%20Materials",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["materials"].as_array().unwrap().len(), 1);
    assert_eq!(body["materials"][0]["category"], "Moon Materials");
    assert_eq!(body["kpis"]["materialsShort"], 0);
}

#[tokio::test]
async fn runway_rejects_an_unknown_category_before_storage_access() {
    let fake = Arc::new(FakeRunway::default());
    let (status, body) = get(app(fake.clone()), "/api/inventory/runway?category=Nonsense").await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "validation_failed");
    assert_eq!(*fake.calls.lock().unwrap(), 0);
}

#[tokio::test]
async fn runway_without_storage_is_unavailable() {
    let app = build_router(AppState::new(Arc::new(configured_workspace("Runway Test"))));
    let (status, _) = get(app, "/api/inventory/runway").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}
