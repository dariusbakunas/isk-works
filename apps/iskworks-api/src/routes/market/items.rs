//! Market item summary + order book: the paginated item summary table, a
//! cheap page-freshness poll signal, and a single item's full order book.
//!
//! Scope-based reads over the existing observation/coverage storage, via a
//! temporary internal shim (`MarketRepository::scoped_order_books`) that
//! resolves a `MarketScope` to whichever `market_price_source_configs` rows
//! already cover it. Never registers coverage or
//! triggers an ESI fetch: both endpoints are pure reads, and the item
//! summary table only ever asks for market data on the current page's
//! types, never the whole catalog.

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use iskworks_core::MarketError;
use serde::{Deserialize, Serialize};

use crate::{workspace_context, ApiError, AppState};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarketItemsQuery {
    region_id: i64,
    location_id: Option<i64>,
    market_group_id: Option<i64>,
    #[serde(default)]
    search: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
}

async fn list_market_items(
    State(state): State<AppState>,
    Query(query): Query<MarketItemsQuery>,
) -> Result<Json<iskworks_core::MarketItemPage>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let filter = iskworks_core::MarketItemFilter {
        scope: iskworks_core::MarketScope {
            region_id: query.region_id,
            location_id: query.location_id,
        },
        market_group_id: query.market_group_id,
        search: query.search,
        page: query.page.unwrap_or(1),
        page_size: query
            .page_size
            .unwrap_or(iskworks_core::DEFAULT_MARKET_ITEM_PAGE_SIZE),
    }
    .validate()?;

    let (candidates, total_count) = state
        .sde_repository
        .list_market_items(
            filter.market_group_id,
            filter.search.as_deref().unwrap_or(""),
            filter.page,
            filter.page_size,
        )
        .await?;
    let type_ids: Vec<i64> = candidates.iter().map(|item| item.type_id).collect();
    let market_data = state
        .market_repository()?
        .scoped_order_books(workspace_id, filter.scope, &type_ids)
        .await?;

    let rows = candidates
        .into_iter()
        .map(|item| {
            let data = market_data
                .get(&item.type_id)
                .map(|orders| iskworks_core::summarize_scoped_orders(orders))
                .unwrap_or_default();
            iskworks_core::MarketItemSummary {
                type_id: item.type_id,
                type_name: item.type_name,
                best_sell: data.best_sell,
                best_buy: data.best_buy,
                spread: data.spread,
                sell_order_count: data.sell_order_count,
                buy_order_count: data.buy_order_count,
                observed_at: data.observed_at,
            }
        })
        .collect();

    Ok(Json(iskworks_core::MarketItemPage {
        rows,
        total_count,
        page: filter.page,
        page_size: filter.page_size,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarketItemsFreshnessQuery {
    region_id: i64,
    location_id: Option<i64>,
    /// Comma-separated type_ids -- always the currently-rendered page's
    /// ids in practice, so a simple query-string list is enough; never
    /// needs its own request body.
    type_ids: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MarketItemsFreshnessResponse {
    most_recent_updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn parse_freshness_type_ids(raw: &str) -> Result<Vec<i64>, MarketError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<i64> = trimmed
        .split(',')
        .map(|part| {
            part.trim().parse::<i64>().map_err(|_| {
                MarketError::Validation(
                    "typeIds must be a comma-separated list of integers".to_string(),
                )
            })
        })
        .collect::<Result<_, _>>()?;
    if ids.len() > iskworks_core::MAX_MARKET_ITEM_PAGE_SIZE as usize {
        return Err(MarketError::Validation(format!(
            "typeIds must not list more than {} items",
            iskworks_core::MAX_MARKET_ITEM_PAGE_SIZE
        )));
    }
    Ok(ids)
}

/// The Market Browser's live-row-refresh polling signal: a cheap scalar
/// check for "has anything changed" over exactly the ids the client is
/// currently rendering, so it never has to re-run `list_market_items`'s
/// full paginated/joined query just to find out nothing changed yet.
async fn get_market_items_freshness(
    State(state): State<AppState>,
    Query(query): Query<MarketItemsFreshnessQuery>,
) -> Result<Json<MarketItemsFreshnessResponse>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let type_ids = parse_freshness_type_ids(&query.type_ids)?;
    let scope = iskworks_core::MarketScope {
        region_id: query.region_id,
        location_id: query.location_id,
    };
    let most_recent_updated_at = state
        .market_repository()?
        .item_freshness(workspace_id, scope, &type_ids)
        .await?;
    Ok(Json(MarketItemsFreshnessResponse {
        most_recent_updated_at,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarketItemOrdersQuery {
    region_id: i64,
    location_id: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MarketOrderRowResponse {
    price: iskworks_core::Money,
    quantity: u64,
    min_quantity: u64,
    location_id: i64,
    /// Resolved station/structure name (NPC stations from the SDE,
    /// including their solar system's security status; player structures
    /// from `market_location_names`) -- a raw numeric id when neither
    /// source has it, which is the graceful fallback, not the expected
    /// normal case.
    location_name: String,
    /// ESI's `range` field rendered as a short label ("Station"/"System"/
    /// "Region"/"N jumps") -- meaningful for buy orders only, but computed
    /// for both sides so the frontend doesn't need a second lookup.
    order_range: String,
    observed_at: chrono::DateTime<chrono::Utc>,
    /// `issued_at + duration_days`, computed here rather than shipping both
    /// raw fields so the frontend doesn't need to duplicate this math.
    expires_at: chrono::DateTime<chrono::Utc>,
}

impl MarketOrderRowResponse {
    fn from_order(
        order: &iskworks_core::MarketOrderView,
        location_names: &std::collections::BTreeMap<i64, String>,
    ) -> Self {
        Self {
            price: order.price,
            quantity: order.remaining_volume,
            min_quantity: order.minimum_volume,
            location_id: order.location_id,
            location_name: location_names
                .get(&order.location_id)
                .cloned()
                .unwrap_or_else(|| format!("Location {}", order.location_id)),
            order_range: iskworks_core::format_order_range(order.order_range),
            observed_at: order.observed_at,
            expires_at: order.issued_at + chrono::Duration::days(i64::from(order.duration_days)),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MarketItemOrdersResponse {
    type_id: i64,
    type_name: Option<String>,
    /// The type's SDE market group -- lets the frontend resolve a
    /// category/group breadcrumb from the category tree it already has
    /// loaded, without a second dedicated endpoint. `None` when the type
    /// isn't a market-browsable item or classification metadata is
    /// unavailable.
    market_group_id: Option<i64>,
    summary: iskworks_core::MarketItemMarketData,
    sell_orders: Vec<MarketOrderRowResponse>,
    buy_orders: Vec<MarketOrderRowResponse>,
}

async fn get_market_item_orders(
    State(state): State<AppState>,
    Path(type_id): Path<i64>,
    Query(query): Query<MarketItemOrdersQuery>,
) -> Result<Json<MarketItemOrdersResponse>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let scope = iskworks_core::MarketScope {
        region_id: query.region_id,
        location_id: query.location_id,
    };
    let market_repository = state.market_repository()?;
    let orders = market_repository
        .scoped_order_books(workspace_id, scope, &[type_id])
        .await?
        .remove(&type_id)
        .unwrap_or_default();
    let type_name = market_repository.resolve_type_name(type_id).await?;
    let market_group_id = market_repository.resolve_type_market_group(type_id).await?;
    let summary = iskworks_core::summarize_scoped_orders(&orders);
    let location_ids: Vec<i64> = orders
        .iter()
        .map(|order| order.location_id)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let location_names = market_repository
        .resolve_order_location_names(workspace_id, &location_ids)
        .await?;
    let sell_orders =
        iskworks_core::applicable_orders_sorted(&orders, iskworks_core::MarketOrderSide::Sell)
            .into_iter()
            .map(|order| MarketOrderRowResponse::from_order(order, &location_names))
            .collect();
    let buy_orders =
        iskworks_core::applicable_orders_sorted(&orders, iskworks_core::MarketOrderSide::Buy)
            .into_iter()
            .map(|order| MarketOrderRowResponse::from_order(order, &location_names))
            .collect();

    Ok(Json(MarketItemOrdersResponse {
        type_id,
        type_name,
        market_group_id,
        summary,
        sell_orders,
        buy_orders,
    }))
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/market/items", get(list_market_items))
        .route(
            "/api/market/items/freshness",
            get(get_market_items_freshness),
        )
        .route(
            "/api/market/items/:type_id/orders",
            get(get_market_item_orders),
        )
}
