use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use iskworks_api::{build_router, AppState};
use tower::ServiceExt;

mod support;
use support::workspace::configured_workspace;

#[tokio::test]
async fn flat_asset_query_rejects_invalid_limit_before_storage_access() {
    let app = build_router(AppState::new(Arc::new(configured_workspace("Assets Test"))));
    let response = app
        .oneshot(
            Request::get("/api/assets?limit=201")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["error"]["code"], "validation_failed");
}

#[tokio::test]
async fn flat_asset_query_accepts_comma_delimited_multi_select_filters() {
    let app = build_router(AppState::new(Arc::new(configured_workspace("Assets Test"))));
    let response = app
        .oneshot(
            Request::get("/api/assets?assetKinds=material%2Cship&locationIds=60003760%2C60008494")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}
