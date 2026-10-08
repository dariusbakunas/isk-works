use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use iskworks_core::{
    supported_profitability_scopes, EvaluateOpportunitiesCommand, OpportunityEvaluation,
    OpportunityRefreshAcceptance, ProfitabilityScopeDefinition,
};

use crate::{workspace_context, ApiError, AppState};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/opportunities/scopes", get(list_scopes))
        .route("/api/opportunities/evaluate", post(evaluate))
        .route("/api/opportunities/refresh", post(refresh))
}

async fn refresh(
    State(state): State<AppState>,
    Json(command): Json<EvaluateOpportunitiesCommand>,
) -> Result<Json<OpportunityRefreshAcceptance>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    Ok(Json(
        state
            .opportunity_service()?
            .prioritize_evidence(workspace_id, owner_id, command, Utc::now())
            .await?,
    ))
}

async fn list_scopes(
    State(state): State<AppState>,
) -> Result<Json<Vec<ProfitabilityScopeDefinition>>, ApiError> {
    let _ = workspace_context(&state).await?;
    Ok(Json(supported_profitability_scopes()))
}

async fn evaluate(
    State(state): State<AppState>,
    Json(command): Json<EvaluateOpportunitiesCommand>,
) -> Result<Json<OpportunityEvaluation>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    Ok(Json(
        state
            .opportunity_service()?
            .evaluate(workspace_id, owner_id, command, Utc::now())
            .await?,
    ))
}
