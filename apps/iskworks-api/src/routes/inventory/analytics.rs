//! Inventory › Runway and › Consumption: read-only analytics over
//! the inventory ledger and the Epics that draw on it.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{Duration, NaiveDate, Utc};
use iskworks_core::{
    build_consumption, build_runway, ConsumptionQuery, Granularity, InventoryConsumption,
    InventoryError, InventoryRunway, MaterialCategory, RunwayFilter, MAX_CONSUMPTION_RANGE_DAYS,
    MAX_DAILY_RANGE_DAYS,
};
use serde::Deserialize;

use crate::{workspace_context, ApiError, AppState};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/inventory/runway", get(get_runway))
        .route("/api/inventory/consumption", get(get_consumption))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunwayParams {
    category: Option<String>,
}

fn parse_category(value: Option<&str>) -> Result<Option<MaterialCategory>, InventoryError> {
    value
        .filter(|value| !value.is_empty())
        .map(str::parse::<MaterialCategory>)
        .transpose()
        .map_err(InventoryError::Validation)
}

fn parse_date(value: Option<&str>, name: &str) -> Result<Option<NaiveDate>, InventoryError> {
    value
        .filter(|value| !value.is_empty())
        .map(|value| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|_| InventoryError::Validation(format!("{name} must be YYYY-MM-DD")))
        })
        .transpose()
}

impl RunwayParams {
    fn filter(&self) -> Result<RunwayFilter, InventoryError> {
        Ok(RunwayFilter {
            category: parse_category(self.category.as_deref())?,
        })
    }
}

async fn get_runway(
    State(state): State<AppState>,
    Query(params): Query<RunwayParams>,
) -> Result<Json<InventoryRunway>, ApiError> {
    let filter = params.filter()?;
    let (workspace_id, _) = workspace_context(&state).await?;
    let facts = state
        .inventory_runway_repository()?
        .runway_facts(workspace_id)
        .await?;
    Ok(Json(build_runway(&facts, &filter)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConsumptionParams {
    /// Omitted: from the first recorded consumption.
    from: Option<String>,
    /// Omitted: today (UTC).
    to: Option<String>,
    granularity: Option<String>,
    category: Option<String>,
}

impl ConsumptionParams {
    /// The query to fetch facts for, and whether `from` was given. Without
    /// one, the longest range the granularity allows is fetched and the
    /// report then starts at the first recorded consumption.
    fn query(&self, today: NaiveDate) -> Result<(ConsumptionQuery, bool), InventoryError> {
        let granularity = match self.granularity.as_deref().unwrap_or("week") {
            "day" => Granularity::Day,
            "week" => Granularity::Week,
            other => {
                return Err(InventoryError::Validation(format!(
                    "unsupported granularity: {other}"
                )))
            }
        };
        let to = parse_date(self.to.as_deref(), "to")?.unwrap_or(today);
        let from = parse_date(self.from.as_deref(), "from")?;
        let longest = if granularity == Granularity::Day {
            MAX_DAILY_RANGE_DAYS
        } else {
            MAX_CONSUMPTION_RANGE_DAYS
        };
        let query = ConsumptionQuery {
            from: from.unwrap_or(to - Duration::days(longest - 1)),
            to,
            granularity,
            category: parse_category(self.category.as_deref())?,
        }
        .validate()?;
        Ok((query, from.is_some()))
    }
}

async fn get_consumption(
    State(state): State<AppState>,
    Query(params): Query<ConsumptionParams>,
) -> Result<Json<InventoryConsumption>, ApiError> {
    let today = Utc::now().date_naive();
    let (mut query, from_given) = params.query(today)?;
    let (workspace_id, _) = workspace_context(&state).await?;
    let facts = state
        .inventory_consumption_repository()?
        .consumption_facts(workspace_id, query.fetch_from(), query.to)
        .await?;
    if !from_given {
        query.from = facts
            .earliest_observed()
            .map_or(query.to, |earliest| earliest.max(query.from))
            .min(query.to);
    }
    Ok(Json(build_consumption(&facts, &query, today)))
}
