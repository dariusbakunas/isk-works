//! `GET /api/ready` — a readiness probe that, unlike `/api/health`
//! (process liveness only), also round-trips to Postgres. Meant for a
//! Kubernetes `readinessProbe`: a DB outage takes the pod out of the
//! Service without the liveness probe restarting it.

use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use crate::state::AppState;

/// How long the dependency check may take before the pod is reported
/// unready — kept under a typical probe `timeoutSeconds` so the probe sees
/// a clean 503 rather than its own timeout.
const READINESS_TIMEOUT: Duration = Duration::from_secs(2);

/// A dependency the API cannot serve traffic without.
#[async_trait::async_trait]
pub trait ReadinessCheck: Send + Sync {
    async fn check(&self) -> Result<(), String>;
}

#[async_trait::async_trait]
impl ReadinessCheck for sqlx::PgPool {
    async fn check(&self) -> Result<(), String> {
        sqlx::query("SELECT 1")
            .execute(self)
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

/// Status only — no version, unlike `/api/health`, so this extra anonymous
/// route adds no build-disclosure surface.
#[derive(Debug, Serialize)]
pub struct ReadinessResponse {
    pub status: &'static str,
}

pub(crate) async fn ready(State(state): State<AppState>) -> Response {
    let outcome = match state.readiness_check() {
        // No dependency wired (in-memory test harness): nothing to wait on.
        None => Ok(()),
        Some(check) => tokio::time::timeout(READINESS_TIMEOUT, check.check())
            .await
            .unwrap_or_else(|_| Err("readiness check timed out".to_string())),
    };

    match outcome {
        Ok(()) => Json(ReadinessResponse { status: "ready" }).into_response(),
        Err(error) => {
            // Anonymous endpoint: log the cause, never return it.
            tracing::warn!(%error, "readiness check failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ReadinessResponse {
                    status: "unavailable",
                }),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_router;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;

    use std::sync::Arc;
    use tower::ServiceExt;

    use crate::test_support::EmptyWorkspaceRepository;

    struct FakeCheck(Result<(), String>);

    #[async_trait::async_trait]
    impl ReadinessCheck for FakeCheck {
        async fn check(&self) -> Result<(), String> {
            self.0.clone()
        }
    }

    struct HangingCheck;

    #[async_trait::async_trait]
    impl ReadinessCheck for HangingCheck {
        async fn check(&self) -> Result<(), String> {
            std::future::pending().await
        }
    }

    async fn get_ready(state: AppState) -> (StatusCode, serde_json::Value) {
        let response = build_router(state)
            .oneshot(
                Request::builder()
                    .uri("/api/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    fn state() -> AppState {
        AppState::new(Arc::new(EmptyWorkspaceRepository))
    }

    #[tokio::test]
    async fn ready_when_the_database_check_passes() {
        let (status, body) =
            get_ready(state().with_readiness_check(Arc::new(FakeCheck(Ok(()))))).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ready");
    }

    #[tokio::test]
    async fn unavailable_without_leaking_the_cause_when_the_database_check_fails() {
        let (status, body) = get_ready(state().with_readiness_check(Arc::new(FakeCheck(Err(
            "connection refused to db.internal:5432".to_string(),
        )))))
        .await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["status"], "unavailable");
        assert!(!body.to_string().contains("db.internal"));
    }

    #[tokio::test(start_paused = true)]
    async fn unavailable_when_the_database_check_hangs() {
        let (status, body) = get_ready(state().with_readiness_check(Arc::new(HangingCheck))).await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["status"], "unavailable");
    }

    #[tokio::test]
    async fn ready_when_no_dependency_is_wired() {
        let (status, _) = get_ready(state()).await;

        assert_eq!(status, StatusCode::OK);
    }
}
