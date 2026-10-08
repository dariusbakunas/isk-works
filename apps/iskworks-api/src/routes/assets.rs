use std::collections::HashSet;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, Response, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use iskworks_core::{
    AssetBlueprintKindFilter, AssetKindFilter, AssetReconciliationFilter, AssetSortColumn,
    AssetSortDirection, ConnectedCharacterId, EsiSyncKind, FlatAssetQuery, InventoryError,
};
use serde::{Deserialize, Serialize};

use crate::csv_cell::safe_cell;
use crate::{workspace_context, ApiError, AppState};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/assets", get(list_flat_assets))
        .route("/api/assets/export", get(export_flat_assets))
        .route("/api/assets/sync", post(sync_assets))
        .route("/api/assets/summary", get(get_asset_browser_summary))
        .route("/api/assets/filters", get(get_asset_browser_filters))
        .route("/api/assets/locations", get(list_asset_locations))
        .route(
            "/api/assets/locations/:location_id",
            get(get_asset_location),
        )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssetExportQuery {
    #[serde(flatten)]
    query: AssetHttpQuery,
    columns: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct AssetHttpQuery {
    #[serde(default)]
    search: String,
    connection_ids: Option<String>,
    location_ids: Option<String>,
    asset_kinds: Option<String>,
    group_ids: Option<String>,
    blueprint_kinds: Option<String>,
    reconciliation_states: Option<String>,
    #[serde(default)]
    sort: AssetSortColumn,
    #[serde(default)]
    order: AssetSortDirection,
    cursor: Option<String>,
    limit: Option<u16>,
}

impl AssetHttpQuery {
    fn into_core(self) -> Result<FlatAssetQuery, InventoryError> {
        Ok(FlatAssetQuery {
            search: self.search,
            connection_ids: parse_csv(self.connection_ids.as_deref(), "connection ID")?,
            location_ids: parse_csv(self.location_ids.as_deref(), "location ID")?,
            asset_kinds: parse_asset_kinds(self.asset_kinds.as_deref())?,
            group_ids: parse_csv(self.group_ids.as_deref(), "group ID")?,
            blueprint_kinds: parse_blueprint_kinds(self.blueprint_kinds.as_deref())?,
            reconciliation_states: parse_reconciliation_states(
                self.reconciliation_states.as_deref(),
            )?,
            sort: self.sort,
            order: self.order,
            cursor: self.cursor,
            limit: self.limit,
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum AssetColumn {
    Item,
    Quantity,
    PackagedVolume,
    Character,
    Location,
    Container,
    Group,
    Status,
    Observed,
}

async fn export_flat_assets(
    State(state): State<AppState>,
    Query(request): Query<AssetExportQuery>,
) -> Result<Response<Body>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let columns = parse_columns(request.columns.as_deref())?;
    let query = request.query.into_core()?.validate()?;
    let rows = state
        .asset_browser_repository()?
        .all_flat_assets(workspace_id, &query)
        .await?;
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer
        .write_record(columns.iter().map(|column| column_label(*column)))
        .map_err(|error| InventoryError::Persistence(error.to_string()))?;
    for row in rows {
        writer
            .write_record(
                columns
                    .iter()
                    .map(|column| match column {
                        AssetColumn::Item => row.type_name.clone().unwrap_or_default(),
                        AssetColumn::Quantity => row.quantity.to_string(),
                        AssetColumn::PackagedVolume => {
                            row.total_packaged_volume.clone().unwrap_or_default()
                        }
                        AssetColumn::Character => row.character_name.clone(),
                        AssetColumn::Location => row.location_name.clone().unwrap_or_default(),
                        AssetColumn::Container => row.container_name.clone().unwrap_or_default(),
                        AssetColumn::Group => row.group_name.clone().unwrap_or_default(),
                        AssetColumn::Status => row.reconciliation.state.clone(),
                        AssetColumn::Observed => row.observed_at.to_rfc3339(),
                    })
                    .map(|cell| safe_cell(&cell).into_owned()),
            )
            .map_err(|error| InventoryError::Persistence(error.to_string()))?;
    }
    let csv = writer
        .into_inner()
        .map_err(|error| InventoryError::Persistence(error.to_string()))?;
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment; filename=isk-works-assets.csv"),
        )
        .body(Body::from(csv))
        .expect("static Assets response headers are valid"))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssetSyncRequest {
    #[serde(default)]
    connection_ids: Vec<ConnectedCharacterId>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AssetSyncOutcome {
    connection_id: ConnectedCharacterId,
    character_name: String,
    succeeded: bool,
    runs: Vec<iskworks_core::EsiSyncRun>,
    error: Option<String>,
}

async fn sync_assets(
    State(state): State<AppState>,
    Json(request): Json<AssetSyncRequest>,
) -> Result<Json<Vec<AssetSyncOutcome>>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let service = state.esi_service()?;
    if service.fixture_mode() {
        return Err(InventoryError::Validation(
            "Asset synchronization requires real EVE connections; fixture mode is not supported."
                .to_string(),
        )
        .into());
    }
    let connections = state
        .esi_repository()?
        .list_connections(workspace_id)
        .await?;
    let requested = if request.connection_ids.is_empty() {
        connections.iter().map(|connection| connection.id).collect()
    } else {
        request.connection_ids.into_iter().collect::<HashSet<_>>()
    };
    let known = connections
        .iter()
        .map(|connection| connection.id)
        .collect::<HashSet<_>>();
    if !requested.is_subset(&known) {
        return Err(InventoryError::Validation(
            "One or more selected characters do not belong to this workspace.".to_string(),
        )
        .into());
    }
    let mut outcomes = Vec::new();
    for connection in connections
        .into_iter()
        .filter(|connection| requested.contains(&connection.id))
    {
        match service.sync(connection.id, EsiSyncKind::Assets).await {
            Ok(runs) => outcomes.push(AssetSyncOutcome {
                connection_id: connection.id,
                character_name: connection.character_name,
                succeeded: true,
                runs,
                error: None,
            }),
            Err(error) => outcomes.push(AssetSyncOutcome {
                connection_id: connection.id,
                character_name: connection.character_name,
                succeeded: false,
                runs: Vec::new(),
                error: Some(crate::error::public_outcome_message(error)),
            }),
        }
    }
    Ok(Json(outcomes))
}

fn parse_columns(value: Option<&str>) -> Result<Vec<AssetColumn>, InventoryError> {
    let requested = value.unwrap_or(
        "item,quantity,packagedVolume,character,location,container,group,status,observed",
    );
    requested
        .split(',')
        .map(|column| match column {
            "item" => Ok(AssetColumn::Item),
            "quantity" => Ok(AssetColumn::Quantity),
            "packagedVolume" => Ok(AssetColumn::PackagedVolume),
            "character" => Ok(AssetColumn::Character),
            "location" => Ok(AssetColumn::Location),
            "container" => Ok(AssetColumn::Container),
            "group" => Ok(AssetColumn::Group),
            "status" => Ok(AssetColumn::Status),
            "observed" => Ok(AssetColumn::Observed),
            _ => Err(InventoryError::Validation(
                "one or more asset export columns are unsupported".to_string(),
            )),
        })
        .collect()
}

fn column_label(column: AssetColumn) -> &'static str {
    match column {
        AssetColumn::Item => "Item",
        AssetColumn::Quantity => "Quantity",
        AssetColumn::PackagedVolume => "Packaged volume (m3)",
        AssetColumn::Character => "Character",
        AssetColumn::Location => "Location",
        AssetColumn::Container => "Container",
        AssetColumn::Group => "Group",
        AssetColumn::Status => "Status",
        AssetColumn::Observed => "Observed",
    }
}

async fn list_flat_assets(
    State(state): State<AppState>,
    Query(query): Query<AssetHttpQuery>,
) -> Result<Json<iskworks_core::FlatAssetPage>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let query = query.into_core()?.validate()?;
    Ok(Json(
        state
            .asset_browser_repository()?
            .flat_assets(workspace_id, &query)
            .await?,
    ))
}

fn parse_csv<T>(value: Option<&str>, label: &str) -> Result<Vec<T>, InventoryError>
where
    T: std::str::FromStr,
{
    value
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .split(',')
                .map(|entry| {
                    entry.parse().map_err(|_| {
                        InventoryError::Validation(format!("asset {label} filter is invalid"))
                    })
                })
                .collect()
        })
        .unwrap_or_else(|| Ok(Vec::new()))
}

fn parse_asset_kinds(value: Option<&str>) -> Result<Vec<AssetKindFilter>, InventoryError> {
    parse_named(value, |entry| match entry {
        "blueprint" => Some(AssetKindFilter::Blueprint),
        "material" => Some(AssetKindFilter::Material),
        "ship" => Some(AssetKindFilter::Ship),
        "container" => Some(AssetKindFilter::Container),
        "other" => Some(AssetKindFilter::Other),
        _ => None,
    })
}

fn parse_blueprint_kinds(
    value: Option<&str>,
) -> Result<Vec<AssetBlueprintKindFilter>, InventoryError> {
    parse_named(value, |entry| match entry {
        "original" => Some(AssetBlueprintKindFilter::Original),
        "copy" => Some(AssetBlueprintKindFilter::Copy),
        "unknown" => Some(AssetBlueprintKindFilter::Unknown),
        _ => None,
    })
}

fn parse_reconciliation_states(
    value: Option<&str>,
) -> Result<Vec<AssetReconciliationFilter>, InventoryError> {
    parse_named(value, |entry| match entry {
        "matched" => Some(AssetReconciliationFilter::Matched),
        "difference" => Some(AssetReconciliationFilter::Difference),
        "noAccountingRecord" => Some(AssetReconciliationFilter::NoAccountingRecord),
        _ => None,
    })
}

fn parse_named<T>(
    value: Option<&str>,
    parse: impl Fn(&str) -> Option<T>,
) -> Result<Vec<T>, InventoryError> {
    value
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .split(',')
                .map(|entry| {
                    parse(entry).ok_or_else(|| {
                        InventoryError::Validation("asset filter value is invalid".to_string())
                    })
                })
                .collect()
        })
        .unwrap_or_else(|| Ok(Vec::new()))
}

async fn get_asset_browser_summary(
    State(state): State<AppState>,
) -> Result<Json<iskworks_core::AssetBrowserSummary>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .asset_browser_repository()?
            .summary(workspace_id)
            .await?,
    ))
}

async fn get_asset_browser_filters(
    State(state): State<AppState>,
) -> Result<Json<iskworks_core::AssetBrowserFilters>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .asset_browser_repository()?
            .filters(workspace_id)
            .await?,
    ))
}

async fn list_asset_locations(
    State(state): State<AppState>,
    Query(query): Query<iskworks_core::AssetBrowserQuery>,
) -> Result<Json<Vec<iskworks_core::AssetLocationSummary>>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .asset_browser_repository()?
            .locations(workspace_id, &query)
            .await?,
    ))
}

async fn get_asset_location(
    State(state): State<AppState>,
    Path(location_id): Path<i64>,
    Query(query): Query<iskworks_core::AssetBrowserQuery>,
) -> Result<Json<iskworks_core::AssetLocationPage>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    Ok(Json(
        state
            .asset_browser_repository()?
            .location(workspace_id, location_id, &query)
            .await?,
    ))
}
