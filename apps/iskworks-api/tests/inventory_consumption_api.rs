use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use chrono::{Duration, NaiveDate, Utc};
use iskworks_api::{build_router, AppState};
use iskworks_core::{
    ConsumptionDay, ConsumptionFacts, ConsumptionTypeFact, InventoryConsumptionRepository,
    InventoryError, MaterialCategory, Money, WorkspaceId,
};
use tower::ServiceExt;

mod support;
use support::workspace::configured_workspace;

fn date(month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, month, day).unwrap()
}

#[derive(Default)]
struct FakeConsumption {
    windows: Mutex<Vec<(NaiveDate, NaiveDate)>>,
}

#[async_trait]
impl InventoryConsumptionRepository for FakeConsumption {
    async fn consumption_facts(
        &self,
        _workspace_id: WorkspaceId,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<ConsumptionFacts, InventoryError> {
        self.windows.lock().unwrap().push((from, to));
        Ok(ConsumptionFacts {
            days: vec![ConsumptionDay {
                day: date(10, 13),
                type_id: 33362,
                quantity: 4597,
                value: Money::parse("512000000").unwrap(),
            }],
            types: vec![ConsumptionTypeFact {
                type_id: 33362,
                name: "Photonic Metamaterials".into(),
                category: MaterialCategory::ReactionIntermediates,
                first_consumed: date(9, 9),
                on_hand: 512,
            }],
        })
    }
}

fn app(fake: Arc<FakeConsumption>) -> axum::Router {
    build_router(
        AppState::new(Arc::new(configured_workspace("Consumption Test")))
            .with_inventory_consumption_repository(fake),
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
async fn consumption_reports_the_requested_range_and_fetches_sparkline_weeks() {
    let fake = Arc::new(FakeConsumption::default());
    let (status, body) = get(
        app(fake.clone()),
        "/api/inventory/consumption?from=2026-10-12&to=2026-10-18&granularity=day",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["from"], "2026-10-12");
    assert_eq!(body["to"], "2026-10-18");
    assert_eq!(body["granularity"], "day");
    assert_eq!(body["buckets"].as_array().unwrap().len(), 7);
    assert_eq!(body["kpis"]["consumedValue"], "512000000.0000");
    assert_eq!(body["materials"][0]["name"], "Photonic Metamaterials");
    assert_eq!(body["earliestObservedAt"], "2026-09-09");
    // Eight sparkline weeks ending with the week of 10-12 start on 2026-08-24.
    assert_eq!(
        *fake.windows.lock().unwrap(),
        vec![(date(8, 24), date(10, 18))]
    );
}

#[tokio::test]
async fn consumption_without_from_starts_at_the_first_recorded_consumption() {
    let fake = Arc::new(FakeConsumption::default());
    let (status, body) = get(
        app(fake.clone()),
        "/api/inventory/consumption?to=2026-10-20",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["from"], "2026-09-09");
    assert_eq!(body["granularity"], "week");
    assert_eq!(body["buckets"][0]["start"], "2026-09-07");
    let (fetched_from, fetched_to) = fake.windows.lock().unwrap()[0];
    assert_eq!(fetched_to, date(10, 20));
    assert_eq!(fetched_from, date(10, 20) - Duration::days(3 * 366 - 1));
}

#[tokio::test]
async fn consumption_defaults_to_today() {
    let fake = Arc::new(FakeConsumption::default());
    let (status, body) = get(app(fake), "/api/inventory/consumption").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["to"], Utc::now().date_naive().to_string());
}

#[tokio::test]
async fn consumption_rejects_invalid_params_before_storage_access() {
    let fake = Arc::new(FakeConsumption::default());
    for uri in [
        "/api/inventory/consumption?granularity=month",
        "/api/inventory/consumption?granularity=hour",
        "/api/inventory/consumption?from=2026-10-02&to=2026-10-01",
        "/api/inventory/consumption?from=10/01/2026",
        "/api/inventory/consumption?category=Nonsense",
        "/api/inventory/consumption?from=2024-01-01&to=2026-10-01&granularity=day",
    ] {
        let (status, body) = get(app(fake.clone()), uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
        assert_eq!(body["error"]["code"], "validation_failed", "{uri}");
    }
    assert!(fake.windows.lock().unwrap().is_empty());
}

#[tokio::test]
async fn consumption_without_storage_is_unavailable() {
    let app = build_router(AppState::new(Arc::new(configured_workspace(
        "Consumption Test",
    ))));
    let (status, _) = get(app, "/api/inventory/consumption").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}
