//! `BuildGraphCoordinator` -- the application orchestration behind
//! `POST /api/builds/:build_id/graph`.
//!
//! Graph is a presentation of the same authoritative
//! allocation-aware planning projection Materials and Worksheet use
//! -- never a second planning engine. This coordinator wraps
//! `BuildMaterialsCoordinator` (not a bare `BuildPreviewCoordinator`) and
//! reuses its `materials_with_planning_cost_for_overlay`: one
//! `list_balances` read, one `IndustryService::project_build_materials`
//! walk (`capture_verification: true`, so it also returns
//! `verification_operations`/`verification_inputs`), one bulk adjusted-price
//! resolution, one `crate::build_cost::project_build_cost` pass. The pure
//! `iskworks_core::build_graph::project_live_build_graph` then translates
//! that same data into `BuildGraphProjection` -- no second planning walk of
//! Graph's own, no separate cost fold, no re-resolved market/adjusted prices
//! for Graph specifically.

use std::collections::BTreeMap;

use iskworks_core::build_graph::{
    project_live_build_graph, BuildGraphProjection, GraphDisplayContext,
};
use iskworks_core::build_materials::MaterialBoundaryResolution;
use iskworks_core::{
    BuildId, IndustryError, OwnerId, PreviewBuildPlanCommand, RecipeSelection, WorkspaceId,
};

use crate::build_preview::BuildPlanningDeps;
use crate::{BuildMaterialsCoordinator, BuildMaterialsError};

/// Every error the Build Graph application operation can produce. Fans out
/// to exactly the same `ApiError`s a preview/materials request produces --
/// `BuildMaterialsError` covers the shared prelude's
/// failures (`Preview`), the domain `IndustryError`s the allocation-aware
/// walk `?`-propagates (`Industry`), a mixed-owner tree, and an
/// unavailable projection.
#[derive(Debug, thiserror::Error)]
pub enum BuildGraphError {
    #[error(transparent)]
    Materials(#[from] BuildMaterialsError),
}

/// Application orchestration for the Build Graph projection. Constructed per
/// request from `AppState` with exactly the collaborator set
/// `BuildMaterialsCoordinator` takes -- the graph reuses the materials/cost
/// prelude, so its dependency surface is identical.
pub struct BuildGraphCoordinator {
    materials: BuildMaterialsCoordinator,
}

impl BuildGraphCoordinator {
    #[must_use]
    pub fn new(deps: BuildPlanningDeps) -> Self {
        Self {
            materials: BuildMaterialsCoordinator::new(deps),
        }
    }

    /// Project `build_id`'s linked-build hierarchy under `command`'s live,
    /// unsaved planning overlay.
    ///
    /// The path `build_id` is authoritative: it overwrites `command.build_id`
    /// so a client cannot graph one build with another's overlay (mirrors
    /// `BuildMaterialsCoordinator::materials`). The graph is read-only -- it
    /// never saves the overlay, creates linked builds, or touches inventory
    /// / orders. The only writes it can trigger are the shared prelude's own
    /// best-effort ESI market-coverage registration, identical to a preview.
    pub async fn graph(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        build_id: BuildId,
        mut command: PreviewBuildPlanCommand,
    ) -> Result<BuildGraphProjection, BuildGraphError> {
        command.build_id = Some(build_id);
        let (materials, cost) = self
            .materials
            .materials_with_planning_cost_for_overlay(workspace_id, owner_id, command)
            .await?;

        // Buildable-recipe lookup for every Buy boundary anywhere in the
        // tree (not just the root) -- sourced from `verification_inputs` instead of a
        // second walk of `ComponentExpansion`s.
        let mut buy_type_ids: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();
        for boundary in &materials.verification_inputs {
            if boundary.resolution == MaterialBoundaryResolution::Buy {
                buy_type_ids.insert(boundary.type_id);
            }
        }
        let mut buildable_recipes: BTreeMap<i64, RecipeSelection> = BTreeMap::new();
        for type_id in buy_type_ids {
            if let Some(recipe) = self
                .resolve_buildable_recipe(type_id)
                .await
                .map_err(BuildMaterialsError::Industry)?
            {
                buildable_recipes.insert(type_id, recipe);
            }
        }

        let mut projection = project_live_build_graph(
            &materials.verification_operations,
            &materials.verification_inputs,
            &cost,
            &GraphDisplayContext {
                buildable_recipes: &buildable_recipes,
            },
            materials.generated_at,
        );
        projection.market_evidence = materials.market_evidence.into_vec();
        Ok(projection)
    }

    /// The recipe a Buy component could be switched to Build with, or `None`
    /// for a genuine raw material. Same precedence as the
    /// `/api/recipes/for-product` route: a manufacturing blueprint wins over
    /// a reaction formula.
    async fn resolve_buildable_recipe(
        &self,
        product_type_id: i64,
    ) -> Result<Option<RecipeSelection>, IndustryError> {
        let sde_repository = self.materials.sde_repository();
        if let Some(blueprint_type_id) = sde_repository
            .manufacturing_blueprint_for_product(product_type_id)
            .await
            .map_err(|error| IndustryError::StaticData(error.to_string()))?
        {
            return Ok(Some(RecipeSelection::Manufacturing { blueprint_type_id }));
        }
        if let Some(reaction_formula_type_id) = sde_repository
            .reaction_formula_for_product(product_type_id)
            .await
            .map_err(|error| IndustryError::StaticData(error.to_string()))?
        {
            return Ok(Some(RecipeSelection::Reaction {
                reaction_formula_type_id,
            }));
        }
        Ok(None)
    }
}
