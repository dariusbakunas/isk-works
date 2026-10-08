//! Local/agent-verification-only login bypass. Never reachable in a
//! production build — three layers of safety:
//! this whole module only compiles behind `feature = "dev-auth"` (absent
//! from `default`, never passed by the release Dockerfile) *and*
//! `debug_assertions` (off in `--release` even if the feature were mistakenly
//! enabled there), and even then the route below is only mounted into the
//! router if `ISKWORKS_DEV_AUTH=1` is set at process start.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;

use crate::cookies::session_cookie_header;
use crate::{ApiError, AppState};

pub(crate) fn router() -> Router<AppState> {
    if std::env::var("ISKWORKS_DEV_AUTH").as_deref() != Ok("1") {
        return Router::new();
    }
    tracing::warn!(
        "DEV AUTH BACKDOOR ENABLED (ISKWORKS_DEV_AUTH=1) — do not run this build against real user data"
    );
    Router::new().route("/api/auth/dev/login", post(dev_login))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DevLoginRequest {
    character_id: i64,
    character_name: String,
}

async fn dev_login(
    State(state): State<AppState>,
    Json(request): Json<DevLoginRequest>,
) -> Result<(HeaderMap, StatusCode), ApiError> {
    let auth_service = state.auth_service()?;
    let token = auth_service
        .dev_login(request.character_id, request.character_name)
        .await?;

    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::SET_COOKIE,
        session_cookie_header(token.expose(), auth_service.cookie_secure()),
    );
    Ok((headers, StatusCode::NO_CONTENT))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::{header, Request};
    use sqlx::PgPool;
    use tower::ServiceExt;

    /// One test, sequential — mutating `ISKWORKS_DEV_AUTH` (a process-global)
    /// across separate parallel tests would race; this covers both the
    /// disabled and enabled states in a single thread instead.
    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn route_only_exists_when_the_env_var_is_set(pool: PgPool) {
        std::env::remove_var("ISKWORKS_DEV_AUTH");
        let disabled_app = crate::build_router(test_state(pool.clone()));
        let response = disabled_app
            .oneshot(dev_login_request(1, "Character A"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        std::env::set_var("ISKWORKS_DEV_AUTH", "1");
        let enabled_app = crate::build_router(test_state(pool));
        let response = enabled_app
            .oneshot(dev_login_request(1, "Character A"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(response.headers().get(header::SET_COOKIE).is_some());

        std::env::remove_var("ISKWORKS_DEV_AUTH");
    }

    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn two_dev_logins_get_two_distinct_workspaces(pool: PgPool) {
        std::env::set_var("ISKWORKS_DEV_AUTH", "1");
        let app = crate::build_router(test_state(pool));

        let cookie_a = extract_cookie(
            app.clone()
                .oneshot(dev_login_request(1, "Character A"))
                .await
                .unwrap(),
        );
        let cookie_b = extract_cookie(
            app.clone()
                .oneshot(dev_login_request(2, "Character B"))
                .await
                .unwrap(),
        );

        let workspace_a = response_json(
            app.clone()
                .oneshot(
                    Request::builder()
                        .uri("/api/workspace")
                        .header(header::COOKIE, cookie_a)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap(),
        )
        .await;
        let workspace_b = response_json(
            app.oneshot(
                Request::builder()
                    .uri("/api/workspace")
                    .header(header::COOKIE, cookie_b)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
        )
        .await;

        assert_eq!(workspace_a["workspace"]["name"], "Character A");
        assert_eq!(workspace_b["workspace"]["name"], "Character B");
        assert_ne!(
            workspace_a["workspace"]["id"],
            workspace_b["workspace"]["id"]
        );

        std::env::remove_var("ISKWORKS_DEV_AUTH");
    }

    fn test_state(pool: PgPool) -> AppState {
        let auth_service = crate::AuthService::new_for_dev_auth_tests(
            std::sync::Arc::new(iskworks_storage::PgUserRepository::new(pool.clone())),
            std::sync::Arc::new(iskworks_storage::PgSessionRepository::new(pool.clone())),
            std::sync::Arc::new(iskworks_storage::PgInviteRepository::new(pool.clone())),
        );
        AppState::new(std::sync::Arc::new(
            iskworks_storage::PgWorkspaceRepository::new(pool),
        ))
        .with_auth(Some(auth_service))
    }

    fn dev_login_request(character_id: i64, character_name: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/api/auth/dev/login")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "characterId": character_id,
                    "characterName": character_name,
                })
                .to_string(),
            ))
            .unwrap()
    }

    fn extract_cookie(response: axum::response::Response) -> String {
        response
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string()
    }

    async fn response_json(response: axum::response::Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
}
