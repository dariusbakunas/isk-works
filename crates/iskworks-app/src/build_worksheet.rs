//! One-pass application coordinator for the saved-Build Worksheet.
//!
//! The coordinator deliberately reuses the same allocation-aware materials
//! and cost prelude as Graph and Execution Plan. After that single walk it
//! performs one bulk SDE recipe read (product -> blueprint), one bulk SDE
//! type-reference read and one bulk blueprint-observation read, then invokes
//! the pure core projector. No recipe expansion, allocation, pricing, or cost arithmetic
//! lives here.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use iskworks_core::build_worksheet::blueprint_coverage::{
    summarize_holdings, WorksheetBlueprintHoldings,
};
use iskworks_core::build_worksheet::{
    project_build_worksheet, BuildWorksheetProjection, BuildWorksheetProjectionError,
    BuildWorksheetProjectionInput, WorksheetTypeMetadata,
};
use iskworks_core::{
    BlueprintObservation, BuildId, IndustryRepository, OwnerId, PreviewBuildPlanCommand,
    WorkspaceId,
};
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
    industry: Arc<dyn IndustryRepository>,
}

impl BuildWorksheetCoordinator {
    #[must_use]
    pub fn new(deps: BuildPlanningDeps) -> Self {
        Self {
            sde: Arc::clone(&deps.sde_repository),
            industry: Arc::clone(&deps.industry_repository),
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
        let recipes = self
            .sde
            .production_recipes_for_products(&type_ids)
            .await
            .map_err(static_data_error)?;
        let product_blueprints: BTreeMap<i64, i64> = recipes
            .iter()
            .filter_map(|(product, refs)| refs.blueprint_type_id.map(|bp| (*product, bp)))
            .collect();
        let blueprint_type_ids: Vec<i64> = product_blueprints
            .values()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let observations = self
            .industry
            .list_blueprint_observations_for_types(workspace_id, owner_id, &blueprint_type_ids)
            .await
            .map_err(|error| {
                BuildWorksheetError::Materials(BuildMaterialsError::Industry(error))
            })?;

        let reference_ids: Vec<i64> = type_ids
            .iter()
            .chain(&blueprint_type_ids)
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut references = self
            .sde
            .type_reference(&reference_ids)
            .await
            .map_err(static_data_error)?;
        let blueprints = blueprint_holdings(&product_blueprints, &observations, &references);
        references.retain(|type_id, _| type_ids.binary_search(type_id).is_ok());
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
            blueprints: &blueprints,
            generated_at: materials.generated_at,
        })?)
    }
}

/// Product type -> its manufacturing blueprint and the workspace's holdings
/// of it. Names prefer the SDE (an unowned blueprint has no observation).
fn blueprint_holdings(
    product_blueprints: &BTreeMap<i64, i64>,
    observations: &[BlueprintObservation],
    references: &BTreeMap<i64, iskworks_sde::TypeReference>,
) -> BTreeMap<i64, WorksheetBlueprintHoldings> {
    let mut by_blueprint = BTreeMap::<i64, Vec<BlueprintObservation>>::new();
    for observation in observations {
        by_blueprint
            .entry(observation.blueprint_type_id)
            .or_default()
            .push(observation.clone());
    }
    product_blueprints
        .iter()
        .map(|(&product, &blueprint_type_id)| {
            let owned = by_blueprint
                .get(&blueprint_type_id)
                .map_or(&[][..], Vec::as_slice);
            let blueprint_name = references
                .get(&blueprint_type_id)
                .and_then(|reference| reference.type_name.clone())
                .or_else(|| {
                    owned
                        .first()
                        .map(|observation| observation.blueprint_name.clone())
                })
                .unwrap_or_else(|| format!("Blueprint {blueprint_type_id}"));
            (
                product,
                WorksheetBlueprintHoldings {
                    blueprint_type_id,
                    blueprint_name,
                    holdings: summarize_holdings(owned),
                },
            )
        })
        .collect()
}

fn static_data_error(error: iskworks_sde::SdeError) -> BuildWorksheetError {
    BuildWorksheetError::Materials(BuildMaterialsError::Industry(
        iskworks_core::IndustryError::StaticData(error.to_string()),
    ))
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use iskworks_core::{BlueprintKind, OwnerId, WorkspaceId};
    use uuid::Uuid;

    use super::*;

    fn observation(eve_item_id: i64, blueprint_type_id: i64, runs: u64) -> BlueprintObservation {
        BlueprintObservation {
            id: Uuid::new_v4(),
            workspace_id: WorkspaceId(Uuid::nil()),
            owner_id: OwnerId(Uuid::nil()),
            owner_name: "Owner".into(),
            eve_item_id,
            blueprint_type_id,
            blueprint_name: "Captured Name".into(),
            kind: BlueprintKind::Copy,
            material_efficiency: 10,
            time_efficiency: 20,
            licensed_runs: Some(runs),
            location_id: 1,
            location_flag: "Hangar".into(),
            location_name: None,
            observed_at: Utc::now(),
            imported_at: Utc::now(),
        }
    }

    #[test]
    fn holdings_group_observations_per_product_and_name_unowned_blueprints_from_the_sde() {
        let product_blueprints = BTreeMap::from([(100, 1_100), (200, 1_200), (300, 1_100)]);
        let observations = [observation(1, 1_100, 4), observation(2, 1_100, 6)];
        let references = BTreeMap::from([(
            1_200,
            iskworks_sde::TypeReference {
                type_name: Some("Unowned Blueprint".into()),
                ..Default::default()
            },
        )]);

        let holdings = blueprint_holdings(&product_blueprints, &observations, &references);

        assert_eq!(holdings[&100].holdings.copies, 2);
        assert_eq!(holdings[&100].holdings.copy_runs, 10);
        assert_eq!(holdings[&100].blueprint_name, "Captured Name");
        // Two products sharing a blueprint see the same holdings.
        assert_eq!(holdings[&300].holdings, holdings[&100].holdings);
        assert_eq!(holdings[&200].blueprint_name, "Unowned Blueprint");
        assert_eq!(holdings[&200].holdings.copies, 0);
    }
}
