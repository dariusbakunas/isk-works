use super::*;

pub(super) fn default_material_pricing_policy() -> crate::MarketPricingPolicy {
    crate::MarketPricingPolicy::HighestBuy
}

pub(super) fn default_output_pricing_policy() -> crate::MarketPricingPolicy {
    crate::MarketPricingPolicy::LowestSell
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedMaterialLine {
    pub type_id: i64,
    pub type_name: String,
    pub quantity_per_run: u64,
    pub total_quantity: u64,
    pub unit_price: Option<Money>,
    pub line_total: Option<Money>,
    pub missing: bool,
    /// Which parent(s) consume this row and how much of it, when it's part
    /// of a component-expanded worksheet -- empty outside of expansion
    /// (a plain build's materials have no parent but the build itself).
    #[serde(default)]
    pub contributions: Vec<MaterialContribution>,
    /// True for a build-resolved row within a component expansion -- the
    /// row is itself a job (manufacturing or reaction), so it carries its
    /// own installation cost rather than being a plain purchased material.
    #[serde(default)]
    pub is_build_resolved: bool,
    /// This row's own job installation cost, when `is_build_resolved` and a
    /// facility of the matching kind was selected and its EIV resolved.
    /// `None` either outside of a component expansion, for a Buy-resolved
    /// row, or when the matching facility/EIV isn't available yet.
    #[serde(default)]
    pub installation_cost: Option<InstallationCostBreakdown>,
    /// How much of this row is being covered from existing inventory
    /// (`Missing` fulfillment scope) rather than bought/built fresh.
    /// `None` for a row with no `FulfillmentScopeOverride` at all (today's
    /// only case for any build predating this feature) -- `Some(0)` is a
    /// real, different state: `Missing` scope selected, but nothing
    /// available yet.
    #[serde(default)]
    pub reused_quantity: Option<u64>,
    /// The portion of this row still being bought or built, after
    /// `reused_quantity` -- `total_quantity` itself when `reused_quantity`
    /// is `None` or `0`.
    #[serde(default)]
    pub missing_quantity: Option<u64>,
    /// The dollar total of just the `reused_quantity` portion, at its own
    /// historical inventory cost -- `line_total` minus this is the
    /// missing/bought-or-built portion's own cost. `None` whenever
    /// `reused_quantity` is `None`/`0`, or the reused portion's own cost
    /// is unknown (in which case the whole row is `missing` instead).
    #[serde(default)]
    pub reused_line_total: Option<Money>,
    /// Allocation-aware child-production evidence for a
    /// `Build`/`Reaction` row costed from [`crate::BuildCostProjection`]
    /// (`BuildCostProjection::apply_to_revision`) -- `None` outside that path
    /// (a plain Buy row, or a revision not passed through
    /// `apply_to_revision`). See [`PlanningChildEvidence`] for why
    /// the child's whole job cost is not what's charged to this row.
    #[serde(default)]
    pub planning_evidence: Option<PlanningChildEvidence>,
}

/// Why a `Build`/`Reaction` row's `line_total` can be less than the child
/// operation's whole `total_production_cost`: the child may have produced
/// more than this row consumed (discrete `output_per_run`), and the
/// unconsumed portion retains its own basis rather than being charged here.
/// `child_consumed_cost + child_surplus_retained_basis == child total
/// production cost`, exactly, at Money scale -- see
/// `crate::build_cost::project_build_cost`'s module doc.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanningChildEvidence {
    /// The child operation's `op_index` in the `BuildCostProjection` this
    /// evidence came from -- join key back to `BuildCostProjection.operations`.
    pub child_op_index: u32,
    pub child_produced_quantity: u64,
    pub child_consumed_quantity: u64,
    /// `child_total_production_cost / child_produced_quantity` --
    /// **display/evidence only**, never multiplied back into `line_total`.
    pub child_unit_production_cost: Option<Money>,
    /// The portion of the child's job cost charged to this row --
    /// `line_total = reused_line_total.unwrap_or(0) + child_consumed_cost`.
    pub child_consumed_cost: Option<Money>,
    pub child_surplus_quantity: u64,
    /// Planning evidence only -- never added to any total. The basis of the
    /// child-produced surplus this row's Build did not consume.
    pub child_surplus_retained_basis: Option<Money>,
}

/// One parent's usage of a worksheet row: either the root build itself
/// (`parent_type_id: None`) or another build-resolved component.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialContribution {
    pub parent_type_id: Option<i64>,
    pub parent_type_name: String,
    pub quantity: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildPlanRevision {
    pub id: BuildPlanId,
    pub revision: u64,
    pub runs: u64,
    pub recipe_fingerprint: String,
    pub snapshot: PriceSnapshot,
    pub pricing_complete: bool,
    pub estimated_material_cost: Money,
    pub expected_revenue: Option<Money>,
    pub estimated_margin: Option<Money>,
    pub missing_price_count: u32,
    pub active: bool,
    pub planned_at: DateTime<Utc>,
    pub superseded_at: Option<DateTime<Utc>>,
    pub material_lines: Vec<PlannedMaterialLine>,
    pub manufacturing_facility: Option<FacilityPlanPreview>,
    pub reaction_facility: Option<FacilityPlanPreview>,
    pub blueprint: Option<crate::BlueprintSnapshot>,
    /// Per-material calculation evidence for **this node's own recipe** --
    /// base quantity per run, run count, blueprint ME, the combined facility
    /// material factor, and the effective quantity after ME / facility / rig
    /// modifiers with the authoritative ceil/`max(runs, …)` rule. One entry
    /// per direct recipe component (Buy leaf *and* Build/Reaction boundary),
    /// keyed by `type_id`. Retained so a consumer can reconstruct
    /// `total_quantity` from primitive inputs (the verification export's
    /// Quantity Audit sheet) without a second calculation. Empty for a
    /// revision built by a path that does not compute it (e.g. an Epic
    /// snapshot) or persisted before this field existed.
    #[serde(default)]
    pub effective_requirements: Vec<crate::EffectiveMaterialRequirement>,
}

impl BuildPlanRevision {
    /// The root job's own facility preview, whichever slot it came from.
    /// Exactly one of the two slots is ever populated for the root --
    /// the other exists only for build-resolved sub-components of that kind.
    pub fn root_facility(&self) -> Option<&FacilityPlanPreview> {
        self.manufacturing_facility
            .as_ref()
            .or(self.reaction_facility.as_ref())
    }

    /// Freeze this revision into the [`TaskExecutionSnapshot`] persisted on a
    /// Manufacturing/Reaction ticket at creation time. It is a *copy* of a
    /// calculation that already ran (see `calculate_build_snapshot_with_coverage`), not a
    /// second one -- a later Build edit never rewrites an existing ticket's
    /// frozen plan. `runs`/`duration_seconds`/`installation_cost`/
    /// `material_value` are only meaningful because the revision was computed
    /// for the ticket's own intended run count, not the linked Build's live
    /// `runs`. Facility-derived fields come from whichever root job slot is
    /// populated (`root_facility()`) and are `None` when no facility of the
    /// job's kind was selected; `blueprint` is `None` for reactions, which
    /// have no blueprint concept here.
    #[must_use]
    pub fn to_task_execution_snapshot(&self) -> TaskExecutionSnapshot {
        let facility = self.root_facility();
        TaskExecutionSnapshot {
            runs: self.runs,
            blueprint: self.blueprint.clone(),
            facility: facility.map(|preview| preview.profile.clone()),
            duration_seconds: facility.and_then(|preview| preview.planned_duration_seconds),
            installation_cost: facility.map(|preview| preview.installation_cost.clone()),
            material_value: Some(self.estimated_material_cost),
        }
    }
}

/// Material acquisition + output valuation as two independent market scopes
/// and strategies, plus an optional manual-price-list fallback for items
/// with no market coverage. Bundled from `DraftPlanningInput`/`PreviewBuildPlanCommand`'s own
/// flat fields (kept flat on those wire types, not nested, so each field
/// keeps its own independent `#[serde(default)]` for old stored drafts --
/// see `DraftPlanningInput`'s doc comment) rather than being the wire
/// format itself.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct BuildPricingConfiguration {
    pub material_scope: crate::MarketScope,
    pub material_strategy: crate::MarketPricingPolicy,
    pub output_scope: crate::MarketScope,
    pub output_strategy: crate::MarketPricingPolicy,
    pub manual_price_list_id: Option<PriceSourceId>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftPlanningInput {
    /// Defaults to Jita (`crate::default_market_scope`) so a draft saved
    /// before per-role market scopes existed -- which has neither field in
    /// its stored JSON -- still deserializes instead of failing to load
    /// (per-field serde defaults, the same established pattern already used below for the
    /// pricing policies).
    #[serde(default = "crate::default_market_scope")]
    pub material_scope: crate::MarketScope,
    #[serde(default = "crate::default_market_scope")]
    pub output_scope: crate::MarketScope,
    #[serde(default)]
    pub manual_price_list_id: Option<PriceSourceId>,
    #[serde(default)]
    pub expected_manual_price_list_revision: Option<u64>,
    #[serde(default = "default_material_pricing_policy")]
    pub material_pricing_policy: crate::MarketPricingPolicy,
    #[serde(default = "default_output_pricing_policy")]
    pub output_pricing_policy: crate::MarketPricingPolicy,
    #[serde(default)]
    pub pricing_selections: Vec<ItemPricingSelectionInput>,
    pub blueprint_selection: Option<crate::BlueprintSelection>,
    #[serde(default)]
    pub manufacturing_facility: Option<FacilityPreviewCommand>,
    #[serde(default)]
    pub reaction_facility: Option<ReactionFacilityPreviewCommand>,
    #[serde(default)]
    pub facility_eiv_manual: bool,
    #[serde(default)]
    pub component_resolutions: Vec<crate::ComponentResolution>,
    #[serde(default)]
    pub fulfillment_scopes: Vec<crate::FulfillmentScopeOverride>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftPlanningSnapshot {
    pub input: DraftPlanningInput,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Build {
    pub id: BuildId,
    pub workspace_id: WorkspaceId,
    pub owner_id: OwnerId,
    pub name: String,
    pub recipe: BuildRecipe,
    pub runs: u64,
    pub notes: String,
    pub revision: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub draft_planning: Option<DraftPlanningSnapshot>,
    pub recipe_currency: RecipeCurrency,
    pub active_sde_version: Option<String>,
    /// Broad EVE classification of the Build's output item (its
    /// `invCategories` category, e.g. "Ship"). Resolved from the active
    /// SDE at read time; `None` when the SDE has no classification for the
    /// product. Read-model only -- surfaced for the Builds library grouping
    /// and filter, never persisted on the Build.
    #[serde(default)]
    pub product_category_name: Option<String>,
    /// The output item's `invGroups` group (e.g. "Heavy Assault Cruiser"),
    /// same provenance as `product_category_name`.
    #[serde(default)]
    pub product_group_name: Option<String>,
    /// The origin of the blueprint actually selected on this Build --
    /// `Original` (BPO) or `Copy` (BPC) -- resolved from
    /// `draft_planning.input.blueprint_selection` (and, for an observed
    /// asset, its observation row). `None` when no concrete blueprint is
    /// selected, for a reaction Build, or when the source observation is
    /// gone. Never inferred from the output item.
    #[serde(default)]
    pub selected_blueprint_origin: Option<crate::BlueprintKind>,
    /// True when this workspace holds a current blueprint observation for
    /// this Build's blueprint (matched by `blueprint_type_id`), i.e. the
    /// blueprint (BPO or BPC) is actually on hand. Always `false` for a
    /// reaction Build -- reaction formulas aren't owned blueprints.
    /// Read-model only, resolved at read time.
    #[serde(default)]
    pub has_owned_blueprint: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NewBuild {
    pub build: Build,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateBuildCommand {
    pub name: String,
    pub recipe: RecipeSelection,
    pub runs: u64,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub draft_planning: Option<DraftPlanningInput>,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateBuildCommand {
    pub expected_revision: u64,
    pub name: String,
    pub recipe: RecipeSelection,
    pub runs: u64,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub draft_planning: Option<DraftPlanningInput>,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameBuildCommand {
    pub name: String,
}

/// Build-ID-addressable "common settings" patches -- the narrow,
/// revision-checked counterparts of `UpdateBuildCommand` that mutate exactly
/// one facet of an *arbitrary* Build's `draft_planning.input` (never the
/// whole draft, never routed through the single-Build worksheet editor).
/// Each is persisted through the same `update_draft` path (with
/// `normalize_draft_planning`) that the worksheet editor uses, and the API
/// follows every one with a best-effort linked-descendant resync.
///
/// This is the same extraction pattern as `set_component_resolution`, widened
/// to the blueprint selection, the recipe-appropriate facility slot, and the
/// pricing configuration -- the fields the unified Graph inspector needs to
/// edit a *linked* Build in place.
#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildBlueprintSettingsPatch {
    pub expected_revision: u64,
    /// `None` clears the selection (back to the unresearched default).
    #[serde(default)]
    pub blueprint_selection: Option<crate::BlueprintSelection>,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildFacilitySettingsPatch {
    pub expected_revision: u64,
    /// `None` clears the Build's own facility for its recipe kind (it then
    /// has no facility of that kind configured). The other kind's slot is
    /// never touched.
    #[serde(default)]
    pub facility_profile_id: Option<FacilityProfileId>,
    /// Carried straight onto the facility command; `None` leaves whatever
    /// the Build already had (manual EIV is toggled via the pricing patch).
    #[serde(default)]
    pub estimated_item_value: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildPricingSettingsPatch {
    pub expected_revision: u64,
    pub material_scope: crate::MarketScope,
    pub output_scope: crate::MarketScope,
    pub material_pricing_policy: crate::MarketPricingPolicy,
    pub output_pricing_policy: crate::MarketPricingPolicy,
    #[serde(default)]
    pub manual_price_list_id: Option<PriceSourceId>,
    #[serde(default)]
    pub expected_manual_price_list_revision: Option<u64>,
    #[serde(default)]
    pub facility_eiv_manual: bool,
}

/// One target of a [`DescendantProductionConfigurationRequest`] -- a
/// descendant Build to update, with the revision the caller last observed
/// (the same per-Build optimistic-concurrency contract `mutate_draft_planning`
/// already uses for a single Build, just addressed per member here since
/// sibling Builds never share one revision counter).
#[derive(Debug, Clone, Copy, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DescendantConfigurationMember {
    pub build_id: BuildId,
    pub expected_revision: u64,
}

/// The Stages inspector's atomic, multi-Build descendant-configuration edit:
/// applies the identical facility
/// or blueprint/formula-selection patch to every canonical member Build one
/// execution-plan production operation represents -- see
/// `crates/iskworks-app`'s `DescendantProductionConfigurationCoordinator`,
/// which validates the requested members still form one current operation
/// before ever touching persistence. Deliberately mirrors
/// [`BuildFacilitySettingsPatch`] / [`BuildBlueprintSettingsPatch`]'s own
/// field shapes exactly -- this is the same patch, just applied to N Builds
/// in one transaction instead of one.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum DescendantProductionConfigurationRequest {
    Facility {
        /// `None` clears every member's facility for its own recipe kind.
        #[serde(default)]
        facility_profile_id: Option<FacilityProfileId>,
        #[serde(default)]
        estimated_item_value: Option<String>,
    },
    BlueprintSelection {
        /// `None` clears every member's selection.
        #[serde(default)]
        blueprint_selection: Option<crate::BlueprintSelection>,
    },
}

#[cfg(test)]
mod task_execution_snapshot_tests {
    use super::*;
    use crate::{
        FacilityKind, FacilityProfileId, FacilityRole, IndustryFacilityProfile,
        InstallationCostBreakdown, Money, PriceSnapshot, PriceSnapshotId, SecurityClass,
        WorkspaceId,
    };
    use chrono::Utc;
    use rust_decimal::Decimal;

    fn facility_profile(role: FacilityRole) -> IndustryFacilityProfile {
        IndustryFacilityProfile {
            id: FacilityProfileId::new(),
            workspace_id: WorkspaceId::new(),
            name: "Test Facility".into(),
            kind: FacilityKind::Manual,
            role,
            structure_id: None,
            structure_type_id: None,
            structure_type_name: String::new(),
            solar_system_id: None,
            solar_system_name: String::new(),
            security_class: SecurityClass::Unknown,
            material_reduction_percent: Decimal::ZERO,
            time_reduction_percent: Decimal::ZERO,
            job_cost_reduction_percent: Decimal::ZERO,
            facility_tax_percent: Decimal::ZERO,
            scc_surcharge_percent: Decimal::ZERO,
            alliance_surcharge_percent: Decimal::ZERO,
            fixed_supplemental_cost: Money::zero(),
            manual_system_cost_index: None,
            notes: String::new(),
            rigs: Vec::new(),
            archived_at: None,
            revision: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn install_cost(total: &str) -> InstallationCostBreakdown {
        InstallationCostBreakdown {
            complete: true,
            estimated_item_value: Some(Money::parse("1000000").unwrap()),
            system_cost_index: Some(Decimal::new(5, 2)),
            unmodified_system_index_cost: Some(Money::parse(total).unwrap()),
            job_cost_reduction_percent: Decimal::ZERO,
            system_index_cost: Some(Money::parse(total).unwrap()),
            facility_tax: None,
            scc_surcharge: None,
            alliance_surcharge: None,
            fixed_supplemental_cost: Money::zero(),
            total: Some(Money::parse(total).unwrap()),
            warnings: Vec::new(),
            formula_version: "test".into(),
        }
    }

    fn facility_preview(role: FacilityRole) -> FacilityPlanPreview {
        FacilityPlanPreview {
            profile: facility_profile(role),
            blueprint_me: Some(0),
            blueprint_te: Some(0),
            requirements: Vec::new(),
            rig_material_factor: rust_decimal::Decimal::ONE,
            planned_duration_seconds: Some(7_200),
            duration_steps: Vec::new(),
            installation_cost: install_cost("12345"),
            warnings: Vec::new(),
            formula_version: "test".into(),
        }
    }

    fn revision() -> BuildPlanRevision {
        BuildPlanRevision {
            id: BuildPlanId(uuid::Uuid::new_v4()),
            revision: 3,
            runs: 50,
            recipe_fingerprint: "fp".into(),
            snapshot: PriceSnapshot {
                id: PriceSnapshotId(uuid::Uuid::new_v4()),
                price_source_id: None,
                source_name: "test".into(),
                source_revision: 1,
                created_at: Utc::now(),
                items: Vec::new(),
            },
            pricing_complete: true,
            estimated_material_cost: Money::parse("987654").unwrap(),
            expected_revenue: None,
            estimated_margin: None,
            missing_price_count: 0,
            active: true,
            planned_at: Utc::now(),
            superseded_at: None,
            material_lines: Vec::new(),
            manufacturing_facility: None,
            reaction_facility: None,
            blueprint: None,
            effective_requirements: Vec::new(),
        }
    }

    #[test]
    fn copies_runs_material_value_and_the_manufacturing_facility_plan() {
        let mut plan = revision();
        plan.runs = 50;
        plan.manufacturing_facility = Some(facility_preview(FacilityRole::Manufacturing));
        plan.blueprint = Some(crate::BlueprintSnapshot {
            id: uuid::Uuid::new_v4(),
            build_id: BuildId::new(),
            source_mode: crate::BlueprintSourceMode::Manual,
            blueprint_type_id: 1000,
            blueprint_name: "Test Blueprint".into(),
            kind: crate::BlueprintKind::Original,
            material_efficiency: 10,
            time_efficiency: 20,
            licensed_runs: None,
            requested_runs: 50,
            source_observation_id: None,
            source_eve_item_id: None,
            source_owner_id: None,
            source_owner_name: None,
            source_location_id: None,
            source_location_name: None,
            observed_at: None,
            imported_at: None,
            manual_notes: None,
            planned_duration_seconds: Some(7_200),
            formula_version: "test".into(),
            captured_at: Utc::now(),
        });

        let snapshot = plan.to_task_execution_snapshot();

        assert_eq!(snapshot.runs, 50);
        assert_eq!(
            snapshot.material_value,
            Some(Money::parse("987654").unwrap())
        );
        assert_eq!(snapshot.duration_seconds, Some(7_200));
        assert_eq!(
            snapshot.installation_cost.and_then(|cost| cost.total),
            Some(Money::parse("12345").unwrap())
        );
        assert_eq!(
            snapshot.facility.map(|facility| facility.role),
            Some(FacilityRole::Manufacturing)
        );
        assert_eq!(
            snapshot
                .blueprint
                .map(|blueprint| blueprint.material_efficiency),
            Some(10)
        );
    }

    #[test]
    fn a_reaction_plan_freezes_its_reaction_facility_and_never_invents_a_blueprint() {
        let mut plan = revision();
        plan.reaction_facility = Some(facility_preview(FacilityRole::Reaction));
        // A reaction `BuildPlanRevision` carries no blueprint at all.
        plan.blueprint = None;

        let snapshot = plan.to_task_execution_snapshot();

        assert!(snapshot.blueprint.is_none());
        assert_eq!(
            snapshot.facility.map(|facility| facility.role),
            Some(FacilityRole::Reaction)
        );
        assert_eq!(snapshot.duration_seconds, Some(7_200));
    }

    #[test]
    fn without_a_facility_every_facility_derived_field_is_none() {
        let snapshot = revision().to_task_execution_snapshot();

        assert_eq!(snapshot.runs, 50);
        assert!(snapshot.facility.is_none());
        assert!(snapshot.duration_seconds.is_none());
        assert!(snapshot.installation_cost.is_none());
        assert!(snapshot.blueprint.is_none());
        assert_eq!(
            snapshot.material_value,
            Some(Money::parse("987654").unwrap())
        );
    }
}

#[cfg(test)]
mod legacy_draft_planning_compat {
    use super::*;

    /// A `builds.draft_planning` row persisted by an older client still
    /// carries `expectedProfileRevision` on the facility slots. The field
    /// was removed from the Rust model; serde must ignore unknown keys so
    /// the row still deserializes and calculates against the current
    /// facility profile -- no jsonb migration required.
    #[test]
    fn draft_planning_input_ignores_a_legacy_expected_profile_revision() {
        let profile_id = FacilityProfileId::new();
        let legacy = serde_json::json!({
            "materialScope": { "regionId": 10_000_002, "locationId": 60_003_760 },
            "outputScope": { "regionId": 10_000_002, "locationId": 60_003_760 },
            "materialPricingPolicy": "highestBuy",
            "outputPricingPolicy": "lowestSell",
            "blueprintSelection": null,
            "manufacturingFacility": {
                "facilityProfileId": profile_id,
                "expectedProfileRevision": 7,
                "blueprintMe": 8,
                "blueprintTe": 16,
                "estimatedItemValue": null
            },
            "reactionFacility": {
                "facilityProfileId": profile_id,
                "expectedProfileRevision": 2,
                "estimatedItemValue": null
            },
            "componentResolutions": [
                {
                    "typeId": 34,
                    "recipe": { "mode": "manufacturing", "blueprintTypeId": 99 },
                    "facilityOverride": {
                        "facilityProfileId": profile_id,
                        "expectedProfileRevision": 3
                    }
                }
            ]
        });

        let parsed: DraftPlanningInput =
            serde_json::from_value(legacy).expect("legacy draft-planning JSON deserializes");
        let round_tripped = serde_json::to_value(&parsed).unwrap();

        let mfg = parsed
            .manufacturing_facility
            .expect("manufacturing facility present");
        assert_eq!(mfg.facility_profile_id, profile_id);
        assert_eq!(mfg.blueprint_me, 8);

        let rxn = parsed.reaction_facility.expect("reaction facility present");
        assert_eq!(rxn.facility_profile_id, profile_id);

        let mut resolutions = parsed.component_resolutions;
        let resolution = resolutions.remove(0);
        let override_ = resolution.facility_override.expect("override present");
        assert_eq!(override_.facility_profile_id, profile_id);

        // Re-serializing drops the vestigial key.
        let mfg_slot = &round_tripped["manufacturingFacility"];
        assert!(mfg_slot.get("expectedProfileRevision").is_none());
    }
}
