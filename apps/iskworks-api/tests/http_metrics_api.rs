//! HTTP request metrics: every request is labelled with the Axum route
//! template it matched, never its raw path, so IDs can't reach Prometheus.

use async_trait::async_trait;
use axum::body::Body;
use axum::http::Request;
use iskworks_api::{build_router, AppState};
use iskworks_core::{AppError, NewWorkspace, WorkspaceId, WorkspaceRepository, WorkspaceState};
use metrics_util::debugging::{DebugValue, DebuggingRecorder};
use tower::ServiceExt;

#[derive(Clone)]
struct EmptyWorkspaceRepository;

#[async_trait]
impl WorkspaceRepository for EmptyWorkspaceRepository {
    async fn get_workspace_state(&self) -> Result<WorkspaceState, AppError> {
        Ok(WorkspaceState::unconfigured())
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

/// `(route, method, status_class, count)` for each request counter, and the
/// `(route, method, observations)` of each duration histogram, sorted.
fn recorded(recorder: &DebuggingRecorder) -> (Vec<Vec<String>>, Vec<Vec<String>>) {
    let mut requests = Vec::new();
    let mut durations = Vec::new();
    for (key, _, _, value) in recorder.snapshotter().snapshot().into_vec() {
        let labels: Vec<String> = key
            .key()
            .labels()
            .map(|label| format!("{}={}", label.key(), label.value()))
            .collect();
        match (key.key().name(), value) {
            ("iskworks_http_requests_total", DebugValue::Counter(count)) => {
                let mut row = labels;
                row.push(count.to_string());
                requests.push(row);
            }
            ("iskworks_http_request_duration_seconds", DebugValue::Histogram(values)) => {
                let mut row = labels;
                row.push(values.len().to_string());
                durations.push(row);
            }
            _ => {}
        }
    }
    requests.sort();
    durations.sort();
    (requests, durations)
}

#[tokio::test(flavor = "current_thread")]
async fn requests_are_labelled_by_route_template_not_raw_path() {
    let recorder = DebuggingRecorder::new();
    let _guard = metrics::set_default_local_recorder(&recorder);
    let app = build_router(AppState::new(std::sync::Arc::new(EmptyWorkspaceRepository)));

    for uri in [
        "/api/health",
        "/api/builds/0b9c8f7e-1111-2222-3333-444455556666",
        "/api/builds/0b9c8f7e-7777-8888-9999-000011112222",
        "/api/no-such-endpoint/12345",
    ] {
        app.clone()
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
    }

    let (requests, durations) = recorded(&recorder);
    let routes: Vec<&str> = requests.iter().map(|row| row[0].as_str()).collect();
    assert_eq!(
        routes,
        vec![
            "route=/api/builds/:build_id",
            "route=/api/health",
            "route=unmatched"
        ]
    );
    let health = requests
        .iter()
        .find(|row| row[0] == "route=/api/health")
        .unwrap();
    assert_eq!(health[1..], ["method=GET", "status_class=2xx", "1"]);
    let builds = requests
        .iter()
        .find(|row| row[0] == "route=/api/builds/:build_id")
        .unwrap();
    assert_eq!(
        builds.last().unwrap(),
        "2",
        "both build IDs share one series"
    );
    let unmatched = requests
        .iter()
        .find(|row| row[0] == "route=unmatched")
        .unwrap();
    assert_eq!(unmatched[2], "status_class=4xx");
    assert_eq!(
        durations,
        vec![
            vec!["route=/api/builds/:build_id", "method=GET", "2"],
            vec!["route=/api/health", "method=GET", "1"],
            vec!["route=unmatched", "method=GET", "1"],
        ]
    );
}
