//! The base for test fakes of [`SdeReadRepository`] (`test-support` only).
//!
//! [`SdeReadRepository`] has no default methods, so the Postgres
//! implementation must answer every one. A fake implements [`FakeSdeRead`]
//! instead and overrides only what its test reads: the blanket impl below
//! makes it an [`SdeReadRepository`], and any other method panics with its
//! name, so a test that reaches an unstubbed read fails loudly instead of
//! seeing empty data.

use std::collections::BTreeMap;

use async_trait::async_trait;

use crate::*;

/// [`SdeReadRepository`] with every method defaulted for fakes: each one
/// panics naming the fake and the method, except
/// [`Self::production_recipes_for_products`], which loops over the
/// single-type lookups.
#[async_trait]
pub trait FakeSdeRead: Send + Sync {
    async fn active_sde(&self) -> Result<Option<ActiveSde>, SdeError> {
        unimplemented!(
            "{} does not stub FakeSdeRead::active_sde",
            std::any::type_name::<Self>()
        )
    }
    async fn manufacturable_candidates(
        &self,
        scope: &ManufacturableCandidateScope,
    ) -> Result<Vec<ManufacturableCandidateRecipe>, SdeError> {
        let _ = scope;
        unimplemented!(
            "{} does not stub FakeSdeRead::manufacturable_candidates",
            std::any::type_name::<Self>()
        )
    }
    async fn search_manufacturing_blueprints(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<BlueprintSearchResult>, SdeError> {
        let _ = (query, limit);
        unimplemented!(
            "{} does not stub FakeSdeRead::search_manufacturing_blueprints",
            std::any::type_name::<Self>()
        )
    }
    async fn manufacturing_recipe(
        &self,
        blueprint_type_id: i64,
    ) -> Result<Option<ManufacturingRecipe>, SdeError> {
        let _ = blueprint_type_id;
        unimplemented!(
            "{} does not stub FakeSdeRead::manufacturing_recipe",
            std::any::type_name::<Self>()
        )
    }
    async fn search_reaction_formulas(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<ReactionFormulaSearchResult>, SdeError> {
        let _ = (query, limit);
        unimplemented!(
            "{} does not stub FakeSdeRead::search_reaction_formulas",
            std::any::type_name::<Self>()
        )
    }
    async fn reaction_formula(
        &self,
        reaction_formula_type_id: i64,
    ) -> Result<Option<ReactionFormulaRecipe>, SdeError> {
        let _ = reaction_formula_type_id;
        unimplemented!(
            "{} does not stub FakeSdeRead::reaction_formula",
            std::any::type_name::<Self>()
        )
    }
    async fn manufacturing_blueprint_for_product(
        &self,
        product_type_id: i64,
    ) -> Result<Option<i64>, SdeError> {
        let _ = product_type_id;
        unimplemented!(
            "{} does not stub FakeSdeRead::manufacturing_blueprint_for_product",
            std::any::type_name::<Self>()
        )
    }
    async fn reaction_formula_for_product(
        &self,
        product_type_id: i64,
    ) -> Result<Option<i64>, SdeError> {
        let _ = product_type_id;
        unimplemented!(
            "{} does not stub FakeSdeRead::reaction_formula_for_product",
            std::any::type_name::<Self>()
        )
    }
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
    ) -> Result<Vec<TypeSearchResult>, SdeError> {
        let _ = (query, limit);
        unimplemented!(
            "{} does not stub FakeSdeRead::search_types",
            std::any::type_name::<Self>()
        )
    }
    async fn type_group_names(
        &self,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, String>, SdeError> {
        let _ = type_ids;
        unimplemented!(
            "{} does not stub FakeSdeRead::type_group_names",
            std::any::type_name::<Self>()
        )
    }
    async fn type_classifications(
        &self,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, SdeTypeClassification>, SdeError> {
        let _ = type_ids;
        unimplemented!(
            "{} does not stub FakeSdeRead::type_classifications",
            std::any::type_name::<Self>()
        )
    }
    async fn type_names(
        &self,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, String>, SdeError> {
        let _ = type_ids;
        unimplemented!(
            "{} does not stub FakeSdeRead::type_names",
            std::any::type_name::<Self>()
        )
    }
    async fn type_reference(
        &self,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, TypeReference>, SdeError> {
        let _ = type_ids;
        unimplemented!(
            "{} does not stub FakeSdeRead::type_reference",
            std::any::type_name::<Self>()
        )
    }
    async fn inventory_type_metadata(
        &self,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, SdeInventoryTypeMetadata>, SdeError> {
        let _ = type_ids;
        unimplemented!(
            "{} does not stub FakeSdeRead::inventory_type_metadata",
            std::any::type_name::<Self>()
        )
    }
    async fn search_structure_types(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<TypeSearchResult>, SdeError> {
        let _ = (query, limit);
        unimplemented!(
            "{} does not stub FakeSdeRead::search_structure_types",
            std::any::type_name::<Self>()
        )
    }
    async fn search_structure_rigs(
        &self,
        query: &str,
        limit: u32,
        structure_type_id: Option<i64>,
    ) -> Result<Vec<TypeSearchResult>, SdeError> {
        let _ = (query, limit, structure_type_id);
        unimplemented!(
            "{} does not stub FakeSdeRead::search_structure_rigs",
            std::any::type_name::<Self>()
        )
    }
    async fn structure_manufacturing_modifiers(
        &self,
        type_id: i64,
    ) -> Result<Option<StructureManufacturingModifiers>, SdeError> {
        let _ = type_id;
        unimplemented!(
            "{} does not stub FakeSdeRead::structure_manufacturing_modifiers",
            std::any::type_name::<Self>()
        )
    }
    async fn industry_target_filters(&self) -> Result<Vec<IndustryTargetFilter>, SdeError> {
        unimplemented!(
            "{} does not stub FakeSdeRead::industry_target_filters",
            std::any::type_name::<Self>()
        )
    }
    async fn industry_target_filters_by_activity(
        &self,
    ) -> Result<Vec<(RigActivity, IndustryTargetFilter)>, SdeError> {
        unimplemented!(
            "{} does not stub FakeSdeRead::industry_target_filters_by_activity",
            std::any::type_name::<Self>()
        )
    }
    async fn group_categories(
        &self,
        group_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, i64>, SdeError> {
        let _ = group_ids;
        unimplemented!(
            "{} does not stub FakeSdeRead::group_categories",
            std::any::type_name::<Self>()
        )
    }
    async fn rig_manufacturing_modifiers(
        &self,
        type_id: i64,
        security_class: &str,
        structure_type_id: Option<i64>,
    ) -> Result<Option<RigManufacturingModifiers>, SdeError> {
        let _ = (type_id, security_class, structure_type_id);
        unimplemented!(
            "{} does not stub FakeSdeRead::rig_manufacturing_modifiers",
            std::any::type_name::<Self>()
        )
    }
    async fn search_solar_systems(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<SolarSystemSearchResult>, SdeError> {
        let _ = (query, limit);
        unimplemented!(
            "{} does not stub FakeSdeRead::search_solar_systems",
            std::any::type_name::<Self>()
        )
    }
    async fn search_npc_stations(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<NpcStationSearchResult>, SdeError> {
        let _ = (query, limit);
        unimplemented!(
            "{} does not stub FakeSdeRead::search_npc_stations",
            std::any::type_name::<Self>()
        )
    }
    async fn list_regions(&self) -> Result<Vec<RegionSummary>, SdeError> {
        unimplemented!(
            "{} does not stub FakeSdeRead::list_regions",
            std::any::type_name::<Self>()
        )
    }
    async fn search_regions(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<RegionSummary>, SdeError> {
        let _ = (query, limit);
        unimplemented!(
            "{} does not stub FakeSdeRead::search_regions",
            std::any::type_name::<Self>()
        )
    }
    async fn list_npc_stations_in_region(
        &self,
        region_id: i64,
    ) -> Result<Vec<NpcStationSearchResult>, SdeError> {
        let _ = region_id;
        unimplemented!(
            "{} does not stub FakeSdeRead::list_npc_stations_in_region",
            std::any::type_name::<Self>()
        )
    }
    async fn resolve_npc_stations(
        &self,
        station_ids: &[i64],
    ) -> Result<Vec<NpcStationSearchResult>, SdeError> {
        let _ = station_ids;
        unimplemented!(
            "{} does not stub FakeSdeRead::resolve_npc_stations",
            std::any::type_name::<Self>()
        )
    }
    async fn list_market_groups(&self) -> Result<Vec<MarketGroupNode>, SdeError> {
        unimplemented!(
            "{} does not stub FakeSdeRead::list_market_groups",
            std::any::type_name::<Self>()
        )
    }
    async fn list_market_items(
        &self,
        market_group_id: Option<i64>,
        search: &str,
        page: u32,
        page_size: u32,
    ) -> Result<(Vec<MarketItemCandidate>, u64), SdeError> {
        let _ = (market_group_id, search, page, page_size);
        unimplemented!(
            "{} does not stub FakeSdeRead::list_market_items",
            std::any::type_name::<Self>()
        )
    }
    async fn list_market_group_subtree_item_ids(
        &self,
        root_group_id: i64,
    ) -> Result<Vec<MarketItemCandidate>, SdeError> {
        let _ = root_group_id;
        unimplemented!(
            "{} does not stub FakeSdeRead::list_market_group_subtree_item_ids",
            std::any::type_name::<Self>()
        )
    }
    async fn search_reaction_rigs(
        &self,
        query: &str,
        limit: u32,
        structure_type_id: Option<i64>,
    ) -> Result<Vec<TypeSearchResult>, SdeError> {
        let _ = (query, limit, structure_type_id);
        unimplemented!(
            "{} does not stub FakeSdeRead::search_reaction_rigs",
            std::any::type_name::<Self>()
        )
    }
    async fn reaction_rig_modifiers(
        &self,
        type_id: i64,
        security_class: &str,
        structure_type_id: Option<i64>,
    ) -> Result<Option<ReactionRigModifiers>, SdeError> {
        let _ = (type_id, security_class, structure_type_id);
        unimplemented!(
            "{} does not stub FakeSdeRead::reaction_rig_modifiers",
            std::any::type_name::<Self>()
        )
    }
    async fn planet_schematics(
        &self,
        schematic_ids: &[i64],
    ) -> Result<BTreeMap<i64, PlanetSchematic>, SdeError> {
        let _ = schematic_ids;
        unimplemented!(
            "{} does not stub FakeSdeRead::planet_schematics",
            std::any::type_name::<Self>()
        )
    }
    async fn planet_schematic_catalogue(&self) -> Result<Vec<PlanetSchematic>, SdeError> {
        unimplemented!(
            "{} does not stub FakeSdeRead::planet_schematic_catalogue",
            std::any::type_name::<Self>()
        )
    }
    async fn planet_pin_types(&self) -> Result<Vec<PlanetPinType>, SdeError> {
        unimplemented!(
            "{} does not stub FakeSdeRead::planet_pin_types",
            std::any::type_name::<Self>()
        )
    }
    async fn planet_commodities(&self) -> Result<BTreeMap<i64, PlanetCommodity>, SdeError> {
        unimplemented!(
            "{} does not stub FakeSdeRead::planet_commodities",
            std::any::type_name::<Self>()
        )
    }
    async fn systems_within_jumps(
        &self,
        solar_system_id: i64,
        max_jumps: u8,
    ) -> Result<Vec<SystemDistance>, SdeError> {
        let _ = (solar_system_id, max_jumps);
        unimplemented!(
            "{} does not stub FakeSdeRead::systems_within_jumps",
            std::any::type_name::<Self>()
        )
    }
    async fn planets_in_systems(
        &self,
        solar_system_ids: &[i64],
    ) -> Result<Vec<PlannerPlanet>, SdeError> {
        let _ = solar_system_ids;
        unimplemented!(
            "{} does not stub FakeSdeRead::planets_in_systems",
            std::any::type_name::<Self>()
        )
    }
    async fn region_stargate_edges(&self, region_id: i64) -> Result<Vec<(i64, i64)>, SdeError> {
        let _ = region_id;
        unimplemented!(
            "{} does not stub FakeSdeRead::region_stargate_edges",
            std::any::type_name::<Self>()
        )
    }
    async fn skill_catalogue(&self) -> Result<Vec<SkillDefinition>, SdeError> {
        unimplemented!(
            "{} does not stub FakeSdeRead::skill_catalogue",
            std::any::type_name::<Self>()
        )
    }
    async fn required_skills(
        &self,
        type_ids: &[i64],
    ) -> Result<BTreeMap<i64, Vec<RequiredSkill>>, SdeError> {
        let _ = type_ids;
        unimplemented!(
            "{} does not stub FakeSdeRead::required_skills",
            std::any::type_name::<Self>()
        )
    }
    async fn type_info_base(&self, type_id: i64) -> Result<Option<TypeInfoBase>, SdeError> {
        let _ = type_id;
        unimplemented!(
            "{} does not stub FakeSdeRead::type_info_base",
            std::any::type_name::<Self>()
        )
    }
    async fn type_attributes(&self, type_id: i64) -> Result<Vec<TypeAttributeValue>, SdeError> {
        let _ = type_id;
        unimplemented!(
            "{} does not stub FakeSdeRead::type_attributes",
            std::any::type_name::<Self>()
        )
    }
    async fn type_bonuses(&self, type_id: i64) -> Result<Vec<TypeBonusLine>, SdeError> {
        let _ = type_id;
        unimplemented!(
            "{} does not stub FakeSdeRead::type_bonuses",
            std::any::type_name::<Self>()
        )
    }
    async fn type_materials(&self, type_id: i64) -> Result<Vec<RecipeLine>, SdeError> {
        let _ = type_id;
        unimplemented!(
            "{} does not stub FakeSdeRead::type_materials",
            std::any::type_name::<Self>()
        )
    }
    async fn type_info_counts(&self, type_id: i64) -> Result<TypeInfoCounts, SdeError> {
        let _ = type_id;
        unimplemented!(
            "{} does not stub FakeSdeRead::type_info_counts",
            std::any::type_name::<Self>()
        )
    }
    async fn type_variations(&self, type_id: i64) -> Result<Vec<TypeListEntry>, SdeError> {
        let _ = type_id;
        unimplemented!(
            "{} does not stub FakeSdeRead::type_variations",
            std::any::type_name::<Self>()
        )
    }
    async fn types_used_with(
        &self,
        type_id: i64,
    ) -> Result<Option<(UsedWithKind, Vec<TypeListEntry>)>, SdeError> {
        let _ = type_id;
        unimplemented!(
            "{} does not stub FakeSdeRead::types_used_with",
            std::any::type_name::<Self>()
        )
    }
    async fn type_usages(
        &self,
        type_id: i64,
        limit: u32,
        offset: u32,
    ) -> Result<(Vec<TypeUsage>, i64), SdeError> {
        let _ = (type_id, limit, offset);
        unimplemented!(
            "{} does not stub FakeSdeRead::type_usages",
            std::any::type_name::<Self>()
        )
    }
    async fn group_names(&self, group_ids: &[i64]) -> Result<BTreeMap<i64, String>, SdeError> {
        let _ = group_ids;
        unimplemented!(
            "{} does not stub FakeSdeRead::group_names",
            std::any::type_name::<Self>()
        )
    }
    async fn attribute_names(
        &self,
        attribute_ids: &[i64],
    ) -> Result<BTreeMap<i64, String>, SdeError> {
        let _ = attribute_ids;
        unimplemented!(
            "{} does not stub FakeSdeRead::attribute_names",
            std::any::type_name::<Self>()
        )
    }
    async fn planet_references(
        &self,
        planet_ids: &[i64],
    ) -> Result<BTreeMap<i64, PlanetReference>, SdeError> {
        let _ = planet_ids;
        unimplemented!(
            "{} does not stub FakeSdeRead::planet_references",
            std::any::type_name::<Self>()
        )
    }
}

/// Every [`FakeSdeRead`] is an [`SdeReadRepository`].
#[async_trait]
impl<T: FakeSdeRead> SdeReadRepository for T {
    async fn active_sde(&self) -> Result<Option<ActiveSde>, SdeError> {
        FakeSdeRead::active_sde(self).await
    }
    async fn manufacturable_candidates(
        &self,
        scope: &ManufacturableCandidateScope,
    ) -> Result<Vec<ManufacturableCandidateRecipe>, SdeError> {
        FakeSdeRead::manufacturable_candidates(self, scope).await
    }
    async fn search_manufacturing_blueprints(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<BlueprintSearchResult>, SdeError> {
        FakeSdeRead::search_manufacturing_blueprints(self, query, limit).await
    }
    async fn manufacturing_recipe(
        &self,
        blueprint_type_id: i64,
    ) -> Result<Option<ManufacturingRecipe>, SdeError> {
        FakeSdeRead::manufacturing_recipe(self, blueprint_type_id).await
    }
    async fn search_reaction_formulas(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<ReactionFormulaSearchResult>, SdeError> {
        FakeSdeRead::search_reaction_formulas(self, query, limit).await
    }
    async fn reaction_formula(
        &self,
        reaction_formula_type_id: i64,
    ) -> Result<Option<ReactionFormulaRecipe>, SdeError> {
        FakeSdeRead::reaction_formula(self, reaction_formula_type_id).await
    }
    async fn manufacturing_blueprint_for_product(
        &self,
        product_type_id: i64,
    ) -> Result<Option<i64>, SdeError> {
        FakeSdeRead::manufacturing_blueprint_for_product(self, product_type_id).await
    }
    async fn reaction_formula_for_product(
        &self,
        product_type_id: i64,
    ) -> Result<Option<i64>, SdeError> {
        FakeSdeRead::reaction_formula_for_product(self, product_type_id).await
    }
    async fn production_recipes_for_products(
        &self,
        product_type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, ProductRecipeRefs>, SdeError> {
        FakeSdeRead::production_recipes_for_products(self, product_type_ids).await
    }
    async fn search_types(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<TypeSearchResult>, SdeError> {
        FakeSdeRead::search_types(self, query, limit).await
    }
    async fn type_group_names(
        &self,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, String>, SdeError> {
        FakeSdeRead::type_group_names(self, type_ids).await
    }
    async fn type_classifications(
        &self,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, SdeTypeClassification>, SdeError> {
        FakeSdeRead::type_classifications(self, type_ids).await
    }
    async fn type_names(
        &self,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, String>, SdeError> {
        FakeSdeRead::type_names(self, type_ids).await
    }
    async fn type_reference(
        &self,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, TypeReference>, SdeError> {
        FakeSdeRead::type_reference(self, type_ids).await
    }
    async fn inventory_type_metadata(
        &self,
        type_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, SdeInventoryTypeMetadata>, SdeError> {
        FakeSdeRead::inventory_type_metadata(self, type_ids).await
    }
    async fn search_structure_types(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<TypeSearchResult>, SdeError> {
        FakeSdeRead::search_structure_types(self, query, limit).await
    }
    async fn search_structure_rigs(
        &self,
        query: &str,
        limit: u32,
        structure_type_id: Option<i64>,
    ) -> Result<Vec<TypeSearchResult>, SdeError> {
        FakeSdeRead::search_structure_rigs(self, query, limit, structure_type_id).await
    }
    async fn structure_manufacturing_modifiers(
        &self,
        type_id: i64,
    ) -> Result<Option<StructureManufacturingModifiers>, SdeError> {
        FakeSdeRead::structure_manufacturing_modifiers(self, type_id).await
    }
    async fn industry_target_filters(&self) -> Result<Vec<IndustryTargetFilter>, SdeError> {
        FakeSdeRead::industry_target_filters(self).await
    }
    async fn industry_target_filters_by_activity(
        &self,
    ) -> Result<Vec<(RigActivity, IndustryTargetFilter)>, SdeError> {
        FakeSdeRead::industry_target_filters_by_activity(self).await
    }
    async fn group_categories(
        &self,
        group_ids: &[i64],
    ) -> Result<std::collections::BTreeMap<i64, i64>, SdeError> {
        FakeSdeRead::group_categories(self, group_ids).await
    }
    async fn rig_manufacturing_modifiers(
        &self,
        type_id: i64,
        security_class: &str,
        structure_type_id: Option<i64>,
    ) -> Result<Option<RigManufacturingModifiers>, SdeError> {
        FakeSdeRead::rig_manufacturing_modifiers(self, type_id, security_class, structure_type_id)
            .await
    }
    async fn search_solar_systems(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<SolarSystemSearchResult>, SdeError> {
        FakeSdeRead::search_solar_systems(self, query, limit).await
    }
    async fn search_npc_stations(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<NpcStationSearchResult>, SdeError> {
        FakeSdeRead::search_npc_stations(self, query, limit).await
    }
    async fn list_regions(&self) -> Result<Vec<RegionSummary>, SdeError> {
        FakeSdeRead::list_regions(self).await
    }
    async fn search_regions(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<RegionSummary>, SdeError> {
        FakeSdeRead::search_regions(self, query, limit).await
    }
    async fn list_npc_stations_in_region(
        &self,
        region_id: i64,
    ) -> Result<Vec<NpcStationSearchResult>, SdeError> {
        FakeSdeRead::list_npc_stations_in_region(self, region_id).await
    }
    async fn resolve_npc_stations(
        &self,
        station_ids: &[i64],
    ) -> Result<Vec<NpcStationSearchResult>, SdeError> {
        FakeSdeRead::resolve_npc_stations(self, station_ids).await
    }
    async fn list_market_groups(&self) -> Result<Vec<MarketGroupNode>, SdeError> {
        FakeSdeRead::list_market_groups(self).await
    }
    async fn list_market_items(
        &self,
        market_group_id: Option<i64>,
        search: &str,
        page: u32,
        page_size: u32,
    ) -> Result<(Vec<MarketItemCandidate>, u64), SdeError> {
        FakeSdeRead::list_market_items(self, market_group_id, search, page, page_size).await
    }
    async fn list_market_group_subtree_item_ids(
        &self,
        root_group_id: i64,
    ) -> Result<Vec<MarketItemCandidate>, SdeError> {
        FakeSdeRead::list_market_group_subtree_item_ids(self, root_group_id).await
    }
    async fn search_reaction_rigs(
        &self,
        query: &str,
        limit: u32,
        structure_type_id: Option<i64>,
    ) -> Result<Vec<TypeSearchResult>, SdeError> {
        FakeSdeRead::search_reaction_rigs(self, query, limit, structure_type_id).await
    }
    async fn reaction_rig_modifiers(
        &self,
        type_id: i64,
        security_class: &str,
        structure_type_id: Option<i64>,
    ) -> Result<Option<ReactionRigModifiers>, SdeError> {
        FakeSdeRead::reaction_rig_modifiers(self, type_id, security_class, structure_type_id).await
    }
    async fn planet_schematics(
        &self,
        schematic_ids: &[i64],
    ) -> Result<BTreeMap<i64, PlanetSchematic>, SdeError> {
        FakeSdeRead::planet_schematics(self, schematic_ids).await
    }
    async fn planet_schematic_catalogue(&self) -> Result<Vec<PlanetSchematic>, SdeError> {
        FakeSdeRead::planet_schematic_catalogue(self).await
    }
    async fn planet_pin_types(&self) -> Result<Vec<PlanetPinType>, SdeError> {
        FakeSdeRead::planet_pin_types(self).await
    }
    async fn planet_commodities(&self) -> Result<BTreeMap<i64, PlanetCommodity>, SdeError> {
        FakeSdeRead::planet_commodities(self).await
    }
    async fn systems_within_jumps(
        &self,
        solar_system_id: i64,
        max_jumps: u8,
    ) -> Result<Vec<SystemDistance>, SdeError> {
        FakeSdeRead::systems_within_jumps(self, solar_system_id, max_jumps).await
    }
    async fn planets_in_systems(
        &self,
        solar_system_ids: &[i64],
    ) -> Result<Vec<PlannerPlanet>, SdeError> {
        FakeSdeRead::planets_in_systems(self, solar_system_ids).await
    }
    async fn region_stargate_edges(&self, region_id: i64) -> Result<Vec<(i64, i64)>, SdeError> {
        FakeSdeRead::region_stargate_edges(self, region_id).await
    }
    async fn skill_catalogue(&self) -> Result<Vec<SkillDefinition>, SdeError> {
        FakeSdeRead::skill_catalogue(self).await
    }
    async fn required_skills(
        &self,
        type_ids: &[i64],
    ) -> Result<BTreeMap<i64, Vec<RequiredSkill>>, SdeError> {
        FakeSdeRead::required_skills(self, type_ids).await
    }
    async fn type_info_base(&self, type_id: i64) -> Result<Option<TypeInfoBase>, SdeError> {
        FakeSdeRead::type_info_base(self, type_id).await
    }
    async fn type_attributes(&self, type_id: i64) -> Result<Vec<TypeAttributeValue>, SdeError> {
        FakeSdeRead::type_attributes(self, type_id).await
    }
    async fn type_bonuses(&self, type_id: i64) -> Result<Vec<TypeBonusLine>, SdeError> {
        FakeSdeRead::type_bonuses(self, type_id).await
    }
    async fn type_materials(&self, type_id: i64) -> Result<Vec<RecipeLine>, SdeError> {
        FakeSdeRead::type_materials(self, type_id).await
    }
    async fn type_info_counts(&self, type_id: i64) -> Result<TypeInfoCounts, SdeError> {
        FakeSdeRead::type_info_counts(self, type_id).await
    }
    async fn type_variations(&self, type_id: i64) -> Result<Vec<TypeListEntry>, SdeError> {
        FakeSdeRead::type_variations(self, type_id).await
    }
    async fn types_used_with(
        &self,
        type_id: i64,
    ) -> Result<Option<(UsedWithKind, Vec<TypeListEntry>)>, SdeError> {
        FakeSdeRead::types_used_with(self, type_id).await
    }
    async fn type_usages(
        &self,
        type_id: i64,
        limit: u32,
        offset: u32,
    ) -> Result<(Vec<TypeUsage>, i64), SdeError> {
        FakeSdeRead::type_usages(self, type_id, limit, offset).await
    }
    async fn group_names(&self, group_ids: &[i64]) -> Result<BTreeMap<i64, String>, SdeError> {
        FakeSdeRead::group_names(self, group_ids).await
    }
    async fn attribute_names(
        &self,
        attribute_ids: &[i64],
    ) -> Result<BTreeMap<i64, String>, SdeError> {
        FakeSdeRead::attribute_names(self, attribute_ids).await
    }
    async fn planet_references(
        &self,
        planet_ids: &[i64],
    ) -> Result<BTreeMap<i64, PlanetReference>, SdeError> {
        FakeSdeRead::planet_references(self, planet_ids).await
    }
}
