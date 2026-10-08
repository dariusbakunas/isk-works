//! The canonical planner's application operations: resolving a Build's
//! canonical root, and the canonical consumer write every sourcing mutation
//! goes through. Pure decisions live in [`crate::canonical_planner`];
//! the repository applies them transactionally.

use chrono::Utc;

use super::*;
use crate::canonical_planner::{
    plan_canonical_edge_writes, CanonicalConsumerWrite, CanonicalEdgeIntent, CanonicalWriteError,
    NewCanonicalProducer, PlannedProducer,
};
use crate::plan_state::plan_state;
use crate::production_dependency::{RootPlanDependencyGraph, RootPlanRecords};

fn persisted_graph(records: &RootPlanRecords) -> Result<RootPlanDependencyGraph, IndustryError> {
    RootPlanDependencyGraph::from_persisted(records)
        .map_err(|error| IndustryError::Persistence(error.to_string()))
}

/// The consumer's intended sourcing per recipe component, read from its new
/// draft: a `component_resolutions` entry means Produce by that recipe, none
/// means Buy; `fulfillment_scopes` gives the scope (Missing by default).
fn intents_from_draft(
    recipe: &BuildRecipe,
    draft: Option<&DraftPlanningInput>,
) -> Vec<CanonicalEdgeIntent> {
    let resolutions: BTreeMap<i64, RecipeSelection> = draft
        .map(|input| {
            input
                .component_resolutions
                .iter()
                .map(|resolution| (resolution.type_id, resolution.recipe))
                .collect()
        })
        .unwrap_or_default();
    let full: BTreeSet<i64> = draft
        .map(|input| {
            input
                .fulfillment_scopes
                .iter()
                .filter(|scope| scope.scope == crate::FulfillmentScope::Full)
                .map(|scope| scope.type_id)
                .collect()
        })
        .unwrap_or_default();
    let mut seen = BTreeSet::new();
    recipe
        .materials()
        .iter()
        .filter(|line| seen.insert(line.type_id))
        .map(|line| CanonicalEdgeIntent {
            component_type_id: line.type_id,
            produce: resolutions
                .get(&line.type_id)
                .map(|recipe| (*recipe).into()),
            fulfillment_scope: if full.contains(&line.type_id) {
                crate::FulfillmentScope::Full
            } else {
                crate::FulfillmentScope::Missing
            },
        })
        .collect()
}

impl IndustryService {
    /// The root of the plan `build_id` belongs to.
    pub async fn plan_root_of(
        &self,
        workspace_id: WorkspaceId,
        build_id: BuildId,
    ) -> Result<Option<BuildId>, IndustryError> {
        self.build_repository()
            .plan_root_of(workspace_id, build_id)
            .await
    }

    /// The root plan's graph read from **persisted** `production_dependencies`
    /// rows via one bounded `IndustryRepository::load_root_plan` call --
    /// never one Build load per descendant. Read-only.
    pub async fn persisted_root_plan_dependency_graph(
        &self,
        workspace_id: WorkspaceId,
        root_build_id: BuildId,
    ) -> Result<RootPlanDependencyGraph, IndustryError> {
        let records = self
            .build_repository()
            .load_root_plan(workspace_id, root_build_id)
            .await?;
        persisted_graph(&records)
    }

    /// The canonical consumer write: persist `update` for `consumer` (a
    /// Build of the canonical root plan `root`) **and** bring its demand
    /// edges in line with the new draft's sourcing, atomically.
    ///
    /// * Buy -> Produce and method changes reference the root plan's
    ///   canonical producer of the new identity, or create it exactly once
    ///   ([`plan_canonical_edge_writes`]); never one Build per consumer, and
    ///   never an incompatible producer mutated in place.
    /// * Produce -> Buy changes only this edge; the producer is kept
    ///   (detached when nothing else references it).
    /// * Every referenced producer is cycle-checked, and the repository
    ///   re-verifies the planned-against plan state under lock and re-checks
    ///   acyclicity before commit.
    /// * A descendant producer's recipe is never replaced in place.
    ///
    /// Descendant `runs` are informational for a canonical root and are not
    /// resynced.
    pub(super) async fn write_canonical_consumer(
        &self,
        workspace_id: WorkspaceId,
        root: BuildId,
        consumer_id: BuildId,
        update: DraftUpdate,
    ) -> Result<Build, IndustryError> {
        let repository = self.build_repository();
        let records = repository.load_root_plan(workspace_id, root).await?;
        let consumer = std::iter::once(&records.root)
            .chain(&records.producers)
            .find(|build| build.id == consumer_id)
            .cloned()
            .ok_or(IndustryError::BuildNotFound)?;
        if consumer.revision != update.expected_revision {
            return Err(IndustryError::RevisionConflict);
        }
        if update.replacement_recipe.is_some() && consumer_id != root {
            return Err(CanonicalWriteError::ProducerRecipeChange {
                producer: consumer_id,
            }
            .into());
        }
        let graph = persisted_graph(&records)?;
        let draft_input: Option<DraftPlanningInput> = update
            .draft_planning
            .as_ref()
            .map(|snapshot| snapshot.input.clone());
        let draft = draft_input.as_ref();
        // Components that already have an edge. A replaced root recipe's new
        // components start as Buy inside the transaction; any Produce intent
        // for them is applied by the follow-up pass below.
        let existing: BTreeSet<i64> = records
            .dependencies
            .iter()
            .filter(|edge| edge.consumer_build_id == consumer_id)
            .map(|edge| edge.component_type_id)
            .collect();
        let recipe = update
            .replacement_recipe
            .as_ref()
            .unwrap_or(&consumer.recipe)
            .clone();
        let intents: Vec<CanonicalEdgeIntent> = intents_from_draft(&recipe, draft)
            .into_iter()
            .filter(|intent| existing.contains(&intent.component_type_id))
            .collect();
        let edge_writes = plan_canonical_edge_writes(&graph, &records, consumer_id, &intents)?;
        let new_producers = self
            .prepare_canonical_producers(&consumer, draft, &edge_writes)
            .await?;
        let replaced_recipe = update.replacement_recipe.is_some();
        let follow_up = update.clone();
        let written = repository
            .apply_canonical_consumer_write(
                workspace_id,
                CanonicalConsumerWrite {
                    root,
                    expected_plan_state: plan_state(&records),
                    consumer: consumer_id,
                    update,
                    new_producers,
                    edge_writes,
                },
            )
            .await?;
        if !replaced_recipe {
            return Ok(written);
        }
        // Second (optimistically checked) pass for a replaced root recipe's
        // new components; a no-op when none of them is Produce.
        let needs_follow_up = intents_from_draft(&recipe, draft).iter().any(|intent| {
            !existing.contains(&intent.component_type_id) && intent.produce.is_some()
        });
        if !needs_follow_up {
            return Ok(written);
        }
        Box::pin(self.write_canonical_consumer(
            workspace_id,
            root,
            consumer_id,
            DraftUpdate {
                expected_revision: written.revision,
                replacement_recipe: None,
                ..follow_up
            },
        ))
        .await
    }

    /// One new producer Build per `Create` edge write (one per identity):
    /// the captured active recipe, informational `runs = 1`, and its initial
    /// configuration prepopulated from the consumer's resolution for that
    /// component.
    async fn prepare_canonical_producers(
        &self,
        consumer: &Build,
        draft: Option<&DraftPlanningInput>,
        edge_writes: &[crate::canonical_planner::PlannedEdgeWrite],
    ) -> Result<Vec<NewCanonicalProducer>, IndustryError> {
        let mut producers: Vec<NewCanonicalProducer> = Vec::new();
        for write in edge_writes {
            let PlannedProducer::Create { key } = write.producer else {
                continue;
            };
            if producers.iter().any(|producer| producer.key == key) {
                continue;
            }
            let selection = match key.method {
                crate::production_dependency::ProductionMethod::Manufacturing {
                    blueprint_type_id,
                } => RecipeSelection::Manufacturing { blueprint_type_id },
                crate::production_dependency::ProductionMethod::Reaction {
                    reaction_formula_type_id,
                } => RecipeSelection::Reaction {
                    reaction_formula_type_id,
                },
            };
            let recipe = self.capture_recipe(&selection).await?;
            let resolution = draft
                .and_then(|input| {
                    input
                        .component_resolutions
                        .iter()
                        .find(|resolution| resolution.type_id == key.output_type_id)
                        .cloned()
                })
                .unwrap_or(crate::ComponentResolution {
                    type_id: key.output_type_id,
                    recipe: selection,
                    facility_override: None,
                    blueprint_selection: None,
                });
            let now = Utc::now();
            let name = format!("{} build", recipe.primary_product().type_name);
            producers.push(NewCanonicalProducer {
                key,
                build: Build {
                    id: BuildId::new(),
                    workspace_id: consumer.workspace_id,
                    owner_id: consumer.owner_id,
                    name,
                    draft_planning: super::service::prepopulated_draft_planning(
                        &resolution,
                        selection,
                        draft,
                    ),
                    recipe,
                    runs: 1,
                    notes: String::new(),
                    revision: 1,
                    created_at: now,
                    updated_at: now,
                    recipe_currency: RecipeCurrency::Current,
                    active_sde_version: None,
                    product_category_name: None,
                    product_group_name: None,
                    selected_blueprint_origin: None,
                    has_owned_blueprint: false,
                },
            });
        }
        Ok(producers)
    }

    /// Canonical form of create-or-reuse: the producer the consumer's demand
    /// edge references for `component_type_id` -- resolving (reusing, or
    /// creating exactly once) the canonical producer when the edge is
    /// Produce but has none yet. Never a per-consumer child.
    pub(super) async fn canonical_linked_producer(
        &self,
        workspace_id: WorkspaceId,
        root: BuildId,
        consumer_id: BuildId,
        component_type_id: i64,
    ) -> Result<Build, IndustryError> {
        let repository = self.build_repository();
        let records = repository.load_root_plan(workspace_id, root).await?;
        let edge = records
            .dependencies
            .iter()
            .find(|edge| {
                edge.consumer_build_id == consumer_id && edge.component_type_id == component_type_id
            })
            .ok_or_else(|| {
                IndustryError::Validation(
                    "This component is not currently resolved to \"Build\".".to_string(),
                )
            })?;
        if !matches!(
            edge.sourcing,
            crate::production_dependency::DependencySourcing::Produce { .. }
        ) {
            return Err(IndustryError::Validation(
                "This component is not currently resolved to \"Build\".".to_string(),
            ));
        }
        if let Some(producer) = edge.producer_build_id {
            return repository.get_build(workspace_id, producer).await;
        }
        let consumer = std::iter::once(&records.root)
            .chain(&records.producers)
            .find(|build| build.id == consumer_id)
            .cloned()
            .ok_or(IndustryError::BuildNotFound)?;
        let written = self
            .write_canonical_consumer(
                workspace_id,
                root,
                consumer_id,
                DraftUpdate {
                    expected_revision: consumer.revision,
                    name: consumer.name.clone(),
                    runs: consumer.runs,
                    notes: consumer.notes.clone(),
                    replacement_recipe: None,
                    draft_planning: consumer.draft_planning.clone(),
                },
            )
            .await?;
        let records = repository.load_root_plan(workspace_id, root).await?;
        let producer = records
            .dependencies
            .iter()
            .find(|edge| {
                edge.consumer_build_id == written.id && edge.component_type_id == component_type_id
            })
            .and_then(|edge| edge.producer_build_id)
            .ok_or_else(|| {
                IndustryError::Persistence("the canonical producer was not resolved".to_string())
            })?;
        repository.get_build(workspace_id, producer).await
    }
}
