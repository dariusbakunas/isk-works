use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use chrono::{TimeZone, Utc};
use iskworks_api::{build_router, AppState};
use iskworks_core::{
    AssetFilterOption, BlueprintActivity, BlueprintGroupFacet, BlueprintKind,
    BlueprintLibraryBuildRef, BlueprintLibraryFacets, BlueprintLibraryOrder, BlueprintLibraryPage,
    BlueprintLibraryRepository, BlueprintLibraryRow, BlueprintLibrarySort, BlueprintLibrarySummary,
    BuildId, IndustryError, ValidatedBlueprintLibraryQuery, WorkspaceId,
};
use tower::ServiceExt;
use uuid::Uuid;

mod support;
use support::workspace::configured_workspace;

#[derive(Default)]
struct FakeLibrary {
    queries: Mutex<Vec<ValidatedBlueprintLibraryQuery>>,
}

#[async_trait]
impl BlueprintLibraryRepository for FakeLibrary {
    async fn list(
        &self,
        _workspace_id: WorkspaceId,
        query: &ValidatedBlueprintLibraryQuery,
    ) -> Result<BlueprintLibraryPage, IndustryError> {
        self.queries.lock().unwrap().push(query.clone());
        Ok(BlueprintLibraryPage {
            rows: vec![BlueprintLibraryRow {
                observation_id: Uuid::nil(),
                eve_item_id: 1,
                blueprint_type_id: 691,
                blueprint_name: "Rifter Blueprint".to_string(),
                activity: BlueprintActivity::Manufacturing,
                product_type_id: Some(587),
                product_name: Some("Rifter".to_string()),
                product_group_id: Some(25),
                product_group_name: Some("Frigate".to_string()),
                product_category_id: Some(6),
                product_category_name: Some("Ship".to_string()),
                kind: BlueprintKind::Copy,
                material_efficiency: 10,
                time_efficiency: 20,
                licensed_runs: Some(5),
                connection_id: None,
                character_name: Some("Valka".to_string()),
                location_id: 60_003_760,
                location_name: Some("Jita IV - Moon 4".to_string()),
                location_flag: "Hangar".to_string(),
                observed_at: Utc.with_ymd_and_hms(2026, 10, 18, 12, 0, 0).unwrap(),
                used_by_builds: vec![BlueprintLibraryBuildRef {
                    build_id: BuildId(Uuid::nil()),
                    name: "Rifter run".to_string(),
                }],
            }],
            total: 1,
            summary: BlueprintLibrarySummary {
                originals: 0,
                copies: 1,
                characters: 1,
                latest_observed_at: None,
            },
        })
    }

    async fn facets(
        &self,
        _workspace_id: WorkspaceId,
    ) -> Result<BlueprintLibraryFacets, IndustryError> {
        Ok(BlueprintLibraryFacets {
            categories: vec![AssetFilterOption {
                value: "6".to_string(),
                label: "Ship".to_string(),
                count: 1,
            }],
            groups: vec![BlueprintGroupFacet {
                value: "25".to_string(),
                label: "Frigate".to_string(),
                count: 1,
                category_id: Some(6),
            }],
            characters: vec![],
            locations: vec![],
        })
    }
}

fn app(fake: Arc<FakeLibrary>) -> axum::Router {
    build_router(
        AppState::new(Arc::new(configured_workspace("Blueprints Test")))
            .with_blueprint_library_repository(fake),
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
async fn list_passes_filters_through_and_serializes_camel_case() {
    let fake = Arc::new(FakeLibrary::default());
    let connection = Uuid::new_v4();

    let (status, body) = get(
        app(fake.clone()),
        &format!(
            "/api/industry/blueprints?search=rif&kinds=original%2Ccopy&categoryIds=6&groupIds=25%2C26\
             &connectionIds={connection}&locationIds=60003760&minMe=8&minTe=16&usableOnly=true\
             &activity=manufacturing&sort=runs&order=desc&limit=50&offset=100"
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let query = fake.queries.lock().unwrap().pop().expect("list called");
    assert_eq!(query.search, "rif");
    assert_eq!(query.kinds, [BlueprintKind::Original, BlueprintKind::Copy]);
    assert_eq!(query.category_ids, [6]);
    assert_eq!(query.group_ids, [25, 26]);
    assert_eq!(query.connection_ids, [connection]);
    assert_eq!(query.location_ids, [60_003_760]);
    assert_eq!((query.min_me, query.min_te), (Some(8), Some(16)));
    assert!(query.usable_only);
    assert_eq!(query.activity, Some(BlueprintActivity::Manufacturing));
    assert_eq!(query.sort, BlueprintLibrarySort::Runs);
    assert_eq!(query.order, BlueprintLibraryOrder::Desc);
    assert_eq!((query.limit, query.offset), (50, 100));

    let row = &body["rows"][0];
    assert_eq!(row["blueprintName"], "Rifter Blueprint");
    assert_eq!(row["productCategoryName"], "Ship");
    assert_eq!(row["kind"], "copy");
    assert_eq!(row["licensedRuns"], 5);
    assert_eq!(row["usedByBuilds"][0]["name"], "Rifter run");
    assert_eq!(body["total"], 1);
    assert_eq!(body["summary"]["copies"], 1);
}

#[tokio::test]
async fn list_defaults_to_the_first_page() {
    let fake = Arc::new(FakeLibrary::default());

    let (status, _) = get(app(fake.clone()), "/api/industry/blueprints").await;

    assert_eq!(status, StatusCode::OK);
    let query = fake.queries.lock().unwrap().pop().expect("list called");
    assert_eq!((query.limit, query.offset), (200, 0));
    assert!(query.kinds.is_empty() && !query.usable_only);
}

#[tokio::test]
async fn list_rejects_bad_filters_before_storage_access() {
    for uri in [
        "/api/industry/blueprints?kinds=original%2Cbogus",
        "/api/industry/blueprints?categoryIds=ship",
        "/api/industry/blueprints?connectionIds=not-a-uuid",
        "/api/industry/blueprints?minMe=11",
        "/api/industry/blueprints?limit=501",
    ] {
        let fake = Arc::new(FakeLibrary::default());
        let (status, body) = get(app(fake.clone()), uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
        assert_eq!(body["error"]["code"], "validation_failed", "{uri}");
        assert!(fake.queries.lock().unwrap().is_empty(), "{uri}");
    }
}

#[tokio::test]
async fn list_is_unavailable_without_a_repository() {
    let app = build_router(AppState::new(Arc::new(configured_workspace(
        "Blueprints Test",
    ))));
    let (status, _) = get(app, "/api/industry/blueprints").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn filters_return_facets_with_group_categories() {
    let (status, body) = get(
        app(Arc::new(FakeLibrary::default())),
        "/api/industry/blueprints/filters",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["categories"][0]["label"], "Ship");
    assert_eq!(body["groups"][0]["categoryId"], 6);
}
