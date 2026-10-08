use super::*;

fn worksheet_app(
    parent: Build,
    owner_id: OwnerId,
) -> (
    axum::Router,
    Arc<support::inventory::SeededInventoryRepository>,
) {
    let inventory = Arc::new(support::inventory::SeededInventoryRepository::new(vec![]));
    let router = build_router(
        AppState::new(Arc::new(configured_workspace_owned_by(
            "Industry", owner_id,
        )))
        .with_industry_repository(Arc::new(FixtureIndustryRepository {
            build: Some(parent),
            ..Default::default()
        }))
        .with_sde_repository(Arc::new(FixtureSdeRepository))
        .with_inventory_repository(inventory.clone())
        .with_production_repository(Arc::new(NeverCalledProductionRepository)),
    );
    (router, inventory)
}

async fn post_worksheet(
    app: axum::Router,
    build_id: BuildId,
    command: serde_json::Value,
    focused_producer_id: Option<BuildId>,
) -> (StatusCode, serde_json::Value) {
    post_worksheet_body(
        app,
        build_id,
        serde_json::json!({
            "command": command,
            "focusedProducerId": focused_producer_id,
            "includeDownstream": true,
        }),
    )
    .await
}

async fn post_worksheet_body(
    app: axum::Router,
    build_id: BuildId,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/builds/{}/worksheet", build_id.0))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
async fn post_build_worksheet_returns_complete_rows_and_authoritative_output() {
    let (parent, _workspace_id, owner_id) = rifter_root(1);
    let parent_id = parent.id;
    let (app, inventory) = worksheet_app(parent, owner_id);

    let (status, json) =
        post_worksheet(app, parent_id, overlay(1, serde_json::json!([])), None).await;

    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["scope"]["rootBuildId"], parent_id.0.to_string());
    assert_eq!(json["economicsAreAdditive"], false);
    assert!(json["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|group| group["rows"].as_array().unwrap())
        .any(|row| row["requiredQuantity"].as_u64().unwrap_or(0) > 0));
    assert_eq!(json["output"]["typeId"], 5876);
    assert_eq!(inventory.list_balances_call_count(), 1);
}

#[tokio::test]
async fn post_build_worksheet_defaults_to_direct_materials_only() {
    let (parent, _workspace_id, owner_id) = rifter_root(1);
    let parent_id = parent.id;
    let (app, _inventory) = worksheet_app(parent, owner_id);
    let body = serde_json::json!({"command": overlay(1, serde_json::json!([]))});

    let (status, json) = post_worksheet_body(app, parent_id, body).await;

    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["scope"]["includeDownstream"], false);
}

#[tokio::test]
async fn post_build_worksheet_uses_path_root_over_body_root() {
    let (parent, _workspace_id, owner_id) = rifter_root(1);
    let parent_id = parent.id;
    let (app, _inventory) = worksheet_app(parent, owner_id);
    let mut command = overlay(1, serde_json::json!([]));
    command["buildId"] = serde_json::json!(uuid::Uuid::new_v4());

    let (status, json) = post_worksheet(app, parent_id, command, None).await;

    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["scope"]["rootBuildId"], parent_id.0.to_string());
}
