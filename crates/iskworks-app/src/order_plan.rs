//! `OrderPlanCoordinator` -- the application orchestration behind the
//! whole-plan Epic freeze, and the single sell-side-enriched
//! planning revision Create Epic derives its financial summary and root
//! ticket compatibility evidence from.
//!
//! Thin, mirroring `BuildGraphCoordinator`: it wraps `BuildMaterialsCoordinator`
//! and reuses `materials_with_planning_cost_for_overlay` (one
//! `list_balances` read, one `IndustryService::project_build_materials`
//! projection with `capture_verification: true`, one bulk adjusted-price
//! resolution, one `crate::build_cost::project_build_cost` pass) -- the
//! *same* authoritative projection Materials/Worksheet/Graph use,
//! never a second planning engine. `iskworks_core::order::freeze_order_plan`
//! (pure, sync) then reduces that one projection's operations/boundaries/cost into
//! the frozen whole-tree snapshot.
//!
//! `freeze` also returns a sell-side-enriched
//! [`iskworks_core::BuildPlanRevision`], built the *same* way
//! `BuildMaterialsCoordinator::preview_plan_with_planning_cost`
//! does for the live Build editor: one root-level preview
//! (`IndustryService::preview_plan` -- materials identity, blueprint,
//! facility profile/requirements, output price resolution),
//! merged with the **same** `BuildCostProjection` this call already
//! computed for the freeze (`BuildCostProjection::apply_to_revision`,
//! never a second cost pass). `revision.expected_revenue` comes from that
//! one root-level preview's own output-price resolution (sell-side
//! evidence the canonical materials/cost projection never resolves);
//! `estimated_material_cost`/`missing_price_count`/`pricing_complete`/
//! `estimated_margin`/`manufacturing_facility`/`reaction_facility`'s
//! installation cost are all overwritten by `apply_to_revision` from the
//! *same* root `PlanOperation`/`BuildCostProjection` the frozen plan
//! itself is built from -- so `revision.to_task_execution_snapshot()` and
//! the whole-tree root `PlanOperation` can never disagree. (A second,
//! independent `calculate_epic_snapshot` call would only see the persisted
//! Build, not the live overlay, and could diverge.)
//!
//! An incomplete *cost* (missing price, missing system cost index, ...) is
//! tolerated -- the same live-model tolerance Worksheet/Graph show
//! (`complete: false`, no zero-substitution) -- and simply freezes with
//! `None` cost fields; this coordinator never blocks on it.
//!
//! Ticket generation from the frozen operations/requirements is **not**
//! this coordinator's job -- it needs display-only data (display ids,
//! notes) and Epic-lifecycle conventions the `create_order` route
//! owns; see `apps/iskworks-api/src/routes/orders.rs`.

use iskworks_core::order::{freeze_order_plan, NewOrderRequirement, NewPlanOperation, OrderError};
use iskworks_core::{
    BuildId, BuildPlanRevision, GraphMarketEvidence, OwnerId, PreviewBuildPlanCommand, WorkspaceId,
};

use crate::build_preview::BuildPlanningDeps;
use crate::{
    BuildMaterialsCoordinator, BuildMaterialsError, BuildPreviewCoordinator, BuildPreviewError,
};

/// Every error the whole-tree Epic-freeze application operation can
/// produce.
#[derive(Debug, thiserror::Error)]
pub enum OrderPlanError {
    #[error(transparent)]
    Materials(#[from] BuildMaterialsError),
    /// The sell-side enrichment's own root-level preview pass (materials
    /// identity, blueprint, facility, output pricing) failed.
    #[error(transparent)]
    Preview(#[from] BuildPreviewError),
    /// The canonical operation graph is
    /// corrupt, or its frozen cost shares do not conserve
    /// (`OrderError::CorruptProductionGraph` / `FrozenCostNotConserved`).
    #[error(transparent)]
    Freeze(#[from] OrderError),
}

/// [`OrderPlanCoordinator::freeze`]'s result: every active operation and
/// every requirement at any depth, ready for the caller to reduce into
/// `NewPlanOperation`/`NewOrderRequirement`/`NewPlanTicket` persistence
/// rows (already exactly those first two types) plus generated tickets --
/// and the one sell-side-enriched `BuildPlanRevision` to
/// source the Order's financial summary and root ticket compatibility
/// evidence from, built from the *same* `BuildCostProjection` the frozen
/// plan itself came from.
pub struct FrozenOrderPlan {
    pub operations: Vec<NewPlanOperation>,
    pub requirements: Vec<NewOrderRequirement>,
    pub revision: BuildPlanRevision,
}

/// Application orchestration for the whole-tree Epic freeze. Constructed
/// per request from `AppState` with exactly the collaborator set
/// `BuildMaterialsCoordinator` takes.
pub struct OrderPlanCoordinator {
    materials: BuildMaterialsCoordinator,
    /// For the one extra root-level preview pass
    /// the sell-side enrichment needs (`IndustryService::preview_plan`)
    /// -- constructed from the same collaborator set as `materials`, same
    /// convention `BuildGraphCoordinator` already follows (each coordinator
    /// holds its own cheap, stateless copy rather than reaching into
    /// another coordinator's private fields).
    preview: BuildPreviewCoordinator,
}

impl OrderPlanCoordinator {
    #[must_use]
    pub fn new(deps: BuildPlanningDeps) -> Self {
        Self {
            materials: BuildMaterialsCoordinator::new(deps.clone()),
            preview: BuildPreviewCoordinator::new(deps),
        }
    }

    /// Freeze `build_id`'s linked-build tree under `command`'s live overlay
    /// into a [`FrozenOrderPlan`]. `build_id` is authoritative: it
    /// overwrites `command.build_id` (mirrors `BuildMaterialsCoordinator::materials`)
    /// so a client can never freeze one Build's tree under another's
    /// overlay. Fully read-only -- reads inventory/prices once each, writes
    /// nothing.
    pub async fn freeze(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        build_id: BuildId,
        mut command: PreviewBuildPlanCommand,
    ) -> Result<FrozenOrderPlan, OrderPlanError> {
        command.build_id = Some(build_id);
        let (materials, cost) = self
            .materials
            .materials_with_planning_cost_for_overlay(workspace_id, owner_id, command.clone())
            .await?;

        let frozen = freeze_order_plan(
            &materials.verification_operations,
            &materials.verification_inputs,
            &cost,
            &materials.inventory_basis,
        )?;

        // The one additional root-level preview
        // pass for sell-side evidence (output price resolution, blueprint,
        // facility profile) -- never a second cost/materials projection. Mirrors
        // `BuildMaterialsCoordinator::preview_plan_with_planning_cost`
        // exactly, except it reuses `cost` (already computed above) instead
        // of running its own second `build_cost_projection_for_overlay`.
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
                command,
                &component_eivs,
                &material_coverage,
                (!market_evidence.is_empty()).then_some(&market_evidence),
            )
            .await
            .map_err(BuildPreviewError::from)?;
        cost.apply_to_revision(&mut revision);

        Ok(FrozenOrderPlan {
            operations: frozen.operations,
            requirements: frozen.requirements,
            revision,
        })
    }
}
