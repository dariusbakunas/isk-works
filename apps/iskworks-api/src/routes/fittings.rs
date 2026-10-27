use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use iskworks_app::{
    AttributeView, FitDetail, FitPilot, FitStatsView, FitSummary, FitTextPreview, PilotChoice,
};
use iskworks_core::fitting::{
    damage_profile_presets, DamageProfile, DamageProfilePreset, ExplainTarget, FitDraft, FitId,
    FitInput, FittingEngineInfo,
};
use iskworks_core::skill_plans::SkillPlan;
use iskworks_core::{ConnectedCharacterId, InventoryError};
use serde::{Deserialize, Serialize};

use crate::{workspace_context, ApiError, AppState};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/fittings/meta", get(meta))
        .route("/api/fittings", get(list_fits).post(create_fit))
        .route("/api/fittings/parse", post(parse_text))
        .route("/api/fittings/import", post(import_text))
        .route("/api/fittings/calculate", post(calculate))
        .route("/api/fittings/:fit_id/stats", get(fit_stats))
        .route("/api/fittings/:fit_id/pilots", get(fit_pilots))
        .route("/api/fittings/:fit_id/explain", get(explain_attribute))
        .route("/api/fittings/:fit_id/skill-plan", post(create_skill_plan))
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

fn bad_request(message: String) -> ApiError {
    ApiError::Inventory(InventoryError::Validation(message))
}

/// `allV` (the default), `allZero`, or a character's connection id.
fn parse_pilot(pilot: Option<&str>) -> Result<PilotChoice, ApiError> {
    match pilot.unwrap_or("allV") {
        "allV" => Ok(PilotChoice::AllV),
        "allZero" => Ok(PilotChoice::AllZero),
        other => uuid::Uuid::parse_str(other)
            .map(|id| PilotChoice::Character(ConnectedCharacterId(id)))
            .map_err(|_| bad_request(format!("unknown pilot {other:?}"))),
    }
}

/// A damage profile preset by id; uniform by default.
fn parse_profile(profile: Option<&str>) -> Result<DamageProfile, ApiError> {
    let id = profile.unwrap_or("uniform");
    damage_profile_presets()
        .into_iter()
        .find(|preset| preset.id == id)
        .map(|preset| preset.profile)
        .ok_or_else(|| bad_request(format!("unknown damage profile {id:?}")))
}

#[derive(Debug, Deserialize)]
struct CalculateBody {
    fit: FitInput,
    pilot: Option<String>,
    profile: Option<String>,
}

/// Calculates a fit sent in the request, saved or not (the editor's draft).
async fn calculate(
    State(state): State<AppState>,
    Json(body): Json<CalculateBody>,
) -> Result<Json<FitStatsView>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let engine = state.fitting_engine()?;
    let pilot = parse_pilot(body.pilot.as_deref())?;
    let profile = parse_profile(body.profile.as_deref())?;
    Ok(Json(
        state
            .fitting_service()?
            .calculate(
                workspace_id,
                engine.as_ref(),
                &body.fit,
                pilot,
                &profile,
                Utc::now(),
            )
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
struct StatsParams {
    pilot: Option<String>,
    profile: Option<String>,
}

/// A saved fit's stats for `pilot` against `profile`.
async fn fit_stats(
    State(state): State<AppState>,
    Path(fit_id): Path<uuid::Uuid>,
    Query(params): Query<StatsParams>,
) -> Result<Json<FitStatsView>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let engine = state.fitting_engine()?;
    let pilot = parse_pilot(params.pilot.as_deref())?;
    let profile = parse_profile(params.profile.as_deref())?;
    state
        .fitting_service()?
        .stats(
            workspace_id,
            engine.as_ref(),
            FitId(fit_id),
            pilot,
            &profile,
            Utc::now(),
        )
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}

#[derive(Debug, Deserialize)]
struct PilotsParams {
    profile: Option<String>,
}

/// Every workspace character against a saved fit. Without the engine the
/// rows still say who can fly it, with no stats.
async fn fit_pilots(
    State(state): State<AppState>,
    Path(fit_id): Path<uuid::Uuid>,
    Query(params): Query<PilotsParams>,
) -> Result<Json<Vec<FitPilot>>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let profile = parse_profile(params.profile.as_deref())?;
    let engine = state.fitting_engine.clone();
    state
        .fitting_service()?
        .pilots(
            workspace_id,
            engine.as_deref(),
            FitId(fit_id),
            &profile,
            Utc::now(),
        )
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkillPlanBody {
    connection_id: Option<uuid::Uuid>,
}

/// A skill plan for the fit: everything it needs, or with `connectionId`,
/// what that character still has to train.
async fn create_skill_plan(
    State(state): State<AppState>,
    Path(fit_id): Path<uuid::Uuid>,
    Json(body): Json<SkillPlanBody>,
) -> Result<(StatusCode, Json<SkillPlan>), ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let plan = state
        .fitting_service()?
        .create_skill_plan(
            workspace_id,
            FitId(fit_id),
            body.connection_id.map(ConnectedCharacterId),
            Utc::now(),
        )
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok((StatusCode::CREATED, Json(plan)))
}

#[derive(Debug, Deserialize)]
struct ExplainParams {
    pilot: Option<String>,
    profile: Option<String>,
    /// `ship`, or an item's index in the fit.
    target: Option<String>,
    /// The dogma attribute name, e.g. `cpuOutput`.
    attribute: Option<String>,
}

/// "Why?": every modifier on one attribute of the ship or an item.
async fn explain_attribute(
    State(state): State<AppState>,
    Path(fit_id): Path<uuid::Uuid>,
    Query(params): Query<ExplainParams>,
) -> Result<Json<AttributeView>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let engine = state.fitting_engine()?;
    let pilot = parse_pilot(params.pilot.as_deref())?;
    let profile = parse_profile(params.profile.as_deref())?;
    let target = match params.target.as_deref().unwrap_or("ship") {
        "ship" => ExplainTarget::Ship,
        index => index
            .parse()
            .map(ExplainTarget::Item)
            .map_err(|_| bad_request(format!("unknown target {index:?}")))?,
    };
    let attribute = params
        .attribute
        .filter(|attribute| !attribute.is_empty())
        .ok_or_else(|| bad_request("an attribute is required".to_string()))?;
    state
        .fitting_service()?
        .explain(
            workspace_id,
            engine.as_ref(),
            FitId(fit_id),
            pilot,
            &profile,
            target,
            &attribute,
        )
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}
