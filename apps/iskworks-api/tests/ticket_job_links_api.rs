//! Linking and unlinking ESI industry jobs on a ticket by hand
//! (`GET /api/tickets/:id/job-choices`, `POST /api/tickets/:id/jobs`,
//! `DELETE /api/tickets/:id/jobs/:job_id`). The matching rules themselves
//! are unit-tested in `iskworks_core::order::ticket_job_match`; this proves
//! the routes wire ESI observations, the check and storage together.

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use iskworks_api::{build_router, AppState};
use iskworks_core::WorkspaceRepository;
use iskworks_storage::{PgEsiRepository, PgOrderRepository, PgWorkspaceRepository};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const PLATE: i64 = 11_545;
const CARBIDE: i64 = 16_673;

struct Fixture {
    app: axum::Router,
    workspace_id: Uuid,
    owner_id: Uuid,
}

async fn fixture(pool: &PgPool) -> Fixture {
    let workspace_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();
    let now = chrono::Utc::now();
    // One transaction: workspace and owner reference each other.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id,display_name,owner_id,created_at,updated_at) VALUES ($1,'Job Links',$2,$3,$3)",
    )
    .bind(workspace_id)
    .bind(owner_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO owners (id,workspace_id,owner_kind,display_name,hidden,created_at,updated_at) VALUES ($1,$2,'manual','Job Links',true,$3,$3)",
    )
    .bind(owner_id)
    .bind(workspace_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let workspace = PgWorkspaceRepository::new(pool.clone());
    let orders = Arc::new(PgOrderRepository::new(pool.clone()));
    let app = build_router(
        AppState::new(Arc::new(workspace) as Arc<dyn WorkspaceRepository>)
            .with_order_repository(orders.clone())
            .with_ticket_job_repository(orders)
            .with_esi(Arc::new(PgEsiRepository::new(pool.clone())), None),
    );
    Fixture {
        app,
        workspace_id,
        owner_id,
    }
}

/// A connected character whose last `industry_jobs` sync reported `jobs`
/// (raw ESI rows).
async fn seed_character_with_jobs(pool: &PgPool, fx: &Fixture, jobs: Value) -> Uuid {
    let connection_id = Uuid::new_v4();
    let now = chrono::Utc::now();
    sqlx::query(
        r#"INSERT INTO eve_connections (id,workspace_id,owner_id,eve_character_id,character_name,
             status,granted_scopes,connected_at,updated_at)
           VALUES ($1,$2,$3,$4,'Builder','connected','{}',$5,$5)"#,
    )
    .bind(connection_id)
    .bind(fx.workspace_id)
    .bind(fx.owner_id)
    .bind((Uuid::new_v4().as_u128() % 1_000_000_000) as i64 + 1)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO character_source_sync_state (connection_id, source_kind, refresh_state, summary, observed_at) \
         VALUES ($1, 'industry_jobs', 'current', $2, $3)",
    )
    .bind(connection_id)
    .bind(json!({ "jobs": jobs }))
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    connection_id
}

fn esi_job(job_id: i64, product: i64, start: &str) -> Value {
    json!({
        "job_id": job_id,
        "activity_id": 1,
        "blueprint_type_id": 1,
        "product_type_id": product,
        "facility_id": 60_000_001,
        "runs": 2,
        "cost": 1500.5,
        "status": "active",
        "start_date": start,
        "end_date": "2099-01-01T00:00:00Z",
    })
}

/// A Manufacturing ticket for `PLATE`, assigned to `assignee`.
async fn seed_ticket(pool: &PgPool, fx: &Fixture, assignee: Option<Uuid>) -> String {
    let (status, ticket) = send(
        &fx.app,
        "POST",
        "/api/tickets",
        Some(json!({ "kind": "generic", "capturedName": "Plates" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{ticket}");
    let id = ticket["id"].as_str().unwrap().to_string();
    sqlx::query(
        "UPDATE tickets SET kind = 'manufacturing', type_id = $2, quantity = 2, assignee_character_id = $3 WHERE id = $1",
    )
    .bind(Uuid::parse_str(&id).unwrap())
    .bind(PLATE)
    .bind(assignee)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn send(
    app: &axum::Router,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(path);
    if body.is_some() {
        request = request.header("content-type", "application/json");
    }
    let response = app
        .clone()
        .oneshot(
            request
                .body(body.map_or_else(Body::empty, |body| Body::from(body.to_string())))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn linked_job_ids(app: &axum::Router, ticket_id: &str) -> Vec<i64> {
    let (status, progress) = send(app, "GET", "/api/tickets/job-progress", None).await;
    assert_eq!(status, StatusCode::OK, "{progress}");
    progress
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["ticketId"] == ticket_id)
        .flat_map(|entry| entry["jobs"].as_array().unwrap().clone())
        .map(|job| job["jobId"].as_i64().unwrap())
        .collect()
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn link_move_and_unlink_jobs_by_hand(pool: PgPool) {
    let fx = fixture(&pool).await;
    // Job 1 started long ago (auto-matching would never take it); job 2 is
    // for another product.
    let builder = seed_character_with_jobs(
        &pool,
        &fx,
        json!([
            esi_job(1, PLATE, "2020-01-01T00:00:00Z"),
            esi_job(2, CARBIDE, "2020-01-01T00:00:00Z"),
        ]),
    )
    .await;
    let first = seed_ticket(&pool, &fx, Some(builder)).await;
    let second = seed_ticket(&pool, &fx, Some(builder)).await;

    // Choices: only the assignee's job for this product.
    let (status, choices) = send(
        &fx.app,
        "GET",
        &format!("/api/tickets/{first}/job-choices"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{choices}");
    let choices = choices.as_array().unwrap();
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0]["job"]["jobId"], 1);
    assert_eq!(choices[0]["linkedTicketId"], Value::Null);

    // A job for another product is refused.
    let (status, response) = send(
        &fx.app,
        "POST",
        &format!("/api/tickets/{first}/jobs"),
        Some(json!({ "jobId": 2 })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
    assert_eq!(response["error"]["code"], "job_does_not_match_ticket");
    // A job ESI doesn't report is not found.
    let (status, response) = send(
        &fx.app,
        "POST",
        &format!("/api/tickets/{first}/jobs"),
        Some(json!({ "jobId": 99 })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{response}");
    assert_eq!(response["error"]["code"], "ticket_job_not_found");

    let (status, response) = send(
        &fx.app,
        "POST",
        &format!("/api/tickets/{first}/jobs"),
        Some(json!({ "jobId": 1 })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{response}");
    assert_eq!(linked_job_ids(&fx.app, &first).await, vec![1]);

    // The other ticket sees the job, on the first ticket; linking moves it.
    let (_, choices) = send(
        &fx.app,
        "GET",
        &format!("/api/tickets/{second}/job-choices"),
        None,
    )
    .await;
    assert_eq!(choices[0]["linkedTicketId"], first.as_str());
    assert!(choices[0]["linkedTicketDisplayId"]
        .as_str()
        .unwrap()
        .starts_with("ISK-"));
    let (status, _) = send(
        &fx.app,
        "POST",
        &format!("/api/tickets/{second}/jobs"),
        Some(json!({ "jobId": 1 })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(linked_job_ids(&fx.app, &first).await.is_empty());
    assert_eq!(linked_job_ids(&fx.app, &second).await, vec![1]);

    // Unlink; the ticket doesn't get it back, and unlinking again is 404.
    let (status, _) = send(
        &fx.app,
        "DELETE",
        &format!("/api/tickets/{second}/jobs/1"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(linked_job_ids(&fx.app, &second).await.is_empty());
    let (status, response) = send(
        &fx.app,
        "DELETE",
        &format!("/api/tickets/{second}/jobs/1"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{response}");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn an_unassigned_ticket_offers_no_jobs(pool: PgPool) {
    let fx = fixture(&pool).await;
    seed_character_with_jobs(
        &pool,
        &fx,
        json!([esi_job(1, PLATE, "2020-01-01T00:00:00Z")]),
    )
    .await;
    let ticket = seed_ticket(&pool, &fx, None).await;

    let (status, response) = send(
        &fx.app,
        "GET",
        &format!("/api/tickets/{ticket}/job-choices"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
    assert_eq!(response["error"]["code"], "ticket_job_needs_assignee");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn an_unlinked_auto_match_stays_unlinked(pool: PgPool) {
    let fx = fixture(&pool).await;
    let builder = seed_character_with_jobs(&pool, &fx, json!([])).await;
    let ticket = seed_ticket(&pool, &fx, Some(builder)).await;
    sqlx::query("UPDATE tickets SET status = 'in_progress' WHERE id = $1")
        .bind(Uuid::parse_str(&ticket).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    // A job started after the ticket: automatic matching links it.
    let started = (chrono::Utc::now() + chrono::Duration::minutes(1)).to_rfc3339();
    sqlx::query("UPDATE character_source_sync_state SET summary = $2 WHERE connection_id = $1")
        .bind(builder)
        .bind(json!({ "jobs": [esi_job(7, PLATE, &started)] }))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(linked_job_ids(&fx.app, &ticket).await, vec![7]);

    let (status, _) = send(
        &fx.app,
        "DELETE",
        &format!("/api/tickets/{ticket}/jobs/7"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    // Reading progress syncs again; the job doesn't come back.
    assert!(linked_job_ids(&fx.app, &ticket).await.is_empty());
}
