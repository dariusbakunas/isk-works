//! App-wide admin endpoints (`/api/admin/*`), gated by
//! `require_admin_middleware`. Invites first; further admin screens (users,
//! ...) mount here.

use axum::extract::{Path, State};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use iskworks_core::{
    generate_invite_code, hash_invite_code_from_raw, validate_new_invite, AppError, FieldErrors,
    InviteId, InviteSummary, NewInvite,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::{ApiError, AppState, AuthApplicationError, CURRENT_USER};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/users", get(list_users))
        .route(
            "/api/admin/orphaned-workspaces",
            get(count_orphaned_workspaces),
        )
        .route(
            "/api/admin/orphaned-workspaces/erase",
            post(erase_orphaned_workspaces),
        )
        .route("/api/admin/users/:user_id", delete(delete_user))
        .route("/api/admin/users/:user_id/disable", post(disable_user))
        .route("/api/admin/users/:user_id/enable", post(enable_user))
        .route("/api/admin/invites", get(list_invites).post(create_invite))
        .route(
            "/api/admin/invites/:invite_id/disable",
            post(disable_invite),
        )
        .route(
            "/api/admin/invites/:invite_id/code",
            get(reveal_invite_code),
        )
        .route("/api/admin/invites/:invite_id", delete(delete_invite))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminUserDto {
    id: String,
    character_id: i64,
    character_name: String,
    created_at: DateTime<Utc>,
    last_login_at: DateTime<Utc>,
    character_count: i64,
    characters_needing_attention: i64,
    active_sessions: i64,
    disabled_at: Option<DateTime<Utc>>,
    /// Listed in `ISKWORKS_ADMIN_CHARACTER_IDS`; cannot be disabled or deleted here.
    is_admin: bool,
    /// The caller's own account; cannot be disabled or deleted.
    is_current_user: bool,
}

async fn list_users(State(state): State<AppState>) -> Result<Json<Vec<AdminUserDto>>, ApiError> {
    let current = CURRENT_USER.try_with(Clone::clone).unwrap_or(None);
    let users = state
        .admin_users_repository()?
        .list_users()
        .await
        .map_err(AuthApplicationError::Persistence)?;
    Ok(Json(
        users
            .into_iter()
            .map(|user| AdminUserDto {
                id: user.user_id.0.to_string(),
                character_id: user.eve_character_id,
                character_name: user.eve_character_name,
                created_at: user.created_at,
                last_login_at: user.last_login_at,
                character_count: user.character_count,
                characters_needing_attention: user.characters_needing_attention,
                active_sessions: user.active_session_count,
                disabled_at: user.disabled_at,
                is_admin: state.admin_config.is_admin_character(user.eve_character_id),
                is_current_user: current
                    .as_ref()
                    .is_some_and(|current| current.user_id == user.user_id),
            })
            .collect(),
    ))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InviteDto {
    id: String,
    status: &'static str,
    created_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    disabled_at: Option<DateTime<Utc>>,
    max_uses: i32,
    use_count: i32,
    note: Option<String>,
    revealable: bool,
}

impl InviteDto {
    fn from_summary(summary: InviteSummary, now: DateTime<Utc>) -> Self {
        let status = summary.status(now).as_str();
        Self {
            id: summary.id.0.to_string(),
            status,
            created_at: summary.created_at,
            expires_at: summary.expires_at,
            disabled_at: summary.disabled_at,
            max_uses: summary.max_uses,
            use_count: summary.use_count,
            note: summary.note,
            revealable: summary.revealable,
        }
    }
}

async fn list_invites(State(state): State<AppState>) -> Result<Json<Vec<InviteDto>>, ApiError> {
    let invites = state
        .invite_admin_repository()?
        .list_invites()
        .await
        .map_err(AuthApplicationError::Persistence)?;
    let now = Utc::now();
    Ok(Json(
        invites
            .into_iter()
            .map(|invite| InviteDto::from_summary(invite, now))
            .collect(),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateInviteRequest {
    max_uses: Option<i32>,
    expires_at: Option<DateTime<Utc>>,
    note: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreatedInviteResponse {
    /// The plaintext code. Returned exactly once — only its hash is stored.
    code: String,
    invite: InviteDto,
}

fn validation(field: &str, message: &str) -> ApiError {
    ApiError::Application(AppError::Validation(FieldErrors {
        fields: BTreeMap::from([(field.to_string(), message.to_string())]),
    }))
}

async fn create_invite(
    State(state): State<AppState>,
    Json(request): Json<CreateInviteRequest>,
) -> Result<Json<CreatedInviteResponse>, ApiError> {
    let max_uses = request.max_uses.unwrap_or(1);
    let now = Utc::now();
    let note = validate_new_invite(max_uses, request.expires_at, request.note, now)
        .map_err(|error| validation(error.field, error.message))?;

    let repository = state.invite_admin_repository()?;
    let code = generate_invite_code();
    // Encrypted at rest so an admin can reveal it again; if no cipher is
    // configured the invite is hash-only.
    let encryption_failed = || {
        ApiError::Auth(AuthApplicationError::Configuration(
            "Could not encrypt the invite code.".to_string(),
        ))
    };
    let code_ciphertext = match &state.invite_cipher {
        Some(cipher) => {
            let envelope = cipher.encrypt(&code).map_err(|_| encryption_failed())?;
            Some(serde_json::to_string(&envelope).map_err(|_| encryption_failed())?)
        }
        None => None,
    };
    let revealable = code_ciphertext.is_some();
    let id = repository
        .create_invite(NewInvite {
            code_hash: hash_invite_code_from_raw(&code),
            max_uses,
            expires_at: request.expires_at,
            note: note.clone(),
            code_ciphertext,
        })
        .await
        .map_err(AuthApplicationError::Persistence)?;

    Ok(Json(CreatedInviteResponse {
        code,
        invite: InviteDto::from_summary(
            InviteSummary {
                id,
                created_at: now,
                expires_at: request.expires_at,
                disabled_at: None,
                max_uses,
                use_count: 0,
                note,
                revealable,
            },
            now,
        ),
    }))
}

#[derive(Debug, Serialize)]
struct RevealedCode {
    code: String,
}

/// Re-reveal a stored invite code. Admin-only (the router gate), `no-store`,
/// and 404 for invites that have no stored copy (CLI-minted or created
/// before reveal existed).
async fn reveal_invite_code(
    State(state): State<AppState>,
    Path(invite_id): Path<uuid::Uuid>,
) -> Result<
    (
        [(axum::http::HeaderName, &'static str); 1],
        Json<RevealedCode>,
    ),
    ApiError,
> {
    let cipher = state.invite_cipher.as_ref().ok_or(ApiError::NotFound)?;
    let stored = state
        .invite_admin_repository()?
        .invite_code_ciphertext(InviteId(invite_id))
        .await
        .map_err(AuthApplicationError::Persistence)?
        .ok_or(ApiError::NotFound)?;
    let envelope: iskworks_esi::EncryptedSecret =
        serde_json::from_str(&stored).map_err(|_| ApiError::NotFound)?;
    let code = cipher.decrypt(&envelope).map_err(|_| ApiError::NotFound)?;
    Ok((
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(RevealedCode { code }),
    ))
}

#[derive(Debug, Serialize)]
struct CountResponse {
    count: i64,
}

/// Workspaces left behind by users deleted before full erase existed.
async fn count_orphaned_workspaces(
    State(state): State<AppState>,
) -> Result<Json<CountResponse>, ApiError> {
    let count = state
        .admin_users_repository()?
        .count_orphaned_workspaces()
        .await
        .map_err(AuthApplicationError::Persistence)?;
    Ok(Json(CountResponse { count }))
}

#[derive(Debug, Deserialize)]
struct EraseOrphansRequest {
    /// Must be the word `erase` (case-insensitive) — mirrors the UI's
    /// type-to-confirm.
    confirmation: String,
}

#[derive(Debug, Serialize)]
struct ErasedResponse {
    erased: i64,
}

async fn erase_orphaned_workspaces(
    State(state): State<AppState>,
    Json(request): Json<EraseOrphansRequest>,
) -> Result<Json<ErasedResponse>, ApiError> {
    if !request.confirmation.trim().eq_ignore_ascii_case("erase") {
        return Err(validation("confirmation", "Type ERASE to confirm."));
    }
    let erased = state
        .admin_users_repository()?
        .erase_orphaned_workspaces()
        .await
        .map_err(AuthApplicationError::Persistence)?;
    Ok(Json(ErasedResponse { erased }))
}

/// Look up the target of a user action and refuse the two cases that would
/// lock the operator out: acting on yourself, or on any configured admin.
async fn guarded_user(
    state: &AppState,
    user_id: uuid::Uuid,
) -> Result<iskworks_core::AdminUserSummary, ApiError> {
    let id = iskworks_core::UserId(user_id);
    let user = state
        .admin_users_repository()?
        .find_user(id)
        .await
        .map_err(AuthApplicationError::Persistence)?
        .ok_or(ApiError::NotFound)?;
    let current = CURRENT_USER.try_with(Clone::clone).unwrap_or(None);
    if current.is_some_and(|current| current.user_id == id) {
        return Err(ApiError::Conflict(
            "You cannot disable or delete your own account.",
        ));
    }
    if state.admin_config.is_admin_character(user.eve_character_id) {
        return Err(ApiError::Conflict(
            "Admin accounts cannot be disabled or deleted here. Remove the character from ISKWORKS_ADMIN_CHARACTER_IDS first.",
        ));
    }
    Ok(user)
}

async fn set_disabled(
    state: AppState,
    user_id: uuid::Uuid,
    disabled: bool,
) -> Result<axum::http::StatusCode, ApiError> {
    guarded_user(&state, user_id).await?;
    let found = state
        .admin_users_repository()?
        .set_user_disabled(iskworks_core::UserId(user_id), disabled)
        .await
        .map_err(AuthApplicationError::Persistence)?;
    if found {
        Ok(axum::http::StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

async fn disable_user(
    State(state): State<AppState>,
    Path(user_id): Path<uuid::Uuid>,
) -> Result<axum::http::StatusCode, ApiError> {
    set_disabled(state, user_id, true).await
}

async fn enable_user(
    State(state): State<AppState>,
    Path(user_id): Path<uuid::Uuid>,
) -> Result<axum::http::StatusCode, ApiError> {
    set_disabled(state, user_id, false).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteUserRequest {
    /// Must match the user's character name (case-insensitive, trimmed) — the
    /// server-side half of the UI's type-to-confirm.
    confirm_character_name: String,
}

async fn delete_user(
    State(state): State<AppState>,
    Path(user_id): Path<uuid::Uuid>,
    Json(request): Json<DeleteUserRequest>,
) -> Result<axum::http::StatusCode, ApiError> {
    let user = guarded_user(&state, user_id).await?;
    if !request
        .confirm_character_name
        .trim()
        .eq_ignore_ascii_case(user.eve_character_name.trim())
    {
        return Err(validation(
            "confirmCharacterName",
            "Type the character name exactly to confirm.",
        ));
    }
    let found = state
        .admin_users_repository()?
        .delete_user(iskworks_core::UserId(user_id))
        .await
        .map_err(AuthApplicationError::Persistence)?;
    if found {
        Ok(axum::http::StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

async fn delete_invite(
    State(state): State<AppState>,
    Path(invite_id): Path<uuid::Uuid>,
) -> Result<axum::http::StatusCode, ApiError> {
    let found = state
        .invite_admin_repository()?
        .delete_invite(InviteId(invite_id))
        .await
        .map_err(AuthApplicationError::Persistence)?;
    if found {
        Ok(axum::http::StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

async fn disable_invite(
    State(state): State<AppState>,
    Path(invite_id): Path<uuid::Uuid>,
) -> Result<axum::http::StatusCode, ApiError> {
    let found = state
        .invite_admin_repository()?
        .disable_invite(InviteId(invite_id))
        .await
        .map_err(AuthApplicationError::Persistence)?;
    if found {
        Ok(axum::http::StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

#[cfg(test)]
mod tests;
