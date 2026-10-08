use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use iskworks_api::{build_router, AppState};
use iskworks_core::WorkspaceRepository;
use iskworks_storage::{PgFacilityRepository, PgWorkspaceRepository};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

async fn fixture(pool: &PgPool) -> AppState {
    let workspace_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();
    let now = chrono::Utc::now();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Facilities API Test',$2,$3,$3)",
    )
    .bind(workspace_id)
    .bind(owner_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Facilities API Test',true,$3,$3)",
    )
    .bind(owner_id)
    .bind(workspace_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let workspace = PgWorkspaceRepository::new(pool.clone());
    AppState::new(Arc::new(workspace) as Arc<dyn WorkspaceRepository>)
        .with_facility_repository(Arc::new(PgFacilityRepository::new(pool.clone())))
}

fn create_json(name: &str, rig: bool) -> String {
    create_json_with_role(name, rig, "manufacturing")
}

fn create_json_with_role(name: &str, rig: bool, role: &str) -> String {
    serde_json::json!({
        "name": name,
        "kind": "upwellStructure",
        "role": role,
        "structureId": 1_234_567_890_i64,
        "structureTypeId": 35_825,
        "structureTypeName": "Raitaru",
        "solarSystemId": 30_000_142,
        "solarSystemName": "Jita",
        "securityClass": "highSec",
        "materialReductionPercent": "1",
        "timeReductionPercent": "15",
        "jobCostReductionPercent": "3",
        "facilityTaxPercent": "1.5",
        "sccSurchargePercent": "0.5",
        "allianceSurchargePercent": "0",
        "fixedSupplementalCost": "1000",
        "manualSystemCostIndex": "0.048",
        "notes": "Home structure",
        "rigs": if rig {
            serde_json::json!([{
                "slotNumber": 1,
                "typeId": 43_920,
                "typeName": "Standup M-Set Basic Material Efficiency I",
                "materialReductionPercent": "2",
                "timeReductionPercent": "0",
            }])
        } else {
            serde_json::json!([])
        },
    })
    .to_string()
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn export_then_import_detects_and_skips_an_equivalent_facility(pool: PgPool) {
    let app = build_router(fixture(&pool).await);

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/industry/facilities")
                .header("content-type", "application/json")
                .body(Body::from(create_json("Raitaru", true)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::CREATED);

    let export_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/industry/facilities/export")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(export_response.status(), StatusCode::OK);
    let export_body: serde_json::Value = serde_json::from_slice(
        &to_bytes(export_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let items = export_body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["name"], "Raitaru");
    // Postgres numeric(9,6) always returns full stored precision, not the
    // trimmed string that was originally submitted — that's fine, since
    // parse_percent on re-import accepts it and reproduces the same value.
    assert_eq!(items[0]["materialReductionPercent"], "1.000000");
    assert_eq!(items[0]["rigs"][0]["typeId"], 43_920);

    let preview_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/industry/facilities/import/preview")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "items": items }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(preview_response.status(), StatusCode::OK);
    let preview_body: serde_json::Value = serde_json::from_slice(
        &to_bytes(preview_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(preview_body["items"][0]["classification"], "duplicate");
    assert_eq!(preview_body["items"][0]["matchBasis"], "eveLocation");

    let import_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/industry/facilities/import")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "actions": [{ "action": "skip", "item": items[0] }] })
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(import_response.status(), StatusCode::OK);
    let import_body: serde_json::Value = serde_json::from_slice(
        &to_bytes(import_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(import_body["results"][0]["status"], "skipped");
    assert_eq!(import_body["results"][0]["name"], "Raitaru");

    let list_response = app
        .oneshot(
            Request::builder()
                .uri("/api/industry/facilities")
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
    let list = list_body.as_array().unwrap();
    assert_eq!(list.len(), 1, "the duplicate import was skipped");
    assert!(list.iter().all(|profile| profile["name"] == "Raitaru"));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn export_skips_archived_facilities(pool: PgPool) {
    let app = build_router(fixture(&pool).await);

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/industry/facilities")
                .header("content-type", "application/json")
                .body(Body::from(create_json("Retiring Station", false)))
                .unwrap(),
        )
        .await
        .unwrap();
    let created: serde_json::Value = serde_json::from_slice(
        &to_bytes(create_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let facility_id = created["id"].as_str().unwrap();

    let archive_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/industry/facilities/{facility_id}/archive"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"expectedRevision":1}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(archive_response.status(), StatusCode::OK);

    let export_response = app
        .oneshot(
            Request::builder()
                .uri("/api/industry/facilities/export")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let export_body: serde_json::Value = serde_json::from_slice(
        &to_bytes(export_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(export_body["items"].as_array().unwrap().len(), 0);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn delete_permanently_removes_profile_and_clears_draft_selections(pool: PgPool) {
    let app = build_router(fixture(&pool).await);
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/industry/facilities")
                .header("content-type", "application/json")
                .body(Body::from(create_json("Disposable Raitaru", false)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::CREATED);
    let created: serde_json::Value = serde_json::from_slice(
        &to_bytes(create_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let facility_id = created["id"].as_str().unwrap();

    let (workspace_id, owner_id): (Uuid, Uuid) =
        sqlx::query_as("SELECT id, owner_id FROM workspaces LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    let sde_id = Uuid::new_v4();
    let build_id = Uuid::new_v4();
    let now = chrono::Utc::now();
    sqlx::query(
        "INSERT INTO sde_imports (id, source_version, source_label, source_checksum, status, active, started_at, completed_at) VALUES ($1, 'test', 'test', $2, 'active', true, $3, $3)",
    )
    .bind(sde_id)
    .bind(Uuid::new_v4().to_string())
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO builds (id, workspace_id, owner_id, display_name, blueprint_type_id, blueprint_name, product_type_id, product_name, product_quantity_per_run, source_sde_dataset_id, source_sde_version, recipe_fingerprint, runs, recipe_kind, created_at, updated_at, plan_root_build_id) VALUES ($1, $2, $3, 'Draft', 1, 'Test Blueprint', 2, 'Test Product', 1, $4, 'test', 'test', 1, 'manufacturing', $5, $5, $1)",
    )
    .bind(build_id)
    .bind(workspace_id)
    .bind(owner_id)
    .bind(sde_id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO build_draft_planning (build_id, planning_input, updated_at) VALUES ($1, $2, $3)",
    )
    .bind(build_id)
    .bind(serde_json::json!({
        "manufacturingFacility": { "facilityProfileId": facility_id },
        "reactionFacility": { "facilityProfileId": facility_id },
        "unrelated": "retained"
    }))
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let delete_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/industry/facilities/{facility_id}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"expectedRevision":1}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

    let planning_input: serde_json::Value =
        sqlx::query_scalar("SELECT planning_input FROM build_draft_planning WHERE build_id = $1")
            .bind(build_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(planning_input.get("manufacturingFacility").is_none());
    assert!(planning_input.get("reactionFacility").is_none());
    assert_eq!(planning_input["unrelated"], "retained");

    let get_response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/industry/facilities/{facility_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn import_reports_per_item_failures_without_aborting_the_batch(pool: PgPool) {
    let app = build_router(fixture(&pool).await);

    let mut valid: serde_json::Value = serde_json::from_str(&create_json("Valid", false)).unwrap();
    let mut invalid: serde_json::Value = serde_json::from_str(&create_json("", false)).unwrap();
    invalid["name"] = serde_json::json!("");
    valid.as_object_mut().unwrap().remove("id");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/industry/facilities/import")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "actions": [
                            { "action": "create", "item": invalid },
                            { "action": "create", "item": valid }
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
    assert_eq!(results[0]["status"], "failed");
    assert!(results[0]["message"]
        .as_str()
        .unwrap()
        .contains("1 to 120 characters"));
    assert_eq!(results[1]["status"], "created");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn import_rolls_back_the_whole_batch_when_a_write_fails(pool: PgPool) {
    let app = build_router(fixture(&pool).await);
    // A test-only constraint makes one otherwise valid item fail at write time.
    sqlx::query(
        "ALTER TABLE industry_facility_profiles ADD CONSTRAINT test_reject_poison CHECK (display_name <> 'Poison')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let mut valid: serde_json::Value = serde_json::from_str(&create_json("Valid", false)).unwrap();
    let mut poison: serde_json::Value =
        serde_json::from_str(&create_json("Poison", false)).unwrap();
    poison["structureId"] = serde_json::json!(1_234_567_891_i64);
    valid.as_object_mut().unwrap().remove("id");

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/industry/facilities/import")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "actions": [
                            { "action": "create", "item": valid },
                            { "action": "create", "item": poison }
                        ]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    let list_response = app
        .oneshot(
            Request::builder()
                .uri("/api/industry/facilities")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let list: Vec<serde_json::Value> = serde_json::from_slice(
        &to_bytes(list_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(list.is_empty(), "the earlier create was rolled back");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn reaction_role_survives_create_update_and_get(pool: PgPool) {
    let app = build_router(fixture(&pool).await);

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/industry/facilities")
                .header("content-type", "application/json")
                .body(Body::from(create_json_with_role(
                    "Athanor", false, "reaction",
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::CREATED);
    let created: serde_json::Value = serde_json::from_slice(
        &to_bytes(create_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(created["role"], "reaction");
    let facility_id = created["id"].as_str().unwrap();

    let get_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/industry/facilities/{facility_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let fetched: serde_json::Value = serde_json::from_slice(
        &to_bytes(get_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(fetched["role"], "reaction", "role persists after create");

    // Round-trip through an update that explicitly resends role: "reaction" --
    // this is the path the Facilities UI must take on every save, since
    // omitting `role` on edit would silently
    // revert a reaction facility to manufacturing.
    let mut update_body = serde_json::from_str::<serde_json::Value>(&create_json_with_role(
        "Athanor (renamed)",
        false,
        "reaction",
    ))
    .unwrap();
    update_body["expectedRevision"] = serde_json::json!(1);

    let update_response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/industry/facilities/{facility_id}"))
                .header("content-type", "application/json")
                .body(Body::from(update_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(update_response.status(), StatusCode::OK);
    let after_update: serde_json::Value = serde_json::from_slice(
        &to_bytes(update_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        after_update["role"], "reaction",
        "role must survive an explicit update, not silently revert"
    );
    assert_eq!(after_update["name"], "Athanor (renamed)");
}

/// Editing the FacilityProfile itself still uses revision-based optimistic
/// concurrency -- unchanged by the live-Build revision model. "Stale
/// revision while editing the facility" is a conflict; "a Build remembers an
/// older facility revision" is not (see industry_api / root_ticket_api).
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn update_facility_rejects_a_stale_expected_revision(pool: PgPool) {
    let app = build_router(fixture(&pool).await);

    let created: serde_json::Value = serde_json::from_slice(
        &to_bytes(
            app.clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/industry/facilities")
                        .header("content-type", "application/json")
                        .body(Body::from(create_json("Home Raitaru", false)))
                        .unwrap(),
                )
                .await
                .unwrap()
                .into_body(),
            usize::MAX,
        )
        .await
        .unwrap(),
    )
    .unwrap();
    let facility_id = created["id"].as_str().unwrap();
    assert_eq!(created["revision"], 1);

    let put = |name: &str| {
        let mut body =
            serde_json::from_str::<serde_json::Value>(&create_json(name, false)).unwrap();
        body["expectedRevision"] = serde_json::json!(1);
        Request::builder()
            .method("PUT")
            .uri(format!("/api/industry/facilities/{facility_id}"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };

    // First edit against revision 1 succeeds -> profile becomes revision 2.
    let first = app.clone().oneshot(put("Home Raitaru v2")).await.unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    // Second edit still carrying revision 1 -> 409 facility_revision_conflict.
    let second = app.oneshot(put("Home Raitaru v3")).await.unwrap();
    assert_eq!(second.status(), StatusCode::CONFLICT);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(second.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["error"]["code"], "facility_revision_conflict");
    assert_eq!(body["error"]["retryable"], false);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn resolve_structure_reports_not_configured_without_esi_sso(pool: PgPool) {
    let app = build_router(fixture(&pool).await);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/industry/structures/resolve")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "structureId": 1_030_000_000_001_i64 }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["configured"], false);
    assert_eq!(body["structure"], serde_json::Value::Null);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn resolve_structure_rejects_non_positive_structure_id(pool: PgPool) {
    let app = build_router(fixture(&pool).await);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/industry/structures/resolve")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "structureId": 0 }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn resolve_structure_returns_an_already_cached_structure_without_calling_esi(pool: PgPool) {
    let app = build_router(fixture(&pool).await);

    // Seed market_location_names directly, exactly as a prior ESI resolution
    // (e.g. from assets) would have -- proves the cache-hit fast path never
    // needs a connected/eligible character.
    let workspace_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM workspaces LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    let now = chrono::Utc::now();
    sqlx::query(
        "INSERT INTO market_location_names \
         (workspace_id, location_id, location_name, owner_id, solar_system_id, resolved_at, updated_at) \
         VALUES ($1,$2,'X-7OMU - Example Raitaru',1,30_000_001,$3,$3)",
    )
    .bind(workspace_id)
    .bind(1_030_000_000_001_i64)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/industry/structures/resolve")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "structureId": 1_030_000_000_001_i64 }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["configured"], true);
    assert_eq!(
        body["structure"]["structureName"],
        "X-7OMU - Example Raitaru"
    );
    assert_eq!(body["structure"]["structureId"], 1_030_000_000_001_i64);
}
