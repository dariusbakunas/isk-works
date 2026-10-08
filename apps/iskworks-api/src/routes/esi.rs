use axum::extract::{Path, Query, State};
use axum::response::Redirect;
use axum::routing::{get, post};
use axum::{Json, Router};
use iskworks_core::{
    ConnectedCharacter, ConnectedCharacterId, EsiSyncKind, EsiSyncRun, EsiSyncRunId,
    InventoryError, WorkspaceId,
};
use serde::{Deserialize, Serialize};

use axum::http::{header, HeaderMap};
use iskworks_app::{AuthorizationStart, EsiApplicationError};

use crate::cookies::{
    clear_oauth_state_cookie_header, oauth_state_cookie_header, oauth_state_matches_cookie,
};
use crate::{workspace_context, ApiError, AppState};

/// Fetches a connection and confirms it belongs to `workspace_id` — every
/// route keyed by a bare `ConnectedCharacterId` must call this before acting
/// on it, so one workspace can't read or trigger actions on another
/// workspace's connection just by guessing/enumerating its UUID. 404s (not
/// 403) on a mismatch so existence isn't leaked across tenants.
pub(crate) async fn require_own_connection(
    state: &AppState,
    workspace_id: WorkspaceId,
    id: ConnectedCharacterId,
) -> Result<ConnectedCharacter, ApiError> {
    let connection = state.esi_repository()?.get_connection(id).await?;
    if connection.workspace_id != workspace_id {
        return Err(EsiApplicationError::Persistence(InventoryError::ItemNotFound).into());
    }
    Ok(connection)
}

/// The one anonymous ESI route: the EVE OAuth callback. It arrives with no
/// session cookie (login is what *creates* the session), so it must sit in
/// the public router, outside `require_session_middleware`. It is still
/// safe — the `state` parameter it consumes is a single-use, 10-minute,
/// server-issued token (see `AuthService`/`EsiApplicationService`).
pub(crate) fn public_router() -> Router<AppState> {
    Router::new().route("/api/eve/oauth/callback", get(complete_eve_oauth_callback))
}

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/eve/connections", get(list_eve_connections))
        .route(
            "/api/eve/connections/authorize",
            post(begin_eve_authorization),
        )
        .route(
            "/api/eve/connections/:connection_id",
            get(get_eve_connection),
        )
        .route(
            "/api/eve/connections/:connection_id/refresh",
            post(refresh_eve_connection),
        )
        .route(
            "/api/eve/connections/:connection_id/disconnect",
            post(disconnect_eve_connection),
        )
        .route(
            "/api/eve/connections/:connection_id/sync/assets",
            post(sync_eve_assets),
        )
        .route(
            "/api/eve/connections/:connection_id/sync/wallet-transactions",
            post(sync_eve_wallet),
        )
        .route(
            "/api/eve/connections/:connection_id/sync",
            post(sync_eve_all),
        )
        .route("/api/eve/sync-runs", get(list_eve_sync_runs))
        .route("/api/eve/sync-runs/:sync_run_id", get(get_eve_sync_run))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EveConnectionsResponse {
    configured: bool,
    fixture_mode: bool,
    connections: Vec<iskworks_core::ConnectedCharacter>,
}

async fn list_eve_connections(
    State(state): State<AppState>,
) -> Result<Json<EveConnectionsResponse>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(EveConnectionsResponse {
        configured: state.esi_service.is_some(),
        fixture_mode: state
            .esi_service
            .as_ref()
            .is_some_and(|service| service.fixture_mode()),
        connections: state
            .esi_repository()?
            .list_connections(workspace_id)
            .await?,
    }))
}

async fn begin_eve_authorization(
    State(state): State<AppState>,
) -> Result<(HeaderMap, Json<AuthorizationStart>), ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let service = state.esi_service()?;
    let started = service.begin_authorization(workspace_id, owner_id).await?;
    let mut headers = HeaderMap::new();
    if let Some(oauth_state) = &started.state {
        headers.insert(
            header::SET_COOKIE,
            oauth_state_cookie_header(oauth_state.expose(), cookie_secure(service.web_app_url())),
        );
    }
    Ok((headers, Json(started)))
}

/// Same rule as the session cookie (`AuthService::cookie_secure`): plain-HTTP
/// local dev can't set a `Secure` cookie and have the browser send it back.
fn cookie_secure(web_app_url: &str) -> bool {
    web_app_url.starts_with("https://")
}

#[derive(Debug, Deserialize)]
struct EveCallbackQuery {
    state: Option<String>,
    code: Option<String>,
    error: Option<String>,
}

/// Login and character-linking share one EVE app registration and this one
/// callback URL — EVE's developer portal only allows a single callback URL
/// per app. `state` (echoed back by EVE even when the user denies consent,
/// per the OAuth spec) is what tells the two flows apart: try login's
/// pending-authorization table first, then character-link's. Whichever
/// claims it decides where this redirects and, on success, what happens
/// next (session cookie vs. a completed `ConnectedCharacter`).
async fn complete_eve_oauth_callback(
    State(state): State<AppState>,
    request_headers: HeaderMap,
    Query(query): Query<EveCallbackQuery>,
) -> Result<(HeaderMap, Redirect), ApiError> {
    let state_value = query.state.clone().ok_or_else(|| {
        ApiError::Integration(EsiApplicationError::Configuration(
            "OAuth callback is missing state.".to_string(),
        ))
    })?;
    // Checked before `state` is consumed, so a forged hit can't burn the
    // real browser's in-flight flow. See `cookies::OAUTH_STATE_COOKIE_NAME`.
    if !oauth_state_matches_cookie(&request_headers, &state_value) {
        return Err(ApiError::Integration(EsiApplicationError::Persistence(
            InventoryError::Validation(
                "This sign-in wasn't started in this browser. Start it again from ISK Works."
                    .to_string(),
            ),
        )));
    }
    let (mut headers, redirect) = finish_eve_oauth_callback(&state, state_value, query).await?;
    let secure = state
        .auth_service
        .as_ref()
        .map(|auth_service| auth_service.cookie_secure())
        .or_else(|| {
            state
                .esi_service
                .as_ref()
                .map(|service| cookie_secure(service.web_app_url()))
        })
        .unwrap_or(false);
    headers.append(header::SET_COOKIE, clear_oauth_state_cookie_header(secure));
    Ok((headers, redirect))
}

async fn finish_eve_oauth_callback(
    state: &AppState,
    state_value: String,
    query: EveCallbackQuery,
) -> Result<(HeaderMap, Redirect), ApiError> {
    if let Ok(auth_service) = state.auth_service() {
        if let Some(pending) = auth_service.try_consume_login(&state_value).await? {
            let base = auth_service.web_app_url().trim_end_matches('/');
            if query.error.is_some() {
                return Ok((
                    axum::http::HeaderMap::new(),
                    Redirect::to(&format!("{base}/login?status=denied")),
                ));
            }
            let code = query.code.ok_or_else(|| {
                ApiError::Integration(EsiApplicationError::Configuration(
                    "OAuth callback is missing authorization code.".to_string(),
                ))
            })?;
            let token = match auth_service.finish_login(pending, &code).await {
                Ok(token) => token,
                // Invite-only mode: a genuinely new identity that arrived
                // without a usable invite. No session, no workspace — bounce
                // back to the sign-in screen with a curated status so the UI
                // can prompt for a code. Distinct code vs. invalid, but
                // never more detail than that (no expired/disabled/exhausted
                // split — see `error.rs`).
                Err(crate::AuthApplicationError::InviteRequired) => {
                    return Ok((
                        axum::http::HeaderMap::new(),
                        Redirect::to(&format!("{base}/login?status=invite_required")),
                    ));
                }
                Err(crate::AuthApplicationError::AccountDisabled) => {
                    return Ok((
                        axum::http::HeaderMap::new(),
                        Redirect::to(&format!("{base}/login?status=account_disabled")),
                    ));
                }
                Err(crate::AuthApplicationError::CharacterTransferred) => {
                    return Ok((
                        axum::http::HeaderMap::new(),
                        Redirect::to(&format!("{base}/login?status=character_transferred")),
                    ));
                }
                Err(crate::AuthApplicationError::InviteInvalid) => {
                    return Ok((
                        axum::http::HeaderMap::new(),
                        Redirect::to(&format!("{base}/login?status=invite_invalid")),
                    ));
                }
                Err(other) => return Err(other.into()),
            };
            let mut headers = axum::http::HeaderMap::new();
            headers.insert(
                axum::http::header::SET_COOKIE,
                crate::cookies::session_cookie_header(token.expose(), auth_service.cookie_secure()),
            );
            return Ok((headers, Redirect::to(&format!("{base}/"))));
        }
    }

    let service = state.esi_service()?;
    let base = service.web_app_url().trim_end_matches('/');
    let pending = service
        .try_consume_authorization(&state_value)
        .await?
        .ok_or_else(|| {
            ApiError::Integration(EsiApplicationError::Configuration(
                "OAuth callback state is invalid, expired, or already used.".to_string(),
            ))
        })?;
    if query.error.is_some() {
        return Ok((
            axum::http::HeaderMap::new(),
            Redirect::to(&format!("{base}/settings/eve/callback?status=denied")),
        ));
    }
    let code = query.code.ok_or_else(|| {
        ApiError::Integration(EsiApplicationError::Configuration(
            "OAuth callback is missing authorization code.".to_string(),
        ))
    })?;
    service.finish_authorization(pending, &code).await?;
    Ok((
        axum::http::HeaderMap::new(),
        Redirect::to(&format!("{base}/settings/eve/callback?status=connected")),
    ))
}

async fn get_eve_connection(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<iskworks_core::ConnectedCharacter>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        require_own_connection(&state, workspace_id, ConnectedCharacterId(id)).await?,
    ))
}

async fn refresh_eve_connection(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<iskworks_core::ConnectedCharacter>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    require_own_connection(&state, workspace_id, ConnectedCharacterId(id)).await?;
    let (connection, _) = state
        .esi_service()?
        .refresh(ConnectedCharacterId(id))
        .await?;
    Ok(Json(connection))
}

async fn disconnect_eve_connection(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<iskworks_core::ConnectedCharacter>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    require_own_connection(&state, workspace_id, ConnectedCharacterId(id)).await?;
    // With EVE SSO configured, also revoke the grant at EVE; otherwise
    // (fixture/local mode) there's no real token to revoke.
    let connection = match &state.esi_service {
        Some(service) => service.disconnect(ConnectedCharacterId(id)).await?,
        None => {
            state
                .esi_repository()?
                .disconnect(ConnectedCharacterId(id))
                .await?
        }
    };
    Ok(Json(connection))
}

async fn sync_eve_assets(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<Vec<iskworks_core::EsiSyncRun>>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    require_own_connection(&state, workspace_id, ConnectedCharacterId(id)).await?;
    Ok(Json(
        state
            .esi_service()?
            .sync(ConnectedCharacterId(id), EsiSyncKind::Assets)
            .await?,
    ))
}

async fn sync_eve_wallet(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<Vec<iskworks_core::EsiSyncRun>>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    require_own_connection(&state, workspace_id, ConnectedCharacterId(id)).await?;
    Ok(Json(
        state
            .esi_service()?
            .sync(ConnectedCharacterId(id), EsiSyncKind::WalletTransactions)
            .await?,
    ))
}

async fn sync_eve_all(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<Vec<iskworks_core::EsiSyncRun>>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    require_own_connection(&state, workspace_id, ConnectedCharacterId(id)).await?;
    Ok(Json(
        state
            .esi_service()?
            .sync(ConnectedCharacterId(id), EsiSyncKind::AllSupported)
            .await?,
    ))
}

async fn list_eve_sync_runs(
    State(state): State<AppState>,
) -> Result<Json<Vec<iskworks_core::EsiSyncRun>>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state.esi_repository()?.list_sync_runs(workspace_id).await?,
    ))
}

async fn get_eve_sync_run(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<iskworks_core::EsiSyncRun>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let sync_run: EsiSyncRun = state
        .esi_repository()?
        .get_sync_run(EsiSyncRunId(id))
        .await?;
    require_own_connection(&state, workspace_id, sync_run.connection_id).await?;
    Ok(Json(sync_run))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::{header, Request, StatusCode};
    use iskworks_esi::SecretCipher;
    use sqlx::PgPool;
    use tower::ServiceExt;

    /// `get_eve_connection`, `refresh_eve_connection`, `disconnect_eve_connection`,
    /// and `sync_eve_assets/wallet/all` all funnel
    /// through the exact same `require_own_connection` call — exercising it
    /// once through the router, via the simplest of those routes, covers the
    /// mechanism for all of them.
    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn a_connection_is_only_visible_to_its_own_workspace(pool: PgPool) {
        let auth_service = crate::AuthService::new_for_tests(
            std::sync::Arc::new(iskworks_storage::PgUserRepository::new(pool.clone())),
            std::sync::Arc::new(iskworks_storage::PgSessionRepository::new(pool.clone())),
            std::sync::Arc::new(iskworks_storage::PgInviteRepository::new(pool.clone())),
            std::sync::Arc::new(crate::auth::MultiIdentityFakeTransport),
        );
        let esi_repository =
            std::sync::Arc::new(iskworks_storage::PgEsiRepository::new(pool.clone()));
        let state = AppState::new(std::sync::Arc::new(
            iskworks_storage::PgWorkspaceRepository::new(pool),
        ))
        .with_auth(Some(auth_service))
        .with_esi(esi_repository.clone(), None);
        let app = crate::build_router(state);

        let cookie_a = login(&app, "1:CharacterA").await;
        let cookie_b = login(&app, "2:CharacterB").await;
        let workspace_a = fetch_json(&app, "/api/workspace", &cookie_a).await;

        let workspace_a_id: uuid::Uuid = workspace_a["workspace"]["id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        let owner_a_id: uuid::Uuid = workspace_a["owner"]["id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();

        let cipher = SecretCipher::for_tests();
        let envelope = cipher.encrypt("dummy-refresh-token").unwrap();
        let connection = esi_repository
            .mock_connect(
                iskworks_core::WorkspaceId(workspace_a_id),
                iskworks_core::OwnerId(owner_a_id),
                envelope,
                &[],
            )
            .await
            .unwrap();

        let wrong_tenant_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/eve/connections/{}", connection.id.0))
                    .header(header::COOKIE, cookie_b)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(wrong_tenant_response.status(), StatusCode::NOT_FOUND);

        let own_tenant_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/eve/connections/{}", connection.id.0))
                    .header(header::COOKIE, cookie_a)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(own_tenant_response.status(), StatusCode::OK);
    }

    /// The core thing the shared-callback rework is about: `state` values
    /// from the two flows are generated the same way (32 random bytes) and
    /// land on the exact same URL — this proves a real character-link state
    /// correctly falls through `AuthService::try_consume_login` (which must
    /// return `None`, not misidentify it) and completes as a character link,
    /// not a login.
    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn a_character_link_state_completes_correctly_through_the_shared_callback(pool: PgPool) {
        let auth_service = crate::AuthService::new_for_tests(
            std::sync::Arc::new(iskworks_storage::PgUserRepository::new(pool.clone())),
            std::sync::Arc::new(iskworks_storage::PgSessionRepository::new(pool.clone())),
            std::sync::Arc::new(iskworks_storage::PgInviteRepository::new(pool.clone())),
            std::sync::Arc::new(crate::auth::MultiIdentityFakeTransport),
        );
        let esi_repository =
            std::sync::Arc::new(iskworks_storage::PgEsiRepository::new(pool.clone()));
        let esi_service = iskworks_app::EsiApplicationService::new_for_tests(
            esi_repository.clone(),
            std::sync::Arc::new(iskworks_app::FakeLinkTransport),
        );
        let workspace_repository =
            std::sync::Arc::new(iskworks_storage::PgWorkspaceRepository::new(pool));
        let workspace_service = iskworks_core::WorkspaceService::new(workspace_repository.clone());
        let workspace_state = workspace_service
            .create_workspace(iskworks_core::CreateWorkspaceCommand {
                name: "Legacy Workspace".to_string(),
            })
            .await
            .unwrap();
        let workspace = workspace_state.workspace.unwrap();
        let owner = workspace_state.owner.unwrap();

        // Called directly on the service (not through the HTTP route, which
        // requires a session) — exactly what begin_eve_authorization does
        // internally, just without needing a logged-in caller for this test.
        let started = esi_service
            .clone()
            .begin_authorization(workspace.id, owner.id)
            .await
            .unwrap();
        let state_param = started
            .authorization_url
            .split('?')
            .nth(1)
            .unwrap()
            .split('&')
            .find_map(|pair| {
                let (key, value) = pair.split_once('=').unwrap();
                (key == "state").then(|| value.to_string())
            })
            .unwrap();

        let state = AppState::new(workspace_repository)
            .with_auth(Some(auth_service))
            .with_esi(esi_repository, Some(esi_service));
        let app = crate::build_router(state);
        let callback_uri = format!("/api/eve/oauth/callback?state={state_param}&code=fake-code");

        // A browser that didn't start this flow (no binding cookie) is
        // refused before `state` is consumed...
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(&callback_uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        // ...so the browser that did start it can still finish.
        let response = app
            .oneshot(
                Request::builder()
                    .uri(&callback_uri)
                    .header(
                        header::COOKIE,
                        format!("{}={state_param}", crate::cookies::OAUTH_STATE_COOKIE_NAME),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let location = response
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(location.ends_with("/settings/eve/callback?status=connected"));
        // A login cookie is never set for the character-link branch; only the
        // binding cookie is cleared.
        let set_cookies: Vec<_> = response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|value| value.to_str().unwrap().to_string())
            .collect();
        assert!(set_cookies
            .iter()
            .all(|value| !value.starts_with(crate::auth::SESSION_COOKIE_NAME)));
        assert!(set_cookies
            .iter()
            .any(|value| value
                .starts_with(&format!("{}=;", crate::cookies::OAUTH_STATE_COOKIE_NAME))));
    }

    /// Login CSRF: an attacker starts a login, approves it with their own
    /// character, and gets a victim's browser to open the callback URL. The
    /// victim's browser never received the attacker's binding cookie, so no
    /// session is issued.
    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn a_login_callback_from_another_browser_issues_no_session(pool: PgPool) {
        let auth_service = crate::AuthService::new_for_tests(
            std::sync::Arc::new(iskworks_storage::PgUserRepository::new(pool.clone())),
            std::sync::Arc::new(iskworks_storage::PgSessionRepository::new(pool.clone())),
            std::sync::Arc::new(iskworks_storage::PgInviteRepository::new(pool.clone())),
            std::sync::Arc::new(crate::auth::MultiIdentityFakeTransport),
        );
        let state = AppState::new(std::sync::Arc::new(
            iskworks_storage::PgWorkspaceRepository::new(pool),
        ))
        .with_auth(Some(auth_service));
        let app = crate::build_router(state);

        let (state_param, _attacker_binding) = begin_login(&app).await;
        let victim_binding = format!(
            "{}=some-other-flows-state",
            crate::cookies::OAUTH_STATE_COOKIE_NAME
        );
        for cookie in [None, Some(victim_binding)] {
            let mut request = Request::builder().uri(format!(
                "/api/eve/oauth/callback?state={state_param}&code=1:Attacker"
            ));
            if let Some(cookie) = cookie {
                request = request.header(header::COOKIE, cookie);
            }
            let response = app
                .clone()
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            assert!(response
                .headers()
                .get_all(header::SET_COOKIE)
                .iter()
                .all(|value| !value
                    .to_str()
                    .unwrap()
                    .starts_with(crate::auth::SESSION_COOKIE_NAME)));
        }
    }

    /// Starts a login and returns the `state` from the authorization URL plus
    /// the browser-binding cookie (`name=value`) the API set alongside it.
    async fn begin_login(app: &axum::Router) -> (String, String) {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/eve/login")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let binding_cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .expect("begin login sets the browser-binding cookie")
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        let begin = response_json(response).await;
        let authorization_url = begin["authorizationUrl"].as_str().unwrap();
        let state_param = authorization_url
            .split('?')
            .nth(1)
            .unwrap()
            .split('&')
            .find_map(|pair| {
                let (key, value) = pair.split_once('=').unwrap();
                (key == "state").then(|| value.to_string())
            })
            .unwrap();
        assert_eq!(
            binding_cookie,
            format!("{}={state_param}", crate::cookies::OAUTH_STATE_COOKIE_NAME)
        );
        (state_param, binding_cookie)
    }

    pub(crate) async fn login(app: &axum::Router, code: &str) -> String {
        let (state_param, binding_cookie) = begin_login(app).await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/eve/oauth/callback?state={state_param}&code={code}"
                    ))
                    .header(header::COOKIE, binding_cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|value| value.to_str().unwrap())
            .find(|value| value.starts_with(crate::auth::SESSION_COOKIE_NAME))
            .expect("login callback sets the session cookie")
            .split(';')
            .next()
            .unwrap()
            .to_string()
    }

    pub(crate) async fn fetch_json(
        app: &axum::Router,
        uri: &str,
        cookie: &str,
    ) -> serde_json::Value {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .header(header::COOKIE, cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        response_json(response).await
    }

    pub(crate) async fn response_json(response: axum::response::Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
}
