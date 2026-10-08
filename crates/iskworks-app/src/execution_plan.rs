//! `ExecutionPlanCoordinator` -- the application orchestration behind
//! `POST /api/builds/:build_id/execution-plan`.
//!
//! Mirrors `BuildGraphCoordinator::graph` exactly. It reuses
//! `BuildMaterialsCoordinator::materials_with_planning_cost_for_overlay`
//! -- the **one** `list_balances` read, the **one** bulk
//! adjusted-price resolution, and the **one**
//! `IndustryService::project_build_materials` walk that `/materials` and
//! `/graph` pay for -- then feeds that exact same evidence
//! (`verification_operations`, `verification_inputs`, the canonical
//! `node_allocations`/`rows` materials rollups, and the resulting
//! `BuildCostProjection`) into
//! `iskworks_core::execution_plan::project_execution_plan`, a pure, sync
//! translation. No second planning walk, no second inventory read, no
//! second adjusted-price batch, no Build re-preview, no per-node lookup of
//! any kind.
//!
//! The coordinator adds exactly two *bulk* SDE reads after the
//! projection: the production recipes of every acquisition/operation type
//! (the Plan inspector's sourcing options) and the packaged volume of
//! every allocated type (the Logistics plan's cargo volume).

use std::collections::{BTreeMap, BTreeSet};

use iskworks_core::execution_plan::{project_execution_plan, ExecutionPlanProjection};
use iskworks_core::logistics::project_logistics;
use iskworks_core::{BuildId, OwnerId, PreviewBuildPlanCommand, RecipeSelection, WorkspaceId};
use rust_decimal::Decimal;

use crate::build_preview::BuildPlanningDeps;
use crate::{BuildMaterialsCoordinator, BuildMaterialsError};

/// Every error the Execution Plan application operation can produce. Fans
/// out to exactly the same `ApiError`s a preview/materials/graph request
/// produces -- `BuildMaterialsError` covers the shared
/// prelude's failures.
#[derive(Debug, thiserror::Error)]
pub enum ExecutionPlanError {
    #[error(transparent)]
    Materials(#[from] BuildMaterialsError),
}

/// Application orchestration for the Execution Plan projection. Constructed
/// per request from `AppState` with exactly the collaborator set
/// `BuildGraphCoordinator` / `BuildMaterialsCoordinator` take -- it reuses
/// the materials/cost prelude, so its dependency surface is
/// identical.
pub struct ExecutionPlanCoordinator {
    materials: BuildMaterialsCoordinator,
}

impl ExecutionPlanCoordinator {
    #[must_use]
    pub fn new(deps: BuildPlanningDeps) -> Self {
        Self {
            materials: BuildMaterialsCoordinator::new(deps),
        }
    }

    /// Project `build_id`'s linked-build hierarchy under `command`'s live,
    /// unsaved planning overlay into a staged, occurrence-preserving
    /// execution plan.
    ///
    /// The path `build_id` is authoritative: it overwrites `command.build_id`
    /// so a client cannot project one Build's tree under another's overlay
    /// (mirrors `BuildGraphCoordinator::graph` /
    /// `BuildMaterialsCoordinator::materials`). Read-only -- it never saves
    /// the overlay, creates linked builds, mutates inventory, creates an
    /// Epic, creates a ticket, or changes any Board/Ticket/Epic status. The
    /// only writes it can trigger are the shared prelude's own best-effort
    /// ESI market-coverage registration, identical to a preview/materials/
    /// graph request.
    pub async fn execution_plan(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        build_id: BuildId,
        mut command: PreviewBuildPlanCommand,
    ) -> Result<ExecutionPlanProjection, ExecutionPlanError> {
        command.build_id = Some(build_id);
        let (materials, cost) = self
            .materials
            .materials_with_planning_cost_for_overlay(workspace_id, owner_id, command)
            .await?;

        let mut projection = project_execution_plan(
            &materials.verification_operations,
            &materials.verification_inputs,
            &cost,
            &materials.node_allocations,
            &materials.rows,
            materials.generated_at,
        );

        // Two bulk SDE reads, never one per type --
        // the production methods each sourcing control may offer, and the
        // packaged volumes Logistics sizes cargo with.
        let sde = self.materials.sde_repository();
        let method_type_ids: Vec<i64> = projection
            .acquisitions
            .iter()
            .map(|line| line.type_id)
            .chain(projection.nodes.iter().map(|node| node.output_type_id))
            .collect::<BTreeSet<i64>>()
            .into_iter()
            .collect();
        let recipes = sde
            .production_recipes_for_products(&method_type_ids)
            .await
            .map_err(static_data_error)?;
        let methods_of = |type_id: i64| -> Vec<RecipeSelection> {
            recipes.get(&type_id).map_or_else(Vec::new, |refs| {
                refs.blueprint_type_id
                    .map(|blueprint_type_id| RecipeSelection::Manufacturing { blueprint_type_id })
                    .into_iter()
                    .chain(
                        refs.reaction_formula_type_id
                            .map(|reaction_formula_type_id| RecipeSelection::Reaction {
                                reaction_formula_type_id,
                            }),
                    )
                    .collect()
            })
        };
        for line in &mut projection.acquisitions {
            line.production_methods = methods_of(line.type_id);
        }
        for node in &mut projection.nodes {
            node.production_methods = methods_of(node.output_type_id);
        }

        let volume_type_ids: Vec<i64> = materials
            .node_allocations
            .iter()
            .map(|allocation| allocation.type_id)
            .collect::<BTreeSet<i64>>()
            .into_iter()
            .collect();
        let metadata = sde
            .inventory_type_metadata(&volume_type_ids)
            .await
            .map_err(static_data_error)?;
        let unit_volumes: BTreeMap<i64, Option<Decimal>> = volume_type_ids
            .iter()
            .map(|type_id| {
                (
                    *type_id,
                    metadata
                        .get(type_id)
                        .and_then(|meta| meta.packaged_volume_m3),
                )
            })
            .collect();
        projection.logistics = project_logistics(
            &materials.verification_operations,
            &materials.node_allocations,
            &unit_volumes,
        );
        Ok(projection)
    }
}

fn static_data_error(error: iskworks_sde::SdeError) -> ExecutionPlanError {
    ExecutionPlanError::Materials(BuildMaterialsError::Industry(
        iskworks_core::IndustryError::StaticData(error.to_string()),
    ))
}
