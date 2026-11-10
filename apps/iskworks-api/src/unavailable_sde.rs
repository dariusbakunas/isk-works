//! `UnavailableSdeRepository`: the null [`SdeReadRepository`] `AppState`
//! starts with until a real one is wired in. Every read answers as if no SDE
//! had been imported: no active import, nothing found. Candidate discovery
//! is the one exception and reports itself unsupported.

use std::collections::BTreeMap;

use iskworks_sde::*;

pub(crate) struct UnavailableSdeRepository;

#[async_trait::async_trait]
impl SdeReadRepository for UnavailableSdeRepository {
    async fn active_sde(&self) -> Result<Option<ActiveSde>, SdeError> {
        Ok(None)
    }
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
    ) -> Result<Vec<BlueprintSearchResult>, SdeError> {
        let _ = (query, limit);
        Ok(Vec::new())
    }
    async fn manufacturing_recipe(
        &self,
        blueprint_type_id: i64,
    ) -> Result<Option<ManufacturingRecipe>, SdeError> {
        let _ = blueprint_type_id;
        Ok(None)
    }
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
    async fn manufacturing_blueprint_for_product(
        &self,
        product_type_id: i64,
    ) -> Result<Option<i64>, SdeError> {
        let _ = product_type_id;
        Ok(None)
    }
    async fn reaction_formula_for_product(
        &self,
        product_type_id: i64,
    ) -> Result<Option<i64>, SdeError> {
        let _ = product_type_id;
        Ok(None)
    }
    async fn production_recipes_for_products(
        &self,
        product_type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, ProductRecipeRefs>, SdeError> {
        let _ = product_type_ids;
        Ok(BTreeMap::new())
    }
    async fn search_types(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<TypeSearchResult>, SdeError> {
        let _ = (query, limit);
        Ok(Vec::new())
    }
    async fn type_group_names(
        &self,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, String>, SdeError> {
        let _ = type_ids;
        Ok(std::collections::BTreeMap::new())
    }
    async fn type_classifications(
        &self,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, SdeTypeClassification>, SdeError> {
        let _ = type_ids;
        Ok(std::collections::BTreeMap::new())
    }
    async fn type_names(
        &self,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, String>, SdeError> {
        let _ = type_ids;
        Ok(std::collections::BTreeMap::new())
    }
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
    async fn industry_target_filters(&self) -> Result<Vec<IndustryTargetFilter>, SdeError> {
        Ok(Vec::new())
    }
    async fn industry_target_filters_by_activity(
        &self,
    ) -> Result<Vec<(RigActivity, IndustryTargetFilter)>, SdeError> {
        Ok(Vec::new())
    }
    async fn group_categories(
        &self,
        group_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, i64>, SdeError> {
        let _ = group_ids;
        Ok(std::collections::BTreeMap::new())
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
    async fn search_npc_stations(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<NpcStationSearchResult>, SdeError> {
        let _ = (query, limit);
        Ok(Vec::new())
    }
    async fn list_regions(&self) -> Result<Vec<RegionSummary>, SdeError> {
        Ok(Vec::new())
    }
    async fn search_regions(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<RegionSummary>, SdeError> {
        let _ = (query, limit);
        Ok(Vec::new())
    }
    async fn list_npc_stations_in_region(
        &self,
        region_id: i64,
    ) -> Result<Vec<NpcStationSearchResult>, SdeError> {
        let _ = region_id;
        Ok(Vec::new())
    }
    async fn resolve_npc_stations(
        &self,
        station_ids: &[i64],
    ) -> Result<Vec<NpcStationSearchResult>, SdeError> {
        let _ = station_ids;
        Ok(Vec::new())
    }
    async fn list_market_groups(&self) -> Result<Vec<MarketGroupNode>, SdeError> {
        Ok(Vec::new())
    }
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
    async fn planet_schematics(
        &self,
        schematic_ids: &[i64],
    ) -> Result<BTreeMap<i64, PlanetSchematic>, SdeError> {
        let _ = schematic_ids;
        Ok(BTreeMap::new())
    }
    async fn planet_schematic_catalogue(&self) -> Result<Vec<PlanetSchematic>, SdeError> {
        Ok(Vec::new())
    }
    async fn planet_pin_types(&self) -> Result<Vec<PlanetPinType>, SdeError> {
        Ok(Vec::new())
    }
    async fn planet_commodities(&self) -> Result<BTreeMap<i64, PlanetCommodity>, SdeError> {
        Ok(BTreeMap::new())
    }
    async fn systems_within_jumps(
        &self,
        solar_system_id: i64,
        max_jumps: u8,
    ) -> Result<Vec<SystemDistance>, SdeError> {
        let _ = (solar_system_id, max_jumps);
        Ok(Vec::new())
    }
    async fn planets_in_systems(
        &self,
        solar_system_ids: &[i64],
    ) -> Result<Vec<PlannerPlanet>, SdeError> {
        let _ = solar_system_ids;
        Ok(Vec::new())
    }
    async fn region_stargate_edges(&self, region_id: i64) -> Result<Vec<(i64, i64)>, SdeError> {
        let _ = region_id;
        Ok(Vec::new())
    }
    async fn skill_catalogue(&self) -> Result<Vec<SkillDefinition>, SdeError> {
        Ok(Vec::new())
    }
    async fn required_skills(
        &self,
        type_ids: &[i64],
    ) -> Result<BTreeMap<i64, Vec<RequiredSkill>>, SdeError> {
        let _ = type_ids;
        Ok(BTreeMap::new())
    }
    async fn type_info_base(&self, type_id: i64) -> Result<Option<TypeInfoBase>, SdeError> {
        let _ = type_id;
        Ok(None)
    }
    async fn type_attributes(&self, type_id: i64) -> Result<Vec<TypeAttributeValue>, SdeError> {
        let _ = type_id;
        Ok(Vec::new())
    }
    async fn type_bonuses(&self, type_id: i64) -> Result<Vec<TypeBonusLine>, SdeError> {
        let _ = type_id;
        Ok(Vec::new())
    }
    async fn type_materials(&self, type_id: i64) -> Result<Vec<RecipeLine>, SdeError> {
        let _ = type_id;
        Ok(Vec::new())
    }
    async fn type_info_counts(&self, type_id: i64) -> Result<TypeInfoCounts, SdeError> {
        let _ = type_id;
        Ok(TypeInfoCounts::default())
    }
    async fn type_variations(&self, type_id: i64) -> Result<Vec<TypeListEntry>, SdeError> {
        let _ = type_id;
        Ok(Vec::new())
    }
    async fn types_used_with(
        &self,
        type_id: i64,
    ) -> Result<Option<(UsedWithKind, Vec<TypeListEntry>)>, SdeError> {
        let _ = type_id;
        Ok(None)
    }
    async fn type_usages(
        &self,
        type_id: i64,
        limit: u32,
        offset: u32,
    ) -> Result<(Vec<TypeUsage>, i64), SdeError> {
        let _ = (type_id, limit, offset);
        Ok((Vec::new(), 0))
    }
    async fn group_names(&self, group_ids: &[i64]) -> Result<BTreeMap<i64, String>, SdeError> {
        let _ = group_ids;
        Ok(BTreeMap::new())
    }
    async fn attribute_names(
        &self,
        attribute_ids: &[i64],
    ) -> Result<BTreeMap<i64, String>, SdeError> {
        let _ = attribute_ids;
        Ok(BTreeMap::new())
    }
    async fn planet_references(
        &self,
        planet_ids: &[i64],
    ) -> Result<BTreeMap<i64, PlanetReference>, SdeError> {
        let _ = planet_ids;
        Ok(BTreeMap::new())
    }
}
