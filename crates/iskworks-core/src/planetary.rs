//! Planetary Interaction: turns a synced colony layout (pins, extractor
//! programs, factory schematics, storage contents) into what the PI page
//! shows -- extraction timers, per-planet import/export flows, storage fill,
//! ISK/month and an attention level. Pure: callers resolve schematics, type
//! facts and prices up front.
//!
//! Throughput is the colony's *configured* rate, not a simulation: expired
//! extractors still count (the expiry is the alert), and an extractor's rate
//! is its current program's average (`qty_per_cycle * 3600 / cycle_time`;
//! real yield decays over a program). Factories whose locally-produced inputs
//! are undersupplied are scaled down proportionally, so a P0 planet with
//! more basic factories than its extractors can feed exports what it can
//! actually make rather than listing the shortfall as an "import".
//!
//! ESI pin contents are a snapshot from the owner's last in-game visit
//! (`last_update`); import stock is projected forward from then, storage
//! fill is shown as of then.

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{InventoryError, WorkspaceId};

/// `invGroups` of the PI pins that hold items, and their fixed capacities.
pub const COMMAND_CENTER_GROUP_ID: i64 = 1027;
pub const STORAGE_FACILITY_GROUP_ID: i64 = 1029;
pub const SPACEPORT_GROUP_ID: i64 = 1030;

const HOURS_PER_MONTH: i64 = 720;
const STORAGE_FULL_PERCENT: i64 = 90;
const WARNING_HOURS: i64 = 24;
/// Fixpoint passes for scaling factories by local supply; PI chains are at
/// most P0 -> P4, so a handful of passes always settles.
const SUPPLY_PASSES: usize = 6;

/// One export the user excluded from ISK totals, keyed by the EVE character
/// id (stable across reconnects, unlike the connection id).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcludedExport {
    pub character_id: i64,
    pub planet_id: i64,
    pub type_id: i64,
}

/// Per-workspace PI page preferences.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanetaryPreferences {
    #[serde(default)]
    pub excluded_exports: Vec<ExcludedExport>,
    /// EVE character ids in display order; characters not listed follow,
    /// sorted by name.
    #[serde(default)]
    pub character_order: Vec<i64>,
}

#[async_trait]
pub trait PlanetaryPreferencesRepository: Send + Sync {
    /// Defaults when nothing has been saved yet.
    async fn planetary_preferences(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<PlanetaryPreferences, InventoryError>;
    async fn save_planetary_preferences(
        &self,
        workspace_id: WorkspaceId,
        preferences: &PlanetaryPreferences,
    ) -> Result<(), InventoryError>;
}

/// One colony as stored in the `planets` character-source summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanetLayout {
    pub planet_id: i64,
    pub planet_type: String,
    pub solar_system_id: i64,
    pub upgrade_level: i64,
    pub last_update: DateTime<Utc>,
    #[serde(default)]
    pub pins: Vec<PinLayout>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinLayout {
    pub pin_id: i64,
    pub type_id: i64,
    #[serde(default)]
    pub schematic_id: Option<i64>,
    #[serde(default)]
    pub contents: Vec<PinContent>,
    #[serde(default)]
    pub expiry_time: Option<DateTime<Utc>>,
    #[serde(default)]
    pub extractor: Option<ExtractorLayout>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinContent {
    pub type_id: i64,
    pub amount: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractorLayout {
    pub product_type_id: i64,
    pub qty_per_cycle: i64,
    pub cycle_time_seconds: i64,
    #[serde(default)]
    pub head_count: i64,
}

#[derive(Deserialize)]
struct PlanetsSummary {
    #[serde(default)]
    planets: Vec<PlanetLayout>,
}

/// Parses the `{"planets": [...]}` summary written by character sync.
pub fn parse_planets_summary(summary: &serde_json::Value) -> Result<Vec<PlanetLayout>, String> {
    serde_json::from_value::<PlanetsSummary>(summary.clone())
        .map(|summary| summary.planets)
        .map_err(|error| error.to_string())
}

/// A PI factory recipe: one cycle consumes `inputs` and yields `outputs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchematicRecipe {
    pub cycle_time_seconds: i64,
    pub inputs: Vec<(i64, i64)>,
    pub outputs: Vec<(i64, i64)>,
}

/// What the derivation needs to know about a type (pins and commodities).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TypeFacts {
    pub group_id: Option<i64>,
    pub volume_m3: Option<Decimal>,
}

pub struct PlanetaryInputs<'a> {
    pub schematics: &'a BTreeMap<i64, SchematicRecipe>,
    pub types: &'a BTreeMap<i64, TypeFacts>,
    /// Best-buy unit price by type; absent types are unpriced.
    pub prices: &'a BTreeMap<i64, Decimal>,
    /// Export type ids the user excluded on this planet.
    pub excluded_exports: &'a BTreeSet<i64>,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Attention {
    Amber,
    Red,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum StorageKind {
    #[serde(rename = "L")]
    Launchpad,
    #[serde(rename = "S")]
    Storage,
    #[serde(rename = "C")]
    CommandCenter,
}

impl StorageKind {
    #[must_use]
    pub fn from_group_id(group_id: i64) -> Option<Self> {
        match group_id {
            SPACEPORT_GROUP_ID => Some(Self::Launchpad),
            STORAGE_FACILITY_GROUP_ID => Some(Self::Storage),
            COMMAND_CENTER_GROUP_ID => Some(Self::CommandCenter),
            _ => None,
        }
    }

    #[must_use]
    pub fn capacity_m3(self) -> Decimal {
        match self {
            Self::Launchpad => Decimal::from(10_000),
            Self::Storage => Decimal::from(12_000),
            Self::CommandCenter => Decimal::from(500),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractorView {
    pub pin_id: i64,
    pub product_type_id: i64,
    pub expires_at: Option<DateTime<Utc>>,
    pub units_per_hour: Decimal,
    pub expired: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductionView {
    pub schematic_id: i64,
    pub factory_count: i64,
    pub output_type_id: i64,
    pub output_per_hour: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportView {
    pub type_id: i64,
    pub qty_per_hour: Decimal,
    /// Projected hours until the on-planet stock runs out (0 = starved).
    pub lasts_hours: Decimal,
    /// When the stock seen at `last_update` runs out at this rate -- a fixed
    /// instant (unlike `lasts_hours`, which shrinks as `now` advances), so a
    /// calendar entry doesn't drift between loads.
    pub depletes_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportView {
    pub type_id: i64,
    pub units_per_hour: Decimal,
    /// `None` when the type has no price in the valuation scope.
    pub isk_per_month: Option<Decimal>,
    pub excluded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageItemView {
    pub type_id: i64,
    pub quantity: i64,
    pub volume_m3: Decimal,
    pub value: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageView {
    pub pin_id: i64,
    pub kind: StorageKind,
    pub capacity_m3: Decimal,
    pub used_m3: Decimal,
    pub fill_percent: Decimal,
    pub value: Decimal,
    pub contents: Vec<StorageItemView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanetView {
    pub planet_id: i64,
    pub extractors: Vec<ExtractorView>,
    pub production: Vec<ProductionView>,
    pub imports: Vec<ImportView>,
    pub exports: Vec<ExportView>,
    pub storage: Vec<StorageView>,
    /// Sum of non-excluded, priced exports.
    pub isk_per_month: Decimal,
    pub expired: bool,
    pub storage_full: bool,
    pub starved: bool,
    pub attention: Option<Attention>,
}

impl PlanetView {
    /// Earliest extractor expiry still in the future.
    #[must_use]
    pub fn next_expiry(&self) -> Option<DateTime<Utc>> {
        self.extractors
            .iter()
            .filter(|extractor| !extractor.expired)
            .filter_map(|extractor| extractor.expires_at)
            .min()
    }
}

fn per_hour(quantity: i64, cycle_time_seconds: i64) -> Decimal {
    if cycle_time_seconds <= 0 {
        return Decimal::ZERO;
    }
    Decimal::from(quantity) * Decimal::from(3_600) / Decimal::from(cycle_time_seconds)
}

#[must_use]
pub fn derive_planet(layout: &PlanetLayout, inputs: &PlanetaryInputs<'_>) -> PlanetView {
    let now = inputs.now;

    let extractors = layout
        .pins
        .iter()
        .filter_map(|pin| {
            let program = pin.extractor?;
            Some(ExtractorView {
                pin_id: pin.pin_id,
                product_type_id: program.product_type_id,
                expires_at: pin.expiry_time,
                units_per_hour: per_hour(program.qty_per_cycle, program.cycle_time_seconds),
                expired: pin.expiry_time.is_some_and(|expiry| expiry <= now),
            })
        })
        .collect::<Vec<_>>();

    let mut factory_counts: BTreeMap<i64, i64> = BTreeMap::new();
    for pin in &layout.pins {
        if let Some(schematic_id) = pin.schematic_id {
            if inputs.schematics.contains_key(&schematic_id) {
                *factory_counts.entry(schematic_id).or_default() += 1;
            }
        }
    }

    let mut extracted: BTreeMap<i64, Decimal> = BTreeMap::new();
    for extractor in &extractors {
        *extracted.entry(extractor.product_type_id).or_default() += extractor.units_per_hour;
    }
    let locally_made: BTreeSet<i64> = extracted
        .keys()
        .copied()
        .chain(factory_counts.keys().flat_map(|id| {
            inputs.schematics[id]
                .outputs
                .iter()
                .map(|(type_id, _)| *type_id)
        }))
        .collect();

    // Nominal (full-speed) demand per input type, then scale each schematic
    // by the worst supply ratio among its locally-made inputs.
    let mut nominal_demand: BTreeMap<i64, Decimal> = BTreeMap::new();
    for (schematic_id, count) in &factory_counts {
        let recipe = &inputs.schematics[schematic_id];
        for (type_id, quantity) in &recipe.inputs {
            *nominal_demand.entry(*type_id).or_default() +=
                per_hour(*quantity, recipe.cycle_time_seconds) * Decimal::from(*count);
        }
    }
    let mut factors: BTreeMap<i64, Decimal> = factory_counts
        .keys()
        .map(|id| (*id, Decimal::ONE))
        .collect();
    let mut produced = extracted.clone();
    for _ in 0..SUPPLY_PASSES {
        produced = extracted.clone();
        for (schematic_id, count) in &factory_counts {
            let recipe = &inputs.schematics[schematic_id];
            for (type_id, quantity) in &recipe.outputs {
                *produced.entry(*type_id).or_default() +=
                    per_hour(*quantity, recipe.cycle_time_seconds)
                        * Decimal::from(*count)
                        * factors[schematic_id];
            }
        }
        let mut changed = false;
        for (schematic_id, factor) in &mut factors {
            let recipe = &inputs.schematics[schematic_id];
            let supply_ratio = recipe
                .inputs
                .iter()
                .filter(|(type_id, _)| locally_made.contains(type_id))
                .map(|(type_id, _)| {
                    let demand = nominal_demand[type_id];
                    if demand.is_zero() {
                        Decimal::ONE
                    } else {
                        (produced.get(type_id).copied().unwrap_or_default() / demand)
                            .min(Decimal::ONE)
                    }
                })
                .min()
                .unwrap_or(Decimal::ONE);
            if supply_ratio != *factor {
                *factor = supply_ratio;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut consumed: BTreeMap<i64, Decimal> = BTreeMap::new();
    let mut production = Vec::new();
    for (schematic_id, count) in &factory_counts {
        let recipe = &inputs.schematics[schematic_id];
        let factor = factors[schematic_id];
        for (type_id, quantity) in &recipe.inputs {
            *consumed.entry(*type_id).or_default() +=
                per_hour(*quantity, recipe.cycle_time_seconds) * Decimal::from(*count) * factor;
        }
        for (type_id, quantity) in &recipe.outputs {
            production.push(ProductionView {
                schematic_id: *schematic_id,
                factory_count: *count,
                output_type_id: *type_id,
                output_per_hour: per_hour(*quantity, recipe.cycle_time_seconds)
                    * Decimal::from(*count)
                    * factor,
            });
        }
    }

    let mut stock: BTreeMap<i64, i64> = BTreeMap::new();
    for pin in &layout.pins {
        for item in &pin.contents {
            *stock.entry(item.type_id).or_default() += item.amount;
        }
    }
    let hours_since_update =
        Decimal::from((now - layout.last_update).num_seconds().max(0)) / Decimal::from(3_600);

    let imports = consumed
        .iter()
        .filter(|(type_id, rate)| !locally_made.contains(type_id) && !rate.is_zero())
        .map(|(type_id, rate)| {
            let on_hand = Decimal::from(stock.get(type_id).copied().unwrap_or_default());
            let projected = (on_hand - *rate * hours_since_update).max(Decimal::ZERO);
            let seconds_of_stock = (on_hand / *rate * Decimal::from(3_600))
                .round()
                .to_i64()
                .unwrap_or(i64::MAX / 2);
            ImportView {
                type_id: *type_id,
                qty_per_hour: *rate,
                lasts_hours: projected / *rate,
                depletes_at: layout.last_update
                    + chrono::Duration::seconds(seconds_of_stock.min(10 * 365 * 86_400)),
            }
        })
        .collect::<Vec<_>>();

    let exports = produced
        .iter()
        .filter_map(|(type_id, rate)| {
            let net = *rate - consumed.get(type_id).copied().unwrap_or_default();
            // Sub-unit residue from proportional scaling is not an export.
            (net >= Decimal::new(1, 2)).then(|| {
                let isk_per_month = inputs
                    .prices
                    .get(type_id)
                    .map(|price| net * Decimal::from(HOURS_PER_MONTH) * *price);
                ExportView {
                    type_id: *type_id,
                    units_per_hour: net,
                    isk_per_month,
                    excluded: inputs.excluded_exports.contains(type_id),
                }
            })
        })
        .collect::<Vec<_>>();

    let mut storage = layout
        .pins
        .iter()
        .filter_map(|pin| {
            let kind = inputs
                .types
                .get(&pin.type_id)
                .and_then(|facts| facts.group_id)
                .and_then(StorageKind::from_group_id)?;
            if kind == StorageKind::CommandCenter && pin.contents.is_empty() {
                return None;
            }
            Some(storage_view(pin, kind, inputs))
        })
        .collect::<Vec<_>>();
    storage.sort_by_key(|view| (view.kind, view.pin_id));

    let isk_per_month = exports
        .iter()
        .filter(|export| !export.excluded)
        .filter_map(|export| export.isk_per_month)
        .sum();

    let warning = chrono::Duration::hours(WARNING_HOURS);
    let expired = extractors.iter().any(|extractor| extractor.expired);
    let storage_full = storage
        .iter()
        .any(|view| view.fill_percent >= Decimal::from(STORAGE_FULL_PERCENT));
    let starved = imports
        .iter()
        .any(|import| import.lasts_hours <= Decimal::ZERO);
    let expiring = extractors.iter().any(|extractor| {
        !extractor.expired
            && extractor
                .expires_at
                .is_some_and(|expiry| expiry - now < warning)
    });
    let running_low = imports
        .iter()
        .any(|import| import.lasts_hours < Decimal::from(WARNING_HOURS));
    let attention = if expired || storage_full || starved {
        Some(Attention::Red)
    } else if expiring || running_low {
        Some(Attention::Amber)
    } else {
        None
    };

    PlanetView {
        planet_id: layout.planet_id,
        extractors,
        production,
        imports,
        exports,
        storage,
        isk_per_month,
        expired,
        storage_full,
        starved,
        attention,
    }
}

fn storage_view(pin: &PinLayout, kind: StorageKind, inputs: &PlanetaryInputs<'_>) -> StorageView {
    let contents = pin
        .contents
        .iter()
        .map(|item| {
            let unit_volume = inputs
                .types
                .get(&item.type_id)
                .and_then(|facts| facts.volume_m3)
                .unwrap_or_default();
            StorageItemView {
                type_id: item.type_id,
                quantity: item.amount,
                volume_m3: unit_volume * Decimal::from(item.amount),
                value: inputs
                    .prices
                    .get(&item.type_id)
                    .map(|price| *price * Decimal::from(item.amount)),
            }
        })
        .collect::<Vec<_>>();
    let capacity_m3 = kind.capacity_m3();
    let used_m3: Decimal = contents.iter().map(|item| item.volume_m3).sum();
    StorageView {
        pin_id: pin.pin_id,
        kind,
        capacity_m3,
        used_m3,
        fill_percent: used_m3 * Decimal::from(100) / capacity_m3,
        value: contents.iter().filter_map(|item| item.value).sum(),
        contents,
    }
}

/// Per-character totals for the group header and summary strip.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlanetaryRollup {
    pub isk_per_month: Decimal,
    pub next_expiry: Option<(i64, DateTime<Utc>)>,
    pub alert_count: usize,
    pub expired_count: usize,
    pub storage_full_count: usize,
    pub starved_count: usize,
}

#[must_use]
pub fn rollup(planets: &[PlanetView]) -> PlanetaryRollup {
    PlanetaryRollup {
        isk_per_month: planets.iter().map(|planet| planet.isk_per_month).sum(),
        next_expiry: planets
            .iter()
            .filter_map(|planet| planet.next_expiry().map(|at| (planet.planet_id, at)))
            .min_by_key(|(_, at)| *at),
        alert_count: planets
            .iter()
            .filter(|planet| planet.attention.is_some())
            .count(),
        expired_count: planets.iter().filter(|planet| planet.expired).count(),
        storage_full_count: planets.iter().filter(|planet| planet.storage_full).count(),
        starved_count: planets.iter().filter(|planet| planet.starved).count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    // Real ids: Base Metals 2267 -> Reactive Metals 2398 (schematic 126),
    // Noble 2270 -> Precious 2399 (127), Reactive + Precious -> Mechanical
    // Parts 3689 (73). Barren ECU 2848, basic 2473, advanced 2474,
    // launchpad 2544, storage 2541, command center 2524.
    const BASE: i64 = 2_267;
    const REACTIVE: i64 = 2_398;
    const PRECIOUS: i64 = 2_399;
    const MECH: i64 = 3_689;
    const LAUNCHPAD: i64 = 2_544;
    const STORAGE: i64 = 2_541;
    const CC: i64 = 2_524;

    fn now() -> DateTime<Utc> {
        "2026-10-02T14:32:00Z".parse().unwrap()
    }

    fn schematics() -> BTreeMap<i64, SchematicRecipe> {
        BTreeMap::from([
            (
                126,
                SchematicRecipe {
                    cycle_time_seconds: 1_800,
                    inputs: vec![(BASE, 3_000)],
                    outputs: vec![(REACTIVE, 20)],
                },
            ),
            (
                73,
                SchematicRecipe {
                    cycle_time_seconds: 3_600,
                    inputs: vec![(REACTIVE, 40), (PRECIOUS, 40)],
                    outputs: vec![(MECH, 5)],
                },
            ),
        ])
    }

    fn types() -> BTreeMap<i64, TypeFacts> {
        let facts = |group_id, volume: Option<Decimal>| TypeFacts {
            group_id: Some(group_id),
            volume_m3: volume,
        };
        BTreeMap::from([
            (LAUNCHPAD, facts(SPACEPORT_GROUP_ID, None)),
            (STORAGE, facts(STORAGE_FACILITY_GROUP_ID, None)),
            (CC, facts(COMMAND_CENTER_GROUP_ID, None)),
            (BASE, facts(1_032, Some(Decimal::new(5, 3)))),
            (REACTIVE, facts(1_042, Some(Decimal::new(19, 2)))),
            (PRECIOUS, facts(1_042, Some(Decimal::new(19, 2)))),
            (MECH, facts(1_034, Some(Decimal::new(75, 2)))),
        ])
    }

    fn pin(pin_id: i64, type_id: i64) -> PinLayout {
        PinLayout {
            pin_id,
            type_id,
            schematic_id: None,
            contents: vec![],
            expiry_time: None,
            extractor: None,
        }
    }

    fn factory(pin_id: i64, schematic_id: i64) -> PinLayout {
        PinLayout {
            schematic_id: Some(schematic_id),
            ..pin(pin_id, 2_473)
        }
    }

    fn extractor(pin_id: i64, product: i64, expires_in: Duration) -> PinLayout {
        PinLayout {
            expiry_time: Some(now() + expires_in),
            extractor: Some(ExtractorLayout {
                product_type_id: product,
                qty_per_cycle: 6_000,
                cycle_time_seconds: 1_800,
                head_count: 8,
            }),
            ..pin(pin_id, 2_848)
        }
    }

    fn with_contents(mut pin: PinLayout, contents: &[(i64, i64)]) -> PinLayout {
        pin.contents = contents
            .iter()
            .map(|(type_id, amount)| PinContent {
                type_id: *type_id,
                amount: *amount,
            })
            .collect();
        pin
    }

    fn layout(pins: Vec<PinLayout>, last_update: DateTime<Utc>) -> PlanetLayout {
        PlanetLayout {
            planet_id: 40_050_359,
            planet_type: "barren".into(),
            solar_system_id: 30_000_797,
            upgrade_level: 5,
            last_update,
            pins,
        }
    }

    fn derive(layout: &PlanetLayout, excluded: &[i64]) -> PlanetView {
        let schematics = schematics();
        let types = types();
        let prices = BTreeMap::from([
            (REACTIVE, Decimal::from(400)),
            (MECH, Decimal::from(12_000)),
            (PRECIOUS, Decimal::from(450)),
        ]);
        let excluded = excluded.iter().copied().collect();
        derive_planet(
            layout,
            &PlanetaryInputs {
                schematics: &schematics,
                types: &types,
                prices: &prices,
                excluded_exports: &excluded,
                now: now(),
            },
        )
    }

    #[test]
    fn extractor_planet_exports_what_its_extractors_can_feed() {
        // 12 000 Base/h extracted; three basic factories would want 18 000/h,
        // so they run at 2/3 speed: 3 x 40/h x 2/3 = 80 Reactive/h.
        let planet = derive(
            &layout(
                vec![
                    pin(1, CC),
                    extractor(2, BASE, Duration::days(3)),
                    factory(3, 126),
                    factory(4, 126),
                    factory(5, 126),
                    pin(6, LAUNCHPAD),
                ],
                now(),
            ),
            &[],
        );

        assert_eq!(planet.extractors[0].units_per_hour, Decimal::from(12_000));
        assert!(
            planet.imports.is_empty(),
            "extracted input is not an import"
        );
        assert_eq!(planet.exports.len(), 1);
        assert_eq!(planet.exports[0].type_id, REACTIVE);
        assert_eq!(
            planet.exports[0].units_per_hour.round_dp(6),
            Decimal::from(80)
        );
        // 80/h x 720h x 400 ISK
        assert_eq!(
            planet.isk_per_month.round_dp(0),
            Decimal::from(80 * 720 * 400)
        );
        assert_eq!(planet.attention, None);
        assert_eq!(planet.production[0].factory_count, 3);
    }

    #[test]
    fn factory_planet_imports_inputs_and_projects_how_long_they_last() {
        // Two advanced factories: 80 Reactive/h and 80 Precious/h in, 10 Mech/h out.
        // Snapshot 10h old with 4 000 Reactive -> 4 000 - 800 = 3 200 left = 40h.
        let planet = derive(
            &layout(
                vec![
                    factory(1, 73),
                    factory(2, 73),
                    with_contents(pin(3, LAUNCHPAD), &[(REACTIVE, 4_000), (MECH, 500)]),
                ],
                now() - Duration::hours(10),
            ),
            &[],
        );

        let reactive = planet
            .imports
            .iter()
            .find(|i| i.type_id == REACTIVE)
            .unwrap();
        assert_eq!(reactive.qty_per_hour, Decimal::from(80));
        assert_eq!(reactive.lasts_hours, Decimal::from(40));
        // 4 000 / 80 per hour = 50h after the 10h-old snapshot = 40h from now.
        assert_eq!(reactive.depletes_at, now() + Duration::hours(40));
        let precious = planet
            .imports
            .iter()
            .find(|i| i.type_id == PRECIOUS)
            .unwrap();
        assert_eq!(precious.lasts_hours, Decimal::ZERO);
        // Nothing on hand: it ran out at the snapshot itself.
        assert_eq!(precious.depletes_at, now() - Duration::hours(10));
        assert!(planet.starved);
        assert_eq!(planet.attention, Some(Attention::Red));
        assert_eq!(planet.exports[0].type_id, MECH);
        assert_eq!(planet.exports[0].units_per_hour, Decimal::from(10));
    }

    #[test]
    fn storage_fill_uses_item_volume_and_pin_capacity() {
        // 51 000 Precious x 0.19 m3 = 9 690 m3 of a 10 000 m3 launchpad.
        let planet = derive(
            &layout(
                vec![
                    with_contents(pin(2, STORAGE), &[(REACTIVE, 1_000)]),
                    with_contents(pin(1, LAUNCHPAD), &[(PRECIOUS, 51_000)]),
                    pin(3, CC),
                ],
                now(),
            ),
            &[],
        );

        assert_eq!(planet.storage.len(), 2, "empty command center is hidden");
        assert_eq!(planet.storage[0].kind, StorageKind::Launchpad);
        assert_eq!(planet.storage[0].fill_percent, Decimal::new(969, 1));
        assert_eq!(planet.storage[0].value, Decimal::from(51_000 * 450));
        assert_eq!(planet.storage[1].kind, StorageKind::Storage);
        assert!(planet.storage_full);
        assert_eq!(planet.attention, Some(Attention::Red));
    }

    #[test]
    fn expired_and_expiring_extractors_drive_attention() {
        let expired = derive(
            &layout(vec![extractor(1, BASE, -Duration::hours(3))], now()),
            &[],
        );
        assert!(expired.extractors[0].expired);
        assert_eq!(expired.attention, Some(Attention::Red));
        assert_eq!(expired.next_expiry(), None);

        let expiring = derive(
            &layout(vec![extractor(1, BASE, Duration::hours(2))], now()),
            &[],
        );
        assert_eq!(expiring.attention, Some(Attention::Amber));
        assert_eq!(expiring.next_expiry(), Some(now() + Duration::hours(2)));
    }

    #[test]
    fn excluded_exports_keep_their_value_but_leave_the_total() {
        // One factory eats half the extracted Base, so surplus Base (unpriced)
        // is exported alongside Reactive.
        let planet = derive(
            &layout(
                vec![extractor(1, BASE, Duration::days(3)), factory(2, 126)],
                now(),
            ),
            &[REACTIVE],
        );
        let reactive = planet
            .exports
            .iter()
            .find(|e| e.type_id == REACTIVE)
            .unwrap();
        assert!(reactive.excluded);
        assert!(reactive.isk_per_month.is_some());
        let base = planet.exports.iter().find(|e| e.type_id == BASE).unwrap();
        assert_eq!(base.units_per_hour, Decimal::from(6_000));
        assert_eq!(base.isk_per_month, None);
        assert_eq!(planet.isk_per_month, Decimal::ZERO);
    }

    #[test]
    fn rollup_counts_alerts_and_picks_the_earliest_live_expiry() {
        let soon = derive(
            &layout(vec![extractor(1, BASE, Duration::hours(2))], now()),
            &[],
        );
        let later = derive(
            &layout(vec![extractor(1, BASE, Duration::days(2))], now()),
            &[],
        );
        let expired = derive(
            &layout(vec![extractor(1, BASE, -Duration::hours(1))], now()),
            &[],
        );
        let rollup = rollup(&[later, soon, expired]);
        assert_eq!(
            rollup.next_expiry.map(|(_, at)| at),
            Some(now() + Duration::hours(2))
        );
        assert_eq!(rollup.alert_count, 2);
        assert_eq!(rollup.expired_count, 1);
    }

    #[test]
    fn parses_the_synced_summary_shape() {
        let summary = serde_json::json!({ "planets": [{
            "planet_id": 40_050_359, "planet_type": "barren", "solar_system_id": 30_000_797,
            "upgrade_level": 5, "num_pins": 1, "last_update": "2026-10-01T06:23:47Z",
            "pins": [{
                "pin_id": 1, "type_id": 2_848, "schematic_id": null, "contents": [],
                "install_time": null, "expiry_time": "2026-10-05T06:00:00Z",
                "last_cycle_start": null,
                "extractor": { "product_type_id": 2_267, "qty_per_cycle": 6_000,
                               "cycle_time_seconds": 1_800, "head_count": 8 }
            }]
        }]});
        let planets = parse_planets_summary(&summary).unwrap();
        assert_eq!(planets[0].pins[0].extractor.unwrap().product_type_id, BASE);
    }
}
