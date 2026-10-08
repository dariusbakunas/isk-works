//! Route-contract coverage for the explicit inventory-recording endpoints
//! (`POST /api/tickets/:id/record-acquisition` and
//! `.../record-production`). The domain behaviour is exhaustively
//! covered by the storage-layer `#[sqlx::test]`s in `iskworks-storage`;
//! these prove the HTTP contract the routes add on top: status codes (201
//! first post, 200 idempotent replay), request/response shape, and the
//! domain-error -> HTTP-status mapping.

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use iskworks_api::{build_router, AppState};
use iskworks_core::WorkspaceRepository;
use iskworks_storage::{PgInventoryRepository, PgOrderRepository, PgWorkspaceRepository};
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

struct Fixture {
    app: axum::Router,
    acquisition_ticket_id: Uuid,
    manufacturing_ticket_id: Uuid,
}

async fn fixture(pool: &PgPool) -> Fixture {
    let workspace_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();
    let import_id = Uuid::new_v4();
    let build_id = Uuid::new_v4();
    let acquisition_ticket_id = Uuid::new_v4();
    let manufacturing_ticket_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Tickets API Test',$2,$3,$3)",
    )
    .bind(workspace_id).bind(owner_id).bind(now).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Tickets API Test',true,$3,$3)",
    )
    .bind(owner_id).bind(workspace_id).bind(now).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "INSERT INTO sde_imports (id,source_version,source_label,source_checksum,status,active,started_at,completed_at) VALUES ($1,'test','fixture',$2,'active',true,$3,$3)",
    )
    .bind(import_id).bind(format!("tickets-api-{}", Uuid::new_v4())).bind(now).execute(&mut *tx).await.unwrap();
    for (type_id, name) in [
        (34_i64, "Tritanium"),
        (20185, "Crystalline Carbonide Armor Plate"),
    ] {
        sqlx::query(
            "INSERT INTO sde_types (import_id,type_id,name_en,published) VALUES ($1,$2,$3,true)",
        )
        .bind(import_id)
        .bind(type_id)
        .bind(name)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    sqlx::query(
        r#"INSERT INTO builds (id,workspace_id,owner_id,display_name,blueprint_type_id,blueprint_name,
             product_type_id,product_name,product_quantity_per_run,source_sde_dataset_id,
             source_sde_version,recipe_fingerprint,runs,notes,revision,created_at,updated_at,recipe_kind,
             plan_root_build_id)
           VALUES ($1,$2,$3,'Test Build',1000,'Test Blueprint',2000,'Test Product',1,$4,
             'test','fp',1,'',1,$5,$5,'manufacturing',$1)"#,
    )
    .bind(build_id).bind(workspace_id).bind(owner_id).bind(import_id).bind(now).execute(&mut *tx).await.unwrap();

    sqlx::query(
        r#"INSERT INTO tickets (id,workspace_id,owner_id,display_id,kind,type_id,captured_name,quantity,
             status,created_at,updated_at)
           VALUES ($1,$2,$3,'ISK-' || nextval('ticket_display_id_seq'),'acquisition',34,'Tritanium',100,
             'todo',$4,$4)"#,
    )
    .bind(acquisition_ticket_id).bind(workspace_id).bind(owner_id).bind(now).execute(&mut *tx).await.unwrap();
    sqlx::query(
        r#"INSERT INTO tickets (id,workspace_id,owner_id,display_id,kind,type_id,captured_name,quantity,
             source_build_id,status,created_at,updated_at)
           VALUES ($1,$2,$3,'ISK-' || nextval('ticket_display_id_seq'),'manufacturing',20185,
             'Crystalline Carbonide Armor Plate',10,$4,'todo',$5,$5)"#,
    )
    .bind(manufacturing_ticket_id).bind(workspace_id).bind(owner_id).bind(build_id).bind(now).execute(&mut *tx).await.unwrap();
    // The manufacturing ticket's frozen material: Tritanium (type 34), plus
    // stock to consume from.
    sqlx::query(
        r#"INSERT INTO ticket_prerequisites (id,ticket_id,type_id,captured_name,kind,required_quantity,
             reused_quantity,fresh_quantity)
           VALUES ($1,$2,34,'Tritanium','buy',1000000,0,1000000)"#,
    )
    .bind(Uuid::new_v4()).bind(manufacturing_ticket_id).execute(&mut *tx).await.unwrap();
    sqlx::query(
        r#"INSERT INTO inventory_balances (workspace_id,owner_id,type_id,captured_name,quantity,
             total_historical_cost,revision,last_activity_at)
           VALUES ($1,$2,34,'Tritanium',1000,10000,1,$3)"#,
    )
    .bind(workspace_id)
    .bind(owner_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let workspace = PgWorkspaceRepository::new(pool.clone());
    let app = build_router(
        AppState::new(Arc::new(workspace) as Arc<dyn WorkspaceRepository>)
            .with_order_repository(Arc::new(PgOrderRepository::new(pool.clone())))
            .with_inventory_repository(Arc::new(PgInventoryRepository::new(pool.clone()))),
    );
    Fixture {
        app,
        acquisition_ticket_id,
        manufacturing_ticket_id,
    }
}

async fn post_json(app: &axum::Router, path: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

async fn patch_json(app: &axum::Router, path: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

async fn get_json(app: &axum::Router, path: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn recording_reversal_returns_updated_history_effects_and_active_totals(pool: PgPool) {
    let fx = fixture(&pool).await;
    let record_path = format!(
        "/api/tickets/{}/record-acquisition",
        fx.acquisition_ticket_id
    );
    let (status, recorded) = post_json(
        &fx.app,
        &record_path,
        serde_json::json!({
            "idempotencyKey": Uuid::new_v4(),
            "quantity": 120,
            "unitCost": "4.25",
            "locationNote": "Jita 4-4"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let recording_id = recorded["recording"]["id"].as_str().unwrap();

    let path = format!(
        "/api/tickets/{}/recordings/{recording_id}/revert",
        fx.acquisition_ticket_id
    );
    let (status, reversed) = post_json(&fx.app, &path, serde_json::json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reversed["recording"]["status"], "reversed");
    assert!(reversed["recording"]["revertedAt"].is_string());
    assert_eq!(reversed["recording"]["recordedQuantity"], 120);
    assert_eq!(reversed["recording"]["locationNote"], "Jita 4-4");
    assert_eq!(
        reversed["recording"]["effects"][0]["capturedName"],
        "Tritanium"
    );
    assert_eq!(reversed["recording"]["effects"][0]["quantityDelta"], 120);
    assert_eq!(reversed["summary"]["recordedQuantity"], 0);
    assert_eq!(reversed["summary"]["remainingQuantity"], 100);
    assert_eq!(reversed["recordings"].as_array().unwrap().len(), 1);

    let (status, tickets) = get_json(&fx.app, "/api/tickets").await;
    assert_eq!(status, StatusCode::OK);
    let ticket = tickets
        .as_array()
        .unwrap()
        .iter()
        .find(|ticket| ticket["id"] == fx.acquisition_ticket_id.to_string())
        .unwrap();
    assert_eq!(ticket["recording"]["recordedQuantity"], 0);
    assert_eq!(ticket["recordings"][0]["status"], "reversed");

    let (status, conflict) = post_json(&fx.app, &path, serde_json::json!({})).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflict["error"]["code"], "recording_already_reversed");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn recording_reversal_rejects_a_recording_from_another_ticket(pool: PgPool) {
    let fx = fixture(&pool).await;
    let (_, recorded) = post_json(
        &fx.app,
        &format!(
            "/api/tickets/{}/record-acquisition",
            fx.acquisition_ticket_id
        ),
        serde_json::json!({
            "idempotencyKey": Uuid::new_v4(),
            "quantity": 10,
            "unitCost": "1"
        }),
    )
    .await;
    let recording_id = recorded["recording"]["id"].as_str().unwrap();
    let (status, body) = post_json(
        &fx.app,
        &format!(
            "/api/tickets/{}/recordings/{recording_id}/revert",
            fx.manufacturing_ticket_id
        ),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "recording_not_found");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_acquisition_returns_201_then_200_on_idempotent_replay(pool: PgPool) {
    let fx = fixture(&pool).await;
    let key = Uuid::new_v4().to_string();
    let path = format!(
        "/api/tickets/{}/record-acquisition",
        fx.acquisition_ticket_id
    );
    let body = serde_json::json!({ "idempotencyKey": key, "quantity": 100, "unitCost": "4.25" });

    let (status, first) = post_json(&fx.app, &path, body.clone()).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(first["recording"]["recordedQuantity"], 100);
    assert_eq!(first["summary"]["state"], "recorded");
    assert_eq!(first["summary"]["remainingQuantity"], 0);
    let recording_id = first["recording"]["id"].clone();

    // Same key -> 200, same recording, nothing new posted.
    let (status, replay) = post_json(&fx.app, &path, body).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["recording"]["id"], recording_id);

    let events: i64 = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM inventory_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(events, 1);
    let recordings: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::bigint FROM ticket_inventory_recordings")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(recordings, 1);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_acquisition_rejects_zero_quantity_with_400(pool: PgPool) {
    let fx = fixture(&pool).await;
    let (status, _) = post_json(
        &fx.app,
        &format!(
            "/api/tickets/{}/record-acquisition",
            fx.acquisition_ticket_id
        ),
        serde_json::json!({ "idempotencyKey": Uuid::new_v4().to_string(), "quantity": 0 }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_acquisition_rejects_a_manufacturing_ticket_with_409(pool: PgPool) {
    let fx = fixture(&pool).await;
    let (status, body) = post_json(
        &fx.app,
        &format!("/api/tickets/{}/record-acquisition", fx.manufacturing_ticket_id),
        serde_json::json!({ "idempotencyKey": Uuid::new_v4().to_string(), "quantity": 5, "unitCost": "1.0" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        body["error"]["code"],
        "recording_requires_acquisition_ticket"
    );

    let events: i64 = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM inventory_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(events, 0);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_acquisition_returns_404_for_an_unknown_ticket(pool: PgPool) {
    let fx = fixture(&pool).await;
    let (status, _) = post_json(
        &fx.app,
        &format!("/api/tickets/{}/record-acquisition", Uuid::new_v4()),
        serde_json::json!({ "idempotencyKey": Uuid::new_v4().to_string(), "quantity": 1, "unitCost": "1.0" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ─── record-production route contract ──────────────────────────

fn production_body(key: &str, output_quantity: u64) -> Value {
    serde_json::json!({
        "idempotencyKey": key,
        "runsCompleted": 1,
        "output": { "typeId": 20185, "quantity": output_quantity },
        "inputs": [ { "typeId": 34, "quantity": 50 } ],
        "installationCost": "500",
    })
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_production_returns_201_then_200_on_idempotent_replay(pool: PgPool) {
    let fx = fixture(&pool).await;
    let key = Uuid::new_v4().to_string();
    let path = format!(
        "/api/tickets/{}/record-production",
        fx.manufacturing_ticket_id
    );

    let (status, first) = post_json(&fx.app, &path, production_body(&key, 10)).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(first["recording"]["kind"], "production");
    assert_eq!(first["recording"]["runsCompleted"], 1);
    assert_eq!(first["recording"]["outputQuantity"], 10);
    assert_eq!(first["recording"]["installationCost"], "500.0000");
    // No captured plan -> requested defaults to recorded -> Recorded.
    assert_eq!(first["summary"]["state"], "recorded");
    let recording_id = first["recording"]["id"].clone();

    let (status, replay) = post_json(&fx.app, &path, production_body(&key, 999)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["recording"]["id"], recording_id);

    let consumptions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM inventory_events WHERE event_kind = 'consumption'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(consumptions, 1);
    let outputs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM inventory_events WHERE event_kind = 'production_output'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(outputs, 1);
    let recordings: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::bigint FROM ticket_inventory_recordings")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(recordings, 1);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_production_rejects_zero_runs_with_400(pool: PgPool) {
    let fx = fixture(&pool).await;
    let mut body = production_body(&Uuid::new_v4().to_string(), 10);
    body["runsCompleted"] = serde_json::json!(0);
    let (status, _) = post_json(
        &fx.app,
        &format!(
            "/api/tickets/{}/record-production",
            fx.manufacturing_ticket_id
        ),
        body,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_production_rejects_negative_installation_cost_with_400(pool: PgPool) {
    let fx = fixture(&pool).await;
    let mut body = production_body(&Uuid::new_v4().to_string(), 10);
    body["installationCost"] = serde_json::json!("-1");
    let (status, _) = post_json(
        &fx.app,
        &format!(
            "/api/tickets/{}/record-production",
            fx.manufacturing_ticket_id
        ),
        body,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_production_rejects_an_acquisition_ticket_with_409(pool: PgPool) {
    let fx = fixture(&pool).await;
    let (status, body) = post_json(
        &fx.app,
        &format!(
            "/api/tickets/{}/record-production",
            fx.acquisition_ticket_id
        ),
        production_body(&Uuid::new_v4().to_string(), 10),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        body["error"]["code"],
        "recording_requires_production_ticket"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_production_reports_insufficient_inventory_with_409(pool: PgPool) {
    let fx = fixture(&pool).await;
    let mut body = production_body(&Uuid::new_v4().to_string(), 10);
    body["inputs"] = serde_json::json!([{ "typeId": 34, "quantity": 5000 }]); // only 1000 on hand
    let (status, err) = post_json(
        &fx.app,
        &format!(
            "/api/tickets/{}/record-production",
            fx.manufacturing_ticket_id
        ),
        body,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(err["error"]["code"], "insufficient_inventory");

    let events: i64 = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM inventory_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(events, 0);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn record_production_returns_404_for_an_unknown_ticket(pool: PgPool) {
    let fx = fixture(&pool).await;
    let (status, _) = post_json(
        &fx.app,
        &format!("/api/tickets/{}/record-production", Uuid::new_v4()),
        production_body(&Uuid::new_v4().to_string(), 10),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn list_tickets_exposes_a_production_recording_summary(pool: PgPool) {
    let fx = fixture(&pool).await;
    post_json(
        &fx.app,
        &format!(
            "/api/tickets/{}/record-production",
            fx.manufacturing_ticket_id
        ),
        production_body(&Uuid::new_v4().to_string(), 10),
    )
    .await;

    let response = fx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/tickets")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let tickets: Value = serde_json::from_slice(&bytes).unwrap();
    let mfg = tickets
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == fx.manufacturing_ticket_id.to_string())
        .unwrap();
    assert_eq!(mfg["recording"]["state"], "recorded");
    assert_eq!(mfg["recording"]["recordedQuantity"], 1);
}

async fn get_mfg_recording(app: &axum::Router, ticket_id: Uuid) -> Value {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/tickets")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let tickets: Value = serde_json::from_slice(&bytes).unwrap();
    tickets
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == ticket_id.to_string())
        .unwrap()["recording"]
        .clone()
}

/// The primary acceptance flow: a Manufacturing ticket whose creation froze
/// a `runs = 100` execution snapshot must report Partial while under-recorded
/// and Recorded (with surplus) once the recorded runs meet or exceed it --
/// GET /api/tickets is the read model the recording drawer will consume.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn list_tickets_reports_partial_then_recorded_for_a_planned_manufacturing_ticket(
    pool: PgPool,
) {
    let fx = fixture(&pool).await;
    // Stand in for what `create_ticket_for_requirement` now persists: a
    // complete plan frozen for this ticket's own intended run count.
    sqlx::query("UPDATE tickets SET execution_snapshot = $1 WHERE id = $2")
        .bind(serde_json::json!({
            "runs": 100,
            "blueprint": null,
            "facility": null,
            "durationSeconds": 43_200,
            "installationCost": null,
            "materialValue": "987654.0000",
        }))
        .bind(fx.manufacturing_ticket_id)
        .execute(&pool)
        .await
        .unwrap();

    let path = format!(
        "/api/tickets/{}/record-production",
        fx.manufacturing_ticket_id
    );

    let partial_body = serde_json::json!({
        "idempotencyKey": Uuid::new_v4().to_string(),
        "runsCompleted": 40,
        "output": { "typeId": 20185, "quantity": 40 },
        "inputs": [ { "typeId": 34, "quantity": 200 } ],
        "installationCost": "0",
    });
    let (status, _) = post_json(&fx.app, &path, partial_body).await;
    assert_eq!(status, StatusCode::CREATED);

    let recording = get_mfg_recording(&fx.app, fx.manufacturing_ticket_id).await;
    assert_eq!(recording["state"], "partiallyRecorded");
    assert_eq!(recording["requestedQuantity"], 100);
    assert_eq!(recording["recordedQuantity"], 40);
    assert_eq!(recording["remainingQuantity"], 60);
    assert_eq!(recording["surplusQuantity"], 0);

    let closing_body = serde_json::json!({
        "idempotencyKey": Uuid::new_v4().to_string(),
        "runsCompleted": 70,
        "output": { "typeId": 20185, "quantity": 70 },
        "inputs": [ { "typeId": 34, "quantity": 300 } ],
        "installationCost": "0",
    });
    let (status, _) = post_json(&fx.app, &path, closing_body).await;
    assert_eq!(status, StatusCode::CREATED);

    let recording = get_mfg_recording(&fx.app, fx.manufacturing_ticket_id).await;
    assert_eq!(recording["state"], "recorded");
    assert_eq!(recording["requestedQuantity"], 100);
    assert_eq!(recording["recordedQuantity"], 110);
    assert_eq!(recording["remainingQuantity"], 0);
    assert_eq!(recording["surplusQuantity"], 10);
}

// -- Ticket lifecycle workflow-only (start/complete/cancel routes) ---------

async fn get_ticket(app: &axum::Router, ticket_id: Uuid) -> Value {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/tickets")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let tickets: Value = serde_json::from_slice(&bytes).unwrap();
    tickets
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == ticket_id.to_string())
        .unwrap()
        .clone()
}

async fn post_empty(app: &axum::Router, path: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

async fn inventory_event_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*)::bigint FROM inventory_events")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn ticket_recording_count(pool: &PgPool, ticket_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM ticket_inventory_recordings WHERE ticket_id = $1",
    )
    .bind(ticket_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn start_route_moves_todo_to_in_progress_and_posts_nothing(pool: PgPool) {
    let fx = fixture(&pool).await;
    let events_before = inventory_event_count(&pool).await;

    let (status, ticket) = post_empty(
        &fx.app,
        &format!("/api/tickets/{}/start", fx.acquisition_ticket_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ticket["status"], "inProgress");
    assert_eq!(inventory_event_count(&pool).await, events_before);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn complete_route_takes_no_body_and_posts_no_inventory_for_acquisition(pool: PgPool) {
    let fx = fixture(&pool).await;
    post_empty(
        &fx.app,
        &format!("/api/tickets/{}/start", fx.acquisition_ticket_id),
    )
    .await;
    let events_before = inventory_event_count(&pool).await;

    // No request body at all -- completion prices and posts nothing, so
    // there is nothing for a caller to pass.
    let (status, ticket) = post_empty(
        &fx.app,
        &format!("/api/tickets/{}/complete", fx.acquisition_ticket_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {ticket}");
    assert_eq!(ticket["status"], "complete");
    assert_eq!(ticket["actualUnitCost"], Value::Null);
    assert_eq!(ticket["actualLineTotal"], Value::Null);
    assert_eq!(inventory_event_count(&pool).await, events_before);
    assert_eq!(
        ticket_recording_count(&pool, fx.acquisition_ticket_id).await,
        0
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn complete_route_ignores_a_legacy_actual_unit_cost_body_rather_than_rejecting_it(
    pool: PgPool,
) {
    let fx = fixture(&pool).await;
    post_empty(
        &fx.app,
        &format!("/api/tickets/{}/start", fx.acquisition_ticket_id),
    )
    .await;

    // A stale client still sending the old body shape must not be broken --
    // the field is simply never read.
    let (status, ticket) = post_json(
        &fx.app,
        &format!("/api/tickets/{}/complete", fx.acquisition_ticket_id),
        serde_json::json!({ "actualUnitCost": "100" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {ticket}");
    assert_eq!(ticket["status"], "complete");
    assert_eq!(
        ticket["actualUnitCost"],
        Value::Null,
        "the legacy field is ignored, never applied"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn complete_route_posts_no_consumption_for_manufacturing(pool: PgPool) {
    let fx = fixture(&pool).await;
    post_empty(
        &fx.app,
        &format!("/api/tickets/{}/start", fx.manufacturing_ticket_id),
    )
    .await;
    let events_before = inventory_event_count(&pool).await;

    let (status, ticket) = post_empty(
        &fx.app,
        &format!("/api/tickets/{}/complete", fx.manufacturing_ticket_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {ticket}");
    assert_eq!(ticket["status"], "complete");
    assert_eq!(inventory_event_count(&pool).await, events_before);
    assert_eq!(
        ticket_recording_count(&pool, fx.manufacturing_ticket_id).await,
        0
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn recording_then_completing_via_routes_posts_no_additional_inventory(pool: PgPool) {
    let fx = fixture(&pool).await;
    post_empty(
        &fx.app,
        &format!("/api/tickets/{}/start", fx.manufacturing_ticket_id),
    )
    .await;
    let (status, _) = post_json(
        &fx.app,
        &format!(
            "/api/tickets/{}/record-production",
            fx.manufacturing_ticket_id
        ),
        production_body(&Uuid::new_v4().to_string(), 10),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let events_after_recording = inventory_event_count(&pool).await;
    assert!(events_after_recording > 0);
    let recordings_after_recording =
        ticket_recording_count(&pool, fx.manufacturing_ticket_id).await;

    let (status, ticket) = post_empty(
        &fx.app,
        &format!("/api/tickets/{}/complete", fx.manufacturing_ticket_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {ticket}");
    assert_eq!(ticket["status"], "complete");

    assert_eq!(
        inventory_event_count(&pool).await,
        events_after_recording,
        "completing after recording posts nothing more"
    );
    assert_eq!(
        ticket_recording_count(&pool, fx.manufacturing_ticket_id).await,
        recordings_after_recording
    );

    // Recording continues to work unimpeded once the ticket is Complete --
    // workflow status never gates it.
    let (status, _) = post_json(
        &fx.app,
        &format!(
            "/api/tickets/{}/record-production",
            fx.manufacturing_ticket_id
        ),
        production_body(&Uuid::new_v4().to_string(), 5),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn cancel_route_is_workflow_only_and_preserves_a_prior_recording(pool: PgPool) {
    let fx = fixture(&pool).await;
    let (status, _) = post_json(
        &fx.app,
        &format!(
            "/api/tickets/{}/record-production",
            fx.manufacturing_ticket_id
        ),
        production_body(&Uuid::new_v4().to_string(), 10),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let events_before = inventory_event_count(&pool).await;
    let recordings_before = ticket_recording_count(&pool, fx.manufacturing_ticket_id).await;

    let (status, ticket) = post_empty(
        &fx.app,
        &format!("/api/tickets/{}/cancel", fx.manufacturing_ticket_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {ticket}");
    assert_eq!(ticket["status"], "canceled");

    assert_eq!(inventory_event_count(&pool).await, events_before);
    assert_eq!(
        ticket_recording_count(&pool, fx.manufacturing_ticket_id).await,
        recordings_before,
        "cancellation is not an accounting reversal"
    );
    let recording = get_mfg_recording(&fx.app, fx.manufacturing_ticket_id).await;
    assert_eq!(
        recording["state"], "recorded",
        "the Recorded indicator survives cancellation"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn canceling_a_fulfiller_via_the_route_does_not_rewrite_a_dependents_status(pool: PgPool) {
    let fx = fixture(&pool).await;

    // Actually link the acquisition ticket as the manufacturing ticket's
    // prerequisite fulfiller -- the fixture creates both but doesn't wire
    // them together (no route exposes that directly), so this test does it
    // at the storage level and drives only `/cancel` itself over HTTP.
    let prerequisite_id: Uuid =
        sqlx::query_scalar("SELECT id FROM ticket_prerequisites WHERE ticket_id = $1")
            .bind(fx.manufacturing_ticket_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query(
        "INSERT INTO ticket_prerequisite_fulfillments \
         (id, ticket_prerequisite_id, fulfilling_ticket_id, allocated_quantity, linked_at) \
         VALUES ($1, $2, $3, 1000000, now())",
    )
    .bind(Uuid::new_v4())
    .bind(prerequisite_id)
    .bind(fx.acquisition_ticket_id)
    .execute(&pool)
    .await
    .unwrap();

    // Put the dependent in a distinctive lane so any accidental cross-ticket
    // write would be obvious.
    let (status, _) = patch_json(
        &fx.app,
        &format!("/api/tickets/{}", fx.manufacturing_ticket_id),
        serde_json::json!({ "status": "inProgress" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, ticket) = post_empty(
        &fx.app,
        &format!("/api/tickets/{}/cancel", fx.acquisition_ticket_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {ticket}");
    assert_eq!(ticket["status"], "canceled");

    let dependent = get_ticket(&fx.app, fx.manufacturing_ticket_id).await;
    assert_eq!(
        dependent["status"], "inProgress",
        "canceling its fulfiller must never rewrite this ticket's workflow status"
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn patch_status_moves_the_ticket_any_direction_with_no_side_effects(pool: PgPool) {
    let fx = fixture(&pool).await;
    let events_before = inventory_event_count(&pool).await;
    let recordings_before = ticket_recording_count(&pool, fx.acquisition_ticket_id).await;

    // There is no state machine: every one of these transitions is legal,
    // forward and backward, straight into and out of `canceled`.
    for next in ["complete", "todo", "canceled", "inProgress", "todo"] {
        let (status, ticket) = patch_json(
            &fx.app,
            &format!("/api/tickets/{}", fx.acquisition_ticket_id),
            serde_json::json!({ "status": next }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "PATCH -> {next}: {ticket}");
        assert_eq!(ticket["status"], next);
    }

    // No inventory event, no recording -- a status move is bookkeeping-free.
    assert_eq!(inventory_event_count(&pool).await, events_before);
    assert_eq!(
        ticket_recording_count(&pool, fx.acquisition_ticket_id).await,
        recordings_before
    );
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn patch_status_leaves_the_derived_blocker_list_untouched(pool: PgPool) {
    let fx = fixture(&pool).await;

    // The fixture's manufacturing ticket has a 1,000,000-unit Tritanium
    // prerequisite and only 1,000 in stock -- it is blocked, whatever its
    // workflow lane.
    let before = get_ticket(&fx.app, fx.manufacturing_ticket_id).await;
    assert_eq!(before["status"], "todo");
    let blockers_before = before["blockedBy"].as_array().unwrap().clone();
    assert!(!blockers_before.is_empty());

    let (status, _) = patch_json(
        &fx.app,
        &format!("/api/tickets/{}", fx.manufacturing_ticket_id),
        serde_json::json!({ "status": "inProgress" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let after = get_ticket(&fx.app, fx.manufacturing_ticket_id).await;
    assert_eq!(after["status"], "inProgress", "the workflow lane changed");
    assert_eq!(
        after["blockedBy"].as_array().unwrap(),
        &blockers_before,
        "the derived blocker list is independent of the workflow status write"
    );
}
