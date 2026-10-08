//! `BuildMaterialsCoordinator` -- the application orchestration behind
//! `POST /api/builds/:build_id/materials` (whole-tree inventory allocator).
//!
//! Thin: it owns the one coherent inventory observation and the
//! `BuildMaterialsError` fan-out; the allocating traversal itself is
//! [`IndustryService::project_build_materials`].
//!
//! 1. **one** `InventoryRepository::list_balances(workspace, owner)` read --
//!    the single, authoritative inventory observation for the whole request.
//! 2. seed [`PlanningInventory`] from free stock: those balance quantities
//!    minus `InventoryRepository::active_reservations` (open Epics' holds).
//! 3. [`IndustryService::project_build_materials`] -- the overlay-rooted
//!    allocating DFS: allocate inventory at **every** component-requirement
//!    boundary, size each surviving Build/Reaction child dynamically to its
//!    post-allocation production demand (`preview_plan_inner`, no
//!    persistence), prune a fully-covered subtree. Materials makes **zero**
//!    `ProductionRepository::coverage` calls and resolves no installation
//!    EIVs.
//!
//! No Axum / `AppState` / `ApiError`; the HTTP route keeps request
//! extraction, `workspace_context`, coordinator construction, the
//! `BuildMaterialsError -> ApiError` fan-out, and `Json`.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;

use iskworks_core::build_cost::{project_build_cost, BuildCostProjection};
use iskworks_core::build_materials::{
    AggregateMaterialLine, InventoryBasisEntry, MaterialSource, MaterialsAggregateWarning,
    NodeMaterialAllocation, PlanningInventory, VerificationBoundaryInput,
    VerificationOperationInput,
};
use iskworks_core::industry::{BuildMaterialsProjection, BuildMaterialsProjectionOutcome};
use iskworks_core::{
    build_create_candidate_preview, project_create_coverage, project_production_worksheet, BuildId,
    BuildPlanRevision, CreateBuildPlanPreview, GraphMarketEvidence, IndustryError, OwnerId,
    PreviewBuildPlanCommand, WorksheetPlanningTotals, WorksheetProjectionInput, WorkspaceId,
};
use iskworks_sde::SdeReadRepository;

use crate::build_preview::BuildPlanningDeps;
use crate::{BuildPreviewCoordinator, BuildPreviewError};

/// Every error the Build Materials application operation can produce.
#[derive(Debug, thiserror::Error)]
pub enum BuildMaterialsError {
    /// The shared preview/graph prelude failed (market coverage, EIVs,
    /// inventory repository unavailable, ...). Fans out to the same
    /// `ApiError`s a build-plan preview produces.
    #[error(transparent)]
    Preview(#[from] BuildPreviewError),
    /// A domain error `IndustryService::project_build_tree_revisions`
    /// `?`-propagates (root `BuildNotFound` -> 404, `Validation` -> 422,
    /// `Persistence` -> 5xx, ...).
    #[error(transparent)]
    Industry(#[from] IndustryError),
    /// The linked-build tree spans more than one owner/workspace. Impossible
    /// under current domain rules -- a linked child inherits its parent's
    /// owner at creation and it is immutable -- so this signals corrupt data
    /// rather than a user error. The offending node id is for server logs.
    #[error("build materials: a linked tree node does not share the root's owner/workspace")]
    MixedOwnerTree(BuildId),
    /// Authoritative per-node material quantities were unavailable for one or
    /// more Build-tree nodes ([`iskworks_core::build_materials::BuildMaterialsError::NodeRevisionUnavailable`]).
    /// The API returns a curated code; the diagnostic build ids stay in the
    /// server log.
    #[error(
        "build materials projection unavailable: {} node(s) lacked authoritative quantities",
        .0.len()
    )]
    ProjectionUnavailable(Vec<BuildId>),
    /// The canonical root plan's persisted
    /// producer graph cannot be planned (cycle, retired/unknown producer
    /// referenced, duplicate active producer, structural edge diagnostic).
    #[error("the canonical production graph of this plan is invalid: {0}")]
    CanonicalGraphInvalid(iskworks_core::canonical_planner::CanonicalGraphError),
}

/// The application read model returned by [`BuildMaterialsCoordinator::materials`]
/// and serialized as-is by the route. `rows` is the aggregate demand by
/// `type_id`; `nodeAllocations` and `sources` are retained for the upcoming
/// Materials UI drill-down and for allocation-correctness inspection.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildMaterialsSummary {
    pub build_id: BuildId,
    pub generated_at: DateTime<Utc>,
    pub rows: Vec<AggregateMaterialLine>,
    pub node_allocations: Vec<NodeMaterialAllocation>,
    pub sources: Vec<MaterialSource>,
    pub warnings: Vec<MaterialsAggregateWarning>,
    /// Primitive calculation inputs per boundary, in allocator traversal
    /// order. Populated only when the caller asks for it (the verification
    /// export); `#[serde(skip)]` -- never crosses the `/materials` wire, so
    /// the live Materials view pays nothing for it.
    #[serde(skip)]
    pub verification_inputs: Vec<VerificationBoundaryInput>,
    /// One entry per walked production node (operation), in DFS-entry order.
    /// Verification export only; `#[serde(skip)]`.
    #[serde(skip)]
    pub verification_operations: Vec<VerificationOperationInput>,
    /// Per-type slice of the single `list_balances` snapshot that seeded the
    /// planning pool -- quantity + weighted-average unit basis + total
    /// historical basis. Verification export only; `#[serde(skip)]`; **no
    /// second inventory read**.
    #[serde(skip)]
    pub inventory_basis: Vec<InventoryBasisEntry>,
    /// The market-evidence identity this overlay was valued
    /// against, resolved once by the same walk -- `BuildGraphCoordinator`
    /// exposes it on `BuildGraphProjection.marketEvidence` without
    /// re-resolving it. Not on the `/materials` wire (that route has no use
    /// for it); `#[serde(skip)]`.
    #[serde(skip)]
    pub market_evidence: iskworks_core::GraphMarketEvidence,
    /// The projection's own work counters (previews, plan loads, Builds
    /// hydrated). Diagnostics only; `#[serde(skip)]`.
    #[serde(skip)]
    pub metrics: iskworks_core::industry::PlannerMetrics,
}

/// One structured line per costed projection: how much work it did (one
/// inventory snapshot, at most one adjusted-price batch, previews, bounded
/// plan loads).
fn log_planner_metrics(materials: &BuildMaterialsSummary, adjusted_price_batch: bool) {
    let metrics = materials.metrics;
    tracing::info!(
        target: "iskworks_app::planner",
        build_id = %materials.build_id.0,
        operations = materials.verification_operations.len(),
        exact_previews = metrics.exact_previews,
        structural_discovery_previews = metrics.structural_discovery_previews,
        root_plan_loads = metrics.root_plan_loads,
        builds_loaded = metrics.builds_loaded,
        list_balances_calls = 1,
        adjusted_price_batches = u8::from(adjusted_price_batch),
        "planning projection"
    );
}

/// Application orchestration for the Build Materials aggregate. Constructed
/// per request from `AppState` with exactly the collaborator set
/// `BuildGraphCoordinator` / `BuildPreviewCoordinator` take -- it reuses the
/// preview's prelude.
pub struct BuildMaterialsCoordinator {
    preview: BuildPreviewCoordinator,
}

impl BuildMaterialsCoordinator {
    #[must_use]
    pub fn new(deps: BuildPlanningDeps) -> Self {
        Self {
            preview: BuildPreviewCoordinator::new(deps),
        }
    }

    /// The SDE read repository -- exposed for `BuildGraphCoordinator`'s own
    /// buildable-recipe lookups (display metadata Graph still needs
    /// beyond what the allocation-aware operations/boundaries carry).
    pub(crate) fn sde_repository(&self) -> Arc<dyn SdeReadRepository> {
        self.preview.sde_repository()
    }

    /// Project `build_id`'s linked-build tree under `command`'s live, unsaved
    /// planning overlay into the aggregate Materials demand.
    ///
    /// The path `build_id` is authoritative: it overwrites `command.build_id`
    /// so a client cannot project one build's tree under another's overlay
    /// (mirrors `BuildGraphCoordinator::graph`). Fully read-only -- it never
    /// saves the overlay, creates linked builds, registers market coverage,
    /// or mutates inventory / orders.
    /// `capture_verification` opts into the per-boundary primitive-input
    /// evidence (`verification_inputs`) the verification export needs; the
    /// live Materials view passes `false` and pays nothing for it.
    pub async fn materials(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        build_id: BuildId,
        mut command: PreviewBuildPlanCommand,
        capture_verification: bool,
    ) -> Result<BuildMaterialsSummary, BuildMaterialsError> {
        command.build_id = Some(build_id);
        self.materials_for_overlay(workspace_id, owner_id, command, capture_verification)
            .await
    }

    /// [`Self::materials`], but never overwrites `command.build_id`: used by
    /// the interactive planning-cost preview, where a brand-new
    /// (never-saved) Build legitimately has no `build_id` at all and forcing
    /// one would make the allocation-aware walk try to load a linked-build
    /// tree for a Build that does not exist. A path-authoritative caller
    /// (the Materials / cost-projection / verification-export routes) must
    /// keep using [`Self::materials`] instead, so a client can never project
    /// one Build's tree under another's overlay.
    async fn materials_for_overlay(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        command: PreviewBuildPlanCommand,
        capture_verification: bool,
    ) -> Result<BuildMaterialsSummary, BuildMaterialsError> {
        let generated_at = Utc::now();

        // (1) The single, authoritative inventory observation for the whole
        // request -- seeded into the ephemeral planning pool the traversal
        // draws down exactly once. Planning counts free stock only: what
        // open Epics have reserved is never available to another plan (or
        // to a new Epic's freeze).
        let inventory = self.preview.inventory_repository()?;
        let balances = inventory
            .list_balances(workspace_id, owner_id)
            .await
            .map_err(BuildPreviewError::from)?;
        let reserved = inventory
            .active_reservations(workspace_id, owner_id)
            .await
            .map_err(BuildPreviewError::from)?;
        let reserved_of = |type_id: i64| reserved.get(&type_id).copied().unwrap_or(0);
        let mut pool = PlanningInventory::seed_free(balances.iter().map(|balance| {
            (
                balance.key.type_id,
                balance.quantity,
                reserved_of(balance.key.type_id),
            )
        }));
        // Retain the per-type basis from that *same* snapshot for the
        // verification workbook -- no second `list_balances`.
        let inventory_basis: Vec<InventoryBasisEntry> = if capture_verification {
            balances
                .iter()
                .map(|balance| InventoryBasisEntry {
                    type_id: balance.key.type_id,
                    quantity: balance.quantity,
                    reserved_quantity: reserved_of(balance.key.type_id),
                    unit_basis: balance.average_unit_cost.map(|money| money.0),
                    total_basis: balance.total_historical_cost.0,
                })
                .collect()
        } else {
            Vec::new()
        };

        // (2) The overlay-rooted allocating traversal (dynamic child sizing,
        // subtree pruning, zero `ProductionRepository::coverage`).
        let projection: BuildMaterialsProjection = self
            .preview
            .industry_service()
            .project_build_materials(
                workspace_id,
                owner_id,
                &command,
                &mut pool,
                generated_at,
                capture_verification,
            )
            .await?;

        let aggregate = match projection.outcome {
            BuildMaterialsProjectionOutcome::Complete(aggregate) => aggregate,
            BuildMaterialsProjectionOutcome::Incomplete { missing_nodes } => {
                return Err(BuildMaterialsError::ProjectionUnavailable(missing_nodes));
            }
            BuildMaterialsProjectionOutcome::MixedOwnerTree { node } => {
                return Err(BuildMaterialsError::MixedOwnerTree(node));
            }
            BuildMaterialsProjectionOutcome::CanonicalGraphInvalid { error } => {
                return Err(BuildMaterialsError::CanonicalGraphInvalid(error));
            }
        };

        Ok(BuildMaterialsSummary {
            build_id: projection.root_build_id,
            generated_at,
            rows: aggregate.lines,
            node_allocations: aggregate.node_allocations,
            sources: aggregate.sources,
            warnings: aggregate.warnings,
            verification_inputs: aggregate.verification_inputs,
            verification_operations: aggregate.verification_operations,
            inventory_basis,
            market_evidence: projection.market_evidence,
            metrics: projection.metrics,
        })
    }

    /// A pure cost enrichment over the *same* allocation-aware plan
    /// projection [`Self::materials`] produces.
    ///
    /// 1. one [`Self::materials`] projection (its **one** `list_balances` is
    ///    the only inventory read; `capture_verification` rides on it);
    /// 2. **one** bulk `EsiApplicationService::adjusted_prices` resolution for
    ///    the union of every operation's base recipe material `type_id`s (no
    ///    per-operation lookup; skipped, with `installation` left incomplete,
    ///    when no `EsiApplicationService` is wired);
    /// 3. [`project_build_cost`] -- a pure, sync bottom-up cost pass.
    ///
    /// Read-only: no inventory write, no reservation, no production posting, no
    /// child-Build mutation, no second materials projection, no second
    /// inventory allocation, no persisted-child-run costing. Exposed directly by
    /// `POST /builds/:build_id/cost-projection` (engineering/debugging;
    /// `command.build_id` is overwritten with the path id, same
    /// authoritative-path contract as `/materials`).
    pub async fn build_cost_projection(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        build_id: BuildId,
        mut command: PreviewBuildPlanCommand,
    ) -> Result<BuildCostProjection, BuildMaterialsError> {
        command.build_id = Some(build_id);
        self.build_cost_projection_for_overlay(workspace_id, owner_id, command)
            .await
    }

    /// [`Self::build_cost_projection`], but never overwrites
    /// `command.build_id` -- see [`Self::materials_for_overlay`] for why.
    async fn build_cost_projection_for_overlay(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        command: PreviewBuildPlanCommand,
    ) -> Result<BuildCostProjection, BuildMaterialsError> {
        let (_materials, cost) = self
            .materials_with_planning_cost_for_overlay(workspace_id, owner_id, command)
            .await?;
        Ok(cost)
    }

    /// [`Self::materials`] + [`Self::build_cost_projection`], **plus** the
    /// raw bulk-resolved adjusted-price map used for EIV, from **one**
    /// allocation-aware walk. The verification-export workbook needs the
    /// per-`type_id` adjusted prices themselves (not just the derived EIV
    /// figure) so its `Types` sheet can expose them as primitive,
    /// independently-checkable evidence -- Excel must build EIV from these
    /// primitives, never from the API's precomputed EIV.
    ///
    /// The path `build_id` is authoritative (mirrors [`Self::materials`] /
    /// [`Self::build_cost_projection`]).
    pub async fn materials_with_planning_cost(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        build_id: BuildId,
        mut command: PreviewBuildPlanCommand,
    ) -> Result<
        (
            BuildMaterialsSummary,
            BuildCostProjection,
            std::collections::BTreeMap<i64, rust_decimal::Decimal>,
        ),
        BuildMaterialsError,
    > {
        command.build_id = Some(build_id);
        let materials = self
            .materials_for_overlay(workspace_id, owner_id, command, true)
            .await?;
        let (adjusted_prices, observed_at) = self
            .resolve_adjusted_prices(&materials.verification_inputs)
            .await?;
        log_planner_metrics(&materials, observed_at.is_some());
        let cost = project_build_cost(
            &materials.verification_operations,
            &materials.verification_inputs,
            &materials.inventory_basis,
            &adjusted_prices,
            observed_at,
        );
        Ok((materials, cost, adjusted_prices))
    }

    /// [`Self::materials_for_overlay`]'s summary *and* its
    /// [`BuildCostProjection`], from **one** allocation-aware walk -- never
    /// two. `BuildGraphCoordinator` needs both (topology/display evidence
    /// from `verification_operations`/`verification_inputs`, cost from the
    /// projection) to present Graph as a view of this same authoritative
    /// planning projection, not a second one.
    pub(crate) async fn materials_with_planning_cost_for_overlay(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        command: PreviewBuildPlanCommand,
    ) -> Result<(BuildMaterialsSummary, BuildCostProjection), BuildMaterialsError> {
        let materials = self
            .materials_for_overlay(workspace_id, owner_id, command, true)
            .await?;

        let (adjusted_prices, observed_at) = self
            .resolve_adjusted_prices(&materials.verification_inputs)
            .await?;
        log_planner_metrics(&materials, observed_at.is_some());

        let cost = project_build_cost(
            &materials.verification_operations,
            &materials.verification_inputs,
            &materials.inventory_basis,
            &adjusted_prices,
            observed_at,
        );
        Ok((materials, cost))
    }

    /// The union of every boundary's base recipe material `type_id`s,
    /// resolved **once, in bulk** -- never per operation. Prefers
    /// `EsiApplicationService::adjusted_prices` (cache, then the repository,
    /// then one ESI fetch-and-cache) when an `EsiApplicationService` is
    /// wired; falls back to a single direct
    /// `AdjustedPriceRepository::latest_adjusted_prices` read (the
    /// testability seam -- fakeable without Postgres/ESI) when
    /// only that narrower repository is wired; otherwise returns an empty
    /// map (installation stays incomplete, never zero-substituted).
    async fn resolve_adjusted_prices(
        &self,
        boundaries: &[iskworks_core::build_materials::VerificationBoundaryInput],
    ) -> Result<
        (
            std::collections::BTreeMap<i64, rust_decimal::Decimal>,
            Option<DateTime<Utc>>,
        ),
        BuildMaterialsError,
    > {
        let mut type_ids: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();
        for boundary in boundaries {
            if boundary.base_quantity_per_run > 0 && boundary.type_id > 0 {
                type_ids.insert(boundary.type_id);
            }
        }
        let type_ids: Vec<i64> = type_ids.into_iter().collect();
        if type_ids.is_empty() {
            return Ok((std::collections::BTreeMap::new(), None));
        }

        if let Some(esi) = self.preview.esi_service() {
            let set = esi
                .adjusted_prices(&type_ids)
                .await
                .map_err(|error| BuildMaterialsError::Preview(BuildPreviewError::Esi(error)))?;
            return Ok((set.values, Some(set.observed_at)));
        }
        if let Some(repository) = self.preview.adjusted_price_repository() {
            let now = Utc::now();
            let values = repository
                .latest_adjusted_prices(&type_ids, now)
                .await
                .map_err(|error| {
                    BuildMaterialsError::Preview(BuildPreviewError::Inventory(error))
                })?;
            return Ok((values, Some(now)));
        }
        Ok((std::collections::BTreeMap::new(), None))
    }

    /// The live Build editor preview
    /// (`POST /api/build-plans/preview`), on the allocation-aware
    /// planning-cost model.
    ///
    /// Computes the SAME overlay's allocation-aware quantity/cost tree
    /// [`Self::build_cost_projection`] produces, plus this node's own
    /// non-cost evidence (materials identity, blueprint, facility
    /// requirements, price-line provenance, contributions) via
    /// [`iskworks_core::IndustryService::preview_plan`] -- then merges the two with
    /// [`BuildCostProjection::apply_to_revision`].
    ///
    /// **Known remaining duplication**: this still
    /// runs the root node's own preview/pricing pass (`prepare_preview_inputs`
    /// and `preview_plan` together) *separately* from
    /// the root pass inside `build_cost_projection`'s own
    /// `project_build_materials` projection -- two root-level `preview_plan_inner`
    /// calls per request, because the former also carries display-only
    /// evidence (`blueprint`, `facility` requirements/duration, `price_lines`
    /// provenance, `contributions`) `BuildCostProjection` does not. Only
    /// **one** inventory
    /// snapshot / adjusted-price batch is read (both live inside
    /// `build_cost_projection`).
    pub async fn preview_plan_with_planning_cost(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        command: PreviewBuildPlanCommand,
    ) -> Result<(BuildPlanRevision, Option<BuildCostProjection>), BuildMaterialsError> {
        // `project_build_materials` (and therefore `BuildCostProjection`)
        // requires a *persisted* root -- it resolves the Build's plan root
        // (same constraint Build Graph has), which cannot exist
        // before the Build itself is saved. A brand-new Build has no `build_id` yet, so it falls back to
        // the single-level preview -- which degrades
        // correctly to "no linked-child costs" for a build with no children.
        if command.build_id.is_none() {
            let revision = self
                .preview
                .preview_unsaved_plan(workspace_id, owner_id, command)
                .await?;
            return Ok((revision, None));
        }

        // `resolve_component_eivs: false` -- every line's `installation_cost`
        // is about to be overwritten by `BuildCostProjection::apply_to_revision`
        // below regardless, so resolving it here would be pure wasted
        // ESI-dependent work (and could needlessly 503 an otherwise-servable
        // request when no `EsiApplicationService` is wired -- see
        // `prepare_preview_inputs`'s doc comment).
        let (material_coverage, component_eivs) = self
            .preview
            .prepare_preview_inputs(workspace_id, owner_id, &command, false)
            .await?;
        let market_evidence = GraphMarketEvidence::new(command.market_evidence.clone());
        let mut revision = self
            .preview
            .industry_service()
            .preview_plan(
                workspace_id,
                owner_id,
                command.clone(),
                &component_eivs,
                &material_coverage,
                (!market_evidence.is_empty()).then_some(&market_evidence),
            )
            .await
            .map_err(BuildPreviewError::from)?;

        let planning = self
            .build_cost_projection_for_overlay(workspace_id, owner_id, command)
            .await?;
        planning.apply_to_revision(&mut revision);
        Ok((revision, Some(planning)))
    }

    /// The frozen plan of one Build-backed ticket for `build` (whose `runs`
    /// the caller has already set to the ticket's runs): the coverage-aware
    /// snapshot ([`BuildPreviewCoordinator::calculate_epic_snapshot`]) with,
    /// for a plan root, the plan's cost projection at those runs overlaid --
    /// so a produced component is priced at its producer's consumed cost,
    /// exactly as the Epic freeze prices it. A producer's own ticket keeps
    /// its produced rows unpriced: the plan sizes that producer from its
    /// aggregate demand, not from one ticket's runs.
    ///
    /// [`BuildPreviewCoordinator::calculate_epic_snapshot`]: crate::BuildPreviewCoordinator::calculate_epic_snapshot
    pub async fn ticket_snapshot(
        &self,
        workspace_id: WorkspaceId,
        build: &iskworks_core::Build,
    ) -> Result<BuildPlanRevision, BuildMaterialsError> {
        let mut revision = self
            .preview
            .calculate_epic_snapshot(workspace_id, build)
            .await?;
        let industry = self.preview.industry_service();
        let is_plan_root = matches!(
            industry.plan_root_of(workspace_id, build.id).await?,
            Some(root) if root == build.id
        );
        if !is_plan_root {
            return Ok(revision);
        }
        let Some(mut command) = industry
            .reconstruct_preview_command(workspace_id, build)
            .await?
        else {
            return Ok(revision);
        };
        command.build_id = Some(build.id);
        let cost = self
            .build_cost_projection_for_overlay(workspace_id, build.owner_id, command)
            .await?;
        cost.apply_to_revision(&mut revision);
        Ok(revision)
    }

    /// The create-flow candidate preview
    /// (`POST /api/build-plans/candidate-preview`), on the allocation-aware
    /// planning-cost model. Same merge as [`Self::preview_plan_with_planning_cost`],
    /// then the create-flow coverage/reservation/worksheet construction,
    /// reading the merged (planning-cost) revision.
    pub async fn candidate_preview_with_planning_cost(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        command: PreviewBuildPlanCommand,
    ) -> Result<CreateBuildPlanPreview, BuildMaterialsError> {
        let recipe = self
            .preview
            .industry_service()
            .capture_recipe(&command.recipe)
            .await
            .map_err(BuildPreviewError::from)?;
        let output_quantity = recipe
            .primary_product()
            .quantity_per_run
            .checked_mul(command.runs)
            .ok_or(IndustryError::InvalidRecipe)
            .map_err(BuildPreviewError::from)?;

        let (candidate, planning) = self
            .preview_plan_with_planning_cost(workspace_id, owner_id, command)
            .await?;

        let inventory = self
            .preview
            .inventory_repository()?
            .list_balances(workspace_id, owner_id)
            .await
            .map_err(BuildPreviewError::from)?;
        let mut reservations = std::collections::BTreeMap::new();
        for material in &candidate.material_lines {
            reservations.insert(
                material.type_id,
                self.preview
                    .reserved_quantity(workspace_id, owner_id, material.type_id)
                    .await?,
            );
        }
        let coverage = project_create_coverage(
            BuildId::new(),
            owner_id,
            &candidate.recipe_fingerprint,
            candidate.runs,
            &candidate.material_lines,
            &inventory,
            &reservations,
        )
        .map_err(BuildPreviewError::from)?;
        let material_type_ids = candidate
            .material_lines
            .iter()
            .map(|line| line.type_id)
            .collect::<Vec<_>>();
        let group_labels = self
            .preview
            .sde_repository()
            .type_group_names(&material_type_ids)
            .await
            .map_err(BuildPreviewError::from)?;
        let worksheet = project_production_worksheet(WorksheetProjectionInput {
            price_lines: &candidate.snapshot.items,
            material_lines: &candidate.material_lines,
            coverage: &coverage,
            facility: candidate.root_facility(),
            material_cost: candidate.estimated_material_cost,
            expected_revenue: candidate.expected_revenue,
            estimated_margin: candidate.estimated_margin,
            pricing_complete: candidate.pricing_complete,
            output_quantity,
            group_labels: &group_labels,
            warnings: Vec::new(),
            planning: planning.as_ref().map(|planning| WorksheetPlanningTotals {
                complete: planning.complete,
                total_fresh_outlay: planning.root.total_fresh_outlay,
                total_surplus_retained_basis: planning.root.total_surplus_retained_basis,
            }),
        })
        .map_err(BuildPreviewError::from)?;
        Ok(
            build_create_candidate_preview(candidate, coverage, worksheet)
                .map_err(BuildPreviewError::from)?,
        )
    }
}
