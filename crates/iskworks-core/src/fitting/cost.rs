//! What a fit costs on the market, and where the workspace's characters
//! already hold its types. Pure: callers pass prices.

use std::collections::BTreeMap;

use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::Serialize;

use super::{FitInput, SlotKind};
use crate::{ConnectedCharacterId, InventoryError, Money, WorkspaceId};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CostSection {
    Hull,
    Fittings,
    Drones,
    Cargo,
}

/// One type of the fit, with how many and what they cost.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CostLine {
    pub type_id: i64,
    pub section: CostSection,
    pub quantity: u32,
    /// `None` when the market has no price for it.
    pub unit_price: Option<Money>,
    pub total: Option<Money>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitCost {
    pub lines: Vec<CostLine>,
    pub hull: Money,
    pub fittings: Money,
    pub drones: Money,
    pub cargo: Money,
    /// Of what has a price; `missing` lists what doesn't.
    pub total: Money,
    pub missing: Vec<i64>,
}

fn section_of(kind: SlotKind) -> CostSection {
    match kind {
        SlotKind::DroneBay | SlotKind::FighterBay | SlotKind::FighterTube => CostSection::Drones,
        SlotKind::Cargo => CostSection::Cargo,
        _ => CostSection::Fittings,
    }
}

/// How many of each type the fit is made of: the hull, its items and
/// stacks. Loaded charges are left out: a fit names a weapon's charge,
/// never how many rounds it carries.
#[must_use]
pub fn fit_needs(fit: &FitInput) -> BTreeMap<i64, u32> {
    let mut needs = BTreeMap::from([(fit.ship_type_id, 1)]);
    for item in &fit.items {
        *needs.entry(item.type_id).or_insert(0) += item.quantity;
    }
    needs
}

/// What `fit` costs at `prices` (per unit, by type), by type and section.
#[must_use]
pub fn fit_cost(fit: &FitInput, prices: &BTreeMap<i64, Money>) -> FitCost {
    // (type, section) → quantity, in the order the fit lists them.
    let mut order: Vec<(i64, CostSection)> = vec![(fit.ship_type_id, CostSection::Hull)];
    let mut quantities = BTreeMap::from([((fit.ship_type_id, CostSection::Hull), 1_u32)]);
    for item in &fit.items {
        let key = (item.type_id, section_of(item.slot_kind));
        if !quantities.contains_key(&key) {
            order.push(key);
        }
        *quantities.entry(key).or_insert(0) += item.quantity;
    }

    let mut sections: BTreeMap<CostSection, Decimal> = BTreeMap::new();
    let mut missing = Vec::new();
    let lines = order
        .into_iter()
        .map(|(type_id, section)| {
            let quantity = quantities[&(type_id, section)];
            let unit_price = prices.get(&type_id).copied();
            let total = unit_price.map(|price| Money(price.0 * Decimal::from(quantity)));
            match total {
                Some(total) => *sections.entry(section).or_default() += total.0,
                None if !missing.contains(&type_id) => missing.push(type_id),
                None => {}
            }
            CostLine {
                type_id,
                section,
                quantity,
                unit_price,
                total,
            }
        })
        .collect();
    let section = |section: CostSection| Money(sections.get(&section).copied().unwrap_or_default());
    let (hull, fittings, drones, cargo) = (
        section(CostSection::Hull),
        section(CostSection::Fittings),
        section(CostSection::Drones),
        section(CostSection::Cargo),
    );
    FitCost {
        lines,
        total: Money(hull.0 + fittings.0 + drones.0 + cargo.0),
        hull,
        fittings,
        drones,
        cargo,
        missing,
    }
}

/// How many of a type a character holds at one location.
/// How many of one type a character holds at one location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetHolding {
    pub connection_id: ConnectedCharacterId,
    pub character_name: String,
    pub location_id: i64,
    pub location_name: String,
    pub type_id: i64,
    pub quantity: i64,
}

#[async_trait]
pub trait AssetHoldingsRepository: Send + Sync {
    /// Every workspace character's current holdings of `type_ids`, by
    /// location (fitted or packaged alike; blueprint copies excluded).
    async fn type_holdings(
        &self,
        workspace_id: WorkspaceId,
        type_ids: &[i64],
    ) -> Result<Vec<AssetHolding>, InventoryError>;
}

#[cfg(test)]
mod tests;
