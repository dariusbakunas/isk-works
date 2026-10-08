use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use iskworks_sde::{RecipeLine, SdeReadRepository};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::build::{
    BuildPlanLine, BuildPlanningError, BuildPlanningService, ReactionPlanningError,
    ReactionPlanningService,
};
use crate::industry::RecipeSelection;

/// One worksheet row the caller wants built instead of bought, and which
/// recipe produces it. `facility_override` is carried through untouched by
/// this service and only consumed later, by `apply_component_expansion`'s
/// installation-cost resolution. `blueprint_selection` (manufacturing-kind
/// resolutions only) IS consumed by this service -- `expand` reduces the
/// row's own job duration by its resolved blueprint TE (never its own
/// material quantities: worksheets are single-level, so a build-resolved
/// row's own materials are never expanded into new rows here in the first
/// place).
/// Not `Copy` -- `BlueprintSelection` carries a `String` field.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentResolution {
    pub type_id: i64,
    pub recipe: RecipeSelection,
    #[serde(default)]
    pub facility_override: Option<ComponentFacilityOverride>,
    #[serde(default)]
    pub blueprint_selection: Option<crate::BlueprintSelection>,
}

/// How much of a row's requirement its acquisition strategy (Buy or
/// Build) should actually cover -- orthogonal to `ComponentResolution`
/// (which only says *how*, not *how much*). `Missing` (only buy/build the
/// shortage past current inventory) is the default: an unset row, or one
/// with no `fulfillment_scopes` entry at all, behaves as `Missing`.
/// Presence in that list means an explicit `Full` override (ignore
/// inventory, cover the entire requirement fresh); `Missing` entries are
/// also valid but redundant with the default.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FulfillmentScope {
    Missing,
    Full,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FulfillmentScopeOverride {
    pub type_id: i64,
    pub scope: FulfillmentScope,
}

/// A per-row facility choice for one build-resolved sub-component,
/// overriding the build-level shared manufacturing/reaction slot for that
/// row's own installation cost only -- never its material quantities (no
/// ME/TE concept exists for sub-components today).
/// Role (manufacturing vs reaction) isn't redeclared here -- it's validated
/// against the referenced profile's own `role` at resolution time, the same
/// way the build-level shared slots already are. The override resolves the
/// *current* profile by id, like every live-Build facility reference; an
/// `expectedProfileRevision` in an older persisted draft is ignored by serde.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentFacilityOverride {
    pub facility_profile_id: crate::FacilityProfileId,
}

/// Request body for the pure "compute the expansion" preview endpoint --
/// self-contained, mirrors `PreviewBuildPlanCommand`'s shape (no build ID,
/// nothing read from persisted state).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewComponentExpansionCommand {
    pub root: RecipeSelection,
    pub runs: u64,
    #[serde(default)]
    pub resolutions: Vec<ComponentResolution>,
}

/// The fully aggregated, flat material list for a root recipe with some of
/// its materials (recursively) resolved to "build" instead of "buy" -- one
/// row per distinct type ID in the whole resolution tree, merged.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentExpansion {
    pub components: Vec<ResolvedComponent>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedComponent {
    pub type_id: i64,
    pub type_name: String,
    pub total_quantity: u64,
    pub contributions: Vec<Contribution>,
    pub resolution: ComponentResolutionOutcome,
}

/// Provenance for one merged row: who contributed how much of its total
/// demand. Kept so a build-resolved row's total can be un-merged later for
/// hover attribution, and so production order can be derived from it (a
/// `Component { type_id }` source means that component consumes this row,
/// i.e. this row must be produced before it).
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Contribution {
    pub source: ContributionSource,
    pub quantity: u64,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ContributionSource {
    Root,
    Component { type_id: i64 },
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ComponentResolutionOutcome {
    Buy,
    Build {
        recipe: RecipeSelection,
        runs: u64,
        produced_quantity: u64,
        surplus: u64,
        duration_seconds: Option<u64>,
    },
}

#[derive(Debug, Error)]
pub enum ComponentExpansionError {
    #[error("runs must be between 1 and 1,000,000")]
    InvalidRuns,
    #[error("no active recipe was found for a requested component resolution")]
    RecipeNotFound,
    #[error("build quantity exceeds the supported range")]
    QuantityOverflow,
    #[error("static data lookup failed: {0}")]
    StaticData(String),
}

fn map_build_error(error: BuildPlanningError) -> ComponentExpansionError {
    match error {
        BuildPlanningError::InvalidRuns => ComponentExpansionError::InvalidRuns,
        BuildPlanningError::BlueprintNotFound(_) => ComponentExpansionError::RecipeNotFound,
        BuildPlanningError::QuantityOverflow => ComponentExpansionError::QuantityOverflow,
        BuildPlanningError::StaticData(message) => ComponentExpansionError::StaticData(message),
    }
}

fn map_reaction_error(error: ReactionPlanningError) -> ComponentExpansionError {
    match error {
        ReactionPlanningError::InvalidRuns => ComponentExpansionError::InvalidRuns,
        ReactionPlanningError::ReactionFormulaNotFound(_) => {
            ComponentExpansionError::RecipeNotFound
        }
        ReactionPlanningError::QuantityOverflow => ComponentExpansionError::QuantityOverflow,
        ReactionPlanningError::StaticData(message) => ComponentExpansionError::StaticData(message),
    }
}

/// Scaled (for a specific `runs`) recipe shape, kind-erased.
struct ScaledPlan {
    materials: Vec<BuildPlanLine>,
    products: Vec<BuildPlanLine>,
    duration_seconds: Option<u64>,
}

#[derive(Clone)]
pub struct ComponentExpansionService {
    repository: Arc<dyn SdeReadRepository>,
    build_planning: BuildPlanningService,
    reaction_planning: ReactionPlanningService,
}

impl ComponentExpansionService {
    #[must_use]
    pub fn new(repository: Arc<dyn SdeReadRepository>) -> Self {
        let build_planning = BuildPlanningService::new(repository.clone());
        let reaction_planning = ReactionPlanningService::new(repository.clone());
        Self {
            repository,
            build_planning,
            reaction_planning,
        }
    }

    pub async fn expand(
        &self,
        root: RecipeSelection,
        root_runs: u64,
        resolutions: &[ComponentResolution],
        component_blueprint_efficiencies: &BTreeMap<i64, (u8, u8)>,
        available_quantities: &BTreeMap<i64, u64>,
    ) -> Result<ComponentExpansion, ComponentExpansionError> {
        self.expand_with_root_requirements(
            root,
            root_runs,
            resolutions,
            component_blueprint_efficiencies,
            available_quantities,
            &BTreeMap::new(),
        )
        .await
    }

    pub async fn expand_with_root_requirements(
        &self,
        root: RecipeSelection,
        root_runs: u64,
        resolutions: &[ComponentResolution],
        component_blueprint_efficiencies: &BTreeMap<i64, (u8, u8)>,
        available_quantities: &BTreeMap<i64, u64>,
        effective_root_requirements: &BTreeMap<i64, u64>,
    ) -> Result<ComponentExpansion, ComponentExpansionError> {
        if !(1..=1_000_000).contains(&root_runs) {
            return Err(ComponentExpansionError::InvalidRuns);
        }
        let resolutions_by_type: HashMap<i64, ComponentResolution> =
            resolutions.iter().map(|r| (r.type_id, r.clone())).collect();

        let mut root_plan = self.scaled_plan(root, root_runs).await?;
        // The industry planner owns ME/facility rounding. Preserve its final
        // direct-material requirements when expansion is only adding
        // acquisition and inventory semantics to those same rows.
        for material in &mut root_plan.materials {
            if let Some(quantity) = effective_root_requirements.get(&material.type_id) {
                material.total_quantity = *quantity;
            }
        }

        let mut demand: HashMap<i64, u64> = HashMap::new();
        let mut contributions: HashMap<i64, Vec<Contribution>> = HashMap::new();
        let mut type_names: HashMap<i64, String> = HashMap::new();

        add_demand(
            &mut demand,
            &mut contributions,
            &mut type_names,
            ContributionSource::Root,
            &root_plan.materials,
        )?;

        let mut outcomes: HashMap<i64, ComponentResolutionOutcome> = HashMap::new();

        // Worksheets are single-level: only the root recipe's own direct
        // materials can ever be build-resolved here. A resolved row's own
        // materials are never expanded into this worksheet as new rows --
        // that breakdown lives only on that row's own linked build's own
        // worksheet, one hop away.
        let mut resolved_type_ids: Vec<i64> = root_plan
            .materials
            .iter()
            .map(|line| line.type_id)
            .filter(|type_id| resolutions_by_type.contains_key(type_id))
            .collect();
        resolved_type_ids.sort_unstable();
        resolved_type_ids.dedup();

        for type_id in resolved_type_ids {
            let total_demand = match demand.get(&type_id).copied() {
                Some(quantity) if quantity > 0 => quantity,
                _ => continue,
            };
            // `available_quantities` only ever carries an entry for a
            // `Missing`-scoped type_id (the caller is responsible for that
            // filtering) -- for `Full` scope (or no scope override at all,
            // today's only behavior), the lookup misses and effective
            // demand is unchanged.
            let effective_demand = total_demand
                .saturating_sub(available_quantities.get(&type_id).copied().unwrap_or(0));
            // An explicit Build resolution is a user commitment to
            // manufacture this component -- it must stay `Build` across the
            // worksheet, the Build Graph, and its linked build, whatever the
            // current stock level. Normally the job is sized to the shortage
            // past inventory (`Missing` scope); when stock happens to cover
            // the whole demand there is simply no shortage to size against,
            // so fall back to the full demand rather than dropping the row
            // to `Buy`. `total_demand > 0` is guaranteed above, so this
            // never produces a degenerate `Build { runs: 0, .. }`.
            let build_demand = if effective_demand == 0 {
                total_demand
            } else {
                effective_demand
            };
            let resolution = resolutions_by_type
                .get(&type_id)
                .expect("filtered to root materials present in resolutions_by_type above");
            let products = self.raw_recipe_products(resolution.recipe).await?;
            let quantity_per_run = products
                .iter()
                .find(|product| product.type_id == type_id)
                .map(|product| product.quantity)
                .filter(|quantity| *quantity > 0)
                .ok_or(ComponentExpansionError::RecipeNotFound)?;
            let runs = build_demand.div_ceil(quantity_per_run as u64);

            let plan = self.scaled_plan(resolution.recipe, runs).await?;
            // TE only ever applies to manufacturing -- reactions have no
            // research efficiency concept in EVE. ME does not apply here
            // at all: a build-resolved row's own materials are never
            // expanded into this worksheet, so there's nothing left for ME
            // to reduce.
            let duration_seconds = match resolution.recipe {
                RecipeSelection::Manufacturing { .. } => {
                    let blueprint_te = component_blueprint_efficiencies
                        .get(&type_id)
                        .map(|(_, te)| *te)
                        .unwrap_or(0);
                    reduce_duration_by_blueprint_te(plan.duration_seconds, blueprint_te)?
                }
                RecipeSelection::Reaction { .. } => plan.duration_seconds,
            };
            let produced_quantity = plan
                .products
                .iter()
                .find(|product| product.type_id == type_id)
                .map_or(0, |product| product.total_quantity);
            // Surplus is relative to what building was actually asked to
            // cover: the shortage for a `Missing`-scoped row inventory only
            // partly covers, or the full demand when there is no shortage
            // (`Full` scope, or a fully stock-covered explicit resolution).
            let surplus = produced_quantity.saturating_sub(build_demand);

            outcomes.insert(
                type_id,
                ComponentResolutionOutcome::Build {
                    recipe: resolution.recipe,
                    runs,
                    produced_quantity,
                    surplus,
                    duration_seconds,
                },
            );
        }

        let mut components: Vec<ResolvedComponent> = demand
            .into_iter()
            .map(|(type_id, total_quantity)| ResolvedComponent {
                type_id,
                type_name: type_names.remove(&type_id).unwrap_or_default(),
                total_quantity,
                contributions: contributions.remove(&type_id).unwrap_or_default(),
                resolution: outcomes
                    .remove(&type_id)
                    .unwrap_or(ComponentResolutionOutcome::Buy),
            })
            .collect();
        components.sort_by_key(|component| component.type_id);

        Ok(ComponentExpansion { components })
    }

    /// A resolved component's own unscaled products -- the only part of its
    /// raw recipe `expand` needs (to compute `quantity_per_run`), since a
    /// resolved row's own materials are never expanded into this worksheet.
    async fn raw_recipe_products(
        &self,
        recipe: RecipeSelection,
    ) -> Result<Vec<RecipeLine>, ComponentExpansionError> {
        match recipe {
            RecipeSelection::Manufacturing { blueprint_type_id } => {
                let recipe = self
                    .repository
                    .manufacturing_recipe(blueprint_type_id)
                    .await
                    .map_err(|error| ComponentExpansionError::StaticData(error.to_string()))?
                    .ok_or(ComponentExpansionError::RecipeNotFound)?;
                Ok(recipe.products)
            }
            RecipeSelection::Reaction {
                reaction_formula_type_id,
            } => {
                let recipe = self
                    .repository
                    .reaction_formula(reaction_formula_type_id)
                    .await
                    .map_err(|error| ComponentExpansionError::StaticData(error.to_string()))?
                    .ok_or(ComponentExpansionError::RecipeNotFound)?;
                Ok(recipe.products)
            }
        }
    }

    async fn scaled_plan(
        &self,
        recipe: RecipeSelection,
        runs: u64,
    ) -> Result<ScaledPlan, ComponentExpansionError> {
        match recipe {
            RecipeSelection::Manufacturing { blueprint_type_id } => {
                let plan = self
                    .build_planning
                    .plan(blueprint_type_id, runs)
                    .await
                    .map_err(map_build_error)?;
                Ok(ScaledPlan {
                    materials: plan.materials,
                    products: plan.products,
                    duration_seconds: plan.duration_seconds,
                })
            }
            RecipeSelection::Reaction {
                reaction_formula_type_id,
            } => {
                let plan = self
                    .reaction_planning
                    .plan(reaction_formula_type_id, runs)
                    .await
                    .map_err(map_reaction_error)?;
                Ok(ScaledPlan {
                    materials: plan.materials,
                    products: plan.products,
                    duration_seconds: plan.duration_seconds,
                })
            }
        }
    }
}

fn add_demand(
    demand: &mut HashMap<i64, u64>,
    contributions: &mut HashMap<i64, Vec<Contribution>>,
    type_names: &mut HashMap<i64, String>,
    source: ContributionSource,
    lines: &[BuildPlanLine],
) -> Result<(), ComponentExpansionError> {
    for line in lines {
        let entry = demand.entry(line.type_id).or_insert(0);
        *entry = entry
            .checked_add(line.total_quantity)
            .ok_or(ComponentExpansionError::QuantityOverflow)?;
        contributions
            .entry(line.type_id)
            .or_default()
            .push(Contribution {
                source,
                quantity: line.total_quantity,
            });
        type_names
            .entry(line.type_id)
            .or_insert_with(|| line.type_name.clone());
    }
    Ok(())
}

/// Reduces a manufacturing component's own job duration by its resolved
/// blueprint TE -- `ceil(duration * (1 - te/100))`, floored at 1 second
/// when a duration is present at all. `blueprint_te == 0` (no selection) is
/// a precise no-op.
fn reduce_duration_by_blueprint_te(
    duration_seconds: Option<u64>,
    blueprint_te: u8,
) -> Result<Option<u64>, ComponentExpansionError> {
    let Some(duration_seconds) = duration_seconds else {
        return Ok(None);
    };
    let te_factor = Decimal::ONE - Decimal::from(blueprint_te) / Decimal::ONE_HUNDRED;
    let exact = Decimal::from(duration_seconds)
        .checked_mul(te_factor)
        .ok_or(ComponentExpansionError::QuantityOverflow)?;
    let reduced = crate::facility::decimal_to_ceiling_u64(exact)
        .map_err(|_| ComponentExpansionError::QuantityOverflow)?
        .max(1);
    Ok(Some(reduced))
}

#[cfg(test)]
mod tests;
