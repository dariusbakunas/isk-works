//! HTTP glue for the Build verification-workbook export.
//!
//! `POST /api/builds/:build_id/export-verification` -- POST, not GET, because
//! the workbook must reflect the editor's current *unsaved* planning overlay,
//! sent in the body as the same [`PreviewBuildPlanCommand`] the build-plan
//! preview / Build Graph / Materials view already take.
//!
//! The handler is read-only. It performs exactly:
//!
//! 1. `IndustryRepository::get_build` -- Build identity (name, owner id).
//! 2. `BuildMaterialsCoordinator::materials_with_planning_cost` -- the single
//!    whole-tree inventory-allocating walk that feeds every quantity sheet
//!    (its one `list_balances` is the only inventory read; the per-node and
//!    per-boundary verification evidence rides on the same traversal), plus
//!    the one bulk adjusted-price resolution and the resulting
//!    `BuildCostProjection` the cost sheets compare Excel's own formulas
//!    against.
//! 3. `SdeReadRepository::type_reference` -- one bulk metadata read for the
//!    `Types` sheet + every name lookup.
//!
//! No Build save, child-run update, inventory write, reservation, ticket, or
//! Epic creation happens on this path.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use axum::routing::post;
use axum::{Json, Router};
use chrono::Utc;
use iskworks_core::{BuildId, PreviewBuildPlanCommand};

use crate::export::{build_verification_workbook, sanitize_filename_stem, VerificationExportModel};
use crate::{workspace_context, ApiError, AppState};

async fn export_verification_workbook(
    State(state): State<AppState>,
    Path(build_id): Path<uuid::Uuid>,
    Json(mut command): Json<PreviewBuildPlanCommand>,
) -> Result<Response<Body>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let build_id = BuildId(build_id);
    // The path id is authoritative -- a client cannot project one Build's
    // tree under another's overlay (mirrors the Graph / Materials routes).
    command.build_id = Some(build_id);

    let generated_at = Utc::now();

    let build = state
        .industry_repository()?
        .get_build(workspace_id, build_id)
        .await?;

    let (materials, cost, adjusted_prices) = state
        .build_materials_coordinator()?
        .materials_with_planning_cost(workspace_id, owner_id, build_id, command.clone())
        .await?;

    // One bulk SDE metadata read for every type_id the workbook references
    // (requirement boundaries, aggregate rows, operation products,
    // blueprint/formula types) -- the `Types` sheet + every name lookup.
    let mut type_ids: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();
    for row in &materials.rows {
        type_ids.insert(row.type_id);
    }
    for boundary in &materials.verification_inputs {
        type_ids.insert(boundary.type_id);
    }
    for operation in &materials.verification_operations {
        type_ids.insert(operation.product_type_id);
        type_ids.insert(operation.blueprint_or_formula_type_id);
    }
    let type_ids: Vec<i64> = type_ids.into_iter().filter(|id| *id > 0).collect();
    let type_reference = state.sde_repository.type_reference(&type_ids).await?;

    let model = VerificationExportModel::assemble(
        &build,
        command.runs,
        materials,
        cost,
        adjusted_prices,
        type_reference,
        generated_at,
    );

    let filename = format!("{}-verification.xlsx", sanitize_filename_stem(&build.name));
    let bytes = tokio::task::spawn_blocking(move || build_verification_workbook(&model))
        .await
        .map_err(|error| {
            ApiError::Industry(iskworks_core::IndustryError::Persistence(format!(
                "verification workbook generation panicked: {error}"
            )))
        })?
        .map_err(|error| {
            ApiError::Industry(iskworks_core::IndustryError::Persistence(format!(
                "verification workbook generation failed: {error}"
            )))
        })?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        )
        .header(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")).unwrap_or_else(
                |_| HeaderValue::from_static("attachment; filename=\"build-verification.xlsx\""),
            ),
        )
        .body(Body::from(bytes))
        .expect("verification export response builder uses only valid header values"))
}

pub(super) fn router() -> Router<AppState> {
    Router::new().route(
        "/api/builds/:build_id/export-verification",
        post(export_verification_workbook),
    )
}
