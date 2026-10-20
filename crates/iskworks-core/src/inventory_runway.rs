//! Inventory › Runway: what committed Epics still have to buy, against stock.
//!
//! Storage hands over plain facts ([`RunwayFacts`]); everything else is pure
//! arithmetic here. Per Epic × type the numbers follow the reservation model
//! (`order::reservation::epic_coverage_line`): what is still to be used is
//! `required - consumed`, of which `reserved` is already held. The rest is
//! met from free stock, which is shared, so it is handed out oldest Epic
//! first. Two Epics never both count the same free units.

use std::collections::{BTreeMap, HashMap};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::Serialize;

use crate::order::OrderId;
use crate::{InventoryError, MaterialCategory, Money, OwnerId, WorkspaceId};

/// An open (live) Epic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunwayEpic {
    pub order_id: OrderId,
    pub owner_id: OwnerId,
    pub display_name: String,
    pub created_at: DateTime<Utc>,
}

/// One `Buy` requirement of an open Epic, with its allocation totals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunwayRequirement {
    pub order_id: OrderId,
    pub type_id: i64,
    pub required: u64,
    /// Active allocations: held, not yet used.
    pub reserved: u64,
    /// Allocations used by recordings.
    pub consumed: u64,
    /// The frozen `estimated_unit_cost`.
    pub unit_cost: Option<Money>,
}

/// Consumption posted by an Epic's tickets, net of reversals. Covers Epics
/// whose recordings predate consume-on-record and so never marked
/// allocations consumed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunwayEventConsumption {
    pub order_id: OrderId,
    pub type_id: i64,
    pub quantity: u64,
}

/// Physical stock and every active reservation of one owner's type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunwayStockPool {
    pub owner_id: OwnerId,
    pub type_id: i64,
    pub on_hand: u64,
    pub active_reserved: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunwayTypeFact {
    pub type_id: i64,
    pub name: String,
    pub category: MaterialCategory,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunwayFacts {
    pub epics: Vec<RunwayEpic>,
    pub requirements: Vec<RunwayRequirement>,
    pub event_consumption: Vec<RunwayEventConsumption>,
    pub pools: Vec<RunwayStockPool>,
    pub types: Vec<RunwayTypeFact>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunwayFilter {
    pub category: Option<MaterialCategory>,
}

#[async_trait]
pub trait InventoryRunwayRepository: Send + Sync {
    async fn runway_facts(&self, workspace_id: WorkspaceId) -> Result<RunwayFacts, InventoryError>;
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryRunway {
    pub kpis: RunwayKpis,
    /// Oldest first, the order free stock is handed out in.
    pub epics: Vec<RunwayEpicReadiness>,
    /// By category display order, then short value (largest first), then name.
    pub materials: Vec<RunwayMaterial>,
    /// Materials with a shortfall that has no unit cost.
    pub missing_price_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunwayKpis {
    pub shortfall_value: Money,
    pub materials_short: u32,
    pub materials_total: u32,
    pub epics_blocked: u32,
    pub epics_total: u32,
    /// Share of outstanding demand value already covered by stock. `None`
    /// when nothing outstanding is priced.
    pub demand_covered_percent: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunwayEpicReadiness {
    pub order_id: OrderId,
    pub display_name: String,
    pub created_at: DateTime<Utc>,
    /// By value; `None` when nothing outstanding is priced.
    pub covered_percent: Option<f64>,
    pub short_value: Money,
    /// Any input short.
    pub blocked: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunwayMaterial {
    pub type_id: i64,
    pub name: String,
    pub category: MaterialCategory,
    /// Still to be used by the Epics (`required - consumed`).
    pub needed: u64,
    pub on_hand: u64,
    /// Held for these Epics.
    pub reserved: u64,
    pub short: u64,
    /// Weighted by what each Epic still needs.
    pub unit_cost: Option<Money>,
    /// `None` when every short line is unpriced.
    pub short_value: Option<Money>,
    pub coverage_percent: f64,
    /// Epics that still need it, largest shortfall first.
    pub epics: Vec<RunwayMaterialEpic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunwayMaterialEpic {
    pub order_id: OrderId,
    pub required: u64,
    pub consumed: u64,
    pub reserved: u64,
    /// `required - consumed`.
    pub outstanding: u64,
    pub short: u64,
}

/// One Epic × type after netting consumption and handing out free stock.
#[derive(Debug, Clone)]
struct Line {
    order_id: OrderId,
    epic_index: usize,
    type_id: i64,
    required: u64,
    consumed: u64,
    reserved: u64,
    outstanding: u64,
    remaining_need: u64,
    short: u64,
    unit_cost: Option<Decimal>,
}

impl Line {
    fn value(&self, quantity: u64) -> Option<Decimal> {
        self.unit_cost.map(|unit| unit * Decimal::from(quantity))
    }
}

fn money(value: Decimal) -> Money {
    let mut value = value;
    value.rescale(4);
    Money(value)
}

fn percent(part: Decimal, whole: Decimal) -> Option<f64> {
    if whole.is_zero() {
        return None;
    }
    (part * Decimal::from(100) / whole).to_f64()
}

/// Σ(quantity × unit) / Σ quantity over priced entries.
fn weighted_unit(entries: impl Iterator<Item = (u64, Option<Decimal>)>) -> Option<Decimal> {
    let (mut quantity, mut value) = (0u64, Decimal::ZERO);
    for (entry_quantity, unit) in entries {
        if let Some(unit) = unit {
            quantity += entry_quantity;
            value += unit * Decimal::from(entry_quantity);
        }
    }
    (quantity > 0).then(|| value / Decimal::from(quantity))
}

fn lines(
    facts: &RunwayFacts,
    filter: &RunwayFilter,
    category_of: impl Fn(i64) -> MaterialCategory,
) -> Vec<Line> {
    let epic_index: HashMap<OrderId, usize> = facts
        .epics
        .iter()
        .enumerate()
        .map(|(index, epic)| (epic.order_id, index))
        .collect();
    let event_consumed: HashMap<(OrderId, i64), u64> =
        facts
            .event_consumption
            .iter()
            .fold(HashMap::new(), |mut acc, row| {
                *acc.entry((row.order_id, row.type_id)).or_default() += row.quantity;
                acc
            });

    let mut grouped: BTreeMap<(usize, i64), Vec<&RunwayRequirement>> = BTreeMap::new();
    for requirement in &facts.requirements {
        let Some(&index) = epic_index.get(&requirement.order_id) else {
            continue;
        };
        if filter
            .category
            .is_some_and(|category| category_of(requirement.type_id) != category)
        {
            continue;
        }
        grouped
            .entry((index, requirement.type_id))
            .or_default()
            .push(requirement);
    }

    grouped
        .into_iter()
        .map(|((index, type_id), requirements)| {
            let order_id = facts.epics[index].order_id;
            let required: u64 = requirements.iter().map(|r| r.required).sum();
            let reserved: u64 = requirements.iter().map(|r| r.reserved).sum();
            let allocation_consumed: u64 = requirements.iter().map(|r| r.consumed).sum();
            let consumed = allocation_consumed.max(
                event_consumed
                    .get(&(order_id, type_id))
                    .copied()
                    .unwrap_or(0),
            );
            let outstanding = required.saturating_sub(consumed);
            Line {
                order_id,
                epic_index: index,
                type_id,
                required,
                consumed,
                reserved,
                outstanding,
                remaining_need: outstanding.saturating_sub(reserved),
                short: 0,
                unit_cost: weighted_unit(
                    requirements
                        .iter()
                        .map(|r| (r.required, r.unit_cost.map(|unit| unit.0))),
                ),
            }
        })
        .collect()
}

/// Hand each owner × type pool's free stock to Epics oldest first.
fn allocate_free_stock(facts: &RunwayFacts, lines: &mut [Line]) {
    let mut free: HashMap<(OwnerId, i64), u64> = facts
        .pools
        .iter()
        .map(|pool| {
            (
                (pool.owner_id, pool.type_id),
                pool.on_hand.saturating_sub(pool.active_reserved),
            )
        })
        .collect();
    let mut order: Vec<usize> = (0..lines.len()).collect();
    order.sort_by_key(|&i| {
        let epic = &facts.epics[lines[i].epic_index];
        (epic.created_at, epic.order_id.0)
    });
    for i in order {
        let line = &mut lines[i];
        let owner_id = facts.epics[line.epic_index].owner_id;
        let pool = free.entry((owner_id, line.type_id)).or_default();
        let taken = line.remaining_need.min(*pool);
        *pool -= taken;
        line.short = line.remaining_need - taken;
    }
}

#[must_use]
pub fn build_runway(facts: &RunwayFacts, filter: &RunwayFilter) -> InventoryRunway {
    let types: HashMap<i64, &RunwayTypeFact> = facts
        .types
        .iter()
        .map(|fact| (fact.type_id, fact))
        .collect();
    let category_of = |type_id: i64| {
        types
            .get(&type_id)
            .map_or(MaterialCategory::Other, |fact| fact.category)
    };

    let mut lines = lines(facts, filter, category_of);
    allocate_free_stock(facts, &mut lines);

    let on_hand: HashMap<(OwnerId, i64), u64> = facts
        .pools
        .iter()
        .map(|pool| ((pool.owner_id, pool.type_id), pool.on_hand))
        .collect();

    // ---- materials
    let mut by_type: BTreeMap<i64, Vec<&Line>> = BTreeMap::new();
    for line in lines.iter().filter(|line| line.outstanding > 0) {
        by_type.entry(line.type_id).or_default().push(line);
    }
    let mut missing_price_count = 0u32;
    let mut materials: Vec<RunwayMaterial> = by_type
        .into_iter()
        .map(|(type_id, mut type_lines)| {
            let needed: u64 = type_lines.iter().map(|l| l.outstanding).sum();
            let short: u64 = type_lines.iter().map(|l| l.short).sum();
            let mut owners: Vec<OwnerId> = type_lines
                .iter()
                .map(|l| facts.epics[l.epic_index].owner_id)
                .collect();
            owners.sort_by_key(|owner| owner.0);
            owners.dedup();
            let short_lines: Vec<&&Line> = type_lines.iter().filter(|l| l.short > 0).collect();
            let unpriced_short = short_lines.iter().any(|l| l.unit_cost.is_none());
            if unpriced_short {
                missing_price_count += 1;
            }
            let priced_short: Vec<Decimal> = short_lines
                .iter()
                .filter_map(|l| l.value(l.short))
                .collect();
            let short_value = if short == 0 {
                Some(Decimal::ZERO)
            } else if priced_short.is_empty() {
                None
            } else {
                Some(priced_short.into_iter().sum())
            };
            type_lines.sort_by(|a, b| b.short.cmp(&a.short).then(a.epic_index.cmp(&b.epic_index)));
            let fact = types.get(&type_id);
            RunwayMaterial {
                type_id,
                name: fact.map_or_else(|| format!("Type {type_id}"), |f| f.name.clone()),
                category: category_of(type_id),
                needed,
                on_hand: owners
                    .iter()
                    .map(|owner| on_hand.get(&(*owner, type_id)).copied().unwrap_or(0))
                    .sum(),
                reserved: type_lines.iter().map(|l| l.reserved).sum(),
                short,
                unit_cost: weighted_unit(type_lines.iter().map(|l| (l.outstanding, l.unit_cost)))
                    .map(money),
                short_value: short_value.map(money),
                coverage_percent: (needed - short) as f64 * 100.0 / needed as f64,
                epics: type_lines
                    .iter()
                    .map(|l| RunwayMaterialEpic {
                        order_id: l.order_id,
                        required: l.required,
                        consumed: l.consumed,
                        reserved: l.reserved,
                        outstanding: l.outstanding,
                        short: l.short,
                    })
                    .collect(),
            }
        })
        .collect();
    materials.sort_by(|a, b| {
        a.category
            .cmp(&b.category)
            .then_with(|| {
                let value = |m: &RunwayMaterial| m.short_value.map_or(Decimal::ZERO, |v| v.0);
                value(b).cmp(&value(a))
            })
            .then_with(|| b.short.cmp(&a.short))
            .then_with(|| a.name.cmp(&b.name))
    });

    // ---- Epic readiness
    let mut epic_order: Vec<usize> = (0..facts.epics.len()).collect();
    epic_order.sort_by_key(|&i| (facts.epics[i].created_at, facts.epics[i].order_id.0));
    let epics: Vec<RunwayEpicReadiness> = epic_order
        .into_iter()
        .map(|index| {
            let epic = &facts.epics[index];
            let epic_lines: Vec<&Line> = lines.iter().filter(|l| l.epic_index == index).collect();
            let outstanding_value: Decimal = epic_lines
                .iter()
                .filter_map(|l| l.value(l.outstanding))
                .sum();
            let short_value: Decimal = epic_lines.iter().filter_map(|l| l.value(l.short)).sum();
            RunwayEpicReadiness {
                order_id: epic.order_id,
                display_name: epic.display_name.clone(),
                created_at: epic.created_at,
                covered_percent: percent(outstanding_value - short_value, outstanding_value),
                short_value: money(short_value),
                blocked: epic_lines.iter().any(|l| l.short > 0),
            }
        })
        .collect();

    // ---- KPIs
    let outstanding_value: Decimal = lines.iter().filter_map(|l| l.value(l.outstanding)).sum();
    let shortfall_value: Decimal = lines.iter().filter_map(|l| l.value(l.short)).sum();
    let kpis = RunwayKpis {
        shortfall_value: money(shortfall_value),
        materials_short: materials.iter().filter(|m| m.short > 0).count() as u32,
        materials_total: materials.len() as u32,
        epics_blocked: epics.iter().filter(|e| e.blocked).count() as u32,
        epics_total: epics.len() as u32,
        demand_covered_percent: percent(outstanding_value - shortfall_value, outstanding_value),
    };

    InventoryRunway {
        kpis,
        epics,
        materials,
        missing_price_count,
    }
}

#[cfg(test)]
mod tests;
