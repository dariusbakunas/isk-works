use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use iskworks_api::{build_router, AppState};
use iskworks_core::{AppError, NewWorkspace, WorkspaceRepository, WorkspaceState};
use serde_json::{json, Value};
use tower::ServiceExt;

#[derive(Clone, Default)]
struct MemoryWorkspaceRepository {
    state: Arc<Mutex<Option<WorkspaceState>>>,
    fail_reads: bool,
}

#[async_trait]
impl WorkspaceRepository for MemoryWorkspaceRepository {
    async fn get_workspace_state(&self) -> Result<WorkspaceState, AppError> {
        if self.fail_reads {
            return Err(AppError::Persistence("database unavailable".to_string()));
        }
        Ok(self
            .state
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(WorkspaceState::unconfigured))
    }

    async fn get_workspace_state_by_id(
        &self,
        _workspace_id: iskworks_core::WorkspaceId,
    ) -> Result<WorkspaceState, AppError> {
        self.get_workspace_state().await
    }

    async fn create_workspace(
        &self,
        new_workspace: NewWorkspace,
    ) -> Result<WorkspaceState, AppError> {
        let mut state = self.state.lock().unwrap();
        if state.is_some() {
            return Err(AppError::WorkspaceAlreadyConfigured);
        }
        let configured = WorkspaceState::configured(new_workspace.workspace, new_workspace.owner);
        *state = Some(configured.clone());
        Ok(configured)
    }
}

#[tokio::test]
async fn health_response_is_explicit() {
    let app = test_app(MemoryWorkspaceRepository::default());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body, json!({ "status": "healthy", "version": "dev" }));
}

#[tokio::test]
async fn esi_status_reports_no_downtime_without_a_configured_esi_host() {
    let app = test_app(MemoryWorkspaceRepository::default());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/esi/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(
        body,
        json!({ "downtime": false, "retryAfterSeconds": null })
    );
}

#[tokio::test]
async fn unconfigured_workspace_response_is_structured() {
    let app = test_app(MemoryWorkspaceRepository::default());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/workspace")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["configured"], false);
    assert_eq!(body["workspace"], Value::Null);
    assert_eq!(body["owner"], Value::Null);
}

#[tokio::test]
async fn valid_workspace_creation_and_retrieval() {
    let app = test_app(MemoryWorkspaceRepository::default());
    let response = app
        .clone()
        .oneshot(json_request(
            "/api/workspace",
            json!({ "name": " Personal Industry " }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let created = response_json(response).await;
    assert_eq!(created["configured"], true);
    assert_eq!(created["workspace"]["name"], "Personal Industry");
    assert_eq!(created["owner"]["kind"], "manual");
    assert_eq!(created["owner"]["hidden"], true);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/workspace")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let reloaded = response_json(response).await;
    assert_eq!(reloaded["workspace"]["name"], "Personal Industry");
}

#[tokio::test]
async fn invalid_workspace_creation_returns_field_errors() {
    let app = test_app(MemoryWorkspaceRepository::default());
    let response = app
        .oneshot(json_request("/api/workspace", json!({ "name": "   " })))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(response).await;
    assert_eq!(body["error"]["code"], "validation_failed");
    assert_eq!(
        body["error"]["fields"]["name"],
        "Workspace name is required."
    );
}

#[tokio::test]
async fn duplicate_setup_returns_conflict() {
    let app = test_app(MemoryWorkspaceRepository::default());
    app.clone()
        .oneshot(json_request("/api/workspace", json!({ "name": "First" })))
        .await
        .unwrap();

    let response = app
        .oneshot(json_request("/api/workspace", json!({ "name": "Second" })))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = response_json(response).await;
    assert_eq!(body["error"]["code"], "workspace_already_configured");
}

#[tokio::test]
async fn persistence_failure_maps_to_service_unavailable() {
    let app = test_app(MemoryWorkspaceRepository {
        fail_reads: true,
        ..MemoryWorkspaceRepository::default()
    });
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/workspace")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = response_json(response).await;
    assert_eq!(body["error"]["code"], "persistence_unavailable");
}

fn test_app(repository: MemoryWorkspaceRepository) -> axum::Router {
    build_router(AppState::new(Arc::new(repository)))
}

fn json_request(uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// JSON endpoints take a modest body; only the market-export upload routes
/// allow the large multipart batches they're built for. A single global
/// 26 MB limit let any JSON endpoint be fed 26 MB to parse item by item.
#[tokio::test]
async fn oversized_json_bodies_are_rejected_but_market_uploads_are_not() {
    let app = build_router(AppState::new(
        Arc::new(MemoryWorkspaceRepository::default()),
    ));
    let oversized = format!(r#"{{"name":"{}"}}"#, "x".repeat(5 * 1024 * 1024));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/workspace")
                .header("content-type", "application/json")
                .body(Body::from(oversized.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

    let boundary = "iskworks-test-boundary";
    let multipart = format!(
        "--{boundary}\r\ncontent-disposition: form-data; name=\"files\"; filename=\"orders.csv\"\r\ncontent-type: text/csv\r\n\r\n{oversized}\r\n--{boundary}--\r\n"
    );
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/industry/market-imports/preview")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(multipart))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}
