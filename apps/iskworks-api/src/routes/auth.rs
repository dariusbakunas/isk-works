use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use iskworks_core::SessionToken;
use serde::{Deserialize, Serialize};

use crate::cookies::{
    clear_session_cookie_header, extract_session_token, oauth_state_cookie_header,
};
use crate::{ApiError, AppState};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/eve/login", post(begin_login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/session", get(session))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BeginLoginRequest {
    /// Raw invite code from the sign-in form. Absent/blank for returning
    /// users and for every login when invite mode is off. Never logged,
    /// never echoed, never placed in the OAuth `state` — the server
    /// normalizes and hashes it, stores only the resolved invite id on the
    /// pending-auth row, and drops the raw value.
    invite_code: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BeginLoginResponse {
    authorization_url: String,
}

async fn begin_login(
    State(state): State<AppState>,
    body: Option<Json<BeginLoginRequest>>,
) -> Result<(HeaderMap, Json<BeginLoginResponse>), ApiError> {
    let request = body.map(|Json(request)| request).unwrap_or_default();
    let auth_service = state.auth_service()?;
    let started = auth_service
        .begin_login(request.invite_code.as_deref())
        .await?;
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::SET_COOKIE,
        oauth_state_cookie_header(started.state.expose(), auth_service.cookie_secure()),
    );
    Ok((
        headers,
        Json(BeginLoginResponse {
            authorization_url: started.authorization_url,
        }),
    ))
}

async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<(HeaderMap, axum::http::StatusCode), ApiError> {
    let auth_service = state.auth_service()?;
    if let Some(raw_token) = extract_session_token(&headers) {
        auth_service
            .logout(&session_token_from_raw(&raw_token))
            .await?;
    }
    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        axum::http::header::SET_COOKIE,
        clear_session_cookie_header(auth_service.cookie_secure()),
    );
    Ok((response_headers, axum::http::StatusCode::NO_CONTENT))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionResponse {
    authenticated: bool,
    character_name: Option<String>,
    workspace_id: Option<String>,
    /// Whether the signed-in character may open the Admin section. Drives
    /// the nav link only; `/api/admin/*` enforces on its own.
    is_admin: bool,
    /// Whether the sign-in screen should offer the invite affordance. Safe
    /// public config — a single boolean, no counts, ids, or secrets. Only
    /// meaningful for an unauthenticated caller; harmless otherwise.
    invite_required: bool,
}

async fn session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SessionResponse>, ApiError> {
    let auth_service = state.auth_service()?;
    let invite_required = auth_service.invite_required();
    let Some(raw_token) = extract_session_token(&headers) else {
        return Ok(Json(SessionResponse {
            authenticated: false,
            character_name: None,
            workspace_id: None,
            is_admin: false,
            invite_required,
        }));
    };
    let resolved = auth_service
        .resolve_session(&session_token_from_raw(&raw_token))
        .await?;
    Ok(Json(match resolved {
        Some(user) => SessionResponse {
            authenticated: true,
            is_admin: state.admin_config.is_admin(&user),
            character_name: Some(user.eve_character_name),
            workspace_id: Some(user.workspace_id.0.to_string()),
            invite_required,
        },
        None => SessionResponse {
            authenticated: false,
            character_name: None,
            workspace_id: None,
            is_admin: false,
            invite_required,
        },
    }))
}

/// The cookie only ever carries the raw, unhashed token value — this just
/// wraps it back into the newtype `SessionService` expects, it does not
/// re-derive anything.
fn session_token_from_raw(raw: &str) -> SessionToken {
    SessionToken::from_raw(raw.to_string())
}
