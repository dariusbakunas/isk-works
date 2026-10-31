use std::collections::{BTreeMap, BTreeSet};

use iskworks_sde::{PlanetCommodity, PlanetPinKind, PlanetPinType, PlanetSchematic};
use rust_decimal::prelude::ToPrimitive;

/// Imports through a customs office pay this share of the export tax
/// (verified in game, 2026-10-31).
pub const IMPORT_TAX_RATIO: f64 = 0.5;

/// Command center CPU (tf) and power (MW) by Command Center Upgrades level.
/// The game labels these levels I–VI. Every planet type's command center has
/// the same figures; level 0 matches the SDE, levels 1–5 were read from the
/// in-game upgrade preview (2026-10-31).
pub const COMMAND_CENTER_LEVELS: [(f64, f64); 6] = [
    (1_675.0, 6_000.0),
    (7_057.0, 9_000.0),
    (12_136.0, 12_000.0),
    (17_215.0, 15_000.0),
    (21_315.0, 17_000.0),
    (25_415.0, 19_000.0),
];

/// CPU and power of a command center at this upgrade level (clamped to 5).
#[must_use]
pub fn command_center_output(level: u8) -> (f64, f64) {
    COMMAND_CENTER_LEVELS[usize::from(level.min(5))]
}

/// One schematic, by its output.
#[derive(Debug, Clone, PartialEq)]
pub struct Recipe {
    pub schematic_id: i64,
    pub output_type_id: i64,
    pub output_quantity: f64,
    pub cycle_hours: f64,
    pub inputs: Vec<(i64, f64)>,
    /// 1 for P1 up to 4 for P4.
    pub tier: u8,
    pub processor_pin_ids: Vec<i64>,
}

impl Recipe {
    /// Output per hour of one factory running this recipe.
    #[must_use]
    pub fn output_per_hour(&self) -> f64 {
        self.output_quantity / self.cycle_hours
    }
}

/// The static PI data the planner works from, built from the SDE.
#[derive(Debug, Clone, Default)]
pub struct PiCatalogue {
    resources_by_planet_type: BTreeMap<i64, BTreeSet<i64>>,
    tiers: BTreeMap<i64, u8>,
    recipes: BTreeMap<i64, Recipe>,
    pins: BTreeMap<i64, PlanetPinType>,
    commodities: BTreeMap<i64, PlanetCommodity>,
    volumes: BTreeMap<i64, f64>,
}

impl PiCatalogue {
    /// `volumes` are m³ per unit by type id.
    #[must_use]
    pub fn build(
        pins: Vec<PlanetPinType>,
        schematics: Vec<PlanetSchematic>,
        commodities: BTreeMap<i64, PlanetCommodity>,
        volumes: BTreeMap<i64, f64>,
    ) -> Self {
        let mut resources_by_planet_type: BTreeMap<i64, BTreeSet<i64>> = BTreeMap::new();
        let mut tiers = BTreeMap::new();
        for pin in pins
            .iter()
            .filter(|pin| pin.kind == PlanetPinKind::ExtractorResource)
        {
            if let (Some(planet_type), Some(resource)) = (pin.planet_type_id, pin.harvester_type_id)
            {
                resources_by_planet_type
                    .entry(planet_type)
                    .or_default()
                    .insert(resource);
                tiers.insert(resource, 0);
            }
        }

        let mut recipes = BTreeMap::new();
        for schematic in &schematics {
            let [output] = schematic.outputs.as_slice() else {
                continue;
            };
            if schematic.cycle_time_seconds <= 0 || output.quantity <= 0 {
                continue;
            }
            recipes.insert(
                output.type_id,
                Recipe {
                    schematic_id: schematic.schematic_id,
                    output_type_id: output.type_id,
                    output_quantity: output.quantity as f64,
                    cycle_hours: schematic.cycle_time_seconds as f64 / 3_600.0,
                    inputs: schematic
                        .inputs
                        .iter()
                        .map(|line| (line.type_id, line.quantity as f64))
                        .collect(),
                    tier: 0,
                    processor_pin_ids: schematic.pin_type_ids.clone(),
                },
            );
        }
        // A recipe's tier is one above its highest input. Resolve until no
        // tier changes; P4 is at most four rounds away from P0.
        for _ in 0..recipes.len() {
            let mut changed = false;
            for recipe in recipes.values_mut() {
                let tier = recipe
                    .inputs
                    .iter()
                    .map(|(input, _)| tiers.get(input).copied())
                    .collect::<Option<Vec<u8>>>()
                    .and_then(|input_tiers| input_tiers.into_iter().max())
                    .map(|max| max + 1);
                if let Some(tier) = tier {
                    if tiers.insert(recipe.output_type_id, tier) != Some(tier) {
                        changed = true;
                    }
                    recipe.tier = tier;
                }
            }
            if !changed {
                break;
            }
        }
        recipes.retain(|_, recipe| recipe.tier > 0);

        Self {
            resources_by_planet_type,
            tiers,
            recipes,
            pins: pins.into_iter().map(|pin| (pin.type_id, pin)).collect(),
            commodities,
            volumes,
        }
    }

    /// The raw resources a planet type yields; empty for unknown types.
    pub fn resources(&self, planet_type_id: i64) -> impl Iterator<Item = i64> + '_ {
        self.resources_by_planet_type
            .get(&planet_type_id)
            .into_iter()
            .flatten()
            .copied()
    }

    #[must_use]
    pub fn tier(&self, type_id: i64) -> Option<u8> {
        self.tiers.get(&type_id).copied()
    }

    #[must_use]
    pub fn recipe(&self, output_type_id: i64) -> Option<&Recipe> {
        self.recipes.get(&output_type_id)
    }

    /// Every recipe, by output type id.
    pub fn recipes(&self) -> impl Iterator<Item = &Recipe> {
        self.recipes.values()
    }

    /// The first structure of `kind` buildable on the planet type.
    #[must_use]
    pub fn pin_on(&self, kind: PlanetPinKind, planet_type_id: i64) -> Option<&PlanetPinType> {
        self.pins
            .values()
            .find(|pin| pin.kind == kind && pin.planet_type_id == Some(planet_type_id))
    }

    /// The structure type, by id.
    #[must_use]
    pub fn pin(&self, type_id: i64) -> Option<&PlanetPinType> {
        self.pins.get(&type_id)
    }

    /// Planets without a command center (Shattered, unique planets) can't
    /// be colonised.
    #[must_use]
    pub fn can_colonise(&self, planet_type_id: i64) -> bool {
        self.pin_on(PlanetPinKind::CommandCenter, planet_type_id)
            .is_some()
    }

    /// The factory that runs `recipe` on this planet type, if any.
    #[must_use]
    pub fn processor_for(&self, recipe: &Recipe, planet_type_id: i64) -> Option<&PlanetPinType> {
        recipe
            .processor_pin_ids
            .iter()
            .filter_map(|id| self.pins.get(id))
            .find(|pin| pin.planet_type_id == Some(planet_type_id))
    }

    /// The link structure's costs: `(cpu, power, cpu per km, power per km)`.
    #[must_use]
    pub fn link_cost(&self) -> (f64, f64, f64, f64) {
        self.pins
            .values()
            .find(|pin| pin.kind == PlanetPinKind::Link)
            .map_or((0.0, 0.0, 0.0, 0.0), |link| {
                (
                    decimal_f64(link.cpu_load),
                    decimal_f64(link.power_load),
                    decimal_f64(link.cpu_load_per_km),
                    decimal_f64(link.power_load_per_km),
                )
            })
    }

    /// m³ per unit; 0 when unknown.
    #[must_use]
    pub fn volume(&self, type_id: i64) -> f64 {
        self.volumes.get(&type_id).copied().unwrap_or(0.0)
    }

    /// Export tax per unit at a customs `rate` (0.1 = 10 %).
    #[must_use]
    pub fn export_tax(&self, type_id: i64, rate: f64) -> f64 {
        self.commodities.get(&type_id).map_or(0.0, |commodity| {
            decimal_f64(Some(commodity.export_tax_multiplier)) * rate
        })
    }

    /// Import tax per unit at a customs `rate`: half of the import multiplier.
    #[must_use]
    pub fn import_tax(&self, type_id: i64, rate: f64) -> f64 {
        self.commodities.get(&type_id).map_or(0.0, |commodity| {
            decimal_f64(Some(commodity.import_tax_multiplier)) * rate * IMPORT_TAX_RATIO
        })
    }
}

pub(super) fn decimal_f64(value: Option<rust_decimal::Decimal>) -> f64 {
    value.and_then(|value| value.to_f64()).unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::*;
    use super::*;

    #[test]
    fn planet_types_yield_their_extractor_resources() {
        let catalogue = catalogue();
        assert_eq!(
            catalogue.resources(BARREN).collect::<Vec<_>>(),
            vec![2073, BASE_METALS, AQUEOUS_LIQUIDS, NOBLE_METALS, 2288]
        );
        assert!(catalogue.resources(LAVA).any(|id| id == HEAVY_METALS));
        assert_eq!(catalogue.resources(GAS).count(), 0);
    }

    #[test]
    fn tiers_follow_the_recipe_graph() {
        let catalogue = catalogue();
        assert_eq!(catalogue.tier(BASE_METALS), Some(0));
        assert_eq!(catalogue.tier(WATER), Some(1));
        assert_eq!(catalogue.tier(MECHANICAL_PARTS), Some(2));
        assert_eq!(catalogue.tier(ROBOTICS), Some(3));
        assert_eq!(
            catalogue.recipe(ROBOTICS).map(|recipe| recipe.tier),
            Some(3)
        );
        assert_eq!(catalogue.recipe(ROBOTICS).unwrap().output_per_hour(), 3.0);
        assert_eq!(catalogue.recipe(WATER).unwrap().output_per_hour(), 40.0);
    }

    #[test]
    fn a_recipe_with_an_unresolvable_input_is_dropped() {
        let mut schematics = schematics();
        schematics[0].inputs[0].type_id = 999;
        let catalogue = PiCatalogue::build(pin_types(), schematics, commodities(), volumes());
        assert!(catalogue.recipe(WATER).is_none());
        assert!(catalogue.recipe(REACTIVE_METALS).is_some());
    }

    #[test]
    fn factories_are_picked_by_planet_type() {
        let catalogue = catalogue();
        let robotics = catalogue.recipe(ROBOTICS).unwrap();
        assert_eq!(
            catalogue
                .processor_for(robotics, PLASMA)
                .map(|pin| pin.type_id),
            Some(2472)
        );
        assert!(catalogue.processor_for(robotics, GAS).is_none());
    }

    #[test]
    fn only_planets_with_a_command_center_can_be_colonised() {
        let catalogue = catalogue();
        assert!(catalogue.can_colonise(BARREN));
        assert!(!catalogue.can_colonise(SHATTERED));
    }

    #[test]
    fn command_center_levels_match_the_game() {
        assert_eq!(command_center_output(0), (1_675.0, 6_000.0));
        assert_eq!(command_center_output(3), (17_215.0, 15_000.0));
        assert_eq!(command_center_output(9), (25_415.0, 19_000.0));
    }

    #[test]
    fn customs_tax_matches_the_in_game_transfers() {
        let catalogue = catalogue();
        // 3 480 Water-tier P1 at 1 % cost 13 920 ISK to export.
        assert!((3_480.0 * catalogue.export_tax(WATER, 0.01) - 13_920.0).abs() < 1e-6);
        // 47 111 Chiral Structures at 1 % cost 94 222 ISK to import.
        assert!((47_111.0 * catalogue.import_tax(CHIRAL_STRUCTURES, 0.01) - 94_222.0).abs() < 1e-6);
    }
}
