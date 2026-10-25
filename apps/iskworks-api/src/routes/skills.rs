use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::Utc;
use iskworks_app::SkillCatalogueResponse;
use iskworks_core::ConnectedCharacterId;
use serde::Deserialize;

use crate::{workspace_context, ApiError, AppState};

pub(crate) fn router() -> Router<AppState> {
    Router::new().route("/api/skills/catalogue", get(catalogue))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogueParams {
    character_id: Option<uuid::Uuid>,
}

/// The skill catalogue, overlaid with `characterId`'s skills when given.
async fn catalogue(
    State(state): State<AppState>,
    Query(params): Query<CatalogueParams>,
) -> Result<Json<SkillCatalogueResponse>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    state
        .skill_catalogue_service()?
        .catalogue(
            workspace_id,
            params.character_id.map(ConnectedCharacterId),
            Utc::now(),
        )
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}
