use super::*;

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveSde {
    pub import_id: Uuid,
    pub source_version: String,
    pub source_label: String,
    pub source_checksum: String,
    pub completed_at: DateTime<Utc>,
    pub counts: ImportCounts,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlueprintSearchResult {
    pub blueprint_type_id: i64,
    pub blueprint_name: String,
    pub product_type_id: i64,
    pub product_name: String,
    pub group_name: Option<String>,
    pub published: bool,
    pub manufacturing_available: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeSearchResult {
    pub type_id: i64,
    pub type_name: String,
    pub group_name: Option<String>,
    pub published: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManufacturingRecipe {
    pub blueprint_type_id: i64,
    pub blueprint_name: String,
    pub duration_seconds: Option<i64>,
    pub materials: Vec<RecipeLine>,
    pub products: Vec<RecipeLine>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeLine {
    pub type_id: i64,
    pub type_name: String,
    pub quantity: i64,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactionFormulaSearchResult {
    pub reaction_formula_type_id: i64,
    pub reaction_formula_name: String,
    pub product_type_id: i64,
    pub product_name: String,
    pub group_name: Option<String>,
    pub published: bool,
}

/// The reaction-formula read-side counterpart of `ManufacturingRecipe`.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactionFormulaRecipe {
    pub reaction_formula_type_id: i64,
    pub reaction_formula_name: String,
    pub duration_seconds: Option<i64>,
    pub materials: Vec<RecipeLine>,
    pub products: Vec<RecipeLine>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SolarSystemSearchResult {
    pub solar_system_id: i64,
    pub solar_system_name: String,
    pub security_class: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NpcStationSearchResult {
    pub station_id: i64,
    pub station_name: String,
    pub station_type_id: i64,
    pub station_type_name: Option<String>,
    pub solar_system_id: i64,
    pub solar_system_name: String,
    pub region_id: i64,
    pub region_name: String,
    pub security_class: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionSummary {
    pub region_id: i64,
    pub region_name: String,
}

/// One row of `sde_market_groups` -- a flat adjacency-list representation
/// (`parent_group_id` is `None` for a root group). Left flat deliberately:
/// nesting it into a tree is a presentation concern for whoever consumes
/// the full list, not something the read repository should impose.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketGroupNode {
    pub market_group_id: i64,
    pub name: String,
    pub parent_group_id: Option<i64>,
    /// Published, market-browsable types whose `market_group_id` is
    /// exactly this group -- direct membership only, not descendant
    /// groups. Nesting this into a tree's rolled-up per-node total (own
    /// items plus every descendant's) is the same presentation-layer
    /// concern that already nests the flat list itself.
    pub item_count: u64,
}

/// One market-browsable type: published, carrying a market group, matched
/// against a market-item listing's category/search filters. Deliberately
/// bare (just enough to join market data onto by `type_id`) -- richer type
/// detail belongs to whichever endpoint actually needs it.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketItemCandidate {
    pub type_id: i64,
    pub type_name: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureManufacturingModifiers {
    pub type_id: i64,
    pub material_reduction_percent: String,
    pub time_reduction_percent: String,
    pub job_cost_reduction_percent: String,
}

/// One of EVE's `industryTargetFilters` -- a named `(categoryIDs, groupIDs)`
/// set that a rig's material or time bonus is restricted to.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndustryTargetFilter {
    pub filter_id: i64,
    pub name: String,
    pub category_ids: Vec<i64>,
    pub group_ids: Vec<i64>,
}

/// What a rig's bonuses are allowed to affect, resolved for one activity.
/// A `None` field is unrestricted (no `filterID`, or an SDE without the
/// dataset).
#[derive(Debug, Clone, Eq, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RigApplicability {
    pub material: Option<IndustryTargetFilter>,
    pub time: Option<IndustryTargetFilter>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RigManufacturingModifiers {
    pub type_id: i64,
    pub material_reduction_percent: String,
    pub time_reduction_percent: String,
    pub compatible_with_structure: Option<bool>,
    pub applies_to: RigApplicability,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactionRigModifiers {
    pub type_id: i64,
    pub material_reduction_percent: String,
    pub time_reduction_percent: String,
    pub compatible_with_structure: Option<bool>,
    pub applies_to: RigApplicability,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SdeInventoryTypeMetadata {
    pub group_name: Option<String>,
    pub packaged_volume_m3: Option<Decimal>,
}

/// A type's broad EVE classification -- its `invGroups` group and that
/// group's `invCategories` category (e.g. group "Heavy Assault Cruiser",
/// category "Ship"). Used by the Builds library to label and filter a
/// Build by what its output item *is*, and by facility-rig applicability to
/// test a job's product against a rig's target filter (which needs the ids).
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct SdeTypeClassification {
    pub category_id: Option<i64>,
    pub category_name: Option<String>,
    pub group_id: Option<i64>,
    pub group_name: Option<String>,
}

/// Everything a "type reference dictionary" needs for one `type_id`: its
/// own name, its `invGroups` group and `invCategories` category, and its
/// packaged volume. A superset of [`SdeTypeClassification`] +
/// [`SdeInventoryTypeMetadata`] returned by one join so a bulk consumer (the
/// verification workbook's `Types` sheet) makes a single query. Every field
/// is optional: a `type_id` absent from the active import yields no map
/// entry, and a present type may still lack a classified group / a
/// published volume.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct TypeReference {
    pub type_name: Option<String>,
    pub group_id: Option<i64>,
    pub group_name: Option<String>,
    pub category_id: Option<i64>,
    pub category_name: Option<String>,
    pub packaged_volume_m3: Option<Decimal>,
}

/// A PI factory schematic: one cycle of `cycle_time_seconds` consumes
/// `inputs` and yields `outputs` (in practice exactly one output line).
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PlanetSchematic {
    pub schematic_id: i64,
    pub name: String,
    pub cycle_time_seconds: i64,
    pub inputs: Vec<PlanetSchematicLine>,
    pub outputs: Vec<PlanetSchematicLine>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct PlanetSchematicLine {
    pub type_id: i64,
    pub quantity: i64,
}

/// A planet's EVE display name (e.g. `Q-3HS5 IV`) plus its solar system.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PlanetReference {
    pub planet_id: i64,
    pub name: String,
    pub solar_system_id: i64,
    pub solar_system_name: String,
    pub security_status: Option<Decimal>,
}

/// The published production recipes that output one type: its
/// manufacturing blueprint and/or reaction formula (the same first
/// published, lowest-id pick [`SdeReadRepository::manufacturing_blueprint_for_product`]
/// / [`SdeReadRepository::reaction_formula_for_product`] make).
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub struct ProductRecipeRefs {
    pub blueprint_type_id: Option<i64>,
    pub reaction_formula_type_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CandidateRecipeKind {
    Manufacturing,
    Reaction,
}

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct ManufacturableCandidateScope {
    pub recipe_kinds: BTreeSet<CandidateRecipeKind>,
    pub category_ids: BTreeSet<i64>,
    pub group_ids: BTreeSet<i64>,
    pub meta_group_ids: BTreeSet<i64>,
    pub market_group_root_ids: BTreeSet<i64>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum CandidateRecipeIdentity {
    Manufacturing { blueprint_type_id: i64 },
    Reaction { reaction_formula_type_id: i64 },
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CandidateMarketGroup {
    pub market_group_id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CandidateProductClassification {
    pub category_id: Option<i64>,
    pub category_name: Option<String>,
    pub group_id: Option<i64>,
    pub group_name: Option<String>,
    pub meta_group_id: Option<i64>,
    pub meta_group_name: Option<String>,
    pub market_group_id: Option<i64>,
    pub market_group_name: Option<String>,
    pub market_group_ancestry: Vec<CandidateMarketGroup>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ManufacturableCandidateRecipe {
    pub import_id: Uuid,
    pub source_version: String,
    pub identity: CandidateRecipeIdentity,
    pub recipe_name: String,
    pub duration_seconds: Option<i64>,
    pub materials: Vec<RecipeLine>,
    pub products: Vec<RecipeLine>,
    pub primary_product_type_id: i64,
    pub primary_product_published: bool,
    pub recipe_type_published: bool,
    pub classification: CandidateProductClassification,
    pub has_additional_products: bool,
}

#[async_trait]
pub trait SdeReadRepository: Send + Sync {
    async fn active_sde(&self) -> Result<Option<ActiveSde>, SdeError>;
    async fn manufacturable_candidates(
        &self,
        scope: &ManufacturableCandidateScope,
    ) -> Result<Vec<ManufacturableCandidateRecipe>, SdeError> {
        let _ = scope;
        Err(SdeError::Storage(
            "candidate discovery is unsupported by this repository".into(),
        ))
    }
    async fn search_manufacturing_blueprints(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<BlueprintSearchResult>, SdeError>;
    async fn manufacturing_recipe(
        &self,
        blueprint_type_id: i64,
    ) -> Result<Option<ManufacturingRecipe>, SdeError>;
    async fn search_reaction_formulas(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<ReactionFormulaSearchResult>, SdeError> {
        let _ = (query, limit);
        Ok(Vec::new())
    }
    async fn reaction_formula(
        &self,
        reaction_formula_type_id: i64,
    ) -> Result<Option<ReactionFormulaRecipe>, SdeError> {
        let _ = reaction_formula_type_id;
        Ok(None)
    }
    /// Reverse lookup: does a manufacturing blueprint produce this type as
    /// output? Returns the blueprint's type ID if so.
    async fn manufacturing_blueprint_for_product(
        &self,
        product_type_id: i64,
    ) -> Result<Option<i64>, SdeError> {
        let _ = product_type_id;
        Ok(None)
    }
    /// Reverse lookup: does a reaction formula produce this type as output?
    /// Returns the formula's type ID if so.
    async fn reaction_formula_for_product(
        &self,
        product_type_id: i64,
    ) -> Result<Option<i64>, SdeError> {
        let _ = product_type_id;
        Ok(None)
    }
    /// Bulk [`ProductRecipeRefs`] for `product_type_ids` -- one entry per
    /// type with at least one published recipe. The default loops over the
    /// single-type lookups (fakes); the Postgres implementation answers in
    /// one query.
    async fn production_recipes_for_products(
        &self,
        product_type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, ProductRecipeRefs>, SdeError> {
        let mut refs = std::collections::BTreeMap::new();
        for &type_id in product_type_ids {
            let recipe = ProductRecipeRefs {
                blueprint_type_id: self.manufacturing_blueprint_for_product(type_id).await?,
                reaction_formula_type_id: self.reaction_formula_for_product(type_id).await?,
            };
            if recipe != ProductRecipeRefs::default() {
                refs.insert(type_id, recipe);
            }
        }
        Ok(refs)
    }
    async fn search_types(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<TypeSearchResult>, SdeError>;
    async fn type_group_names(
        &self,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, String>, SdeError> {
        let _ = type_ids;
        Ok(std::collections::BTreeMap::new())
    }
    /// Bulk type_id -> its group + category names (see
    /// `SdeTypeClassification`). Types absent from the active import are
    /// simply omitted from the map.
    async fn type_classifications(
        &self,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, SdeTypeClassification>, SdeError> {
        let _ = type_ids;
        Ok(std::collections::BTreeMap::new())
    }
    /// Bulk type_id -> the type's own `name_en` (e.g. a skill or item
    /// name), unlike `type_group_names` which returns the *group's* name.
    async fn type_names(
        &self,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, String>, SdeError> {
        let _ = type_ids;
        Ok(std::collections::BTreeMap::new())
    }
    /// Bulk type_id -> [`TypeReference`] (name + group + category + packaged
    /// volume) from one join across `sde_types` / `sde_groups` /
    /// `sde_categories`. The single metadata read the verification-workbook
    /// `Types` sheet needs; replaces separate `type_names` /
    /// `type_classifications` / `inventory_type_metadata` calls at that call
    /// site.
    async fn type_reference(
        &self,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, TypeReference>, SdeError> {
        let _ = type_ids;
        Ok(std::collections::BTreeMap::new())
    }
    async fn inventory_type_metadata(
        &self,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, SdeInventoryTypeMetadata>, SdeError> {
        let _ = type_ids;
        Ok(std::collections::BTreeMap::new())
    }
    async fn search_structure_types(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<TypeSearchResult>, SdeError> {
        let _ = (query, limit);
        Ok(Vec::new())
    }
    async fn search_structure_rigs(
        &self,
        query: &str,
        limit: u32,
        structure_type_id: Option<i64>,
    ) -> Result<Vec<TypeSearchResult>, SdeError> {
        let _ = (query, limit, structure_type_id);
        Ok(Vec::new())
    }
    async fn structure_manufacturing_modifiers(
        &self,
        type_id: i64,
    ) -> Result<Option<StructureManufacturingModifiers>, SdeError> {
        let _ = type_id;
        Ok(None)
    }
    /// Every `industryTargetFilters` row in the active SDE (≤ 18). Empty when
    /// the import predates the dataset, in which case rigs have no resolvable
    /// applicability and are treated as unrestricted.
    async fn industry_target_filters(&self) -> Result<Vec<IndustryTargetFilter>, SdeError> {
        Ok(Vec::new())
    }
    async fn rig_manufacturing_modifiers(
        &self,
        type_id: i64,
        security_class: &str,
        structure_type_id: Option<i64>,
    ) -> Result<Option<RigManufacturingModifiers>, SdeError> {
        let _ = (type_id, security_class, structure_type_id);
        Ok(None)
    }
    async fn search_solar_systems(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<SolarSystemSearchResult>, SdeError> {
        let _ = (query, limit);
        Ok(Vec::new())
    }
    /// Matches on the station's own name *or* its solar system's name (a
    /// station whose name doesn't happen to start with its system's name
    /// would otherwise be unfindable by searching the system, e.g. from the
    /// Market Scope Selector's global search) -- both against the same
    /// `query`, not two separate parameters.
    async fn search_npc_stations(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<NpcStationSearchResult>, SdeError> {
        let _ = (query, limit);
        Ok(Vec::new())
    }
    /// Every region in the active SDE dataset -- the top level of the
    /// market-scope selector, unrelated to `PriceSource`.
    /// Restricted to the regions a normal market browse actually means --
    /// implementations should exclude Anoikis/wormhole, Abyssal, Void, and
    /// other non-standard regions rather than returning the SDE's full,
    /// unfiltered `sde_regions` table. See the concrete implementation for
    /// the exact rule and the live-data investigation behind it (an
    /// SDE-presence heuristic like "has an NPC station" is NOT sufficient --
    /// it wrongly admits Thera's wormhole region, which has 4 stations, and
    /// wrongly excludes real, actively-used deep-nullsec regions that have
    /// zero NPC stations, such as Feythabolis).
    async fn list_regions(&self) -> Result<Vec<RegionSummary>, SdeError> {
        Ok(Vec::new())
    }
    /// Name search over the same market-relevant region set `list_regions`
    /// returns -- the Market Scope Selector's global search needs to match
    /// a typed region name without pulling the whole 67-region list into
    /// the browser to filter client-side. `query.len() < 2` returns empty,
    /// matching `search_solar_systems`'s convention.
    async fn search_regions(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<RegionSummary>, SdeError> {
        let _ = (query, limit);
        Ok(Vec::new())
    }
    /// NPC stations within one region -- the SDE-backed half of the
    /// location list under a selected region (the other half, known player
    /// structures, comes from `MarketRepository::known_locations_in_region`
    /// since structure names are workspace-resolved, not SDE data).
    async fn list_npc_stations_in_region(
        &self,
        region_id: i64,
    ) -> Result<Vec<NpcStationSearchResult>, SdeError> {
        let _ = region_id;
        Ok(Vec::new())
    }
    /// Resolves a specific, small set of station IDs by identity -- unlike
    /// `search_npc_stations` (name search) or `list_npc_stations_in_region`
    /// (region-scoped), this looks stations up directly by ID regardless of
    /// name or region, for a caller that already knows exactly which
    /// stations it wants (the Market Scope Selector's curated "Major Hubs"
    /// list). A station ID absent from the active SDE silently doesn't
    /// appear in the result rather than erroring, so a renamed/removed hub
    /// station degrades to "one fewer hub shown," not a broken endpoint.
    async fn resolve_npc_stations(
        &self,
        station_ids: &[i64],
    ) -> Result<Vec<NpcStationSearchResult>, SdeError> {
        let _ = station_ids;
        Ok(Vec::new())
    }
    /// Every market group in the active SDE dataset, flat -- the raw
    /// material for the market-group/category hierarchy. See
    /// `MarketGroupNode` for why this stays flat rather than pre-nested.
    async fn list_market_groups(&self) -> Result<Vec<MarketGroupNode>, SdeError> {
        Ok(Vec::new())
    }
    /// A paginated, filtered page of market-browsable types (published,
    /// carrying a market group) plus the total match count -- the item
    /// summary table's candidate list. `market_group_id` is an exact match
    /// (not descendant-inclusive); `search` matches on name. Bounded by
    /// `page`/`page_size` so a caller can never pull the whole catalog in
    /// one call.
    async fn list_market_items(
        &self,
        market_group_id: Option<i64>,
        search: &str,
        page: u32,
        page_size: u32,
    ) -> Result<(Vec<MarketItemCandidate>, u64), SdeError> {
        let _ = (market_group_id, search, page, page_size);
        Ok((Vec::new(), 0))
    }
    /// Every published, market-browsable type belonging to `root_group_id`
    /// or any of its descendant market groups -- the descendant-inclusive
    /// counterpart to `list_market_items`'s exact-match `market_group_id`
    /// filter, matching what `MarketCategoryNode.item_count` already rolls
    /// up (`build_market_category_tree`). Unbounded/unpaginated by design:
    /// callers that need this (bulk "request prices for this category")
    /// are registering coverage, not rendering a table, so there's no
    /// per-page cap to apply -- the caller enforces its own defensive
    /// ceiling on the result size instead.
    async fn list_market_group_subtree_item_ids(
        &self,
        root_group_id: i64,
    ) -> Result<Vec<MarketItemCandidate>, SdeError> {
        let _ = root_group_id;
        Ok(Vec::new())
    }
    async fn search_reaction_rigs(
        &self,
        query: &str,
        limit: u32,
        structure_type_id: Option<i64>,
    ) -> Result<Vec<TypeSearchResult>, SdeError> {
        let _ = (query, limit, structure_type_id);
        Ok(Vec::new())
    }
    async fn reaction_rig_modifiers(
        &self,
        type_id: i64,
        security_class: &str,
        structure_type_id: Option<i64>,
    ) -> Result<Option<ReactionRigModifiers>, SdeError> {
        let _ = (type_id, security_class, structure_type_id);
        Ok(None)
    }
    /// PI schematics by id. Ids absent from the active import (or an import
    /// that predates planetary data) yield no entry.
    async fn planet_schematics(
        &self,
        schematic_ids: &[i64],
    ) -> Result<BTreeMap<i64, PlanetSchematic>, SdeError> {
        let _ = schematic_ids;
        Ok(BTreeMap::new())
    }
    /// Planet names and systems by planet id; unknown ids yield no entry.
    async fn planet_references(
        &self,
        planet_ids: &[i64],
    ) -> Result<BTreeMap<i64, PlanetReference>, SdeError> {
        let _ = planet_ids;
        Ok(BTreeMap::new())
    }
}
