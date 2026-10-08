use super::*;

pub const STALE_MARKET_PROVENANCE: &str = "market_freshness=stale";

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CalculationState {
    Complete,
    Incomplete,
    Unavailable,
    NotConfigured,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProfitabilityState {
    Complete,
    Qualified,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProfitabilityCost {
    ProjectedInventoryCost,
    MarketMaterialEstimate,
    InstallationCost,
    MarketFees,
    SalesTax,
    Hauling,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateFieldIssue {
    pub field: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateIssue {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CandidateValidation {
    pub fields: Vec<CandidateFieldIssue>,
    pub blockers: Vec<CandidateIssue>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateCompleteness {
    pub materials: CalculationState,
    pub duration: CalculationState,
    pub pricing: CalculationState,
    pub installation: CalculationState,
    pub inventory_cost: CalculationState,
    pub profitability: ProfitabilityState,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfitabilityBasis {
    pub included_costs: Vec<ProfitabilityCost>,
    pub excluded_costs: Vec<ProfitabilityCost>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateMaterialComparison {
    pub type_id: i64,
    pub type_name: String,
    pub current_quantity: u64,
    pub candidate_quantity: u64,
    pub delta: i64,
    pub available_inventory: u64,
    pub candidate_covered_quantity: u64,
    pub candidate_missing_quantity: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildPlanCandidateComparison {
    pub materials: Vec<CandidateMaterialComparison>,
    pub current_total_material_units: u64,
    pub candidate_total_material_units: u64,
    pub total_material_units_delta: i64,
    pub current_duration_seconds: Option<u64>,
    pub candidate_duration_seconds: Option<u64>,
    pub duration_seconds_delta: Option<i64>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalculatedBuildPlan {
    pub runs: u64,
    pub recipe_fingerprint: String,
    /// The manual price list used as a fallback for this plan, if any --
    /// `None` when every line resolved from market scope pricing alone
    /// (there is not always a single resolved `PriceSource` behind a plan).
    pub price_source_id: Option<PriceSourceId>,
    pub price_source_name: String,
    pub price_source_revision: u64,
    pub price_lines: Vec<PriceSnapshotLine>,
    pub pricing_complete: bool,
    pub estimated_material_cost: Money,
    pub expected_revenue: Option<Money>,
    pub estimated_margin: Option<Money>,
    pub missing_price_count: u32,
    pub material_lines: Vec<PlannedMaterialLine>,
    pub manufacturing_facility: Option<FacilityPlanPreview>,
    pub reaction_facility: Option<FacilityPlanPreview>,
    pub blueprint: Option<ResolvedBlueprintAssumptions>,
    /// Per-material calculation evidence (see
    /// [`BuildPlanRevision::effective_requirements`]). Populated by the
    /// transient-calculation path; empty otherwise.
    pub effective_requirements: Vec<EffectiveMaterialRequirement>,
}

impl CalculatedBuildPlan {
    /// The root job's own facility preview, whichever slot it came from.
    /// Exactly one of the two slots is ever populated for the root --
    /// the other exists only for build-resolved sub-components of that kind.
    pub fn root_facility(&self) -> Option<&FacilityPlanPreview> {
        self.manufacturing_facility
            .as_ref()
            .or(self.reaction_facility.as_ref())
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BuildPlanCandidateCalculation {
    pub build: Build,
    pub current_plan: BuildPlanRevision,
    pub normalized: NormalizedBuildPlanCandidate,
    pub calculated: CalculatedBuildPlan,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CandidateApplyBlockReason {
    CandidateUnchanged,
    ValidationBlocked,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CandidateDecisionTone {
    Positive,
    Warning,
    Blocking,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateDecisionGuidance {
    pub headline: String,
    pub supporting_text: String,
    pub tone: CandidateDecisionTone,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildPlanCandidatePreview {
    pub build_id: crate::BuildId,
    pub expected_build_revision: u64,
    pub expected_active_plan_revision: u64,
    pub expected_price_source_revision: u64,
    pub expected_facility_profile_revision: Option<u64>,
    pub candidate_fingerprint: CandidateFingerprint,
    pub candidate_changed: bool,
    pub can_apply: bool,
    pub apply_block_reason: Option<CandidateApplyBlockReason>,
    pub current: BuildPlanRevision,
    pub candidate: CalculatedBuildPlan,
    pub comparison: BuildPlanCandidateComparison,
    pub current_coverage: crate::BuildCoverageReport,
    pub candidate_coverage: crate::BuildCoverageReport,
    pub decision: CandidateDecisionGuidance,
    pub validation: CandidateValidation,
    pub warnings: Vec<CandidateIssue>,
    pub completeness: CandidateCompleteness,
    pub profitability_basis: ProfitabilityBasis,
    pub calculation_evidence: CalculationEvidenceProjection,
    pub worksheet: crate::ProductionWorksheetProjection,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateBuildPlanPreview {
    pub candidate_fingerprint: CandidateFingerprint,
    pub can_plan: bool,
    pub candidate: BuildPlanRevision,
    pub coverage: crate::BuildCoverageReport,
    pub projected_inventory_cost: Option<Money>,
    pub decision: CandidateDecisionGuidance,
    pub validation: CandidateValidation,
    pub warnings: Vec<CandidateIssue>,
    pub completeness: CandidateCompleteness,
    pub profitability_basis: ProfitabilityBasis,
    pub calculation_evidence: CalculationEvidenceProjection,
    pub worksheet: crate::ProductionWorksheetProjection,
}

/// The three staged totals below (`base_market_value` -> `after_blueprint_me`
/// -> `after_structure`) are **not** "reprice every final row at a per-unit
/// market price" -- they walk the *parent's own* Buy-side ME/structure/rig
/// discounts across the recipe's base quantities, to explain how those
/// discounts change the parent's own material bill.
///
/// A plain Buy row participates in that repricing: each stage multiplies its
/// base (recipe) quantity by that stage's cumulative discount and reprices
/// at the row's own market/manual unit price.
///
/// A self-produced Build/Reaction row does not -- it has no per-unit market
/// price to reprice (`PlannedMaterialLine::unit_price` is `None` for one by
/// design; see `BuildCostProjection::apply_to_revision`'s doc comment). Its
/// planning cost was already computed independently, from its own recipe,
/// own ME/facility effects, own dynamic runs, own installation, and
/// proportional consumed production basis -- the parent's own ME/structure
/// discount has no bearing on a job it never buys into. Such a row's
/// `line_total` is therefore carried unmultiplied and **identical** across
/// all three stages, rather than repriced or excluded.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialCostEvidence {
    pub complete: bool,
    pub base_market_value: Option<Money>,
    pub after_blueprint_me: Option<Money>,
    pub after_structure: Option<Money>,
    pub adjusted_material_cost: Money,
    #[serde(with = "rust_decimal::serde::str")]
    pub blueprint_multiplier: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub structure_multiplier: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub rig_multiplier: Decimal,
    pub requirement_traces: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalculationEvidenceProjection {
    pub profit_margin_percent: Option<String>,
    pub system_cost_index_percent: Option<String>,
    pub material_cost: MaterialCostEvidence,
    pub duration_steps: Vec<crate::DurationCalculationStep>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedBlueprintAssumptions {
    pub source_mode: crate::BlueprintSourceMode,
    pub blueprint_type_id: i64,
    pub blueprint_name: String,
    pub kind: BlueprintKind,
    pub material_efficiency: u8,
    pub time_efficiency: u8,
    pub licensed_runs: Option<u64>,
    pub requested_runs: u64,
    pub source_observation_id: Option<Uuid>,
    pub source_eve_item_id: Option<i64>,
    pub source_owner_id: Option<crate::OwnerId>,
    pub source_owner_name: Option<String>,
    pub source_location_id: Option<i64>,
    pub source_location_name: Option<String>,
    pub observed_at: Option<DateTime<Utc>>,
    pub imported_at: Option<DateTime<Utc>>,
    pub manual_notes: Option<String>,
    pub planned_duration_seconds: Option<u64>,
    pub formula_version: String,
}

impl From<crate::BlueprintSnapshot> for ResolvedBlueprintAssumptions {
    fn from(snapshot: crate::BlueprintSnapshot) -> Self {
        Self {
            source_mode: snapshot.source_mode,
            blueprint_type_id: snapshot.blueprint_type_id,
            blueprint_name: snapshot.blueprint_name,
            kind: snapshot.kind,
            material_efficiency: snapshot.material_efficiency,
            time_efficiency: snapshot.time_efficiency,
            licensed_runs: snapshot.licensed_runs,
            requested_runs: snapshot.requested_runs,
            source_observation_id: snapshot.source_observation_id,
            source_eve_item_id: snapshot.source_eve_item_id,
            source_owner_id: snapshot.source_owner_id,
            source_owner_name: snapshot.source_owner_name,
            source_location_id: snapshot.source_location_id,
            source_location_name: snapshot.source_location_name,
            observed_at: snapshot.observed_at,
            imported_at: snapshot.imported_at,
            manual_notes: snapshot.manual_notes,
            planned_duration_seconds: snapshot.planned_duration_seconds,
            formula_version: snapshot.formula_version,
        }
    }
}

impl ResolvedBlueprintAssumptions {
    fn into_snapshot(
        self,
        build_id: crate::BuildId,
        captured_at: DateTime<Utc>,
    ) -> crate::BlueprintSnapshot {
        crate::BlueprintSnapshot {
            id: Uuid::new_v4(),
            build_id,
            source_mode: self.source_mode,
            blueprint_type_id: self.blueprint_type_id,
            blueprint_name: self.blueprint_name,
            kind: self.kind,
            material_efficiency: self.material_efficiency,
            time_efficiency: self.time_efficiency,
            licensed_runs: self.licensed_runs,
            requested_runs: self.requested_runs,
            source_observation_id: self.source_observation_id,
            source_eve_item_id: self.source_eve_item_id,
            source_owner_id: self.source_owner_id,
            source_owner_name: self.source_owner_name,
            source_location_id: self.source_location_id,
            source_location_name: self.source_location_name,
            observed_at: self.observed_at,
            imported_at: self.imported_at,
            manual_notes: self.manual_notes,
            planned_duration_seconds: self.planned_duration_seconds,
            formula_version: self.formula_version,
            captured_at,
        }
    }
}

impl CalculatedBuildPlan {
    /// `revision` is a caller-supplied number rather than derived here --
    /// Build tracks no plan-revision history of its own. Preview-only
    /// callers can pass `1`.
    pub fn into_plan_revision(
        self,
        build: &Build,
        revision: u64,
        captured_at: DateTime<Utc>,
    ) -> Result<BuildPlanRevision, IndustryError> {
        Ok(BuildPlanRevision {
            id: BuildPlanId(Uuid::new_v4()),
            revision,
            runs: self.runs,
            recipe_fingerprint: self.recipe_fingerprint,
            snapshot: PriceSnapshot {
                id: PriceSnapshotId(Uuid::new_v4()),
                price_source_id: self.price_source_id,
                source_name: self.price_source_name,
                source_revision: self.price_source_revision,
                created_at: captured_at,
                items: self.price_lines,
            },
            pricing_complete: self.pricing_complete,
            estimated_material_cost: self.estimated_material_cost,
            expected_revenue: self.expected_revenue,
            estimated_margin: self.estimated_margin,
            missing_price_count: self.missing_price_count,
            active: true,
            planned_at: captured_at,
            superseded_at: None,
            material_lines: self.material_lines,
            manufacturing_facility: self.manufacturing_facility,
            reaction_facility: self.reaction_facility,
            blueprint: self
                .blueprint
                .map(|blueprint| blueprint.into_snapshot(build.id, captured_at)),
            effective_requirements: self.effective_requirements,
        })
    }
}
