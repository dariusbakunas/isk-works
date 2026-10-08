use std::sync::Arc;

use iskworks_sde::{ManufacturingRecipe, ReactionFormulaRecipe, SdeReadRepository};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildPlan {
    pub blueprint_type_id: i64,
    pub blueprint_name: String,
    pub runs: u64,
    pub duration_seconds: Option<u64>,
    pub materials: Vec<BuildPlanLine>,
    pub products: Vec<BuildPlanLine>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildPlanLine {
    pub type_id: i64,
    pub type_name: String,
    pub quantity_per_run: u64,
    pub total_quantity: u64,
}

#[derive(Debug, Error)]
pub enum BuildPlanningError {
    #[error("runs must be between 1 and 1,000,000")]
    InvalidRuns,
    #[error("manufacturing blueprint {0} was not found in the active SDE")]
    BlueprintNotFound(i64),
    #[error("build quantity exceeds the supported range")]
    QuantityOverflow,
    #[error("static data lookup failed: {0}")]
    StaticData(String),
}

#[derive(Clone)]
pub struct BuildPlanningService {
    repository: Arc<dyn SdeReadRepository>,
}

impl BuildPlanningService {
    #[must_use]
    pub fn new(repository: Arc<dyn SdeReadRepository>) -> Self {
        Self { repository }
    }

    pub async fn plan(
        &self,
        blueprint_type_id: i64,
        runs: u64,
    ) -> Result<BuildPlan, BuildPlanningError> {
        if !(1..=1_000_000).contains(&runs) {
            return Err(BuildPlanningError::InvalidRuns);
        }
        let recipe = self
            .repository
            .manufacturing_recipe(blueprint_type_id)
            .await
            .map_err(|error| BuildPlanningError::StaticData(error.to_string()))?
            .ok_or(BuildPlanningError::BlueprintNotFound(blueprint_type_id))?;

        build_plan(recipe, runs)
    }
}

fn build_plan(recipe: ManufacturingRecipe, runs: u64) -> Result<BuildPlan, BuildPlanningError> {
    let duration_seconds = recipe
        .duration_seconds
        .map(|duration| {
            u64::try_from(duration)
                .ok()
                .and_then(|duration| duration.checked_mul(runs))
                .ok_or(BuildPlanningError::QuantityOverflow)
        })
        .transpose()?;
    let materials = recipe
        .materials
        .into_iter()
        .map(|line| plan_line(line.type_id, line.type_name, line.quantity, runs))
        .collect::<Result<_, _>>()?;
    let products = recipe
        .products
        .into_iter()
        .map(|line| plan_line(line.type_id, line.type_name, line.quantity, runs))
        .collect::<Result<_, _>>()?;

    Ok(BuildPlan {
        blueprint_type_id: recipe.blueprint_type_id,
        blueprint_name: recipe.blueprint_name,
        runs,
        duration_seconds,
        materials,
        products,
    })
}

fn plan_line(
    type_id: i64,
    type_name: String,
    quantity: i64,
    runs: u64,
) -> Result<BuildPlanLine, BuildPlanningError> {
    let quantity_per_run =
        u64::try_from(quantity).map_err(|_| BuildPlanningError::QuantityOverflow)?;
    let total_quantity = quantity_per_run
        .checked_mul(runs)
        .ok_or(BuildPlanningError::QuantityOverflow)?;
    Ok(BuildPlanLine {
        type_id,
        type_name,
        quantity_per_run,
        total_quantity,
    })
}

/// The reaction-formula read-side counterpart of `BuildPlan`: raw
/// quantity-times-runs scaling with no material-efficiency concept, since
/// reaction formulas have none (see `iskworks_core::facility` for the
/// broader rationale).
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactionPlan {
    pub reaction_formula_type_id: i64,
    pub reaction_formula_name: String,
    pub runs: u64,
    pub duration_seconds: Option<u64>,
    pub materials: Vec<BuildPlanLine>,
    pub products: Vec<BuildPlanLine>,
}

#[derive(Debug, Error)]
pub enum ReactionPlanningError {
    #[error("runs must be between 1 and 1,000,000")]
    InvalidRuns,
    #[error("reaction formula {0} was not found in the active SDE")]
    ReactionFormulaNotFound(i64),
    #[error("reaction quantity exceeds the supported range")]
    QuantityOverflow,
    #[error("static data lookup failed: {0}")]
    StaticData(String),
}

#[derive(Clone)]
pub struct ReactionPlanningService {
    repository: Arc<dyn SdeReadRepository>,
}

impl ReactionPlanningService {
    #[must_use]
    pub fn new(repository: Arc<dyn SdeReadRepository>) -> Self {
        Self { repository }
    }

    pub async fn plan(
        &self,
        reaction_formula_type_id: i64,
        runs: u64,
    ) -> Result<ReactionPlan, ReactionPlanningError> {
        if !(1..=1_000_000).contains(&runs) {
            return Err(ReactionPlanningError::InvalidRuns);
        }
        let recipe = self
            .repository
            .reaction_formula(reaction_formula_type_id)
            .await
            .map_err(|error| ReactionPlanningError::StaticData(error.to_string()))?
            .ok_or(ReactionPlanningError::ReactionFormulaNotFound(
                reaction_formula_type_id,
            ))?;

        reaction_plan(recipe, runs)
    }
}

fn reaction_plan(
    recipe: ReactionFormulaRecipe,
    runs: u64,
) -> Result<ReactionPlan, ReactionPlanningError> {
    let duration_seconds = recipe
        .duration_seconds
        .map(|duration| {
            u64::try_from(duration)
                .ok()
                .and_then(|duration| duration.checked_mul(runs))
                .ok_or(ReactionPlanningError::QuantityOverflow)
        })
        .transpose()?;
    let materials = recipe
        .materials
        .into_iter()
        .map(|line| reaction_plan_line(line.type_id, line.type_name, line.quantity, runs))
        .collect::<Result<_, _>>()?;
    let products = recipe
        .products
        .into_iter()
        .map(|line| reaction_plan_line(line.type_id, line.type_name, line.quantity, runs))
        .collect::<Result<_, _>>()?;

    Ok(ReactionPlan {
        reaction_formula_type_id: recipe.reaction_formula_type_id,
        reaction_formula_name: recipe.reaction_formula_name,
        runs,
        duration_seconds,
        materials,
        products,
    })
}

fn reaction_plan_line(
    type_id: i64,
    type_name: String,
    quantity: i64,
    runs: u64,
) -> Result<BuildPlanLine, ReactionPlanningError> {
    let quantity_per_run =
        u64::try_from(quantity).map_err(|_| ReactionPlanningError::QuantityOverflow)?;
    let total_quantity = quantity_per_run
        .checked_mul(runs)
        .ok_or(ReactionPlanningError::QuantityOverflow)?;
    Ok(BuildPlanLine {
        type_id,
        type_name,
        quantity_per_run,
        total_quantity,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use iskworks_sde::{ActiveSde, BlueprintSearchResult, RecipeLine, SdeError};

    struct FixtureRepository;

    #[async_trait]
    impl SdeReadRepository for FixtureRepository {
        async fn active_sde(&self) -> Result<Option<ActiveSde>, SdeError> {
            Ok(None)
        }

        async fn search_manufacturing_blueprints(
            &self,
            _query: &str,
            _limit: u32,
        ) -> Result<Vec<BlueprintSearchResult>, SdeError> {
            Ok(Vec::new())
        }

        async fn manufacturing_recipe(
            &self,
            blueprint_type_id: i64,
        ) -> Result<Option<ManufacturingRecipe>, SdeError> {
            Ok((blueprint_type_id == 6830).then(|| ManufacturingRecipe {
                blueprint_type_id,
                blueprint_name: "Rifter Blueprint".to_string(),
                duration_seconds: Some(600),
                materials: vec![RecipeLine {
                    type_id: 34,
                    type_name: "Tritanium".to_string(),
                    quantity: 1_000,
                }],
                products: vec![RecipeLine {
                    type_id: 5_876,
                    type_name: "Rifter".to_string(),
                    quantity: 1,
                }],
            }))
        }

        async fn search_types(
            &self,
            _query: &str,
            _limit: u32,
        ) -> Result<Vec<iskworks_sde::TypeSearchResult>, SdeError> {
            Ok(Vec::new())
        }

        async fn reaction_formula(
            &self,
            reaction_formula_type_id: i64,
        ) -> Result<Option<ReactionFormulaRecipe>, SdeError> {
            Ok(
                (reaction_formula_type_id == 46_157).then(|| ReactionFormulaRecipe {
                    reaction_formula_type_id,
                    reaction_formula_name: "Methanofullerene Reaction Formula".to_string(),
                    duration_seconds: Some(1_200),
                    materials: vec![RecipeLine {
                        type_id: 16_272,
                        type_name: "Amber".to_string(),
                        quantity: 2,
                    }],
                    products: vec![RecipeLine {
                        type_id: 16_662,
                        type_name: "Methanofullerene".to_string(),
                        quantity: 100,
                    }],
                }),
            )
        }
    }

    #[tokio::test]
    async fn calculates_direct_materials_products_and_duration_for_runs() {
        let service = BuildPlanningService::new(Arc::new(FixtureRepository));
        let plan = service.plan(6_830, 3).await.unwrap();

        assert_eq!(plan.duration_seconds, Some(1_800));
        assert_eq!(plan.materials[0].total_quantity, 3_000);
        assert_eq!(plan.products[0].total_quantity, 3);
    }

    #[tokio::test]
    async fn rejects_zero_runs_and_missing_blueprints() {
        let service = BuildPlanningService::new(Arc::new(FixtureRepository));

        assert!(matches!(
            service.plan(6_830, 0).await,
            Err(BuildPlanningError::InvalidRuns)
        ));
        assert!(matches!(
            service.plan(42, 1).await,
            Err(BuildPlanningError::BlueprintNotFound(42))
        ));
    }

    #[tokio::test]
    async fn scales_reaction_materials_products_and_duration_for_runs() {
        let service = ReactionPlanningService::new(Arc::new(FixtureRepository));
        let plan = service.plan(46_157, 3).await.unwrap();

        assert_eq!(plan.duration_seconds, Some(3_600));
        assert_eq!(plan.materials[0].total_quantity, 6);
        assert_eq!(plan.products[0].total_quantity, 300);
    }

    #[tokio::test]
    async fn rejects_zero_runs_and_missing_reaction_formulas() {
        let service = ReactionPlanningService::new(Arc::new(FixtureRepository));

        assert!(matches!(
            service.plan(46_157, 0).await,
            Err(ReactionPlanningError::InvalidRuns)
        ));
        assert!(matches!(
            service.plan(42, 1).await,
            Err(ReactionPlanningError::ReactionFormulaNotFound(42))
        ));
    }
}
