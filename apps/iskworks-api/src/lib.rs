use std::env;
use std::net::SocketAddr;

use axum::extract::{DefaultBodyLimit, Request, State};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use iskworks_core::{AuthenticatedUser, IndustryError, WorkspaceId};

/// Request body limit for everything except the market-export uploads.
const DEFAULT_BODY_LIMIT_BYTES: usize = 4 * 1024 * 1024;
use serde::Serialize;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

mod auth;
mod cookies;
mod csv_cell;
mod error;
mod export;
pub mod metrics_exporter;
mod observability;
mod origin_check;
mod readiness;
mod routes;
mod state;
#[cfg(test)]
mod test_support;

// The application-service layer -- `PublicMarketService`,
// `EsiApplicationService`, `CharacterSyncService`, `CharacterRosterService`,
// `BuildPreviewCoordinator` and their ports/DTOs -- lives in `iskworks-app`
// and is imported directly from `iskworks_app::` by the API modules that
// need it. This crate re-exports only its own surface: `ApiError` &
// friends, `AppState`, `build_router`, `AuthService`, and the route DTOs.

pub use auth::{AuthApplicationError, AuthService};
pub use error::ApiError;

pub use routes::sde::BlueprintSearchQuery;

pub use state::AppState;

tokio::task_local! {
    /// The session resolved from the request's cookie, for the duration of
    /// handling this one request — set by `resolve_session_middleware`, read
    /// by `workspace_context()`. A task-local rather than an extractor
    /// threaded through every handler's signature: every workspace-scoped
    /// route already funnels through `workspace_context()` as its one choke
    /// point, so this keeps the auth changeover to that function's body
    /// instead of touching ~100 call sites across every route file.
    static CURRENT_USER: Option<AuthenticatedUser>;
}

async fn resolve_session_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let authenticated_user = match &state.auth_service {
        Some(auth_service) => match cookies::extract_session_token(request.headers()) {
            Some(raw_token) => auth_service
                .resolve_session(&iskworks_core::SessionToken::from_raw(raw_token))
                .await
                .ok()
                .flatten(),
            None => None,
        },
        None => None,
    };
    CURRENT_USER
        .scope(authenticated_user, next.run(request))
        .await
}

/// Gate on the authenticated product router. When EVE SSO is configured
/// (`auth_service.is_some()` — always true in a production deploy, see
/// `ensure_auth_available` / `ISKWORKS_AUTH_REQUIRED`), a request with no
/// valid session (`CURRENT_USER` unset by `resolve_session_middleware`) is
/// rejected here, before any handler runs — including the SDE / reference /
/// preview routes that never call `workspace_context()` themselves.
///
/// When auth is *not* configured (local dev / the integration-test harness,
/// which builds `AppState` with no `AuthService`), this is a pass-through:
/// `workspace_context()` then serves the single legacy workspace.
/// Production can never reach that mode — `main.rs` refuses to start
/// with `ISKWORKS_AUTH_REQUIRED=true` and no `AuthService`.
async fn require_session_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if state.auth_service.is_some() {
        let authenticated = CURRENT_USER.try_with(Clone::clone).unwrap_or(None);
        if authenticated.is_none() {
            return ApiError::Unauthenticated.into_response();
        }
    }
    next.run(request).await
}

/// Gate on `/api/admin/*`. Stricter than `require_session_middleware`: the
/// caller must be a signed-in user whose EVE character id is listed in
/// `ISKWORKS_ADMIN_CHARACTER_IDS`. With auth off there is never a current
/// user, so admin routes fail closed (403) rather than open.
async fn require_admin_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let user = CURRENT_USER.try_with(Clone::clone).unwrap_or(None);
    match user {
        None => ApiError::Forbidden.into_response(),
        Some(user) if !state.admin_config.is_admin(&user) => ApiError::Forbidden.into_response(),
        Some(_) => next.run(request).await,
    }
}

fn admin_router(state: AppState) -> Router<AppState> {
    // `route_layer`, not `layer`: a plain layer would also wrap this
    // router's fallback, and merging it would make every unknown URL 403.
    routes::admin::router().route_layer(middleware::from_fn_with_state(
        state,
        require_admin_middleware,
    ))
}

pub fn build_router(state: AppState) -> Router {
    #[cfg(all(feature = "dev-auth", debug_assertions))]
    let dev_auth_router = routes::dev_auth::router();
    #[cfg(not(all(feature = "dev-auth", debug_assertions)))]
    let dev_auth_router = Router::new();

    // A specific origin, not `CorsLayer::permissive()` — the login cookie
    // makes this a credentialed API, and browsers reject
    // `Access-Control-Allow-Origin: *` alongside `allow_credentials(true)`
    // regardless. Methods/headers are listed explicitly for the same
    // reason: tower-http rejects combining `Any` there with credentials too.
    let cors_origin = axum::http::HeaderValue::from_str(&state.web_app_origin)
        .unwrap_or_else(|_| axum::http::HeaderValue::from_static("http://127.0.0.1:5173"));
    let cors = CorsLayer::new()
        .allow_origin(cors_origin)
        .allow_credentials(true)
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PUT,
            axum::http::Method::PATCH,
            axum::http::Method::DELETE,
        ])
        .allow_headers([axum::http::header::CONTENT_TYPE]);

    // The complete anonymous allowlist: liveness, readiness, login initiation, the
    // session probe, logout, and the EVE OAuth callback (no session exists
    // yet when it lands). `dev_auth` (feature+debug+env gated, absent from
    // the release image) issues a session, so it belongs here too.
    // Everything else is a product/reference endpoint and requires a valid
    // session in auth-required mode.
    let public_router = Router::new()
        .route("/api/health", get(health))
        .route("/api/ready", get(readiness::ready))
        .merge(routes::auth::router())
        .merge(routes::esi::public_router())
        .merge(dev_auth_router);

    let authenticated_router = Router::new()
        .route("/api/esi/status", get(esi_status))
        .merge(routes::assets::router())
        .merge(routes::workspace::router())
        .merge(routes::sde::router())
        .merge(routes::builds::router())
        .merge(routes::calendar::router())
        .merge(routes::orders::router())
        .merge(routes::order_acquisition_runs::router())
        .merge(routes::market::router())
        .merge(routes::facilities::router())
        .merge(routes::finance::router())
        .merge(routes::finance_analytics::router())
        .merge(routes::inventory::router())
        .merge(routes::opportunities::router())
        .merge(routes::esi::router())
        .merge(routes::characters::router())
        .merge(routes::planetary::router())
        .merge(admin_router(state.clone()))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_session_middleware,
        ));

    Router::new()
        .merge(public_router)
        .merge(authenticated_router)
        // Outermost, so `CURRENT_USER` is populated before
        // `require_session_middleware` (on the authenticated sub-router)
        // reads it, and before any handler's `workspace_context()`.
        .layer(middleware::from_fn_with_state(
            state.clone(),
            resolve_session_middleware,
        ))
        // Outside session resolution, so a forged write is refused before
        // its cookie is even looked up; inside CORS, so preflights are still
        // answered there and a refusal still carries the CORS headers.
        .layer(middleware::from_fn_with_state(
            state.clone(),
            origin_check::reject_cross_origin_writes,
        ))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        // Outside the trace span so the LogRocket session URL (when the
        // web client sent one) is a parent field on every log line the
        // request produces, including the redacted-error diagnostic.
        .layer(middleware::from_fn(observability::tag_logrocket_session))
        // Every JSON endpoint gets a modest limit; the market-export upload
        // routes raise their own (`routes::market::imports`).
        .layer(DefaultBodyLimit::max(DEFAULT_BODY_LIMIT_BYTES))
        .with_state(state)
}

/// Whether a production deploy has declared authentication mandatory via
/// `ISKWORKS_AUTH_REQUIRED`. Accepts `true` / `1` / `yes` (case- and
/// whitespace-insensitive); anything else — including unset — is `false`
/// (the local-dev / integration-test default). Deliberately explicit:
/// production must not silently drop into the legacy unauthenticated mode
/// just because `EVE_SSO_CLIENT_ID` was omitted or blank.
#[must_use]
pub fn auth_required_from_env() -> bool {
    parse_bool_flag(env::var("ISKWORKS_AUTH_REQUIRED").ok().as_deref())
}

/// Whether invite-only new-user provisioning is enforced
/// (`ISKWORKS_INVITE_REQUIRED`). Same accepted spellings as
/// `ISKWORKS_AUTH_REQUIRED`; unset/anything-else is `false` (open
/// registration — the local-dev / integration-test default). Never inferred
/// from whether `invite_codes` rows exist. Production alpha sets this to
/// `true`. Read by `AuthService::from_env`.
#[must_use]
pub(crate) fn invite_required_from_env() -> bool {
    parse_bool_flag(env::var("ISKWORKS_INVITE_REQUIRED").ok().as_deref())
}

/// Operator-configured app admins (`ISKWORKS_ADMIN_CHARACTER_IDS`, a
/// comma-separated list of EVE character ids). Unset/blank means no admins;
/// a malformed value is an error so startup aborts instead of silently
/// running with the wrong admin set.
pub fn admin_config_from_env() -> Result<iskworks_core::AdminConfig, iskworks_core::AdminConfigError>
{
    iskworks_core::AdminConfig::parse(
        env::var("ISKWORKS_ADMIN_CHARACTER_IDS")
            .unwrap_or_default()
            .as_str(),
    )
}

fn parse_bool_flag(raw: Option<&str>) -> bool {
    matches!(
        raw.map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("true" | "1" | "yes")
    )
}

/// Fail-closed startup contract: in auth-required mode there must be a
/// usable `AuthService`. `main.rs` calls this right after
/// `AuthService::from_env`, so a missing/blank `EVE_SSO_CLIENT_ID` (which
/// makes `from_env` return `Ok(None)`) aborts startup instead of booting
/// with `auth_service = None` and serving the singleton workspace to
/// anyone. (`from_env` already returns `Err` — and `main`'s `?` already
/// aborts — when the client id is set but `TOKEN_ENCRYPTION_KEY` is missing
/// or malformed.)
pub fn ensure_auth_available(
    auth_required: bool,
    auth_service_configured: bool,
) -> Result<(), ConfigError> {
    if auth_required && !auth_service_configured {
        return Err(ConfigError::AuthRequiredButUnconfigured);
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ApiConfig {
    pub database_url: String,
    pub listen_addr: SocketAddr,
    /// Where the Prometheus `/metrics` listener binds
    /// (`ISKWORKS_METRICS_ADDR`). `None` keeps metrics off entirely. Never
    /// the public listener: Traefik only routes `listen_addr`.
    pub metrics_addr: Option<SocketAddr>,
}

impl ApiConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let database_url = env::var("DATABASE_URL").map_err(|_| ConfigError::MissingDatabaseUrl)?;
        let listen_addr = env::var("ISKWORKS_API_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:8080".to_string())
            .parse()
            .map_err(|_| ConfigError::InvalidListenAddr)?;

        let metrics_addr = metrics_exporter::metrics_addr(env::var("ISKWORKS_METRICS_ADDR").ok())?;

        Ok(Self {
            database_url,
            listen_addr,
            metrics_addr,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("DATABASE_URL is required")]
    MissingDatabaseUrl,
    #[error("ISKWORKS_API_ADDR must be a valid socket address")]
    InvalidListenAddr,
    #[error("ISKWORKS_METRICS_ADDR must be a valid socket address")]
    InvalidMetricsAddr,
    #[error(
        "ISKWORKS_AUTH_REQUIRED is set but EVE SSO authentication is not configured. \
         Set EVE_SSO_CLIENT_ID and TOKEN_ENCRYPTION_KEY (and EVE_SSO_REDIRECT_URI / \
         WEB_APP_URL for a non-default deployment), or unset ISKWORKS_AUTH_REQUIRED \
         for local development."
    )]
    AuthRequiredButUnconfigured,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HealthResponse {
    pub status: &'static str,
    pub version: String,
    /// Non-production deployment marker (`ISKWORKS_ENVIRONMENT_LABEL`, e.g.
    /// "STAGE"). The web app renders it as a loud banner so a stage instance
    /// can't be mistaken for prod. Omitted entirely when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment_label: Option<String>,
}

/// Whether ESI is paused for Tranquility's daily downtime, so the web app
/// can say why EVE data isn't refreshing. Costs ESI nothing outside the
/// downtime window and at most one shared `/status/` check per 30 s inside it.
async fn esi_status(State(state): State<AppState>) -> Json<iskworks_esi::EsiAvailability> {
    Json(match state.esi_status() {
        Some(transport) => transport.availability().await,
        None => iskworks_esi::EsiAvailability::default(),
    })
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy",
        version: app_version(),
        environment_label: environment_label_from(std::env::var("ISKWORKS_ENVIRONMENT_LABEL").ok()),
    })
}

/// Normalizes the raw `ISKWORKS_ENVIRONMENT_LABEL` value: blank or
/// whitespace-only means "no label" (production), so a compose file can pass
/// `${ISKWORKS_ENVIRONMENT_LABEL:-}` through unconditionally.
fn environment_label_from(raw: Option<String>) -> Option<String> {
    raw.map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// The running build's version, set at container build time via the
/// `APP_VERSION` build arg (e.g. "v1.2.3 (a1b2c3d)"). Falls back to "dev"
/// for local `cargo run` and CI builds that don't set it.
pub(crate) fn app_version() -> String {
    std::env::var("APP_VERSION").unwrap_or_else(|_| "dev".to_string())
}

/// The single choke point every workspace-scoped route calls to learn "whose
/// data is this." Two modes: when EVE SSO login isn't configured, legacy
/// single-workspace mode (the one singleton workspace, no session needed) —
/// this is what lets a purely local, no-SSO deployment and the integration
/// tests run without sessions. When SSO *is* configured, requires a valid session
/// (set by `resolve_session_middleware`) and resolves to that session's own
/// workspace, never "whichever workspace happens to be oldest."
async fn workspace_context(
    state: &AppState,
) -> Result<(WorkspaceId, iskworks_core::OwnerId), ApiError> {
    if state.auth_service.is_some() {
        let authenticated = CURRENT_USER
            .try_with(Clone::clone)
            .unwrap_or(None)
            .ok_or(ApiError::Unauthenticated)?;
        let workspace = state
            .workspace_service()
            .get_workspace_state_by_id(authenticated.workspace_id)
            .await?;
        return match (workspace.workspace, workspace.owner) {
            (Some(workspace), Some(owner)) => Ok((workspace.id, owner.id)),
            _ => Err(ApiError::Unauthenticated),
        };
    }

    let workspace = state.workspace_service().get_workspace_state().await?;
    match (workspace.workspace, workspace.owner) {
        (Some(workspace), Some(owner)) => Ok((workspace.id, owner.id)),
        _ => Err(ApiError::Industry(IndustryError::Validation(
            "Workspace setup must be completed first.".to_string(),
        ))),
    }
}

#[cfg(test)]
mod environment_label_tests {
    use super::environment_label_from;

    #[test]
    fn unset_or_blank_label_means_production() {
        assert_eq!(environment_label_from(None), None);
        assert_eq!(environment_label_from(Some(String::new())), None);
        assert_eq!(environment_label_from(Some("   ".to_string())), None);
    }

    #[test]
    fn label_is_trimmed() {
        assert_eq!(
            environment_label_from(Some("  STAGE \n".to_string())),
            Some("STAGE".to_string())
        );
    }
}

#[cfg(test)]
mod cors_tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{header, Request};

    use std::sync::Arc;
    use tower::ServiceExt;

    use crate::test_support::EmptyWorkspaceRepository;

    #[tokio::test]
    async fn only_the_configured_origin_is_allowed_and_credentials_are_permitted() {
        let state = AppState::new(Arc::new(EmptyWorkspaceRepository))
            .with_web_app_origin("https://iskworks.example".to_string());
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .header(header::ORIGIN, "https://iskworks.example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .unwrap(),
            "https://iskworks.example"
        );
        assert_eq!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
                .unwrap(),
            "true"
        );
    }

    #[tokio::test]
    async fn a_different_requesting_origin_never_gets_itself_reflected_back() {
        // tower-http's `allow_origin(HeaderValue)` always advertises the one
        // *configured* origin, never the caller's own `Origin` header — this
        // is what makes it secure: a browser on evil.example sees an
        // Access-Control-Allow-Origin that doesn't match its own origin
        // (iskworks.example, not evil.example) and refuses the response,
        // same as `CorsLayer::permissive()`'s `*` being gone entirely.
        let state = AppState::new(Arc::new(EmptyWorkspaceRepository))
            .with_web_app_origin("https://iskworks.example".to_string());
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .header(header::ORIGIN, "https://evil.example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .unwrap(),
            "https://iskworks.example"
        );
    }
}

#[cfg(test)]
mod auth_mode_tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};

    use std::sync::Arc;
    use tower::ServiceExt;

    use crate::test_support::EmptyWorkspaceRepository;

    #[test]
    fn parse_bool_flag_only_accepts_explicit_affirmatives() {
        for yes in ["true", "TRUE", " true ", "1", "yes", "YES"] {
            assert!(parse_bool_flag(Some(yes)), "{yes:?} should enable");
        }
        for no in [
            None,
            Some(""),
            Some("  "),
            Some("false"),
            Some("0"),
            Some("no"),
            Some("on"),
        ] {
            assert!(!parse_bool_flag(no), "{no:?} should not enable");
        }
    }

    #[test]
    fn ensure_auth_available_fails_closed_only_when_required_and_unconfigured() {
        assert!(matches!(
            ensure_auth_available(true, false),
            Err(ConfigError::AuthRequiredButUnconfigured)
        ));
        assert!(ensure_auth_available(true, true).is_ok());
        assert!(ensure_auth_available(false, false).is_ok());
        assert!(ensure_auth_available(false, true).is_ok());
    }

    /// Auth-disabled mode (no `AuthService` — the integration-test /
    /// local-dev shape) must keep the reference surface reachable without a
    /// session.
    #[tokio::test]
    async fn reference_routes_stay_open_when_auth_is_not_configured() {
        let app = build_router(AppState::new(Arc::new(EmptyWorkspaceRepository)));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/blueprints/search?q=rifter")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn health_stays_public_when_auth_is_not_configured() {
        let app = build_router(AppState::new(Arc::new(EmptyWorkspaceRepository)));

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
    }

    /// The release image builds with default features (no `dev-auth`), so
    /// this compiles into the default `cargo test` pass and proves the
    /// dev-login route is not in the binary. It is `#[cfg]`-excluded from
    /// the separate `--features dev-auth` CI pass, where the opposite
    /// (`routes::dev_auth::tests::route_only_exists_when_the_env_var_is_set`)
    /// runs instead.
    #[cfg(not(feature = "dev-auth"))]
    #[tokio::test]
    async fn dev_login_route_is_absent_without_the_dev_auth_feature() {
        let app = build_router(AppState::new(Arc::new(EmptyWorkspaceRepository)));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/dev/login")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
