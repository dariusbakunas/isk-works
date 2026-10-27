//! What a fit costs on the market, and how much of it the workspace's
//! characters already own. Pure: callers pass prices and asset holdings.

use std::collections::BTreeMap;

use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::Serialize;

use super::{FitInput, SlotKind};
use crate::{ConnectedCharacterId, InventoryError, Money, WorkspaceId};

/// The most locations an asset coverage answer lists.
const COVERAGE_LIMIT: usize = 10;

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
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// A type a location has fewer of than the fit needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shortfall {
    pub type_id: i64,
    pub needed: u32,
    pub have: u32,
}

/// How much of a fit one character already has at one location.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationCoverage {
    pub connection_id: ConnectedCharacterId,
    pub character_name: String,
    pub location_id: i64,
    pub location_name: String,
    /// Units of the fit there (up to what it needs of each type).
    pub covered: u32,
    pub needed: u32,
    /// What's short, in type order.
    pub short: Vec<Shortfall>,
}

/// For each character and location holding any of the fit, how much of it
/// is there: best covered first, the ten best.
#[must_use]
pub fn asset_coverage(
    needed: &BTreeMap<i64, u32>,
    holdings: &[AssetHolding],
) -> Vec<LocationCoverage> {
    let total: u32 = needed.values().sum();
    // (character name, location name, quantity by type) per place.
    type Place = (String, String, BTreeMap<i64, i64>);
    let mut places: BTreeMap<(uuid::Uuid, i64), Place> = BTreeMap::new();
    for holding in holdings
        .iter()
        .filter(|holding| needed.contains_key(&holding.type_id))
    {
        let place = places
            .entry((holding.connection_id.0, holding.location_id))
            .or_insert_with(|| {
                (
                    holding.character_name.clone(),
                    holding.location_name.clone(),
                    BTreeMap::new(),
                )
            });
        *place.2.entry(holding.type_id).or_insert(0) += holding.quantity;
    }
    let mut coverage: Vec<LocationCoverage> = places
        .into_iter()
        .map(
            |((connection_id, location_id), (character_name, location_name, have))| {
                let mut covered = 0;
                let mut short = Vec::new();
                for (type_id, need) in needed {
                    let have = u32::try_from(have.get(type_id).copied().unwrap_or(0).max(0))
                        .unwrap_or(u32::MAX);
                    covered += have.min(*need);
                    if have < *need {
                        short.push(Shortfall {
                            type_id: *type_id,
                            needed: *need,
                            have,
                        });
                    }
                }
                LocationCoverage {
                    connection_id: ConnectedCharacterId(connection_id),
                    character_name,
                    location_id,
                    location_name,
                    covered,
                    needed: total,
                    short,
                }
            },
        )
        .collect();
    coverage.sort_by(|a, b| {
        b.covered
            .cmp(&a.covered)
            .then_with(|| a.character_name.cmp(&b.character_name))
            .then_with(|| a.location_name.cmp(&b.location_name))
    });
    coverage.truncate(COVERAGE_LIMIT);
    coverage
}

#[cfg(test)]
mod tests;
