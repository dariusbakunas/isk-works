use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use iskworks_core::{
    resolve_market_price_items, CostInputQuality, EsiObservation, InventoryBalance, InventoryError,
    InventoryEvent, InventoryEventId, InventoryEventKind, InventoryHistory, InventoryItemKey,
    InventoryReservation, MarketCoverageItem, MarketDepthResult, MarketError, MarketFreshnessState,
    MarketPricePreviewCommand, MarketPriceRequest, MarketPricingPolicy, MarketRefreshState,
    MarketScope, Money, MoneyDelta, PostAdjustmentCommand, PostInventoryCommand, PriceSource,
    PriceSourceId, PriceSourceKind, ReverseInventoryCommand, WorkspaceId,
};
use iskworks_sde::SdeInventoryTypeMetadata;
use serde::{Deserialize, Serialize};

use crate::{workspace_context, ApiError, AppState};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/inventory", get(list_inventory))
        .route("/api/inventory/:type_id", get(get_inventory_item))
        .route(
            "/api/inventory/:type_id/esi-holdings",
            get(get_esi_holdings),
        )
        .route(
            "/api/inventory/:type_id/esi-holdings/reconciliation-inclusion",
            put(set_esi_holding_reconciliation_inclusion),
        )
        .route(
            "/api/inventory/opening-balance/preview",
            post(preview_opening_balance),
        )
        .route(
            "/api/inventory/opening-balance",
            post(record_opening_balance),
        )
        .route("/api/inventory/purchases/preview", post(preview_purchase))
        .route("/api/inventory/purchases", post(record_purchase))
        .route(
            "/api/inventory/adjustments/preview",
            post(preview_adjustment),
        )
        .route("/api/inventory/adjustments", post(record_adjustment))
        .route(
            "/api/inventory/:type_id/events/:event_id/reverse",
            post(reverse_inventory_event),
        )
        .route("/api/inventory/export", get(export_inventory))
        .route("/api/inventory/import", post(import_inventory))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InventoryQuery {
    price_source_id: Option<uuid::Uuid>,
    #[serde(default)]
    scope: InventoryListScope,
}

/// `GET /api/inventory`'s two shapes -- see `list_tracked_inventory` and
/// `list_untracked_inventory`. Deliberately not a filter applied after
/// fetching everything: an ESI-heavy multi-character workspace can have
/// far more observed types than accounted ones, so materializing
/// "untracked" phantom rows (their own SDE name lookup, market coverage
/// registration, and per-row market preview) only happens when a caller
/// actually asks for that view.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
enum InventoryListScope {
    #[default]
    Tracked,
    Untracked,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InventoryItemResponse {
    balance: InventoryBalance,
    group_name: Option<String>,
    packaged_volume_m3: Option<String>,
    total_volume_m3: Option<String>,
    reserved_quantity: u64,
    /// Physical minus reserved, unclamped: a shortfall (reserved exceeding
    /// physical stock) surfaces as a real negative number rather than
    /// disappearing at zero -- see `order::reporting_available_quantity`.
    available_quantity: i64,
    cost_quality: CostInputQuality,
    current_price: Option<Money>,
    current_value: Option<Money>,
    historical_difference: Option<MoneyDelta>,
    historical_comparison_complete: bool,
    /// Set only when pricing came from an explicitly-selected `?priceSourceId=`
    /// Manual Price List -- `None` (alongside `marketRegionId`/`marketLocationId`
    /// being set instead) when priced against the workspace's default
    /// `MarketScope`, which has no `PriceSource` identity at all.
    price_source_id: Option<PriceSourceId>,
    price_source_name: Option<String>,
    /// The default-`MarketScope` counterpart to `price_source_id` --
    /// mutually exclusive with it, same reasoning.
    market_region_id: Option<i64>,
    market_location_id: Option<i64>,
    price_source_updated_at: Option<chrono::DateTime<chrono::Utc>>,
    /// `None` means no ESI observation exists for this type -- never
    /// coerced to zero, since ESI never emits a zero-quantity asset row
    /// and a real "observed nothing" and "never observed" are different
    /// facts. See `ProductionRepository::list_esi_observations`.
    esi_observed_quantity: Option<u64>,
    ignored_esi_quantity: Option<u64>,
    included_esi_quantity: Option<u64>,
    esi_observed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// `esi_observed_quantity - balance.quantity`. Present only alongside
    /// an actual observation, for the same reason as above.
    reconciliation_difference: Option<i64>,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InventoryDetailResponse {
    #[serde(flatten)]
    item: InventoryItemResponse,
    events: Vec<iskworks_core::InventoryEvent>,
    /// Every active `inventory_allocations` row for this type, resolved to
    /// its owning Order/Ticket -- the Reservations tab. Only fetched for
    /// the single-item detail response, not the list, since the list can
    /// be hundreds of rows and nothing there needs per-reservation detail
    /// (the list already has `reserved_quantity`, a plain sum).
    reservations: Vec<InventoryReservation>,
}

async fn list_inventory(
    State(state): State<AppState>,
    Query(query): Query<InventoryQuery>,
) -> Result<Json<Vec<InventoryItemResponse>>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let balances = state
        .inventory_repository()?
        .list_balances(workspace_id, owner_id)
        .await?;
    let observations = state.list_esi_observations(workspace_id, owner_id).await?;
    let source = inventory_pricing_source(&state, workspace_id, query.price_source_id).await?;

    let response = match query.scope {
        InventoryListScope::Tracked => {
            list_tracked_inventory(
                &state,
                workspace_id,
                owner_id,
                balances,
                &observations,
                &source,
            )
            .await?
        }
        InventoryListScope::Untracked => {
            list_untracked_inventory(
                &state,
                workspace_id,
                owner_id,
                &balances,
                &observations,
                &source,
            )
            .await?
        }
    };
    Ok(Json(response))
}

/// The default view: every `inventory_balances` row the user has actually
/// accounted for, each enriched with its ESI observation (if any) for the
/// row's Match/discrepancy state. `inventory_balances` drives this query;
/// no ESI-only row is ever unioned in here.
async fn list_tracked_inventory(
    state: &AppState,
    workspace_id: WorkspaceId,
    owner_id: iskworks_core::OwnerId,
    balances: Vec<InventoryBalance>,
    observations: &std::collections::BTreeMap<i64, EsiObservation>,
    source: &InventoryPricingSource,
) -> Result<Vec<InventoryItemResponse>, ApiError> {
    let coverage = inventory_market_coverage(state, workspace_id, source, &balances).await?;
    let coverage_by_type: std::collections::BTreeMap<_, _> = coverage
        .into_iter()
        .map(|item| (item.type_id, item))
        .collect();
    let type_ids = balances
        .iter()
        .map(|balance| balance.key.type_id)
        .collect::<Vec<_>>();
    let type_metadata = state
        .sde_repository
        .inventory_type_metadata(&type_ids)
        .await?;
    // One read per kind for the whole list, never one per row.
    let events_by_type = state
        .inventory_repository()?
        .list_events_by_type(workspace_id, owner_id, &type_ids)
        .await?;
    let reserved_by_type = state
        .reserved_quantities(workspace_id, owner_id, &type_ids)
        .await?;
    let market_previews = inventory_market_previews(state, workspace_id, source, &balances).await?;
    let mut response = Vec::with_capacity(balances.len());
    for (balance, market_preview) in balances.into_iter().zip(market_previews) {
        let events = events_by_type
            .get(&balance.key.type_id)
            .map_or(&[][..], Vec::as_slice);
        let reserved = reserved_by_type
            .get(&balance.key.type_id)
            .copied()
            .unwrap_or(0);
        let market_coverage = coverage_by_type.get(&balance.key.type_id);
        let metadata = type_metadata.get(&balance.key.type_id);
        let observation = observations.get(&balance.key.type_id).copied();
        response.push(inventory_item_response(
            balance,
            events,
            reserved,
            InventoryEnrichment {
                source,
                market_preview: market_preview.as_ref(),
                market_coverage,
                metadata,
                observation,
            },
        )?);
    }
    Ok(response)
}

/// ESI-observed fungible types with no `inventory_balances` row at all --
/// where a brand-new item (the "new item observed by ESI" state) is
/// discovered and, eventually, given a cost basis via Review Discrepancy.
/// The ESI projection drives this query; `inventory_balances` is only
/// consulted to know which observed types to *exclude*.
async fn list_untracked_inventory(
    state: &AppState,
    workspace_id: WorkspaceId,
    owner_id: iskworks_core::OwnerId,
    balances: &[InventoryBalance],
    observations: &std::collections::BTreeMap<i64, EsiObservation>,
    source: &InventoryPricingSource,
) -> Result<Vec<InventoryItemResponse>, ApiError> {
    let known_type_ids: std::collections::BTreeSet<i64> =
        balances.iter().map(|balance| balance.key.type_id).collect();
    let phantom_type_ids: Vec<i64> = observations
        .keys()
        .filter(|type_id| !known_type_ids.contains(type_id))
        .copied()
        .collect();
    let phantom_names = state.sde_repository.type_names(&phantom_type_ids).await?;
    let phantom_balances: Vec<InventoryBalance> = phantom_type_ids
        .iter()
        .map(|&type_id| {
            InventoryBalance::empty(
                InventoryItemKey {
                    workspace_id,
                    owner_id,
                    type_id,
                },
                phantom_name(&phantom_names, type_id),
            )
        })
        .collect();

    let coverage =
        inventory_market_coverage(state, workspace_id, source, &phantom_balances).await?;
    let coverage_by_type: std::collections::BTreeMap<_, _> = coverage
        .into_iter()
        .map(|item| (item.type_id, item))
        .collect();
    let type_metadata = state
        .sde_repository
        .inventory_type_metadata(&phantom_type_ids)
        .await?;
    let market_previews =
        inventory_market_previews(state, workspace_id, source, &phantom_balances).await?;
    let mut response = Vec::with_capacity(phantom_balances.len());
    for (balance, market_preview) in phantom_balances.into_iter().zip(market_previews) {
        let market_coverage = coverage_by_type.get(&balance.key.type_id);
        let metadata = type_metadata.get(&balance.key.type_id);
        let observation = observations.get(&balance.key.type_id).copied();
        response.push(inventory_item_response(
            balance,
            &[],
            0,
            InventoryEnrichment {
                source,
                market_preview: market_preview.as_ref(),
                market_coverage,
                metadata,
                observation,
            },
        )?);
    }
    Ok(response)
}

fn phantom_name(names: &std::collections::BTreeMap<i64, String>, type_id: i64) -> String {
    names
        .get(&type_id)
        .cloned()
        .unwrap_or_else(|| format!("Unknown type {type_id}"))
}

async fn get_inventory_item(
    State(state): State<AppState>,
    Path(type_id): Path<i64>,
    Query(query): Query<InventoryQuery>,
) -> Result<Json<InventoryDetailResponse>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let key = InventoryItemKey {
        workspace_id,
        owner_id,
        type_id,
    };
    let observation = state
        .list_esi_observations(workspace_id, owner_id)
        .await?
        .get(&type_id)
        .copied();
    let history = match state.inventory_repository()?.get_history(&key).await {
        Ok(history) => history,
        // A type ESI observes but Inventory never accounted for -- see the
        // matching comment in `list_inventory`. Only synthesized when an
        // observation actually exists; otherwise this stays a real 404.
        Err(InventoryError::ItemNotFound) if observation.is_some() => {
            let names = state.sde_repository.type_names(&[type_id]).await?;
            InventoryHistory {
                balance: InventoryBalance::empty(key, phantom_name(&names, type_id)),
                events: Vec::new(),
            }
        }
        Err(error) => return Err(error.into()),
    };
    let source = inventory_pricing_source(&state, workspace_id, query.price_source_id).await?;
    let coverage = inventory_market_coverage(
        &state,
        workspace_id,
        &source,
        std::slice::from_ref(&history.balance),
    )
    .await?;
    let market_preview =
        inventory_market_preview(&state, workspace_id, &source, &history.balance).await?;
    let type_metadata = state
        .sde_repository
        .inventory_type_metadata(&[type_id])
        .await?;
    let reservations = state
        .list_reservations(workspace_id, owner_id, type_id)
        .await?;
    Ok(Json(InventoryDetailResponse {
        item: inventory_item_response(
            history.balance,
            &history.events,
            state
                .reserved_quantity(workspace_id, owner_id, type_id)
                .await?,
            InventoryEnrichment {
                source: &source,
                market_preview: market_preview.as_ref(),
                market_coverage: coverage.first(),
                metadata: type_metadata.get(&type_id),
                observation,
            },
        )?,
        events: history.events,
        reservations,
    }))
}

/// The ESI discrepancy drill-down: which characters/locations make up
/// `esiObservedQuantity` on the list/detail response for this type. Not
/// included on either of those responses themselves -- lazy-loaded only
/// when a user actually opens the "View ESI holdings" details affordance,
/// so a large multi-character workspace doesn't pay per-row holdings cost
/// on every Inventory load.
async fn get_esi_holdings(
    State(state): State<AppState>,
    Path(type_id): Path<i64>,
) -> Result<Json<iskworks_core::EsiHoldings>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    Ok(Json(
        state.esi_holdings(workspace_id, owner_id, type_id).await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetEsiHoldingInclusionRequest {
    eve_character_id: i64,
    effective_location_id: i64,
    included: bool,
}

async fn set_esi_holding_reconciliation_inclusion(
    State(state): State<AppState>,
    Path(type_id): Path<i64>,
    Json(request): Json<SetEsiHoldingInclusionRequest>,
) -> Result<Json<iskworks_core::EsiHoldings>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    Ok(Json(
        state
            .set_esi_holding_reconciliation_inclusion(
                workspace_id,
                owner_id,
                iskworks_core::SetEsiHoldingReconciliationInclusion {
                    type_id,
                    eve_character_id: request.eve_character_id,
                    effective_location_id: request.effective_location_id,
                    included: request.included,
                },
            )
            .await?,
    ))
}

async fn preview_opening_balance(
    State(state): State<AppState>,
    Json(command): Json<PostInventoryCommand>,
) -> Result<Json<iskworks_core::InventoryPreview>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    Ok(Json(
        state
            .inventory_service()?
            .preview_opening(workspace_id, owner_id, command)
            .await?,
    ))
}

async fn record_opening_balance(
    State(state): State<AppState>,
    Json(command): Json<PostInventoryCommand>,
) -> Result<(StatusCode, Json<InventoryHistory>), ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let history = state
        .inventory_service()?
        .post_opening(workspace_id, owner_id, command)
        .await?;
    Ok((StatusCode::CREATED, Json(history)))
}

async fn preview_purchase(
    State(state): State<AppState>,
    Json(command): Json<PostInventoryCommand>,
) -> Result<Json<iskworks_core::InventoryPreview>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    Ok(Json(
        state
            .inventory_service()?
            .preview_purchase(workspace_id, owner_id, command)
            .await?,
    ))
}

async fn record_purchase(
    State(state): State<AppState>,
    Json(command): Json<PostInventoryCommand>,
) -> Result<(StatusCode, Json<InventoryHistory>), ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let history = state
        .inventory_service()?
        .post_purchase(workspace_id, owner_id, command)
        .await?;
    Ok((StatusCode::CREATED, Json(history)))
}

async fn preview_adjustment(
    State(state): State<AppState>,
    Json(command): Json<PostAdjustmentCommand>,
) -> Result<Json<iskworks_core::InventoryPreview>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    Ok(Json(
        state
            .inventory_service()?
            .preview_adjustment(workspace_id, owner_id, command)
            .await?,
    ))
}

async fn record_adjustment(
    State(state): State<AppState>,
    Json(command): Json<PostAdjustmentCommand>,
) -> Result<(StatusCode, Json<InventoryHistory>), ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let history = state
        .inventory_service()?
        .post_adjustment(workspace_id, owner_id, command)
        .await?;
    Ok((StatusCode::CREATED, Json(history)))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InventoryExportItem {
    type_id: i64,
    type_name: String,
    quantity: u64,
    average_unit_cost: Option<Money>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InventoryExport {
    exported_at: chrono::DateTime<chrono::Utc>,
    items: Vec<InventoryExportItem>,
}

async fn export_inventory(
    State(state): State<AppState>,
) -> Result<Json<InventoryExport>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let balances = state
        .inventory_repository()?
        .list_balances(workspace_id, owner_id)
        .await?;
    let items = balances
        .into_iter()
        .filter(|balance| balance.quantity > 0)
        .map(|balance| InventoryExportItem {
            type_id: balance.key.type_id,
            type_name: balance.type_name.clone(),
            quantity: balance.quantity,
            average_unit_cost: balance.average_unit_cost,
        })
        .collect();
    Ok(Json(InventoryExport {
        exported_at: chrono::Utc::now(),
        items,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InventoryImportItem {
    type_id: i64,
    type_name: String,
    quantity: u64,
    average_unit_cost: Option<Money>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InventoryImportRequest {
    items: Vec<InventoryImportItem>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InventoryImportItemResult {
    type_id: i64,
    type_name: String,
    imported: bool,
    message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InventoryImportResponse {
    results: Vec<InventoryImportItemResult>,
}

async fn import_inventory(
    State(state): State<AppState>,
    Json(request): Json<InventoryImportRequest>,
) -> Result<Json<InventoryImportResponse>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    let service = state.inventory_service()?;
    let mut results = Vec::with_capacity(request.items.len());
    for item in request.items {
        results.push(import_inventory_item(&service, workspace_id, owner_id, item).await);
    }
    Ok(Json(InventoryImportResponse { results }))
}

/// Reconstructs one exported item as a single opening balance. Every
/// positive accounted quantity requires a cost basis, so an import
/// item with no `average_unit_cost` is rejected rather than silently
/// imported as unknown-cost -- the same invariant a manual Opening Balance
/// posting already enforces.
async fn import_inventory_item(
    service: &iskworks_core::InventoryService,
    workspace_id: WorkspaceId,
    owner_id: iskworks_core::OwnerId,
    item: InventoryImportItem,
) -> InventoryImportItemResult {
    let type_id = item.type_id;
    let type_name = item.type_name.clone();
    if item.quantity == 0 {
        return InventoryImportItemResult {
            type_id,
            type_name,
            imported: false,
            message: Some("Nothing to import: quantity is zero.".to_string()),
        };
    }
    let Some(average_unit_cost) = item.average_unit_cost else {
        return InventoryImportItemResult {
            type_id,
            type_name,
            imported: false,
            message: Some("A unit cost is required to import this item.".to_string()),
        };
    };

    let command = iskworks_core::PostInventoryCommand {
        type_id: item.type_id,
        type_name: item.type_name,
        quantity: item.quantity,
        unit_cost: Some(average_unit_cost.0.to_string()),
        cost_quality: CostInputQuality::Known,
        source_reference: "Inventory import".to_string(),
        note: String::new(),
        effective_at: chrono::Utc::now(),
        expected_revision: 0,
        acknowledge_zero_cost: false,
    };
    match service.post_opening(workspace_id, owner_id, command).await {
        Ok(_) => InventoryImportItemResult {
            type_id,
            type_name,
            imported: true,
            message: None,
        },
        Err(error) => InventoryImportItemResult {
            type_id,
            type_name,
            imported: false,
            message: Some(crate::error::public_outcome_message(error)),
        },
    }
}

async fn reverse_inventory_event(
    State(state): State<AppState>,
    Path((type_id, event_id)): Path<(i64, uuid::Uuid)>,
    Json(command): Json<ReverseInventoryCommand>,
) -> Result<Json<InventoryHistory>, ApiError> {
    let (workspace_id, owner_id) = workspace_context(&state).await?;
    Ok(Json(
        state
            .inventory_repository()?
            .reverse_latest(
                &InventoryItemKey {
                    workspace_id,
                    owner_id,
                    type_id,
                },
                InventoryEventId(event_id),
                command.expected_revision,
                command.reason,
            )
            .await?,
    ))
}

/// Which market this row's pricing came from -- an explicit `?priceSourceId=`
/// selection (a Manual Price List, or a market-derived `PriceSource`) uses
/// the `PriceSource`-shaped path; with no explicit selection, the default is
/// the workspace's own `MarketScope` -- no `PriceSource` identity involved
/// at all. `Unavailable` is the "pricing infrastructure isn't wired up"
/// fallback (e.g. a test harness or deployment that configures storage
/// without market pricing): rows are returned unpriced.
enum InventoryPricingSource {
    Explicit(PriceSource),
    DefaultScope(MarketScope),
    Unavailable,
}

async fn inventory_pricing_source(
    state: &AppState,
    workspace_id: WorkspaceId,
    requested: Option<uuid::Uuid>,
) -> Result<InventoryPricingSource, ApiError> {
    if let Some(id) = requested {
        let Some(repository) = state.industry_repository.clone() else {
            return Ok(InventoryPricingSource::Unavailable);
        };
        return Ok(InventoryPricingSource::Explicit(
            repository
                .get_price_source(workspace_id, PriceSourceId(id))
                .await?,
        ));
    }
    if state.market_repository.is_none() {
        return Ok(InventoryPricingSource::Unavailable);
    }
    Ok(InventoryPricingSource::DefaultScope(
        state.workspace_default_market_scope(workspace_id).await?,
    ))
}

async fn inventory_market_coverage(
    state: &AppState,
    workspace_id: WorkspaceId,
    source: &InventoryPricingSource,
    balances: &[InventoryBalance],
) -> Result<Vec<MarketCoverageItem>, ApiError> {
    // Only an explicitly selected `PriceSource` of kind
    // `esi_market_orders` auto-registers/refreshes coverage (at that
    // source's own configured location). `MarketScope` reads never
    // implicitly register or fetch coverage: only an explicit per-item
    // action may do that, and Inventory has no such action, so the
    // default-scope path returns no coverage rather than fetching
    // implicitly.
    let InventoryPricingSource::Explicit(source) = source else {
        return Ok(Vec::new());
    };
    if source.kind != PriceSourceKind::EsiMarketOrders {
        return Ok(Vec::new());
    }
    Ok(state
        .public_market_service()?
        .register_and_refresh(
            workspace_id,
            source.id,
            balances
                .iter()
                .map(|balance| iskworks_core::MarketCoverageRegistration {
                    type_id: balance.key.type_id,
                    type_name: balance.type_name.clone(),
                })
                .collect(),
        )
        .await?)
}

/// The subset of a resolved market price preview that `inventory_item_response`
/// (and the warnings it derives) actually reads -- unified across both
/// pricing paths so that code doesn't need to know which one produced it.
struct InventoryMarketPreview {
    depth: MarketDepthResult,
    freshness: MarketFreshnessState,
    observed_at: DateTime<Utc>,
}

async fn inventory_market_preview(
    state: &AppState,
    workspace_id: WorkspaceId,
    source: &InventoryPricingSource,
    balance: &InventoryBalance,
) -> Result<Option<InventoryMarketPreview>, ApiError> {
    match source {
        InventoryPricingSource::Explicit(source) if source.kind.uses_order_book() => {
            match state
                .market_service()?
                .preview_price(
                    workspace_id,
                    source.id,
                    MarketPricePreviewCommand {
                        type_id: balance.key.type_id,
                        requested_quantity: balance.quantity.max(1),
                    },
                )
                .await
            {
                Ok(preview) => Ok(Some(InventoryMarketPreview {
                    depth: preview.depth,
                    freshness: preview.freshness,
                    observed_at: preview.observed_at,
                })),
                Err(MarketError::OrdersUnavailable) => Ok(None),
                Err(error) => Err(error.into()),
            }
        }
        InventoryPricingSource::Explicit(_) | InventoryPricingSource::Unavailable => Ok(None),
        InventoryPricingSource::DefaultScope(scope) => {
            inventory_scope_preview(state, workspace_id, *scope, balance).await
        }
    }
}

/// `inventory_market_preview` for every balance of a list, aligned with
/// `balances`: each pricing path reads its order books once for the whole
/// list (`MarketService::preview_prices` / one `scoped_order_books` call)
/// instead of once per row. Every row resolves exactly as it would alone.
async fn inventory_market_previews(
    state: &AppState,
    workspace_id: WorkspaceId,
    source: &InventoryPricingSource,
    balances: &[InventoryBalance],
) -> Result<Vec<Option<InventoryMarketPreview>>, ApiError> {
    if balances.is_empty() {
        return Ok(Vec::new());
    }
    match source {
        InventoryPricingSource::Explicit(source) if source.kind.uses_order_book() => {
            let commands: Vec<MarketPricePreviewCommand> = balances
                .iter()
                .map(|balance| MarketPricePreviewCommand {
                    type_id: balance.key.type_id,
                    requested_quantity: balance.quantity.max(1),
                })
                .collect();
            Ok(state
                .market_service()?
                .preview_prices(workspace_id, source.id, &commands)
                .await?
                .into_iter()
                .map(|preview| {
                    preview.map(|preview| InventoryMarketPreview {
                        depth: preview.depth,
                        freshness: preview.freshness,
                        observed_at: preview.observed_at,
                    })
                })
                .collect())
        }
        InventoryPricingSource::Explicit(_) | InventoryPricingSource::Unavailable => {
            Ok(balances.iter().map(|_| None).collect())
        }
        InventoryPricingSource::DefaultScope(scope) => {
            let type_ids: Vec<i64> = balances.iter().map(|balance| balance.key.type_id).collect();
            let books = state
                .market_repository()?
                .scoped_order_books(workspace_id, *scope, &type_ids)
                .await?;
            balances
                .iter()
                .map(|balance| scope_preview_from_books(&books, balance))
                .collect()
        }
    }
}

/// The default-`MarketScope` counterpart to `MarketService::preview_price`
/// -- no `PriceSource` involved, reading directly
/// through `scoped_order_books` the same way Build/Order pricing and the
/// Market Browser do, and reusing the shared `resolve_market_price_items`
/// for the actual depth-to-price resolution. A pure
/// read like every other `MarketScope` read: never registers or refreshes
/// coverage itself (see the comment in `inventory_market_coverage`).
///
/// `HighestBuy` is the valuation policy -- "what this balance could
/// currently be liquidated for" -- matching Build's own material-default
/// convention (`default_material_pricing_policy`) rather than inventing a
/// third default pair; `require_full_coverage: false` mirrors Build/Order
/// pricing's "best-available depth is still data" convention (Opportunities'
/// stricter one doesn't apply here -- see `resolve_market_price_items`'s
/// own doc on that fork).
async fn inventory_scope_preview(
    state: &AppState,
    workspace_id: WorkspaceId,
    scope: MarketScope,
    balance: &InventoryBalance,
) -> Result<Option<InventoryMarketPreview>, ApiError> {
    let books = state
        .market_repository()?
        .scoped_order_books(workspace_id, scope, &[balance.key.type_id])
        .await?;
    scope_preview_from_books(&books, balance)
}

/// The pure half of `inventory_scope_preview`: resolves one balance's
/// preview from already-read `scoped_order_books` output (one type's books
/// or a whole list's -- each type resolves only from its own orders).
fn scope_preview_from_books(
    books: &std::collections::BTreeMap<i64, Vec<iskworks_core::MarketOrderView>>,
    balance: &InventoryBalance,
) -> Result<Option<InventoryMarketPreview>, ApiError> {
    const FRESH_AFTER_HOURS: u32 = 1;
    const STALE_AFTER_HOURS: u32 = 24;

    let type_id = balance.key.type_id;
    let Some(observed_at) = books
        .get(&type_id)
        .and_then(|orders| orders.iter().map(|order| order.observed_at).max())
    else {
        return Ok(None);
    };
    let now = Utc::now();
    let resolution = resolve_market_price_items(
        &[MarketPriceRequest {
            type_id,
            type_name: balance.type_name.clone(),
            requested_quantity: balance.quantity.max(1),
            pricing_policy: MarketPricingPolicy::HighestBuy,
        }],
        |lookup_type_id| books.get(&lookup_type_id).map(Vec::as_slice),
        now,
        FRESH_AFTER_HOURS,
        STALE_AFTER_HOURS,
        false,
    )?;
    let Some(depth) = resolution.depth.into_values().next() else {
        return Ok(None);
    };
    let freshness =
        iskworks_core::market_freshness(observed_at, now, FRESH_AFTER_HOURS, STALE_AFTER_HOURS);
    Ok(Some(InventoryMarketPreview {
        depth,
        freshness,
        observed_at,
    }))
}

/// Everything used to enrich a bare `InventoryBalance` into an
/// `InventoryItemResponse` beyond its own identity/domain data (balance,
/// events, reserved quantity) -- grouped into one struct purely to keep
/// `inventory_item_response` under clippy's argument-count lint; there's
/// no shared lifetime/ownership reason for the grouping.
struct InventoryEnrichment<'a> {
    source: &'a InventoryPricingSource,
    market_preview: Option<&'a InventoryMarketPreview>,
    market_coverage: Option<&'a MarketCoverageItem>,
    metadata: Option<&'a SdeInventoryTypeMetadata>,
    observation: Option<EsiObservation>,
}

fn inventory_item_response(
    balance: InventoryBalance,
    events: &[InventoryEvent],
    reserved_quantity: u64,
    enrichment: InventoryEnrichment<'_>,
) -> Result<InventoryItemResponse, ApiError> {
    let InventoryEnrichment {
        source,
        market_preview,
        market_coverage,
        metadata,
        observation,
    } = enrichment;
    let explicit_source = match source {
        InventoryPricingSource::Explicit(source) => Some(source),
        InventoryPricingSource::DefaultScope(_) | InventoryPricingSource::Unavailable => None,
    };
    let default_scope = match source {
        InventoryPricingSource::DefaultScope(scope) => Some(*scope),
        InventoryPricingSource::Explicit(_) | InventoryPricingSource::Unavailable => None,
    };
    let is_market_source = explicit_source.is_some_and(|source| source.kind.uses_order_book());
    let is_esi_market_source =
        explicit_source.is_some_and(|source| source.kind == PriceSourceKind::EsiMarketOrders);
    let price_item = explicit_source
        .filter(|_| !is_market_source)
        .and_then(|source| {
            source
                .items
                .iter()
                .find(|item| item.type_id == balance.key.type_id)
        });
    let current_price = market_preview
        .and_then(|preview| preview.depth.average_unit_price)
        .or_else(|| price_item.map(|item| item.price));
    let current_value = if let Some(preview) = market_preview {
        if !preview.depth.fully_covered {
            None
        } else if balance.quantity == 0 {
            Some(Money::zero())
        } else if matches!(
            preview.depth.policy,
            MarketPricingPolicy::AcquireQuantityFromSellOrders
                | MarketPricingPolicy::LiquidateQuantityIntoBuyOrders
        ) {
            Some(preview.depth.total)
        } else {
            current_price
                .map(|price| price.checked_mul_quantity(balance.quantity))
                .transpose()?
        }
    } else {
        current_price
            .map(|price| price.checked_mul_quantity(balance.quantity))
            .transpose()?
    };
    // Every positive accounted quantity has a resolvable cost basis, so
    // this comparison is always complete; the field stays in the response
    // because it is part of the API contract.
    let historical_comparison_complete = true;
    let historical_difference =
        current_value.map(|value| MoneyDelta(value.0 - balance.total_historical_cost.0));
    let mut warnings = Vec::new();
    let active_events: Vec<_> = events
        .iter()
        .filter(|event| {
            event.kind != InventoryEventKind::Reversal && event.reversed_by_event_id.is_none()
        })
        .collect();
    let has_estimated = active_events
        .iter()
        .any(|event| event.cost_quality == CostInputQuality::Estimated);
    let has_zero_cost = active_events
        .iter()
        .any(|event| event.cost_quality == CostInputQuality::ZeroCost);
    if has_estimated {
        warnings.push(
            "Part of this balance was recorded at an estimated cost because no actual cost was entered, so its average cost is approximate."
                .to_string(),
        );
    }
    if has_zero_cost {
        warnings.push("This inventory includes quantity explicitly recorded at zero cost. Future accounting profit may appear unusually high.".to_string());
    }
    if let Some(preview) = market_preview {
        if !preview.depth.fully_covered {
            warnings.push(format!(
                "Market orders cover {} of {} units. Current value is unavailable for the full balance.",
                preview.depth.covered_quantity, balance.quantity
            ));
        }
        match preview.freshness {
            MarketFreshnessState::Aging => warnings.push(
                "The selected market observations are aging; consider refreshing the Price Source."
                    .to_string(),
            ),
            MarketFreshnessState::Stale => warnings.push(
                "The selected market observations are stale; current value may no longer reflect the market."
                    .to_string(),
            ),
            MarketFreshnessState::Fresh | MarketFreshnessState::Unavailable => {}
        }
    }
    if is_esi_market_source {
        if let (Some(source), Some(coverage)) = (explicit_source, market_coverage) {
            let market = &source.name;
            match coverage.refresh_state {
                MarketRefreshState::Missing | MarketRefreshState::Refreshing => warnings.push(format!(
                    "{market} market prices are being refreshed; current value will appear when observations are available."
                )),
                MarketRefreshState::Failed => warnings.push(if market_preview.is_some() {
                    format!(
                        "The latest {market} refresh failed; current value uses the previous successful observations."
                    )
                } else {
                    format!("{market} market prices could not be refreshed; current value is unavailable.")
                }),
                MarketRefreshState::Current if market_preview.is_none() => warnings.push(format!(
                    "No compatible {market} market orders are available for this item."
                )),
                MarketRefreshState::Current => {}
            }
        }
    } else if explicit_source.is_some() && current_price.is_none() {
        warnings.push(if is_market_source {
            "The selected market Price Source has no compatible orders for this item.".to_string()
        } else {
            "The selected Price Source has no current price for this item.".to_string()
        });
    } else if default_scope.is_some() && current_price.is_none() {
        warnings.push(
            "No market orders are available for this item in the default market scope.".to_string(),
        );
    }
    let reconciliation_difference = observation
        .map(|observation| observation.included_quantity as i64 - balance.quantity as i64);
    Ok(InventoryItemResponse {
        available_quantity: iskworks_core::order::reporting_available_quantity(
            balance.quantity,
            reserved_quantity,
        ),
        group_name: metadata.and_then(|item| item.group_name.clone()),
        packaged_volume_m3: metadata
            .and_then(|item| item.packaged_volume_m3)
            .map(|volume| volume.to_string()),
        total_volume_m3: metadata
            .and_then(|item| item.packaged_volume_m3)
            .map(|volume| (volume * rust_decimal::Decimal::from(balance.quantity)).to_string()),
        reserved_quantity,
        cost_quality: if has_estimated {
            CostInputQuality::Estimated
        } else if has_zero_cost
            && active_events
                .iter()
                .all(|event| event.cost_quality == CostInputQuality::ZeroCost)
        {
            CostInputQuality::ZeroCost
        } else {
            CostInputQuality::Known
        },
        balance,
        current_price,
        current_value,
        historical_difference,
        historical_comparison_complete,
        price_source_id: explicit_source.map(|item| item.id),
        price_source_name: explicit_source.map(|item| item.name.clone()),
        market_region_id: default_scope.map(|scope| scope.region_id),
        market_location_id: default_scope.and_then(|scope| scope.location_id),
        price_source_updated_at: market_preview
            .map(|preview| preview.observed_at)
            .or_else(|| market_coverage.and_then(|coverage| coverage.observed_at))
            .or_else(|| explicit_source.map(|item| item.updated_at)),
        esi_observed_quantity: observation.map(|observation| observation.quantity),
        ignored_esi_quantity: observation.map(|observation| observation.ignored_quantity),
        included_esi_quantity: observation.map(|observation| observation.included_quantity),
        esi_observed_at: observation.map(|observation| observation.observed_at),
        reconciliation_difference,
        warnings,
    })
}

#[cfg(test)]
mod tests;
