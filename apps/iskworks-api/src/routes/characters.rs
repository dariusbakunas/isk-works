use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use iskworks_core::{CharacterSourceKind, ConnectedCharacterId};
use serde::Serialize;

use iskworks_app::{CharacterDetail, CharacterRosterEntry, CharacterSyncOutcome};

use crate::routes::esi::require_own_connection;
use crate::{workspace_context, ApiError, AppState};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/characters", get(list_characters))
        .route("/api/characters/:connection_id", get(get_character))
        .route("/api/characters/:connection_id/sync", post(sync_character))
}

async fn list_characters(
    State(state): State<AppState>,
) -> Result<Json<Vec<CharacterRosterEntry>>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state.character_roster_service()?.list(workspace_id).await?,
    ))
}

async fn get_character(
    State(state): State<AppState>,
    Path(connection_id): Path<uuid::Uuid>,
) -> Result<Json<CharacterDetail>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    require_own_connection(&state, workspace_id, ConnectedCharacterId(connection_id)).await?;
    Ok(Json(
        state
            .character_roster_service()?
            .detail(ConnectedCharacterId(connection_id))
            .await?,
    ))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CharacterSourceSyncOutcome {
    source_kind: CharacterSourceKind,
    outcome: CharacterSyncOutcome,
}

async fn sync_character(
    State(state): State<AppState>,
    Path(connection_id): Path<uuid::Uuid>,
) -> Result<Json<Vec<CharacterSourceSyncOutcome>>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    require_own_connection(&state, workspace_id, ConnectedCharacterId(connection_id)).await?;
    // This is a manual "sync now" request with its own HTTP lifecycle, not
    // worker shutdown -- a fresh, never-cancelled token.
    let cancel = tokio_util::sync::CancellationToken::new();
    let outcomes = state
        .character_sync_service()?
        .sync_connection(ConnectedCharacterId(connection_id), &cancel)
        .await
        .into_iter()
        .map(|(source_kind, outcome)| CharacterSourceSyncOutcome {
            source_kind,
            outcome,
        })
        .collect();
    Ok(Json(outcomes))
}
