use super::*;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NormalizedSde {
    pub source_label: String,
    pub source_checksum: String,
    pub source_version: String,
    pub types: Vec<ImportType>,
    pub categories: Vec<ImportCategory>,
    pub groups: Vec<ImportGroup>,
    pub meta_groups: Vec<ImportMetaGroup>,
    pub market_groups: Vec<ImportMarketGroup>,
    pub blueprints: Vec<ImportBlueprint>,
    pub solar_systems: Vec<ImportSolarSystem>,
    pub constellations: Vec<ImportConstellation>,
    pub regions: Vec<ImportRegion>,
    pub npc_stations: Vec<ImportNpcStation>,
    pub structure_modifiers: Vec<ImportStructureModifiers>,
    pub rig_modifiers: Vec<ImportRigModifiers>,
    pub reaction_formulas: Vec<ImportReactionFormula>,
    pub reaction_rig_modifiers: Vec<ImportRigModifiers>,
    /// EVE's `industryTargetFilters` -- the fixed set (≤ 18) of
    /// `(categoryIDs, groupIDs)` filters that a rig's `material_filter_ids` /
    /// `time_filter_ids` point at. Empty when the SDE predates the dataset.
    pub industry_target_filters: Vec<ImportIndustryTargetFilter>,
    /// PI factory schematics. Empty when the SDE lacks `planetSchematics`.
    pub planet_schematics: Vec<ImportPlanetSchematic>,
    /// Planets with EVE-style derived names. Empty when the SDE lacks
    /// `mapPlanets` (or solar systems to name them from).
    pub planets: Vec<ImportPlanet>,
    pub skipped_blueprints: u64,
    pub skipped_reaction_formulas: u64,
}

impl NormalizedSde {
    #[must_use]
    pub fn counts(&self) -> ImportCounts {
        ImportCounts {
            types: self.types.len() as u64,
            categories: self.categories.len() as u64,
            groups: self.groups.len() as u64,
            meta_groups: self.meta_groups.len() as u64,
            market_groups: self.market_groups.len() as u64,
            classified_types: self
                .types
                .iter()
                .filter(|item| item.meta_group_id.is_some())
                .count() as u64,
            blueprints: self.blueprints.len() as u64,
            material_lines: self
                .blueprints
                .iter()
                .map(|blueprint| blueprint.materials.len() as u64)
                .sum(),
            product_lines: self
                .blueprints
                .iter()
                .map(|blueprint| blueprint.products.len() as u64)
                .sum(),
            skipped_blueprints: self.skipped_blueprints,
            reaction_formulas: self.reaction_formulas.len() as u64,
            reaction_material_lines: self
                .reaction_formulas
                .iter()
                .map(|formula| formula.materials.len() as u64)
                .sum(),
            reaction_product_lines: self
                .reaction_formulas
                .iter()
                .map(|formula| formula.products.len() as u64)
                .sum(),
            skipped_reaction_formulas: self.skipped_reaction_formulas,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ImportType {
    pub type_id: i64,
    pub name: String,
    pub group_id: Option<i64>,
    pub group_name: Option<String>,
    pub market_group_id: Option<i64>,
    pub meta_group_id: Option<i64>,
    pub packaged_volume_m3: Option<Decimal>,
    pub published: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ImportCategory {
    pub category_id: i64,
    pub name: String,
    pub published: bool,
}
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ImportGroup {
    pub group_id: i64,
    pub name: String,
    pub category_id: Option<i64>,
    pub published: bool,
}
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ImportMetaGroup {
    pub meta_group_id: i64,
    pub name: String,
}
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ImportMarketGroup {
    pub market_group_id: i64,
    pub name: String,
    pub parent_group_id: Option<i64>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ImportBlueprint {
    pub blueprint_type_id: i64,
    pub name: String,
    pub duration_seconds: Option<i64>,
    pub materials: Vec<ImportMaterial>,
    pub products: Vec<ImportProduct>,
}

/// A reaction formula (the SDE's `"reaction"` blueprint activity). Unlike a
/// manufacturing blueprint, reaction formulas have no material efficiency --
/// the game gives Refinery structures no base reaction discount, only
/// reaction rigs do (see `ImportRigModifiers` reaction rig entries).
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ImportReactionFormula {
    pub reaction_formula_type_id: i64,
    pub name: String,
    pub duration_seconds: Option<i64>,
    pub materials: Vec<ImportMaterial>,
    pub products: Vec<ImportProduct>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ImportSolarSystem {
    pub solar_system_id: i64,
    pub name: String,
    pub constellation_id: Option<i64>,
    pub region_id: Option<i64>,
    pub security_status: Option<Decimal>,
    pub wormhole_class_id: Option<i64>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ImportConstellation {
    pub constellation_id: i64,
    pub name: String,
    pub region_id: i64,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ImportRegion {
    pub region_id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ImportNpcStation {
    pub station_id: i64,
    pub name: String,
    pub solar_system_id: i64,
    pub owner_corporation_id: i64,
    pub station_type_id: i64,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ImportPlanetSchematic {
    pub schematic_id: i64,
    pub name: String,
    pub cycle_time_seconds: i64,
    pub types: Vec<ImportPlanetSchematicType>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ImportPlanetSchematicType {
    pub type_id: i64,
    pub is_input: bool,
    pub quantity: i64,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ImportPlanet {
    pub planet_id: i64,
    pub solar_system_id: i64,
    pub celestial_index: i64,
    /// `"<system> <ROMAN(celestial_index)>"`, e.g. `Q-3HS5 IV`.
    pub name: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ImportStructureModifiers {
    pub type_id: i64,
    pub material_reduction_percent: Decimal,
    pub time_reduction_percent: Decimal,
    pub job_cost_reduction_percent: Decimal,
    pub structure_size: Option<i64>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ImportRigModifiers {
    pub type_id: i64,
    pub material_reduction_percent: Decimal,
    pub time_reduction_percent: Decimal,
    pub high_sec_multiplier: Decimal,
    pub low_sec_multiplier: Decimal,
    pub null_sec_multiplier: Decimal,
    pub compatible_structure_group_ids: Vec<i64>,
    pub rig_size: Option<i64>,
    /// Every `industryTargetFilters._key` this rig's material bonus is
    /// restricted to, from `industryModifierSources[type].<activity>.material[].filterID`
    /// (a multi-scope rig references several). Empty = unrestricted (no
    /// `filterID`, or the SDE lacks the dataset). Sorted, de-duplicated.
    /// Applicability is the union of these filters' category/group sets.
    pub material_filter_ids: Vec<i64>,
    /// The same for the time bonus (`...time[].filterID`).
    pub time_filter_ids: Vec<i64>,
}

/// One row of EVE's `industryTargetFilters` -- a named `(categoryIDs,
/// groupIDs)` set. A rig's bonus applies to a job iff the produced item's
/// category or group is listed here (an empty vec means "not constrained on
/// that axis", not "matches nothing").
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ImportIndustryTargetFilter {
    pub filter_id: i64,
    pub name: String,
    pub category_ids: Vec<i64>,
    pub group_ids: Vec<i64>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ImportMaterial {
    pub type_id: i64,
    pub quantity: i64,
    pub position: i32,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ImportProduct {
    pub type_id: i64,
    pub quantity: i64,
    pub position: i32,
}
