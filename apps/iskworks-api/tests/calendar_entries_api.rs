use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use iskworks_api::{build_router, AppState};
use iskworks_core::calendar_entry::FakeCalendarEntries;
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

mod support;
use support::workspace::configured_workspace;

fn app() -> axum::Router {
    build_router(
        AppState::new(Arc::new(configured_workspace("Calendar Test")))
            .with_calendar_entry_repository(Arc::new(FakeCalendarEntries::default())),
    )
}

async fn send(app: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn with_json(method: &str, uri: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn entry(title: &str) -> Value {
    json!({
        "occursAt": "2026-11-20T23:00:00Z",
        "timeZone": "America/New_York",
        "title": title,
        "description": "Bring ammo"
    })
}

#[tokio::test]
async fn custom_entries_are_created_updated_and_deleted() {
    let app = app();

    let (status, created) = send(
        &app,
        with_json("POST", "/api/calendar/entries", &entry(" Fleet op ")),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["kind"], "custom");
    assert_eq!(created["title"], "Fleet op");
    assert_eq!(created["occursAt"], "2026-11-20T23:00:00Z");
    assert_eq!(created["timeZone"], "America/New_York");
    assert_eq!(created["description"], "Bring ammo");
    assert_eq!(created["connectionId"], Value::Null);
    let id = created["entryId"].as_str().unwrap().to_string();
    assert_eq!(created["id"], format!("custom:{id}"));

    let (status, updated) = send(
        &app,
        with_json(
            "PUT",
            &format!("/api/calendar/entries/{id}"),
            &json!({ "occursAt": "2026-11-21T08:00:00Z", "timeZone": "UTC", "title": "Moved" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["title"], "Moved");
    assert_eq!(updated["timeZone"], "UTC");
    assert_eq!(updated["description"], "");

    let delete = Request::delete(format!("/api/calendar/entries/{id}"))
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&app, delete).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let again = Request::delete(format!("/api/calendar/entries/{id}"))
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&app, again).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn invalid_entries_and_unknown_ids_are_refused() {
    let app = app();

    let (status, body) = send(
        &app,
        with_json("POST", "/api/calendar/entries", &entry("   ")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    let mut bad_zone = entry("Op");
    bad_zone["timeZone"] = json!("Not a zone");
    let (status, _) = send(&app, with_json("POST", "/api/calendar/entries", &bad_zone)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let mut foreign = entry("Op");
    foreign["connectionId"] = json!(Uuid::new_v4());
    let (status, _) = send(&app, with_json("POST", "/api/calendar/entries", &foreign)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = send(
        &app,
        with_json(
            "PUT",
            &format!("/api/calendar/entries/{}", Uuid::new_v4()),
            &entry("Op"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
