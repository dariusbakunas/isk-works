//! The whole-tree frozen planning snapshot an Epic owns.
//!
//! [`PlanOperation`] is one frozen row per production operation in the
//! tree (the root, and every active Build/Reaction descendant), addressed
//! by a stable `occurrence_key` (matches `crate::build_graph`'s
//! `graph_node_id`: `root:<uuid>` / `build:<uuid>`) rather than `build_id`
//! alone. `OrderRequirement` (`aggregate.rs`) carries an
//! `operation_occurrence_key` placing each row in this tree, so it
//! remains the **one** authoritative whole-tree requirement table instead
//! of a second, competing one.
//!
//! The frozen evidence types here ([`PlanOperationEvidence`],
//! [`PlanRequirementEvidence`]) are deliberately **not** the live
//! `crate::build_cost`/`crate::build_materials` projection types
//! (`OperationInstallationCost`, `VerificationBoundaryInput`, ...): those
//! are `Serialize`-only, evidence-shaped for one live HTTP response, and
//! free to evolve with the live planning engine. A frozen Epic's stored
//! JSON must stay readable forever regardless of later live-model changes,
//! so it gets its own small, stable, `Serialize + Deserialize` shape,
//! populated by copying fields at freeze time -- the same reasoning that
//! gives `TaskExecutionSnapshot` its own independent shape rather than
//! embedding a live preview type directly.
//!
//! ## Authoritative model for a version-2 (whole-tree) Epic
//!
//! Three layers exist for a version-2 Epic; each has exactly one job, and
//! nothing here should leave a reader guessing which one "wins":
//!
//! 1. **`PlanOperation` / `OrderRequirement`** (this crate, `order_plan_operations`
//!    / `order_requirements`) -- the **authoritative frozen whole-tree plan**.
//!    Every quantity, cost, and evidence field a version-2 Epic can report
//!    comes from here. Nothing else is ever recomputed from these once
//!    written; they are read-only after `create_order_plan`'s one
//!    transaction commits.
//! 2. **`Ticket` / `TicketPrerequisite`** (`ticket.rs`) -- **derived execution
//!    copies**, generated once (eagerly, one ticket per active operation)
//!    directly from (1) at Create-Epic time. `Ticket::produced_quantity` /
//!    `material_component_cost` / `own_installation_cost` /
//!    `total_production_cost` / `plan_evidence` mirror the owning
//!    `PlanOperation` field-for-field; `TicketPrerequisite` mirrors the
//!    matching `OrderRequirement` rows. They exist so Board/ticket UI and
//!    execution actions (`record-production`, status transitions) have a
//!    natural per-ticket home for this data, never as a second source of
//!    truth -- a reader who finds `Ticket` and `PlanOperation` disagreeing
//!    has found a bug, not two valid answers.
//!    `Ticket::execution_snapshot` (the legacy root-only shape) is populated
//!    **only** for the root ticket, and only from the narrower
//!    `calculate_epic_snapshot` call `create_order`'s own doc describes --
//!    it is informational display data for the root only, never
//!    authoritative for a version-2 Epic, and absent entirely on every
//!    generated child ticket.
//! 3. **`Order`'s own summary fields** (`estimated_material_cost`,
//!    `expected_revenue`, `estimated_margin`, `missing_price_count`) --
//!    **denormalized summary only**, still sourced from that same narrower
//!    `calculate_epic_snapshot` call (sell-side pricing the whole-tree walk
//!    never computes). **Known, confirmed divergence**: these fields do not reflect overlay fields beyond
//!    `runs` (facility, pricing selections, component resolutions, ...), so
//!    they can disagree with the root `PlanOperation`'s own totals for the
//!    same Epic. Never treat `Order`'s summary as authoritative for
//!    anything below the whole-Epic-total level; prefer the root
//!    `PlanOperation` (or, for version-1 Epics, the root `OrderRequirement`
//!    set) instead. Recommended follow-up: either thread the whole overlay
//!    into that narrower call, or derive these fields from the root
//!    `PlanOperation`'s own totals and delete the second calculation.
//!
//! A version-1 (legacy, root-only) Epic has no `PlanOperation` rows at
//! all; its `OrderRequirement` set (all `operation_occurrence_key IS NULL`)
//! and its root ticket's `execution_snapshot` remain authoritative for it;
//! the version-2 layer never reinterprets version 1.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::build_materials::MaterialActivity;
use crate::{BuildId, Money, PricingSelectionKind, RecipeCurrency};

use super::OrderId;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlanOperationId(pub Uuid);

impl PlanOperationId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for PlanOperationId {
    fn default() -> Self {
        Self::new()
    }
}

/// One production operation's frozen installation-cost evidence -- the
/// audit trail behind [`PlanOperation::own_installation_cost`]. Mirrors
/// `crate::build_cost::OperationInstallationCost`'s field set (copied at
/// freeze time, never re-derived), but is its own independent, storage-
/// stable type -- see this module's own doc for why.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanInstallationEvidence {
    pub estimated_item_value: Option<Money>,
    pub eiv_missing_type_ids: Vec<i64>,
    pub system_cost_index: Option<String>,
    pub job_cost_reduction_percent: String,
    pub facility_tax_percent: String,
    pub scc_surcharge_percent: String,
    pub alliance_surcharge_percent: String,
    pub fixed_supplemental_cost: Money,
    pub unmodified_system_index_cost: Option<Money>,
    pub system_index_cost: Option<Money>,
    pub facility_tax: Option<Money>,
    pub scc_surcharge: Option<Money>,
    pub alliance_surcharge: Option<Money>,
    pub complete: bool,
    pub formula_version: String,
    pub facility_profile_id: Option<Uuid>,
    pub facility_profile_revision: Option<u64>,
}

fn one_job() -> u64 {
    1
}

/// A frozen operation's own non-cost display/audit evidence -- everything
/// [`PlanOperation`]'s typed columns don't already carry.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanOperationEvidence {
    pub effective_me: Option<u8>,
    pub effective_te: Option<u8>,
    /// How many industry jobs the frozen `runs` split into (a copy's
    /// licensed runs per job). Epics frozen before multi-BPC job splits read
    /// as one job.
    #[serde(default = "one_job")]
    pub job_count: u64,
    pub recipe_currency: RecipeCurrency,
    pub installation: Option<PlanInstallationEvidence>,
    /// This operation's own incomplete-cost / stale-evidence reasons at
    /// freeze time, human-readable (the same messages
    /// `crate::build_cost::CostWarning` renders live) -- audit only, never
    /// re-interpreted.
    pub warnings: Vec<String>,
}

/// One frozen production operation -- the root, or an active (not
/// fully-covered-by-inventory) Build/Reaction descendant. See this
/// module's own doc for the whole-tree design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanOperation {
    pub id: PlanOperationId,
    pub order_id: OrderId,
    /// `root:<uuid>` / `build:<uuid>` -- see this module's own doc.
    pub occurrence_key: String,
    /// `None` for the root operation. **Version 3** (canonical): also
    /// `None` for a fan-in operation serving more than one consuming
    /// operation -- never an arbitrary consumer. The operation DAG is the
    /// requirement rows' `child_occurrence_key -> operation_occurrence_key`
    /// relation (see `super::derive_operation_dag`); the root is the
    /// operation whose key starts with `root:`.
    pub parent_occurrence_key: Option<String>,
    /// The Build this operation was frozen from. `None` after deletion of
    /// the live planning object; captured operation data remains readable.
    pub build_id: Option<BuildId>,
    pub activity: MaterialActivity,
    /// The PROJECTED runs this plan required at freeze time -- never the
    /// Build's own persisted `runs` (see `persisted_runs`).
    pub runs: u64,
    /// This operation's own Build.runs at freeze time -- informational
    /// only, exactly like the live Graph's `persistedRuns`. Never
    /// re-derived later from the Build's current (possibly since changed)
    /// persisted runs.
    pub persisted_runs: u64,
    pub product_type_id: i64,
    pub product_name: String,
    pub output_per_run: u64,
    pub produced_quantity: u64,
    pub blueprint_or_formula_type_id: i64,
    pub material_component_cost: Option<Money>,
    pub own_installation_cost: Option<Money>,
    pub total_production_cost: Option<Money>,
    /// `false` iff any of this operation's own boundaries, or a child
    /// operation it consumes, had an incomplete cost at freeze time --
    /// never zero-substituted; the cost fields above are simply `None`.
    pub complete: bool,
    /// This operation's aggregate physical
    /// consumption across every requirement it serves (sum of those rows'
    /// `child_consumed_quantity`), its one surplus
    /// (`produced_quantity - consumed_quantity`), and that surplus's
    /// retained cost basis -- frozen once per operation rather than only on
    /// one arbitrary "surplus owner" requirement. `None` for the root
    /// operation and for every version-1/2 operation.
    pub consumed_quantity: Option<u64>,
    pub surplus_quantity: Option<u64>,
    pub surplus_retained_basis: Option<Money>,
    pub evidence: PlanOperationEvidence,
    pub created_at: DateTime<Utc>,
}

/// [`PlanOperation`], before persistence -- everything but `order_id`
/// (supplied once by the enclosing [`super::NewOrder`]-equivalent) and
/// `created_at` (stamped by the repository).
#[derive(Debug, Clone, PartialEq)]
pub struct NewPlanOperation {
    pub id: PlanOperationId,
    pub occurrence_key: String,
    pub parent_occurrence_key: Option<String>,
    pub build_id: BuildId,
    pub activity: MaterialActivity,
    pub runs: u64,
    pub persisted_runs: u64,
    pub product_type_id: i64,
    pub product_name: String,
    pub output_per_run: u64,
    pub produced_quantity: u64,
    pub blueprint_or_formula_type_id: i64,
    pub material_component_cost: Option<Money>,
    pub own_installation_cost: Option<Money>,
    pub total_production_cost: Option<Money>,
    pub complete: bool,
    pub consumed_quantity: Option<u64>,
    pub surplus_quantity: Option<u64>,
    pub surplus_retained_basis: Option<Money>,
    pub evidence: PlanOperationEvidence,
}

/// One requirement boundary's frozen fresh-price provenance -- the audit
/// trail behind a `Buy`/`Unresolved` row's `estimated_unit_cost`. Mirrors
/// `crate::build_materials::VerificationBoundaryInput`'s price-evidence
/// fields (copied at freeze time), independently typed for the same reason
/// as [`PlanOperationEvidence`].
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanRequirementEvidence {
    pub fresh_price_selection: PricingSelectionKind,
    pub fresh_price_note: String,
    pub fresh_price_stale: bool,
    pub market_region_id: Option<i64>,
    pub market_location_id: Option<i64>,
}
