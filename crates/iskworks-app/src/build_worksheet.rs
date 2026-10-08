//! One-pass application coordinator for the saved-Build Worksheet.
//!
//! The coordinator deliberately reuses the same allocation-aware materials
//! and cost prelude as Graph and Execution Plan. After that single walk it
//! performs one bulk SDE type-reference read, then invokes the pure core
//! projector. No recipe expansion, allocation, pricing, or cost arithmetic
//! lives here.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use iskworks_core::build_worksheet::{
    project_build_worksheet, BuildWorksheetProjection, BuildWorksheetProjectionError,
    BuildWorksheetProjectionInput, WorksheetTypeMetadata,
};
use iskworks_core::{BuildId, OwnerId, PreviewBuildPlanCommand, WorkspaceId};
use iskworks_sde::SdeReadRepository;

use crate::build_preview::BuildPlanningDeps;
use crate::{BuildMaterialsCoordinator, BuildMaterialsError};

#[derive(Debug, thiserror::Error)]
pub enum BuildWorksheetError {
    #[error(transparent)]
    Materials(#[from] BuildMaterialsError),
    #[error(transparent)]
    Projection(#[from] BuildWorksheetProjectionError),
}

pub struct BuildWorksheetCoordinator {
    materials: BuildMaterialsCoordinator,
    sde: Arc<dyn SdeReadRepository>,
}

impl BuildWorksheetCoordinator {
    #[must_use]
    pub fn new(deps: BuildPlanningDeps) -> Self {
        Self {
            sde: Arc::clone(&deps.sde_repository),
            materials: BuildMaterialsCoordinator::new(deps),
        }
    }

    pub async fn worksheet(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        root_build_id: BuildId,
        mut command: PreviewBuildPlanCommand,
        focused_producer_id: Option<BuildId>,
        include_downstream: bool,
    ) -> Result<BuildWorksheetProjection, BuildWorksheetError> {
        command.build_id = Some(root_build_id);
        let (materials, cost) = self
            .materials
            .materials_with_planning_cost_for_overlay(workspace_id, owner_id, command)
            .await?;

        let type_ids: Vec<i64> = materials
            .node_allocations
            .iter()
            .map(|allocation| allocation.type_id)
            .chain(
                materials
                    .verification_operations
                    .iter()
                    .map(|operation| operation.product_type_id),
            )
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let references = self
            .sde
            .type_reference(&type_ids)
            .await
            .map_err(static_data_error)?;
        let metadata: BTreeMap<i64, WorksheetTypeMetadata> = references
            .into_iter()
            .map(|(type_id, reference)| {
                (
                    type_id,
                    WorksheetTypeMetadata {
                        category_id: reference.category_id,
                        category_name: reference.category_name,
                        group_id: reference.group_id,
                        group_name: reference.group_name,
                    },
                )
            })
            .collect();

        Ok(project_build_worksheet(BuildWorksheetProjectionInput {
            operations: &materials.verification_operations,
            boundaries: &materials.verification_inputs,
            allocations: &materials.node_allocations,
            aggregate_rows: &materials.rows,
            cost: &cost,
            focused_producer_id,
            include_downstream,
            metadata: &metadata,
            generated_at: materials.generated_at,
        })?)
    }
}

fn static_data_error(error: iskworks_sde::SdeError) -> BuildWorksheetError {
    BuildWorksheetError::Materials(BuildMaterialsError::Industry(
        iskworks_core::IndustryError::StaticData(error.to_string()),
    ))
}
