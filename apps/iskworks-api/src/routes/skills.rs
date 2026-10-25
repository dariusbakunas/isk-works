use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use iskworks_app::{
    ImportedSkillPlan, PlanTextPreview, SkillCatalogueResponse, SkillPlanDetail, SkillPlanSummary,
};
use iskworks_core::skill_plans::{PlanEntry, SkillPlanId, SkillPlanInput};
use iskworks_core::ConnectedCharacterId;
use serde::Deserialize;

use crate::{workspace_context, ApiError, AppState};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/skills/catalogue", get(catalogue))
        .route("/api/skill-plans", get(list_plans).post(create_plan))
        .route("/api/skill-plans/import", post(import_plan))
        .route("/api/skill-plans/import/preview", post(preview_import))
        .route("/api/skill-plans/:plan_id/export", get(export_plan))
        .route(
            "/api/skill-plans/:plan_id",
            get(get_plan).put(update_plan).delete(delete_plan),
        )
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanParams {
    character_id: Option<uuid::Uuid>,
}

/// A whole plan, as the editor sends it on create and update.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanBody {
    name: String,
    #[serde(default)]
    description: String,
    entries: Vec<PlanEntry>,
}

impl From<PlanBody> for SkillPlanInput {
    fn from(body: PlanBody) -> Self {
        Self {
            name: body.name,
            description: body.description,
            entries: body.entries,
        }
    }
}

/// Every plan, with `characterId`'s progress when given.
async fn list_plans(
    State(state): State<AppState>,
    Query(params): Query<PlanParams>,
) -> Result<Json<Vec<SkillPlanSummary>>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    state
        .skill_plan_service()?
        .list(
            workspace_id,
            params.character_id.map(ConnectedCharacterId),
            Utc::now(),
        )
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}

async fn create_plan(
    State(state): State<AppState>,
    Json(body): Json<PlanBody>,
) -> Result<(StatusCode, Json<SkillPlanDetail>), ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let detail = state
        .skill_plan_service()?
        .create(workspace_id, body.into(), Utc::now())
        .await?;
    Ok((StatusCode::CREATED, Json(detail)))
}

async fn get_plan(
    State(state): State<AppState>,
    Path(plan_id): Path<uuid::Uuid>,
) -> Result<Json<SkillPlanDetail>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    state
        .skill_plan_service()?
        .get(workspace_id, SkillPlanId(plan_id), Utc::now())
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}

async fn update_plan(
    State(state): State<AppState>,
    Path(plan_id): Path<uuid::Uuid>,
    Json(body): Json<PlanBody>,
) -> Result<Json<SkillPlanDetail>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    state
        .skill_plan_service()?
        .update(workspace_id, SkillPlanId(plan_id), body.into(), Utc::now())
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}

async fn delete_plan(
    State(state): State<AppState>,
    Path(plan_id): Path<uuid::Uuid>,
) -> Result<StatusCode, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    if state
        .skill_plan_service()?
        .delete(workspace_id, SkillPlanId(plan_id))
        .await?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

#[derive(Debug, Deserialize)]
struct PreviewBody {
    text: String,
}

/// What importing `text` would make; nothing is saved.
async fn preview_import(
    State(state): State<AppState>,
    Json(body): Json<PreviewBody>,
) -> Result<Json<PlanTextPreview>, ApiError> {
    workspace_context(&state).await?;
    Ok(Json(
        state.skill_plan_service()?.preview_text(&body.text).await?,
    ))
}

#[derive(Debug, Deserialize)]
struct ImportBody {
    name: String,
    #[serde(default)]
    description: String,
    text: String,
}

/// Creates a plan from text pasted from the client.
async fn import_plan(
    State(state): State<AppState>,
    Json(body): Json<ImportBody>,
) -> Result<(StatusCode, Json<ImportedSkillPlan>), ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let imported = state
        .skill_plan_service()?
        .import_text(
            workspace_id,
            body.name,
            body.description,
            &body.text,
            Utc::now(),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(imported)))
}

/// The plan as the client's plan text, for pasting into the game.
async fn export_plan(
    State(state): State<AppState>,
    Path(plan_id): Path<uuid::Uuid>,
) -> Result<([(header::HeaderName, &'static str); 1], String), ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let text = state
        .skill_plan_service()?
        .export_text(workspace_id, SkillPlanId(plan_id))
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], text))
}
