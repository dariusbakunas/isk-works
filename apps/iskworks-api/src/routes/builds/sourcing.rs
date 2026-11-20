//! HTTP glue for Optimize sourcing.
//!
//! - `POST /api/builds/:build_id/optimize-sourcing/preview` -- read-only:
//!   the proposed Buy/Produce switches for the root plan `build_id`, with the
//!   current and proposed margins.
//! - `POST /api/builds/:build_id/optimize-sourcing/apply` -- writes the
//!   chosen switches top-down; `409 revision_conflict` when the plan changed
//!   since the preview's fingerprint.

use std::collections::BTreeSet;

use axum::extract::{Path, State};
use axum::routing::post;
use axum::{Json, Router};
use iskworks_core::industry::sourcing_optimizer::{
    SourcingApplyChange, SourcingApplyOutcome, SourcingOptions, SourcingProposal,
};
use iskworks_core::BuildId;
use rust_decimal::Decimal;
use serde::Deserialize;

use crate::{workspace_context, ApiError, AppState};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewRequest {
    #[serde(default, with = "rust_decimal::serde::str_option")]
    min_saving_percent: Option<Decimal>,
    #[serde(default)]
    include_without_blueprint: bool,
    #[serde(default)]
    exclude: BTreeSet<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApplyRequest {
    fingerprint: String,
    changes: Vec<SourcingApplyChange>,
}

/// The spec's default minimum saving (2 %).
const DEFAULT_MIN_SAVING_PERCENT: Decimal = Decimal::from_parts(2, 0, 0, false, 0);

async fn preview(
    State(state): State<AppState>,
    Path(build_id): Path<uuid::Uuid>,
    Json(request): Json<PreviewRequest>,
) -> Result<Json<SourcingProposal>, ApiError> {
    let min_saving_percent = request
        .min_saving_percent
        .unwrap_or(DEFAULT_MIN_SAVING_PERCENT);
    if !(Decimal::ZERO..Decimal::ONE_HUNDRED).contains(&min_saving_percent) {
        return Err(ApiError::Industry(
            iskworks_core::IndustryError::Validation(
                "minSavingPercent must be at least 0 and below 100.".to_string(),
            ),
        ));
    }
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let coordinator = state.build_materials_coordinator()?;
    let options = SourcingOptions {
        min_saving_percent,
        include_without_blueprint: request.include_without_blueprint,
        exclude: request.exclude,
    };
    Ok(Json(
        coordinator
            .optimize_sourcing_preview(workspace_id, owner_id, BuildId(build_id), &options)
            .await?,
    ))
}

async fn apply(
    State(state): State<AppState>,
    Path(build_id): Path<uuid::Uuid>,
    Json(request): Json<ApplyRequest>,
) -> Result<Json<SourcingApplyOutcome>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let coordinator = state.build_materials_coordinator()?;
    Ok(Json(
        coordinator
            .optimize_sourcing_apply(
                workspace_id,
                owner_id,
                BuildId(build_id),
                &request.fingerprint,
                &request.changes,
            )
            .await?,
    ))
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/builds/:build_id/optimize-sourcing/preview",
            post(preview),
        )
        .route("/api/builds/:build_id/optimize-sourcing/apply", post(apply))
}
