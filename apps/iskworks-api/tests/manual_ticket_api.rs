//! Canonical manual Ticket creation (`POST /api/tickets`) and the
//! organizational metadata PATCH (`PATCH /api/tickets/:id`) -- proves the
//! HTTP contract for Generic standalone/Epic/Assignee creation, manual
//! Acquisition creation through the existing explicit recording path, and
//! metadata-only Epic/assignee changes. Domain-level coverage (membership
//! persistence, isolation, delete semantics) lives in
//! `iskworks-storage`'s `order::tests`; this proves the route layer wires
//! it correctly.

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
    workspace_id: Uuid,
    owner_id: Uuid,
}

async fn fixture(pool: &PgPool) -> Fixture {
    let workspace_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Manual Ticket Test',$2,$3,$3)",
    )
    .bind(workspace_id).bind(owner_id).bind(now).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Manual Ticket Test',true,$3,$3)",
    )
    .bind(owner_id).bind(workspace_id).bind(now).execute(&mut *tx).await.unwrap();
    // An active SDE import with Tritanium -- real Purchase/Consumption
    // postings (`verify_active_type`) reject any type_id not present in
    // the active import, so the manual-Acquisition-recording test needs
    // this even though nothing else in this fixture touches the SDE.
    let import_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sde_imports (id,source_version,source_label,source_checksum,status,active,started_at,completed_at) \
         VALUES ($1,'test','fixture',$2,'active',true,$3,$3)",
    )
    .bind(import_id)
    .bind(format!("manual-ticket-api-{}", Uuid::new_v4()))
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("INSERT INTO sde_types (import_id,type_id,name_en,published) VALUES ($1,34,'Tritanium',true)")
        .bind(import_id)
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
        workspace_id,
        owner_id,
    }
}

/// A minimal, directly-seeded Order/Epic -- just enough for `orders.id` to
/// exist as a valid FK target for the manual-creation tests below, without
/// the full SDE/blueprint/snapshot ceremony `create_order` itself needs.
async fn seed_epic(pool: &PgPool, workspace_id: Uuid, owner_id: Uuid) -> Uuid {
    let import_id = Uuid::new_v4();
    let build_id = Uuid::new_v4();
    let snapshot_id = Uuid::new_v4();
    let order_id = Uuid::new_v4();
    let now = chrono::Utc::now();
    let mut tx = pool.begin().await.unwrap();
    // `active = false` -- `sde_imports_one_active_idx` allows only one
    // *active* import globally, and `seed_epic` may be called more than
    // once per test (two Epics). Nothing here needs an active import; the
    // FK only needs the row to exist.
    sqlx::query(
        "INSERT INTO sde_imports (id,source_version,source_label,source_checksum,status,active,started_at,completed_at) \
         VALUES ($1,'test','fixture',$2,'superseded',false,$3,$3)",
    )
    .bind(import_id)
    .bind(format!("manual-ticket-api-{}", Uuid::new_v4()))
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO builds (id,workspace_id,owner_id,display_name,blueprint_type_id,blueprint_name,
             product_type_id,product_name,product_quantity_per_run,source_sde_dataset_id,
             source_sde_version,recipe_fingerprint,runs,notes,revision,created_at,updated_at,recipe_kind,
             plan_root_build_id)
           VALUES ($1,$2,$3,'Test Build',1000,'Test Blueprint',2000,'Test Product',1,$4,
             'test','fp',1,'',1,$5,$5,'manufacturing',$1)"#,
    )
    .bind(build_id)
    .bind(workspace_id)
    .bind(owner_id)
    .bind(import_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO price_snapshots (id,workspace_id,build_id,captured_source_name,
             captured_source_revision,purpose,created_at)
           VALUES ($1,$2,$3,'manual',1,'build_planning',$4)"#,
    )
    .bind(snapshot_id)
    .bind(workspace_id)
    .bind(build_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO orders (id,workspace_id,owner_id,source_build_id,source_build_revision,
             display_name,runs,recipe_fingerprint,price_snapshot_id,estimated_material_cost,
             missing_price_count,created_at,updated_at)
           VALUES ($1,$2,$3,$4,1,'Test Epic',1,'fp',$5,0,0,$6,$6)"#,
    )
    .bind(order_id)
    .bind(workspace_id)
    .bind(owner_id)
    .bind(build_id)
    .bind(snapshot_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    order_id
}

/// A minimal connected-character row (`eve_connections`) for the Assignee
/// tests -- the canonical internal connected-character identity
/// `tickets.assignee_character_id` references.
async fn seed_character(pool: &PgPool, workspace_id: Uuid, owner_id: Uuid, name: &str) -> Uuid {
    let connection_id = Uuid::new_v4();
    let now = chrono::Utc::now();
    sqlx::query(
        r#"INSERT INTO eve_connections (id,workspace_id,owner_id,eve_character_id,character_name,
             status,granted_scopes,connected_at,updated_at)
           VALUES ($1,$2,$3,$4,$5,'connected','{}',$6,$6)"#,
    )
    .bind(connection_id)
    .bind(workspace_id)
    .bind(owner_id)
    .bind(rand_character_id())
    .bind(name)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    connection_id
}

fn rand_character_id() -> i64 {
    // Any positive i64 works -- the CHECK is just `> 0`.
    (Uuid::new_v4().as_u128() % 1_000_000_000) as i64 + 1
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
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
    (status, body_json(response).await)
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
    (status, body_json(response).await)
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_generic_ticket_standalone(pool: PgPool) {
    let fx = fixture(&pool).await;

    let (status, body) = post_json(
        &fx.app,
        "/api/tickets",
        serde_json::json!({ "kind": "generic", "capturedName": "Move blueprints to C-J6MT" }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["kind"], "generic");
    assert_eq!(body["capturedName"], "Move blueprints to C-J6MT");
    assert_eq!(body["typeId"], Value::Null);
    assert_eq!(body["quantity"], Value::Null);
    assert_eq!(body["sourceBuildId"], Value::Null);
    assert_eq!(body["executionSnapshot"], Value::Null);
    assert_eq!(body["orderId"], Value::Null);
    assert_eq!(body["assigneeCharacterId"], Value::Null);
    assert_eq!(body["notes"], "");
    assert_eq!(body["status"], "todo");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_generic_ticket_rejects_a_blank_title(pool: PgPool) {
    let fx = fixture(&pool).await;

    let (status, body) = post_json(
        &fx.app,
        "/api/tickets",
        serde_json::json!({ "kind": "generic", "capturedName": "   " }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_ticket_title");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_generic_ticket_with_epic_and_assignee(pool: PgPool) {
    let fx = fixture(&pool).await;
    let epic_id = seed_epic(&pool, fx.workspace_id, fx.owner_id).await;
    let character_id = seed_character(&pool, fx.workspace_id, fx.owner_id, "Alt One").await;

    let (status, body) = post_json(
        &fx.app,
        "/api/tickets",
        serde_json::json!({
            "kind": "generic",
            "capturedName": "Move blueprints to C-J6MT",
            "notes": "Use covert route",
            "orderId": epic_id,
            "assigneeCharacterId": character_id,
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["orderId"], epic_id.to_string());
    assert_eq!(body["assigneeCharacterId"], character_id.to_string());
    assert_eq!(body["notes"], "Use covert route");
}

/// A ticket may only reference this workspace's own Epic, character, and
/// price source. The foreign keys are single-column, so without a check a
/// caller could attach another workspace's ids -- dangling cross-tenant
/// references, and an oracle for whether an id exists anywhere.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn tickets_cannot_reference_another_workspaces_epic_character_or_price_source(pool: PgPool) {
    let fx = fixture(&pool).await;
    let other_workspace_id = Uuid::new_v4();
    let other_owner_id = Uuid::new_v4();
    let later = chrono::Utc::now() + chrono::Duration::hours(1);
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Other',$2,$3,$3)",
    )
    .bind(other_workspace_id).bind(other_owner_id).bind(later).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Other',true,$3,$3)",
    )
    .bind(other_owner_id).bind(other_workspace_id).bind(later).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let other_epic = seed_epic(&pool, other_workspace_id, other_owner_id).await;
    let other_character =
        seed_character(&pool, other_workspace_id, other_owner_id, "Their Alt").await;
    let other_price_source = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO price_sources (id,workspace_id,display_name,description,source_kind,revision,created_at,updated_at) \
         VALUES ($1,$2,'Their prices','','manual',1,now(),now())",
    )
    .bind(other_price_source)
    .bind(other_workspace_id)
    .execute(&pool)
    .await
    .unwrap();

    for body in [
        serde_json::json!({ "kind": "generic", "capturedName": "Haul", "orderId": other_epic }),
        serde_json::json!({ "kind": "generic", "capturedName": "Haul", "assigneeCharacterId": other_character }),
        serde_json::json!({
            "kind": "acquisition", "capturedName": "Tritanium", "typeId": 34, "quantity": 10,
            "priceSourceId": other_price_source,
        }),
    ] {
        let (status, response) = post_json(&fx.app, "/api/tickets", body.clone()).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body} -> {response}");
        assert_eq!(response["error"]["code"], "ticket_reference_not_found");
    }
    let tickets: i64 = sqlx::query_scalar("SELECT count(*) FROM tickets")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tickets, 0);

    // Updating an own ticket to point at another workspace's ids fails too,
    // and leaves the ticket as it was.
    let (status, ticket) = post_json(
        &fx.app,
        "/api/tickets",
        serde_json::json!({ "kind": "generic", "capturedName": "Haul" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let ticket_id = ticket["id"].as_str().unwrap();
    for body in [
        serde_json::json!({ "orderId": other_epic }),
        serde_json::json!({ "assigneeCharacterId": other_character }),
    ] {
        let (status, response) =
            patch_json(&fx.app, &format!("/api/tickets/{ticket_id}"), body.clone()).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body} -> {response}");
    }
    let (order_id, assignee): (Option<Uuid>, Option<Uuid>) =
        sqlx::query_as("SELECT order_id, assignee_character_id FROM tickets WHERE id = $1")
            .bind(Uuid::parse_str(ticket_id).unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((order_id, assignee), (None, None));
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn create_acquisition_ticket_manually_and_record_it_through_the_existing_path(pool: PgPool) {
    let fx = fixture(&pool).await;

    // `capturedName` must be the item's own real SDE name, not a free-form
    // title -- explicit acquisition recording validates it against the
    // active SDE's `name_en` for `typeId` (the same invariant every
    // generated Acquisition ticket already relies on). The frontend
    // TicketEditor enforces this by deriving the title from the selected
    // item rather than offering a separate Title field for this kind.
    let (status, ticket) = post_json(
        &fx.app,
        "/api/tickets",
        serde_json::json!({
            "kind": "acquisition",
            "capturedName": "Tritanium",
            "typeId": 34,
            "quantity": 10_000_000,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(ticket["orderId"], Value::Null);
    assert_eq!(
        ticket["estimatedUnitCost"],
        Value::Null,
        "no price snapshot to freeze one from"
    );
    let ticket_id = ticket["id"].as_str().unwrap();

    // The existing explicit acquisition-recording path works unmodified --
    // no special manual-ticket recording endpoint.
    let (status, recorded) = post_json(
        &fx.app,
        &format!("/api/tickets/{ticket_id}/record-acquisition"),
        serde_json::json!({
            "idempotencyKey": Uuid::new_v4(),
            "quantity": 10_000_000,
            "unitCost": "5.00",
        }),
    )
    .await;
    if status != StatusCode::CREATED {
        eprintln!("record-acquisition failed: {status} {recorded}");
    }
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(recorded["summary"]["state"], "recorded");
    assert_eq!(recorded["summary"]["recordedQuantity"], 10_000_000);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn patch_moves_a_ticket_between_epics_and_clears_assignee_without_touching_status(
    pool: PgPool,
) {
    let fx = fixture(&pool).await;
    let epic_a = seed_epic(&pool, fx.workspace_id, fx.owner_id).await;
    let epic_b = seed_epic(&pool, fx.workspace_id, fx.owner_id).await;
    let character_id = seed_character(&pool, fx.workspace_id, fx.owner_id, "Alt One").await;

    let (_, ticket) = post_json(
        &fx.app,
        "/api/tickets",
        serde_json::json!({
            "kind": "generic",
            "capturedName": "Move blueprints",
            "orderId": epic_a,
            "assigneeCharacterId": character_id,
        }),
    )
    .await;
    let ticket_id = ticket["id"].as_str().unwrap();
    let original_status = ticket["status"].clone();

    let (status, moved) = patch_json(
        &fx.app,
        &format!("/api/tickets/{ticket_id}"),
        serde_json::json!({ "orderId": epic_b }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(moved["orderId"], epic_b.to_string());
    assert_eq!(
        moved["assigneeCharacterId"],
        character_id.to_string(),
        "assignee untouched by an Epic move"
    );
    assert_eq!(moved["status"], original_status);

    let (status, cleared) = patch_json(
        &fx.app,
        &format!("/api/tickets/{ticket_id}"),
        serde_json::json!({ "orderId": null, "assigneeCharacterId": null }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cleared["orderId"], Value::Null);
    assert_eq!(cleared["assigneeCharacterId"], Value::Null);
    assert_eq!(
        cleared["capturedName"], "Move blueprints",
        "title untouched by clearing relationships"
    );
    assert_eq!(cleared["status"], original_status);
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn patch_status_and_metadata_are_independent(pool: PgPool) {
    let fx = fixture(&pool).await;
    let (_, ticket) = post_json(
        &fx.app,
        "/api/tickets",
        serde_json::json!({ "kind": "generic", "capturedName": "Original title" }),
    )
    .await;
    let ticket_id = ticket["id"].as_str().unwrap();
    assert_eq!(ticket["status"], "todo");

    // A metadata-only PATCH never touches status.
    let (status, renamed) = patch_json(
        &fx.app,
        &format!("/api/tickets/{ticket_id}"),
        serde_json::json!({ "capturedName": "Renamed" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(renamed["capturedName"], "Renamed");
    assert_eq!(renamed["status"], "todo");

    // A status-only PATCH (the existing Board-drag contract) never touches
    // the title just set above.
    let (status, moved) = patch_json(
        &fx.app,
        &format!("/api/tickets/{ticket_id}"),
        serde_json::json!({ "status": "inProgress" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(moved["status"], "inProgress");
    assert_eq!(moved["capturedName"], "Renamed");
}
