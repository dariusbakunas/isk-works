//! Inventory › Consumption: recorded consumption over time, by category and
//! by material, with burn rate and days of cover once there is enough
//! history to trust them.
//!
//! Storage hands over daily per-type totals ([`ConsumptionFacts`]); bucketing
//! and every derived figure are pure here. Weeks start on Monday (UTC), as in
//! Finance analytics.

use std::collections::{BTreeMap, HashMap};

use async_trait::async_trait;
use chrono::{Duration, NaiveDate};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::Serialize;

use crate::{
    bucket_start, next_bucket, Granularity, InventoryError, MaterialCategory, Money, WorkspaceId,
};

/// A type needs this much history (first consumption at least this many days
/// before the range end) before its burn rate and days of cover are shown.
pub const BURN_RATE_MIN_HISTORY_DAYS: i64 = 28;
/// Weeks in each top material's sparkline, ending with the range end's week.
pub const SPARKLINE_WEEKS: usize = 8;
/// Longest range the API accepts, and the longest at day granularity.
pub const MAX_CONSUMPTION_RANGE_DAYS: i64 = 3 * 366;
pub const MAX_DAILY_RANGE_DAYS: i64 = 366;

/// Net consumption of one type on one UTC day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsumptionDay {
    pub day: NaiveDate,
    pub type_id: i64,
    pub quantity: u64,
    /// Cost basis at consumption.
    pub value: Money,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsumptionTypeFact {
    pub type_id: i64,
    pub name: String,
    pub category: MaterialCategory,
    /// All-time first consumption, whatever the range.
    pub first_consumed: NaiveDate,
    pub on_hand: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConsumptionFacts {
    /// Days in the fetched window, which starts early enough for sparklines.
    pub days: Vec<ConsumptionDay>,
    /// Every type the workspace has ever consumed.
    pub types: Vec<ConsumptionTypeFact>,
}

impl ConsumptionFacts {
    #[must_use]
    pub fn earliest_observed(&self) -> Option<NaiveDate> {
        self.types.iter().map(|fact| fact.first_consumed).min()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsumptionQuery {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub granularity: Granularity,
    pub category: Option<MaterialCategory>,
}

impl ConsumptionQuery {
    /// First day storage must return: the range start, or earlier when the
    /// sparkline window reaches further back.
    #[must_use]
    pub fn fetch_from(&self) -> NaiveDate {
        let sparkline_start =
            bucket_start(self.to, Granularity::Week) - Duration::weeks(SPARKLINE_WEEKS as i64 - 1);
        self.from.min(sparkline_start)
    }

    pub fn validate(self) -> Result<Self, InventoryError> {
        if self.from > self.to {
            return Err(InventoryError::Validation(
                "from must not be after to".into(),
            ));
        }
        let days = (self.to - self.from).num_days() + 1;
        if days > MAX_CONSUMPTION_RANGE_DAYS {
            return Err(InventoryError::Validation(format!(
                "range must be at most {MAX_CONSUMPTION_RANGE_DAYS} days"
            )));
        }
        if self.granularity == Granularity::Day && days > MAX_DAILY_RANGE_DAYS {
            return Err(InventoryError::Validation(format!(
                "a daily range must be at most {MAX_DAILY_RANGE_DAYS} days"
            )));
        }
        if self.granularity == Granularity::Month {
            return Err(InventoryError::Validation(
                "granularity must be day or week".into(),
            ));
        }
        Ok(self)
    }
}

#[async_trait]
pub trait InventoryConsumptionRepository: Send + Sync {
    /// Daily totals for `[from, to]` and every consumed type's facts.
    async fn consumption_facts(
        &self,
        workspace_id: WorkspaceId,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<ConsumptionFacts, InventoryError>;
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryConsumption {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub granularity: Granularity,
    /// First recorded consumption in the workspace, any type.
    pub earliest_observed_at: Option<NaiveDate>,
    pub kpis: ConsumptionKpis,
    /// Every bucket of the range, empty ones included.
    pub buckets: Vec<ConsumptionBucket>,
    /// Share of the range's value by category. Ignores the category filter,
    /// so the donut can switch between categories.
    pub categories: Vec<CategoryShare>,
    /// Every material used in the range, largest value first.
    pub materials: Vec<ConsumedMaterial>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsumptionKpis {
    pub consumed_value: Money,
    pub materials_used: u32,
    pub busiest_bucket: Option<BusiestBucket>,
    pub top_category: Option<MaterialCategory>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BusiestBucket {
    pub start: NaiveDate,
    pub value: Money,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsumptionBucket {
    pub start: NaiveDate,
    /// Not over yet, or cut short by the range end.
    pub partial: bool,
    pub total: Money,
    /// Non-zero categories only, in display order.
    pub by_category: Vec<CategoryValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryValue {
    pub category: MaterialCategory,
    pub value: Money,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryShare {
    pub category: MaterialCategory,
    pub value: Money,
    pub share_percent: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsumedMaterial {
    pub type_id: i64,
    pub name: String,
    pub category: MaterialCategory,
    pub quantity: u64,
    pub value: Money,
    pub share_percent: f64,
    /// Quantity per week for the [`SPARKLINE_WEEKS`] weeks ending with the
    /// range end's week, oldest first.
    pub weekly_quantity: Vec<u64>,
    pub on_hand: u64,
    /// `None` until the type has [`BURN_RATE_MIN_HISTORY_DAYS`] of history.
    pub average_per_week: Option<f64>,
    pub days_of_cover: Option<f64>,
}

fn money(value: Decimal) -> Money {
    let mut value = value;
    value.rescale(4);
    Money(value)
}

fn share(part: Decimal, whole: Decimal) -> f64 {
    if whole.is_zero() {
        return 0.0;
    }
    (part * Decimal::from(100) / whole).to_f64().unwrap_or(0.0)
}

#[derive(Default)]
struct TypeTotal {
    quantity: u64,
    value: Decimal,
}

#[must_use]
pub fn build_consumption(
    facts: &ConsumptionFacts,
    query: &ConsumptionQuery,
    today: NaiveDate,
) -> InventoryConsumption {
    let types: HashMap<i64, &ConsumptionTypeFact> = facts
        .types
        .iter()
        .map(|fact| (fact.type_id, fact))
        .collect();
    let category_of = |type_id: i64| {
        types
            .get(&type_id)
            .map_or(MaterialCategory::Other, |fact| fact.category)
    };
    let in_range = |day: NaiveDate| day >= query.from && day <= query.to;
    let in_filter = |type_id: i64| query.category.map_or(true, |c| category_of(type_id) == c);

    // ---- categories (unfiltered) and per-type totals (filtered)
    let mut category_values: BTreeMap<MaterialCategory, Decimal> = BTreeMap::new();
    let mut totals: BTreeMap<i64, TypeTotal> = BTreeMap::new();
    for day in facts.days.iter().filter(|day| in_range(day.day)) {
        *category_values.entry(category_of(day.type_id)).or_default() += day.value.0;
        if in_filter(day.type_id) {
            let total = totals.entry(day.type_id).or_default();
            total.quantity += day.quantity;
            total.value += day.value.0;
        }
    }
    let all_value: Decimal = category_values.values().copied().sum();
    let mut categories: Vec<CategoryShare> = category_values
        .iter()
        .filter(|(_, value)| !value.is_zero())
        .map(|(category, value)| CategoryShare {
            category: *category,
            value: money(*value),
            share_percent: share(*value, all_value),
        })
        .collect();
    categories.sort_by(|a, b| b.value.cmp(&a.value).then(a.category.cmp(&b.category)));

    // ---- buckets (filtered)
    let mut bucket_values: BTreeMap<NaiveDate, BTreeMap<MaterialCategory, Decimal>> =
        BTreeMap::new();
    let mut start = bucket_start(query.from, query.granularity);
    while start <= query.to {
        bucket_values.insert(start, BTreeMap::new());
        start = next_bucket(start, query.granularity);
    }
    for day in facts
        .days
        .iter()
        .filter(|day| in_range(day.day) && in_filter(day.type_id))
    {
        let values = bucket_values
            .entry(bucket_start(day.day, query.granularity))
            .or_default();
        *values.entry(category_of(day.type_id)).or_default() += day.value.0;
    }
    let buckets: Vec<ConsumptionBucket> = bucket_values
        .into_iter()
        .map(|(start, values)| {
            let last_day = next_bucket(start, query.granularity) - Duration::days(1);
            ConsumptionBucket {
                start,
                partial: last_day > query.to || last_day >= today,
                total: money(values.values().copied().sum()),
                by_category: values
                    .into_iter()
                    .filter(|(_, value)| !value.is_zero())
                    .map(|(category, value)| CategoryValue {
                        category,
                        value: money(value),
                    })
                    .collect(),
            }
        })
        .collect();

    // ---- materials (filtered)
    let filtered_value: Decimal = totals.values().map(|total| total.value).sum();
    let sparkline_start =
        bucket_start(query.to, Granularity::Week) - Duration::weeks(SPARKLINE_WEEKS as i64 - 1);
    let mut weekly: HashMap<i64, Vec<u64>> = HashMap::new();
    for day in facts
        .days
        .iter()
        .filter(|day| day.day >= sparkline_start && day.day <= query.to)
    {
        let week = ((day.day - sparkline_start).num_days() / 7) as usize;
        weekly
            .entry(day.type_id)
            .or_insert_with(|| vec![0; SPARKLINE_WEEKS])[week] += day.quantity;
    }
    let mut materials: Vec<ConsumedMaterial> = totals
        .iter()
        .filter(|(_, total)| total.quantity > 0)
        .map(|(&type_id, total)| {
            let fact = types.get(&type_id);
            let on_hand = fact.map_or(0, |fact| fact.on_hand);
            let first_consumed = fact.map_or(query.to, |fact| fact.first_consumed);
            let enough_history =
                (query.to - first_consumed).num_days() >= BURN_RATE_MIN_HISTORY_DAYS;
            let observed_days = ((query.to - first_consumed.max(query.from)).num_days() + 1) as f64;
            let per_day = total.quantity as f64 / observed_days;
            ConsumedMaterial {
                type_id,
                name: fact.map_or_else(|| format!("Type {type_id}"), |fact| fact.name.clone()),
                category: category_of(type_id),
                quantity: total.quantity,
                value: money(total.value),
                share_percent: share(total.value, filtered_value),
                weekly_quantity: weekly
                    .get(&type_id)
                    .cloned()
                    .unwrap_or_else(|| vec![0; SPARKLINE_WEEKS]),
                on_hand,
                average_per_week: enough_history.then_some(per_day * 7.0),
                days_of_cover: enough_history.then_some(on_hand as f64 / per_day),
            }
        })
        .collect();
    materials.sort_by(|a, b| {
        b.value
            .cmp(&a.value)
            .then_with(|| b.quantity.cmp(&a.quantity))
            .then_with(|| a.name.cmp(&b.name))
    });

    // ---- KPIs (filtered)
    let busiest_bucket = buckets
        .iter()
        .filter(|bucket| !bucket.total.0.is_zero())
        .max_by(|a, b| a.total.cmp(&b.total).then(b.start.cmp(&a.start)))
        .map(|bucket| BusiestBucket {
            start: bucket.start,
            value: bucket.total,
        });
    let mut filtered_categories: BTreeMap<MaterialCategory, Decimal> = BTreeMap::new();
    for material in &materials {
        *filtered_categories.entry(material.category).or_default() += material.value.0;
    }
    let top_category = filtered_categories
        .into_iter()
        .filter(|(_, value)| !value.is_zero())
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|(category, _)| category);

    InventoryConsumption {
        from: query.from,
        to: query.to,
        granularity: query.granularity,
        earliest_observed_at: facts.earliest_observed(),
        kpis: ConsumptionKpis {
            consumed_value: money(filtered_value),
            materials_used: materials.len() as u32,
            busiest_bucket,
            top_category,
        },
        buckets,
        categories,
        materials,
    }
}

#[cfg(test)]
mod tests;
