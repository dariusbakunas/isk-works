//! On-demand ESI market-data refresh requests: a single item, or every
//! published type in a market-group subtree. Both reuse the existing
//! `register_and_refresh` mechanism (bounded, background) via
//! `ensure_esi_price_source_for_scope`.

use axum::extract::{Path, Query, State};
use axum::routing::post;
use axum::{Json, Router};
use iskworks_core::MarketError;
use serde::{Deserialize, Serialize};

use crate::{workspace_context, ApiError, AppState};

/// Refuses a caller-supplied scope ESI would reject. Once registered,
/// coverage for a bogus region/location is retried against ESI on every
/// worker pass, and each failure spends the per-IP error budget every tenant
/// shares. The region must be one the Market Scope Selector offers; a
/// location must be an NPC station in that region or a structure this
/// workspace already knows there.
async fn validate_request_scope(
    state: &AppState,
    workspace_id: iskworks_core::WorkspaceId,
    region_id: i64,
    location_id: Option<i64>,
) -> Result<(), ApiError> {
    let known_region = state
        .sde_repository
        .list_regions()
        .await?
        .iter()
        .any(|region| region.region_id == region_id);
    if !known_region {
        return Err(MarketError::Validation(format!("{region_id} is not a market region.")).into());
    }
    let Some(location_id) = location_id else {
        return Ok(());
    };
    let npc_station_in_region = state
        .sde_repository
        .resolve_npc_stations(&[location_id])
        .await?
        .iter()
        .any(|station| station.station_id == location_id && station.region_id == region_id);
    if npc_station_in_region {
        return Ok(());
    }
    let known_structure_in_region = state
        .market_repository()?
        .known_locations_in_region(workspace_id, region_id)
        .await?
        .iter()
        .any(|location| location.location_id == location_id);
    if known_structure_in_region {
        return Ok(());
    }
    Err(MarketError::Validation(format!(
        "{location_id} is not a known market location in region {region_id}."
    ))
    .into())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RequestMarketDataQuery {
    region_id: i64,
    location_id: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RequestMarketDataResponse {
    requested: bool,
    sources_notified: u32,
}

/// "Request market data": bounded, on-demand, reuses the existing
/// `register_and_refresh` mechanism Build/Inventory already rely on (bumps
/// coverage for exactly this one type, spawns a bounded background
/// refresh) -- never a scan, never anything beyond what was asked for.
/// Works for any scope, not just Jita: `ensure_esi_price_source_for_scope`
/// gets or creates the workspace's per-scope `esi_market_orders` coverage
/// anchor for whatever region/location the caller asks for, so no source
/// has to exist beforehand (a workspace may hold one such source per
/// scope).
async fn request_market_data(
    State(state): State<AppState>,
    Path(type_id): Path<i64>,
    Query(query): Query<RequestMarketDataQuery>,
) -> Result<Json<RequestMarketDataResponse>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    validate_request_scope(&state, workspace_id, query.region_id, query.location_id).await?;
    let market_repository = state.market_repository()?;
    let type_name = market_repository
        .resolve_type_name(type_id)
        .await?
        .ok_or(ApiError::StaticDataEntryNotFound)?;
    let scope = iskworks_core::MarketScope {
        region_id: query.region_id,
        location_id: query.location_id,
    };
    let source_id = market_repository
        .ensure_esi_price_source_for_scope(workspace_id, scope)
        .await?;
    let coverage = state
        .public_market_service()?
        .register_and_refresh_now(
            workspace_id,
            source_id,
            vec![iskworks_core::MarketCoverageRegistration { type_id, type_name }],
        )
        .await?;
    if let Some(item) = coverage.into_iter().find(|item| item.type_id == type_id) {
        if item.refresh_state == iskworks_core::MarketRefreshState::Failed {
            return Err(MarketError::RefreshFailed(
                item.last_error
                    .unwrap_or_else(|| "market price refresh failed".to_string()),
            )
            .into());
        }
    }
    Ok(Json(RequestMarketDataResponse {
        requested: true,
        sources_notified: 1,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RequestMarketDataForGroupQuery {
    region_id: i64,
    location_id: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RequestMarketDataForGroupResponse {
    /// Every published type found in `market_group_id`'s subtree --
    /// descendant-inclusive, matching what the category tree already shows
    /// as this node's rolled-up `itemCount`.
    requested_count: u32,
    /// How many of those already had a completed refresh before this call
    /// (register's `ON CONFLICT` update never resets `refresh_state`, so
    /// this is a real, cheap snapshot, not an estimate). The remainder is
    /// queued: `register_and_refresh` immediately dispatches up to its own
    /// `item_batch_size`, and the rest drains via the worker's normal
    /// polling loop over the following ticks -- there is no separate
    /// tracked job, so "queued" is simply `requestedCount -
    /// alreadyCurrentCount` and isn't re-reported as it drains.
    already_current_count: u32,
}

/// Bulk sibling of `request_market_data`: resolves every published type in
/// `market_group_id`'s subtree and registers coverage for all of them in
/// one `register_and_refresh` call, rather than requiring one click per
/// item. Unbounded up to `MAX_MARKET_GROUP_BULK_REQUEST_ITEMS` (a
/// defensive ceiling far above any real category size, not a product
/// cap -- see that constant's doc comment); `register_and_refresh` itself
/// already keeps the immediate synchronous ESI fetch bounded regardless of
/// how many items are registered, and also prioritizes whatever it
/// just registered so a later, smaller request isn't stuck behind an
/// earlier large category's backlog.
async fn request_market_data_for_group(
    State(state): State<AppState>,
    Path(market_group_id): Path<i64>,
    Query(query): Query<RequestMarketDataForGroupQuery>,
) -> Result<Json<RequestMarketDataForGroupResponse>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    validate_request_scope(&state, workspace_id, query.region_id, query.location_id).await?;
    let items = state
        .sde_repository
        .list_market_group_subtree_item_ids(market_group_id)
        .await?;
    if items.len() > iskworks_core::MAX_MARKET_GROUP_BULK_REQUEST_ITEMS {
        return Err(MarketError::Validation(format!(
            "category has {} items, which exceeds the {}-item bulk-request limit",
            items.len(),
            iskworks_core::MAX_MARKET_GROUP_BULK_REQUEST_ITEMS
        ))
        .into());
    }
    let requested_count = items.len() as u32;
    let scope = iskworks_core::MarketScope {
        region_id: query.region_id,
        location_id: query.location_id,
    };
    let market_repository = state.market_repository()?;
    let source_id = market_repository
        .ensure_esi_price_source_for_scope(workspace_id, scope)
        .await?;
    let registrations = items
        .into_iter()
        .map(|item| iskworks_core::MarketCoverageRegistration {
            type_id: item.type_id,
            type_name: item.type_name,
        })
        .collect::<Vec<_>>();
    let coverage = state
        .public_market_service()?
        .register_and_refresh(workspace_id, source_id, registrations)
        .await?;
    let already_current_count = coverage
        .iter()
        .filter(|item| item.refresh_state == iskworks_core::MarketRefreshState::Current)
        .count() as u32;
    Ok(Json(RequestMarketDataForGroupResponse {
        requested_count,
        already_current_count,
    }))
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/market/items/:type_id/request",
            post(request_market_data),
        )
        .route(
            "/api/market/groups/:market_group_id/request",
            post(request_market_data_for_group),
        )
}
