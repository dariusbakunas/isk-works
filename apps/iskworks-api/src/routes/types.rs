//! The Type Info window (EVE's "Show Info") for any type in the active SDE
//! import.

use std::collections::BTreeMap;

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::Utc;
use iskworks_app::TypeInfoView;
use serde::Deserialize;

use super::fittings::{parse_pilot, register_price_coverage};
use crate::{workspace_context, ApiError, AppState};

pub(crate) fn router() -> Router<AppState> {
    Router::new().route("/api/types/:type_id/info", get(type_info))
}

#[derive(Debug, Deserialize)]
struct TypeInfoQuery {
    /// `allV` (the default), `allZero`, or a character's connection id.
    pilot: Option<String>,
}

async fn type_info(
    State(state): State<AppState>,
    Path(type_id): Path<i64>,
    Query(query): Query<TypeInfoQuery>,
) -> Result<Json<TypeInfoView>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let pilot = parse_pilot(query.pilot.as_deref())?;
    let scope = state.workspace_default_market_scope(workspace_id).await?;
    let info = state
        .type_info_service()
        .info(workspace_id, type_id, pilot, Some(scope), Utc::now())
        .await?
        .ok_or(ApiError::TypeNotFound)?;
    // Keep it priced next time, the way fit costs do.
    if info.published {
        register_price_coverage(
            &state,
            workspace_id,
            scope,
            BTreeMap::from([(info.type_id, info.name.clone())]),
        );
    }
    Ok(Json(info))
}
