//! The LogRocket request tag: an `X-LogRocket-URL` header on a request is
//! lifted into the `tracing` span that wraps it, so the redacted
//! internal-error diagnostic carries the session-replay URL next to its
//! correlation id. See `apps/iskworks-api/src/observability.rs`.

use std::io::Write;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use iskworks_api::{build_router, AppState};
use iskworks_core::{AppError, NewWorkspace, WorkspaceId, WorkspaceRepository, WorkspaceState};
use serde_json::Value;
use tower::ServiceExt;
use tracing_subscriber::fmt::MakeWriter;

const SESSION_URL: &str =
    "https://app.logrocket.com/example-org/example-app/s/6-00000000-0000-0000-0000-000000000000/0";

/// A workspace repository whose reads always fail with a persistence error,
/// which the API redacts into an opaque 5xx plus a single server-side log
/// line (`error::log_internal_failure`).
#[derive(Clone)]
struct FailingWorkspaceRepository;

#[async_trait]
impl WorkspaceRepository for FailingWorkspaceRepository {
    async fn get_workspace_state(&self) -> Result<WorkspaceState, AppError> {
        Err(AppError::Persistence(
            "relation \"workspaces\" does not exist".to_string(),
        ))
    }

    async fn get_workspace_state_by_id(
        &self,
        _workspace_id: WorkspaceId,
    ) -> Result<WorkspaceState, AppError> {
        self.get_workspace_state().await
    }

    async fn create_workspace(
        &self,
        _new_workspace: NewWorkspace,
    ) -> Result<WorkspaceState, AppError> {
        Err(AppError::WorkspaceAlreadyConfigured)
    }
}

#[derive(Clone, Default)]
struct Buffer(Arc<Mutex<Vec<u8>>>);

impl Buffer {
    fn contents(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

impl Write for Buffer {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'writer> MakeWriter<'writer> for Buffer {
    type Writer = Buffer;
    fn make_writer(&'writer self) -> Self::Writer {
        self.clone()
    }
}

fn app() -> axum::Router {
    build_router(AppState::new(Arc::new(FailingWorkspaceRepository)))
}

async fn internal_error_request(logrocket_header: Option<&str>) -> (StatusCode, Value, String) {
    let buffer = Buffer::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(buffer.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let mut builder = Request::builder().uri("/api/workspace");
    if let Some(value) = logrocket_header {
        builder = builder.header("x-logrocket-url", value);
    }
    let response = app()
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap();

    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    (status, body, buffer.contents())
}

#[tokio::test]
async fn session_url_lands_in_the_internal_error_log_line() {
    let (status, body, logs) = internal_error_request(Some(SESSION_URL)).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    let correlation_id = body["error"]["correlationId"].as_str().unwrap();

    assert!(
        logs.contains("internal error redacted from API response"),
        "the redacted-error diagnostic was emitted: {logs}"
    );
    assert!(
        logs.contains(SESSION_URL),
        "the log carries the LogRocket session URL: {logs}"
    );
    assert!(
        logs.contains(correlation_id),
        "the same log output carries the correlation id returned to the client: {logs}"
    );
}

#[tokio::test]
async fn a_spoofed_non_logrocket_header_is_dropped() {
    let forged = "https://evil.example.com/x correlation_id=deadbeef";
    let (status, _body, logs) = internal_error_request(Some(forged)).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        !logs.contains("evil.example.com"),
        "a header that is not a LogRocket URL never reaches the logs: {logs}"
    );
}

#[tokio::test]
async fn requests_without_the_header_are_unaffected() {
    let (status, body, _logs) = internal_error_request(None).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(body["error"]["correlationId"].is_string());
}
