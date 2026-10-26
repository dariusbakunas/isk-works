use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use iskworks_app::{FitDetail, FitSummary, FitTextPreview};
use iskworks_core::fitting::{
    damage_profile_presets, DamageProfilePreset, FitDraft, FitId, FittingEngineInfo,
};
use serde::{Deserialize, Serialize};

use crate::{workspace_context, ApiError, AppState};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/fittings/meta", get(meta))
        .route("/api/fittings", get(list_fits).post(create_fit))
        .route("/api/fittings/parse", post(parse_text))
        .route("/api/fittings/import", post(import_text))
        .route("/api/fittings/:fit_id/export", get(export_fit))
        .route(
            "/api/fittings/:fit_id",
            get(get_fit).put(update_fit).delete(delete_fit),
        )
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FittingsMeta {
    /// Whether fit stats can be calculated (the engine data loaded).
    available: bool,
    engine: Option<FittingEngineInfo>,
    sde: Option<SdeBuild>,
    damage_profiles: Vec<DamageProfilePreset>,
}

/// The active SDE import's build, which names, groups and skill
/// requirements come from; the engine data has its own build.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SdeBuild {
    build_number: String,
}

/// What the Fittings page needs before anything else: whether stats work,
/// both data builds (they are versioned apart), and the damage profiles.
async fn meta(State(state): State<AppState>) -> Result<Json<FittingsMeta>, ApiError> {
    let engine = state.fitting_engine.as_ref().map(|engine| engine.info());
    let sde = state
        .sde_repository
        .active_sde()
        .await?
        .map(|active| SdeBuild {
            build_number: active.source_version,
        });
    Ok(Json(FittingsMeta {
        available: engine.is_some(),
        engine,
        sde,
        damage_profiles: damage_profile_presets(),
    }))
}

/// Every saved fit of the workspace, by name.
async fn list_fits(State(state): State<AppState>) -> Result<Json<Vec<FitSummary>>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(state.fitting_service()?.list(workspace_id).await?))
}

async fn create_fit(
    State(state): State<AppState>,
    Json(draft): Json<FitDraft>,
) -> Result<(StatusCode, Json<FitDetail>), ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let detail = state
        .fitting_service()?
        .create(workspace_id, draft, Utc::now())
        .await?;
    Ok((StatusCode::CREATED, Json(detail)))
}

async fn get_fit(
    State(state): State<AppState>,
    Path(fit_id): Path<uuid::Uuid>,
) -> Result<Json<FitDetail>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    state
        .fitting_service()?
        .get(workspace_id, FitId(fit_id))
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}

/// Replaces the whole fit: name, description, ship and every item.
async fn update_fit(
    State(state): State<AppState>,
    Path(fit_id): Path<uuid::Uuid>,
    Json(draft): Json<FitDraft>,
) -> Result<Json<FitDetail>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    state
        .fitting_service()?
        .update(workspace_id, FitId(fit_id), draft, Utc::now())
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}

async fn delete_fit(
    State(state): State<AppState>,
    Path(fit_id): Path<uuid::Uuid>,
) -> Result<StatusCode, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    if state
        .fitting_service()?
        .delete(workspace_id, FitId(fit_id))
        .await?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

#[derive(Debug, Deserialize)]
struct ParseBody {
    text: String,
}

/// What importing `text` would make; nothing is saved.
async fn parse_text(
    State(state): State<AppState>,
    Json(body): Json<ParseBody>,
) -> Result<Json<FitTextPreview>, ApiError> {
    workspace_context(&state).await?;
    let engine = state.fitting_engine()?;
    Ok(Json(
        state
            .fitting_service()?
            .preview_text(engine.as_ref(), &body.text)
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
struct ImportBody {
    text: String,
    #[serde(default)]
    name: Option<String>,
}

/// Saves a fit pasted from the client or pyfa (EFT).
async fn import_text(
    State(state): State<AppState>,
    Json(body): Json<ImportBody>,
) -> Result<(StatusCode, Json<FitDetail>), ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let engine = state.fitting_engine()?;
    let detail = state
        .fitting_service()?
        .import_text(
            workspace_id,
            engine.as_ref(),
            &body.text,
            body.name,
            Utc::now(),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(detail)))
}

/// The fit as EFT, for pasting into the client.
async fn export_fit(
    State(state): State<AppState>,
    Path(fit_id): Path<uuid::Uuid>,
) -> Result<([(header::HeaderName, &'static str); 1], String), ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let engine = state.fitting_engine()?;
    let text = state
        .fitting_service()?
        .export_text(workspace_id, engine.as_ref(), FitId(fit_id))
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], text))
}
