use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use iskworks_core::fitting::{damage_profile_presets, DamageProfilePreset, FittingEngineInfo};
use serde::Serialize;

use crate::{ApiError, AppState};

pub(crate) fn router() -> Router<AppState> {
    Router::new().route("/api/fittings/meta", get(meta))
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
