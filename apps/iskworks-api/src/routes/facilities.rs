use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use iskworks_app::SystemCostIndex;
use iskworks_core::{
    BuildId, CreateFacilityProfileCommand, FacilityPreviewCommand, FacilityProfileId, Money,
    ResolvedMarketLocation,
};
use serde::{Deserialize, Serialize};

use crate::{workspace_context, ApiError, AppState, BlueprintSearchQuery};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/industry/facilities",
            get(list_facilities).post(create_facility),
        )
        .route(
            "/api/industry/structures/search",
            get(search_known_structures),
        )
        .route("/api/industry/structures/resolve", post(resolve_structure))
        .route(
            "/api/industry/systems/:solar_system_id/cost-index",
            get(get_system_cost_index),
        )
        .route(
            "/api/industry/facilities/:facility_id",
            get(get_facility)
                .put(update_facility)
                .delete(delete_facility),
        )
        .route(
            "/api/industry/facilities/:facility_id/archive",
            post(archive_facility),
        )
        .route("/api/industry/facilities/export", get(export_facilities))
        .route(
            "/api/industry/facilities/import/preview",
            post(preview_facility_import),
        )
        .route("/api/industry/facilities/import", post(import_facilities))
        .route(
            "/api/builds/:build_id/facility-preview",
            post(preview_build_facility),
        )
}

async fn get_system_cost_index(
    State(state): State<AppState>,
    Path(solar_system_id): Path<i64>,
) -> Result<Json<SystemCostIndex>, ApiError> {
    Ok(Json(
        state
            .esi_service()?
            .system_cost_index(solar_system_id)
            .await?,
    ))
}

async fn list_facilities(
    State(state): State<AppState>,
) -> Result<Json<Vec<iskworks_core::IndustryFacilityProfile>>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(state.facility_repository()?.list(workspace_id).await?))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FacilityExport {
    exported_at: chrono::DateTime<chrono::Utc>,
    items: Vec<CreateFacilityProfileCommand>,
}

async fn export_facilities(
    State(state): State<AppState>,
) -> Result<Json<FacilityExport>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let profiles = state.facility_repository()?.list(workspace_id).await?;
    let items = profiles
        .iter()
        .filter(|profile| profile.archived_at.is_none())
        .map(iskworks_core::export_command)
        .collect();
    Ok(Json(FacilityExport {
        exported_at: chrono::Utc::now(),
        items,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FacilityImportRequest {
    actions: Vec<FacilityImportActionRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FacilityImportActionRequest {
    action: String,
    item: CreateFacilityProfileCommand,
    existing_id: Option<FacilityProfileId>,
    expected_revision: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FacilityImportPreviewItem {
    index: usize,
    name: String,
    classification: String,
    existing_id: Option<FacilityProfileId>,
    existing_name: Option<String>,
    existing_revision: Option<u64>,
    match_basis: Option<iskworks_core::FacilityIdentityBasis>,
    message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FacilityImportPreviewResponse {
    items: Vec<FacilityImportPreviewItem>,
}

async fn preview_facility_import(
    State(state): State<AppState>,
    Json(request): Json<FacilityImportPreviewRequest>,
) -> Result<Json<FacilityImportPreviewResponse>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let profiles = state.facility_repository()?.list(workspace_id).await?;
    let items = request
        .items
        .into_iter()
        .enumerate()
        .map(|(index, command)| {
            let name = command.name.clone();
            match iskworks_core::parse_profile(workspace_id, command.clone())
                .and_then(|_| iskworks_core::find_active_facility_match(&command, &profiles))
            {
                Ok(Some((existing, basis))) => FacilityImportPreviewItem {
                    index,
                    name,
                    classification: "duplicate".into(),
                    existing_id: Some(existing.id),
                    existing_name: Some(existing.name.clone()),
                    existing_revision: Some(existing.revision),
                    match_basis: Some(basis),
                    message: None,
                },
                Ok(None) => FacilityImportPreviewItem {
                    index,
                    name,
                    classification: "new".into(),
                    existing_id: None,
                    existing_name: None,
                    existing_revision: None,
                    match_basis: None,
                    message: None,
                },
                Err(error) => FacilityImportPreviewItem {
                    index,
                    name,
                    classification: "invalid".into(),
                    existing_id: None,
                    existing_name: None,
                    existing_revision: None,
                    match_basis: None,
                    message: Some(error.to_string()),
                },
            }
        })
        .collect();
    Ok(Json(FacilityImportPreviewResponse { items }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FacilityImportPreviewRequest {
    items: Vec<CreateFacilityProfileCommand>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FacilityImportItemResult {
    name: String,
    status: String,
    message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FacilityImportResponse {
    results: Vec<FacilityImportItemResult>,
}

/// Applies the whole import atomically (see `PgFacilityRepository::import`):
/// items the import rejects are reported per item as `failed`, while a write
/// that fails rolls every item back and fails the request with that error.
async fn import_facilities(
    State(state): State<AppState>,
    Json(request): Json<FacilityImportRequest>,
) -> Result<Json<FacilityImportResponse>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let actions = request
        .actions
        .into_iter()
        .map(|action| iskworks_core::FacilityImportAction {
            kind: iskworks_core::FacilityImportActionKind::from_wire(&action.action),
            item: action.item,
            existing_id: action.existing_id,
            expected_revision: action.expected_revision,
        })
        .collect();
    let outcomes = state
        .facility_repository()?
        .import(workspace_id, actions)
        .await?;
    let results = outcomes
        .into_iter()
        .map(|outcome| match outcome.result {
            Ok(status) => FacilityImportItemResult {
                name: outcome.name,
                status: status.as_str().into(),
                message: None,
            },
            Err(error) => FacilityImportItemResult {
                name: outcome.name,
                status: "failed".into(),
                message: Some(crate::error::public_outcome_message(error)),
            },
        })
        .collect();
    Ok(Json(FacilityImportResponse { results }))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct KnownStructureResponse {
    structure_id: i64,
    structure_name: String,
    structure_type_id: Option<i64>,
    structure_type_name: Option<String>,
    solar_system_id: i64,
    solar_system_name: Option<String>,
    security_class: String,
}

async fn search_known_structures(
    State(state): State<AppState>,
    Query(query): Query<BlueprintSearchQuery>,
) -> Result<Json<Vec<KnownStructureResponse>>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .facility_repository()?
            .search_known_structures(
                workspace_id,
                &query.q,
                crate::routes::clamp_search_limit(query.limit, 50),
            )
            .await?
            .into_iter()
            .map(known_structure_response)
            .collect(),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResolveStructureRequest {
    structure_id: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResolveStructureResponse {
    configured: bool,
    structure: Option<KnownStructureResponse>,
    needs_reconnection: bool,
    eligible_character_count: u64,
    warnings: Vec<String>,
}

/// Resolves a single structure by ID via ESI, using any connected character
/// with docking access there -- regardless of whether the workspace has any
/// prior assets/wallet/market activity at that structure. This is the path
/// for a structure a player has docking access to but has never otherwise
/// touched through ISK Works (e.g. pasted from an EVE client "Show Info"
/// link, `<url=showinfo:TYPE_ID//STRUCTURE_ID>Name</url>`).
async fn resolve_structure(
    State(state): State<AppState>,
    Json(request): Json<ResolveStructureRequest>,
) -> Result<Json<ResolveStructureResponse>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    if request.structure_id <= 0 {
        return Err(ApiError::Inventory(
            iskworks_core::InventoryError::Validation("Structure ID must be positive.".to_string()),
        ));
    }
    let facility_repository = state.facility_repository()?;
    if let Some(known) = facility_repository
        .get_known_structure(workspace_id, request.structure_id)
        .await?
    {
        return Ok(Json(ResolveStructureResponse {
            configured: true,
            structure: Some(known_structure_response(known)),
            needs_reconnection: false,
            eligible_character_count: 0,
            warnings: Vec::new(),
        }));
    }
    let Some(service) = state.esi_service.as_ref() else {
        return Ok(Json(ResolveStructureResponse {
            configured: false,
            structure: None,
            needs_reconnection: false,
            eligible_character_count: 0,
            warnings: vec!["EVE SSO is not configured.".to_string()],
        }));
    };
    let resolution = service
        .resolve_structures(workspace_id, &[request.structure_id])
        .await?;
    let now = chrono::Utc::now();
    let locations = resolution
        .resolved
        .iter()
        .map(|item| ResolvedMarketLocation {
            location_id: item.structure.structure_id,
            location_name: item.structure.name.clone(),
            owner_id: item.structure.owner_id,
            solar_system_id: item.structure.solar_system_id,
            structure_type_id: item.structure.type_id,
            resolved_by_connection_id: item.connection_id,
            resolved_at: now,
        })
        .collect::<Vec<_>>();
    let resolved_any = !locations.is_empty();
    state
        .market_repository()?
        .save_location_names(workspace_id, locations)
        .await?;
    let structure = if resolved_any {
        facility_repository
            .get_known_structure(workspace_id, request.structure_id)
            .await?
    } else {
        None
    };
    Ok(Json(ResolveStructureResponse {
        configured: true,
        structure: structure.map(known_structure_response),
        needs_reconnection: resolution.needs_reconnection,
        eligible_character_count: resolution.eligible_character_count,
        warnings: resolution.warnings,
    }))
}

fn known_structure_response(structure: iskworks_storage::KnownStructure) -> KnownStructureResponse {
    KnownStructureResponse {
        structure_id: structure.structure_id,
        structure_name: structure.structure_name,
        structure_type_id: structure.structure_type_id,
        structure_type_name: structure.structure_type_name,
        solar_system_id: structure.solar_system_id,
        solar_system_name: structure.solar_system_name,
        security_class: structure.security_class,
    }
}

async fn get_facility(
    State(state): State<AppState>,
    Path(facility_id): Path<uuid::Uuid>,
) -> Result<Json<iskworks_core::IndustryFacilityProfile>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .facility_repository()?
            .get(workspace_id, FacilityProfileId(facility_id))
            .await?,
    ))
}

async fn create_facility(
    State(state): State<AppState>,
    Json(command): Json<CreateFacilityProfileCommand>,
) -> Result<(StatusCode, Json<iskworks_core::IndustryFacilityProfile>), ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let profile = iskworks_core::parse_profile(workspace_id, command)?;
    Ok((
        StatusCode::CREATED,
        Json(state.facility_repository()?.create(profile).await?),
    ))
}

async fn update_facility(
    State(state): State<AppState>,
    Path(facility_id): Path<uuid::Uuid>,
    Json(command): Json<iskworks_core::UpdateFacilityProfileCommand>,
) -> Result<Json<iskworks_core::IndustryFacilityProfile>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let replacement = iskworks_core::parse_profile(workspace_id, command.profile)?;
    Ok(Json(
        state
            .facility_repository()?
            .update(
                workspace_id,
                FacilityProfileId(facility_id),
                command.expected_revision,
                replacement,
            )
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FacilityRevisionRequest {
    expected_revision: u64,
}

async fn archive_facility(
    State(state): State<AppState>,
    Path(facility_id): Path<uuid::Uuid>,
    Json(request): Json<FacilityRevisionRequest>,
) -> Result<Json<iskworks_core::IndustryFacilityProfile>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .facility_repository()?
            .archive(
                workspace_id,
                FacilityProfileId(facility_id),
                request.expected_revision,
            )
            .await?,
    ))
}

async fn delete_facility(
    State(state): State<AppState>,
    Path(facility_id): Path<uuid::Uuid>,
    Json(request): Json<FacilityRevisionRequest>,
) -> Result<StatusCode, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    state
        .facility_repository()?
        .delete(
            workspace_id,
            FacilityProfileId(facility_id),
            request.expected_revision,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn preview_build_facility(
    State(state): State<AppState>,
    Path(build_id): Path<uuid::Uuid>,
    Json(command): Json<FacilityPreviewCommand>,
) -> Result<Json<iskworks_core::FacilityPlanPreview>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let build = state
        .industry_repository()?
        .get_build(workspace_id, BuildId(build_id))
        .await?;
    // A live Build's facility is resolved by id against its current
    // settings (see `IndustryService::resolve_facility_profile`).
    // Revision-based optimistic concurrency still guards edits to the
    // profile itself (`update_facility` / `archive_facility` /
    // `delete_facility`).
    let profile = state
        .facility_repository()?
        .get(workspace_id, command.facility_profile_id)
        .await?;
    let eiv = command
        .estimated_item_value
        .as_deref()
        .map(Money::parse)
        .transpose()?;
    let iskworks_core::BuildRecipe::Manufacturing(recipe) = &build.recipe else {
        return Err(iskworks_core::FacilityError::Validation(
            "Reaction Builds are not yet supported by this preview endpoint.".to_string(),
        )
        .into());
    };
    let product_type_id = recipe.primary_product().type_id;
    let product = state
        .sde_repository
        .type_classifications(&[product_type_id])
        .await?
        .get(&product_type_id)
        .map(|classification| iskworks_core::ProductClassification {
            category_id: classification.category_id,
            group_id: classification.group_id,
        })
        .unwrap_or_default();
    Ok(Json(iskworks_core::preview_facility(
        recipe,
        build.runs,
        profile,
        product,
        command.blueprint_me,
        command.blueprint_te,
        eiv,
        None,
    )?))
}
