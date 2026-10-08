//! The canonical dependency-graph planner.
//!
//! For a Build of a root plan, [`IndustryService::project_build_materials`]
//! runs this planner. The persisted producer DAG
//! ([`crate::canonical_planner::CanonicalPlanTopology`], loaded with one
//! bounded `load_root_plan`) **is** the production topology -- no
//! `BuildTree` is reconstructed from it, no occurrence is fingerprinted, no
//! representative is elected, and nothing is previewed at `runs = 1` just to
//! discover what could be pooled.
//!
//! ```text
//! demand edges
//!   -> PlanningInventory allocation per edge (its own fulfillment scope)
//!   -> remaining production demand
//!   -> aggregate by canonical producer (once every incoming edge arrived)
//!   -> exact run sizing ONCE (pooled_production_runs)
//!   -> exact producer preview ONCE at those runs (preview_plan_inner)
//!   -> the producer's own demand edges
//!   -> repeat (FIFO worklist, topological by construction)
//! ```
//!
//! Order is deterministic: the FIFO worklist (breadth-first for a
//! tree-shaped plan); per node, Buy leaves first, then production
//! boundaries, each in ascending component type. The evidence it
//! emits is the same
//! `BuildMaterialsAggregate` shape, with **one** operation per active
//! producer carrying every demand edge it serves
//! ([`crate::build_materials::VerificationOperationInput::incoming`]).

use std::collections::{HashMap, VecDeque};

use super::service::{boundary_verification, register_production_operation};
use super::*;
use crate::build_materials::{
    allocate, BoundaryRecord, BoundaryVerification, MaterialActivity, MaterialBoundaryResolution,
    MaterialRowStrategy, MaterialsAccumulator, OperationIncomingDemand, PlanningInventory,
    ProductionEvidence,
};
use crate::canonical_planner::{CanonicalPlanTopology, PlanningEdge};
use crate::production_dependency::{ProductionMethod, RootPlanDependencyGraph};
use crate::GraphMarketEvidence;

/// One demand edge whose production demand is waiting for its producer to
/// be sized (every incoming edge must arrive first).
struct PendingDemand {
    dependency_id: String,
    consumer: BuildId,
    consumer_graph_node_id: String,
    consumer_op_index: u32,
    boundary_tree_path: Vec<i64>,
    type_id: i64,
    type_name: String,
    scope: crate::FulfillmentScope,
    required: u64,
    allocated: u64,
    remaining: u64,
    resolution: MaterialBoundaryResolution,
    verification: Option<BoundaryVerification>,
}

/// One producer's incoming demand, filled as its edges arrive.
struct ProducerDemandState {
    outstanding_edges: usize,
    pending: Vec<PendingDemand>,
}

/// The planner's mutable walk state.
struct Worklist {
    demand: HashMap<uuid::Uuid, ProducerDemandState>,
    ready: VecDeque<BuildId>,
}

impl Worklist {
    fn new(topology: &CanonicalPlanTopology) -> Self {
        Self {
            demand: topology
                .incoming_edge_count
                .iter()
                .map(|(producer, count)| {
                    (
                        *producer,
                        ProducerDemandState {
                            outstanding_edges: *count,
                            pending: Vec::new(),
                        },
                    )
                })
                .collect(),
            ready: VecDeque::new(),
        }
    }

    /// One incoming edge of `producer` has been decided: with production
    /// demand (`Some`), or none (fully covered, or its consumer is not
    /// active). The producer becomes ready once every incoming edge arrived.
    fn arrive(&mut self, producer: BuildId, demand: Option<PendingDemand>) {
        let Some(state) = self.demand.get_mut(&producer.0) else {
            return;
        };
        if let Some(demand) = demand {
            state.pending.push(demand);
        }
        state.outstanding_edges = state.outstanding_edges.saturating_sub(1);
        if state.outstanding_edges == 0 {
            self.ready.push_back(producer);
        }
    }

    /// `node` will not be expanded: every producer it would have demanded
    /// from receives an empty arrival (its subtree is pruned unless another
    /// consumer is active).
    fn settle(&mut self, topology: &CanonicalPlanTopology, node: BuildId) {
        for edge in topology.edges_of(node) {
            if let Some(producer) = edge.producer {
                self.arrive(producer, None);
            }
        }
    }
}

fn activity_of(build: &Build) -> MaterialActivity {
    match build.recipe.kind() {
        BuildRecipeKind::Manufacturing => MaterialActivity::Manufacturing,
        BuildRecipeKind::Reaction => MaterialActivity::Reaction,
    }
}

fn intended_recipe_of(method: Option<ProductionMethod>) -> Option<RecipeSelection> {
    method.map(|method| match method {
        ProductionMethod::Manufacturing { blueprint_type_id } => {
            RecipeSelection::Manufacturing { blueprint_type_id }
        }
        ProductionMethod::Reaction {
            reaction_formula_type_id,
        } => RecipeSelection::Reaction {
            reaction_formula_type_id,
        },
    })
}

/// The projected Build's own demand edges under the live overlay: its
/// component resolutions / fulfillment scopes come from `command` (unsaved
/// edits are authoritative for the projected Build), while each Produce
/// edge keeps the persisted edge's producer only when that producer is of
/// the requested identity -- otherwise it is unresolved (provisional): a
/// Produce intent with no producer yet.
fn overlay_start_edges(
    graph: &RootPlanDependencyGraph,
    start: BuildId,
    command: &PreviewBuildPlanCommand,
    revision: &BuildPlanRevision,
) -> Vec<PlanningEdge> {
    let persisted: HashMap<i64, &crate::production_dependency::ProductionDependency> = graph
        .outgoing(start)
        .into_iter()
        .map(|edge| (edge.component_type_id, edge))
        .collect();
    let resolutions: HashMap<i64, RecipeSelection> = command
        .component_resolutions
        .iter()
        .map(|resolution| (resolution.type_id, resolution.recipe))
        .collect();
    let full: std::collections::HashSet<i64> = command
        .fulfillment_scopes
        .iter()
        .filter(|scope| scope.scope == crate::FulfillmentScope::Full)
        .map(|scope| scope.type_id)
        .collect();
    let mut type_ids: Vec<i64> = revision
        .material_lines
        .iter()
        .map(|line| line.type_id)
        .collect();
    type_ids.sort_unstable();
    type_ids.dedup();
    type_ids
        .into_iter()
        .map(|type_id| {
            let edge = persisted.get(&type_id).copied();
            let produce: Option<ProductionMethod> =
                resolutions.get(&type_id).map(|recipe| (*recipe).into());
            let producer = produce.and_then(|method| {
                let edge = edge?;
                let producer = crate::canonical_planner::active_producer(edge)?;
                let config = graph.producer(producer)?;
                (config.key.output_type_id == type_id && config.key.method == method)
                    .then_some(producer)
            });
            PlanningEdge {
                dependency_id: edge.map_or_else(
                    || crate::canonical_planner::natural_dependency_id(start, type_id),
                    crate::canonical_planner::dependency_evidence_id,
                ),
                component_type_id: type_id,
                fulfillment_scope: if full.contains(&type_id) {
                    crate::FulfillmentScope::Full
                } else {
                    crate::FulfillmentScope::Missing
                },
                produce,
                producer,
            }
        })
        .collect()
}

/// Everything one node expansion needs to know about the operation.
struct NodeContext<'a> {
    build: &'a Build,
    revision: &'a BuildPlanRevision,
    is_root: bool,
    runs: u64,
    path: Vec<i64>,
    incoming: Vec<OperationIncomingDemand>,
}

impl IndustryService {
    /// The canonical planner -- see the module doc. `command.build_id` is the
    /// projected Build (the plan root, or any producer of the plan viewed on
    /// its own); `plan_root` is its root plan, loaded once.
    pub(super) async fn project_canonical_build_materials(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        command: &PreviewBuildPlanCommand,
        inventory: &mut PlanningInventory,
        capture_verification: bool,
        plan_root: BuildId,
    ) -> Result<BuildMaterialsProjection, IndustryError> {
        validate_runs(command.runs)?;
        let start = command.build_id.ok_or_else(|| {
            IndustryError::Validation("A graph requires a saved build.".to_string())
        })?;
        // One bounded load of the whole plan (a fixed number of queries,
        // independent of its Build count) -- no per-producer hydration.
        let records = self
            .build_repository()
            .load_root_plan(workspace_id, plan_root)
            .await?;
        {
            use std::sync::atomic::Ordering::Relaxed;
            let counters = self.counters();
            counters.root_plan_loads.fetch_add(1, Relaxed);
            counters.builds_loaded.fetch_add(
                u64::try_from(records.producers.len() + 1).unwrap_or(u64::MAX),
                Relaxed,
            );
        }
        let graph = RootPlanDependencyGraph::from_persisted(&records)
            .map_err(|error| IndustryError::Persistence(error.to_string()))?;
        let builds: HashMap<uuid::Uuid, &Build> = std::iter::once(&records.root)
            .chain(&records.producers)
            .map(|build| (build.id.0, build))
            .collect();
        let persisted_start = *builds.get(&start.0).ok_or(IndustryError::BuildNotFound)?;

        // The projected Build's live overlay (recipe / runs / planning).
        let recipe = self.capture_recipe(&command.recipe).await?;
        let overlay_input = crate::normalize_draft_planning(
            DraftPlanningInput {
                material_scope: command.material_scope,
                output_scope: command.output_scope,
                manual_price_list_id: command.manual_price_list_id,
                expected_manual_price_list_revision: command.expected_manual_price_list_revision,
                material_pricing_policy: command.material_pricing_policy,
                output_pricing_policy: command.output_pricing_policy,
                pricing_selections: command.pricing_selections.clone(),
                blueprint_selection: command.blueprint_selection.clone(),
                manufacturing_facility: command.manufacturing_facility.clone(),
                reaction_facility: command.reaction_facility.clone(),
                facility_eiv_manual: false,
                component_resolutions: command.component_resolutions.clone(),
                fulfillment_scopes: command.fulfillment_scopes.clone(),
            },
            command.runs,
        )?;
        let overlay_start = Build {
            recipe,
            runs: command.runs,
            recipe_currency: RecipeCurrency::Current,
            draft_planning: Some(DraftPlanningSnapshot {
                input: overlay_input,
                updated_at: Utc::now(),
            }),
            ..persisted_start.clone()
        };

        // Market evidence: one coherent snapshot per scope any active node
        // prices against, resolved before anything is priced.
        let mut scopes: Vec<crate::MarketScope> = vec![command.material_scope];
        if !scopes.contains(&command.output_scope) {
            scopes.push(command.output_scope);
        }
        for producer in graph.producers() {
            if producer.producer == start || graph.is_detached(producer.producer) {
                continue;
            }
            if let Some(draft) = builds
                .get(&producer.producer.0)
                .and_then(|build| build.draft_planning.as_ref())
            {
                for scope in [draft.input.material_scope, draft.input.output_scope] {
                    if !scopes.contains(&scope) {
                        scopes.push(scope);
                    }
                }
            }
        }
        let mut market_evidence = GraphMarketEvidence::default();
        for scope in scopes {
            if market_evidence.get(scope).is_none() {
                market_evidence.set(
                    self.build_repository()
                        .resolve_market_evidence(workspace_id, scope)
                        .await?,
                );
            }
        }

        let projection = |outcome: BuildMaterialsProjectionOutcome,
                          market_evidence: GraphMarketEvidence| {
            BuildMaterialsProjection {
                root_build_id: start,
                outcome,
                market_evidence,
                metrics: super::service::PlannerMetrics::default(),
            }
        };

        // The projected Build's own real preview (costs are irrelevant to the
        // quantity walk).
        let root_revision = Box::pin(self.preview_plan_inner(
            workspace_id,
            owner_id,
            command.clone(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            Some(&market_evidence),
        ))
        .await?;

        let start_edges = overlay_start_edges(&graph, start, command, &root_revision);
        let topology = match CanonicalPlanTopology::new(&graph, start, Some(start_edges)) {
            Ok(topology) => topology,
            Err(error) => {
                return Ok(projection(
                    BuildMaterialsProjectionOutcome::CanonicalGraphInvalid { error },
                    market_evidence,
                ));
            }
        };
        if let Some(foreign) = topology.reachable.iter().find(|id| {
            builds
                .get(&id.0)
                .is_some_and(|build| build.owner_id != owner_id)
        }) {
            return Ok(projection(
                BuildMaterialsProjectionOutcome::MixedOwnerTree { node: *foreign },
                market_evidence,
            ));
        }

        let mut acc = MaterialsAccumulator::new();
        let mut worklist = Worklist::new(&topology);
        self.expand_canonical_node(
            &topology,
            &builds,
            NodeContext {
                build: &overlay_start,
                revision: &root_revision,
                is_root: true,
                runs: command.runs,
                path: Vec::new(),
                incoming: Vec::new(),
            },
            capture_verification,
            inventory,
            &mut worklist,
            &mut acc,
        );

        while let Some(producer_id) = worklist.ready.pop_front() {
            let state = worklist
                .demand
                .remove(&producer_id.0)
                .expect("a ready producer always has demand state");
            let Some(producer) = builds.get(&producer_id.0).copied() else {
                acc.note_missing_node(producer_id);
                continue;
            };
            let total_demand = state
                .pending
                .iter()
                .map(|pending| pending.remaining)
                .fold(0u64, u64::saturating_add);
            if total_demand == 0 {
                // Every incoming edge is fully covered by inventory (or its
                // consumer is inactive): the producer stays configured but is
                // not an active operation, and its inputs are not expanded.
                worklist.settle(&topology, producer_id);
                continue;
            }

            // Size ONCE from the aggregate, never per consumer.
            let output_per_run = producer.recipe.primary_product().quantity_per_run.max(1);
            let runs = crate::build_materials::pooled_production_runs(total_demand, output_per_run);
            let produced = runs.saturating_mul(output_per_run);

            // Record every incoming edge (arrival order). The operation's one
            // surplus is attributed to the first edge only.
            let mut incoming = Vec::with_capacity(state.pending.len());
            for (position, pending) in state.pending.iter().enumerate() {
                let production = if position == 0 {
                    ProductionEvidence::sized(runs, output_per_run, total_demand)
                } else {
                    ProductionEvidence {
                        child_runs: runs,
                        output_per_run,
                        produced_quantity: produced,
                        surplus_quantity: 0,
                    }
                };
                let traversal_index = acc.record_intermediate(
                    BoundaryRecord {
                        build_id: pending.consumer,
                        graph_node_id: &pending.consumer_graph_node_id,
                        tree_path: &pending.boundary_tree_path,
                        type_id: pending.type_id,
                        type_name: &pending.type_name,
                        scope: pending.scope,
                        required: pending.required,
                        allocated: pending.allocated,
                        remaining: pending.remaining,
                        dependency_id: Some(&pending.dependency_id),
                        producer_build_id: Some(producer_id),
                    },
                    pending.resolution,
                    production,
                    pending.verification.clone(),
                );
                incoming.push(OperationIncomingDemand {
                    traversal_index,
                    consumer_op_index: pending.consumer_op_index,
                    dependency_id: pending.dependency_id.clone(),
                });
            }

            // Preview ONCE, exactly, at the aggregate run count.
            let command = match self
                .reconstruct_preview_command(workspace_id, producer)
                .await
            {
                Ok(Some(mut command)) => {
                    command.runs = runs;
                    command
                }
                Ok(None) | Err(_) => {
                    acc.note_missing_node(producer_id);
                    worklist.settle(&topology, producer_id);
                    continue;
                }
            };
            let revision = match Box::pin(self.preview_plan_inner(
                workspace_id,
                producer.owner_id,
                command,
                &BTreeMap::new(),
                &BTreeMap::new(),
                Some(&market_evidence),
            ))
            .await
            {
                Ok(revision) if !revision.material_lines.is_empty() => revision,
                _ => {
                    acc.note_missing_node(producer_id);
                    worklist.settle(&topology, producer_id);
                    continue;
                }
            };
            let path = state
                .pending
                .first()
                .map(|pending| pending.boundary_tree_path.clone())
                .unwrap_or_default();
            self.expand_canonical_node(
                &topology,
                &builds,
                NodeContext {
                    build: producer,
                    revision: &revision,
                    is_root: false,
                    runs,
                    path,
                    incoming,
                },
                capture_verification,
                inventory,
                &mut worklist,
                &mut acc,
            );
        }

        // Defensive: an acyclic topology always drains every producer. A
        // leftover would mean a demand was never sized -- fail loudly.
        for (producer, state) in &worklist.demand {
            if state.outstanding_edges > 0 {
                acc.note_missing_node(BuildId(*producer));
            }
        }

        let outcome = match acc.finish(inventory) {
            Ok(aggregate) => BuildMaterialsProjectionOutcome::Complete(aggregate),
            Err(crate::build_materials::BuildMaterialsError::NodeRevisionUnavailable {
                missing_nodes,
            }) => BuildMaterialsProjectionOutcome::Incomplete { missing_nodes },
        };
        Ok(projection(outcome, market_evidence))
    }

    /// Expand one active operation at its already-decided exact runs:
    /// register it, then allocate inventory for each of its demand edges --
    /// Buy leaves first, then production boundaries, ascending component
    /// type -- recording leaves and fully covered boundaries now and handing
    /// each remaining production demand to its producer.
    #[allow(clippy::too_many_arguments)]
    fn expand_canonical_node(
        &self,
        topology: &CanonicalPlanTopology,
        builds: &HashMap<uuid::Uuid, &Build>,
        node: NodeContext<'_>,
        capture_verification: bool,
        inventory: &mut PlanningInventory,
        worklist: &mut Worklist,
        acc: &mut MaterialsAccumulator,
    ) {
        let NodeContext {
            build,
            revision,
            is_root,
            runs,
            path,
            incoming,
        } = node;
        let build_id = build.id;
        let graph_node_id = if is_root {
            format!("root:{}", build_id.0)
        } else {
            format!("build:{}", build_id.0)
        };
        let activity = activity_of(build);
        let parent_traversal_index = incoming.first().map(|demand| demand.traversal_index);
        let op_index = register_production_operation(
            build,
            revision,
            is_root,
            runs,
            &path,
            incoming,
            capture_verification,
            acc,
        );
        let verification = |type_id: i64, starting_inventory: u64| {
            capture_verification.then(|| {
                boundary_verification(
                    revision,
                    activity,
                    runs,
                    type_id,
                    starting_inventory,
                    parent_traversal_index,
                    op_index,
                )
            })
        };

        // One slot per requirement line of the exact preview, classified by
        // the node's demand edge (a line without one -- a component the
        // active SDE recipe added -- is Buy).
        let mut lines: Vec<&PlannedMaterialLine> = revision.material_lines.iter().collect();
        lines.sort_by_key(|line| line.type_id);
        lines.dedup_by_key(|line| line.type_id);
        let edges: HashMap<i64, &PlanningEdge> = topology
            .edges_of(build_id)
            .iter()
            .map(|edge| (edge.component_type_id, edge))
            .collect();
        // A Produce edge naming a producer must have an authoritative line.
        for edge in edges.values() {
            if edge.producer.is_some()
                && !lines
                    .iter()
                    .any(|line| line.type_id == edge.component_type_id)
            {
                acc.note_missing_node(build_id);
                if let Some(producer) = edge.producer {
                    worklist.arrive(producer, None);
                }
            }
        }

        // Pass 1: Buy leaves.
        for line in &lines {
            let edge = edges.get(&line.type_id).copied();
            if edge.is_some_and(|edge| edge.produce.is_some()) {
                continue;
            }
            let scope = edge.map_or(crate::FulfillmentScope::Missing, |edge| {
                edge.fulfillment_scope
            });
            let (allocated, remaining) =
                allocate(inventory, line.type_id, line.total_quantity, scope);
            acc.record_leaf(
                BoundaryRecord {
                    build_id,
                    graph_node_id: &graph_node_id,
                    tree_path: &path,
                    type_id: line.type_id,
                    type_name: &line.type_name,
                    scope,
                    required: line.total_quantity,
                    allocated,
                    remaining,
                    dependency_id: edge.map(|edge| edge.dependency_id.as_str()),
                    producer_build_id: None,
                },
                MaterialRowStrategy::Buy,
                false,
                verification(line.type_id, inventory.available(line.type_id)),
            );
        }

        // Pass 2: production boundaries.
        for line in &lines {
            let Some(edge) = edges.get(&line.type_id).copied() else {
                continue;
            };
            if edge.produce.is_none() {
                continue;
            }
            let mut child_path = path.clone();
            child_path.push(line.type_id);
            let (allocated, remaining) = allocate(
                inventory,
                line.type_id,
                line.total_quantity,
                edge.fulfillment_scope,
            );
            let boundary_verification =
                verification(line.type_id, inventory.available(line.type_id));

            let Some(producer_id) = edge.producer else {
                // Produce with no producer yet: provisional demand at its own
                // scope; its recipe inputs are not expanded.
                let intended = intended_recipe_of(edge.produce);
                let strategy = match intended {
                    Some(RecipeSelection::Reaction { .. }) => MaterialRowStrategy::Reaction,
                    Some(RecipeSelection::Manufacturing { .. }) | None => {
                        MaterialRowStrategy::Build
                    }
                };
                acc.record_leaf(
                    BoundaryRecord {
                        build_id,
                        graph_node_id: &graph_node_id,
                        tree_path: &child_path,
                        type_id: line.type_id,
                        type_name: &line.type_name,
                        scope: edge.fulfillment_scope,
                        required: line.total_quantity,
                        allocated,
                        remaining,
                        dependency_id: Some(&edge.dependency_id),
                        producer_build_id: None,
                    },
                    strategy,
                    true,
                    boundary_verification.map(|mut verification| {
                        verification.intended_recipe = intended;
                        verification
                    }),
                );
                continue;
            };
            let Some(producer) = builds.get(&producer_id.0).copied() else {
                acc.note_missing_node(producer_id);
                worklist.arrive(producer_id, None);
                continue;
            };
            let resolution = match producer.recipe.kind() {
                BuildRecipeKind::Manufacturing => MaterialBoundaryResolution::Build,
                BuildRecipeKind::Reaction => MaterialBoundaryResolution::Reaction,
            };
            if remaining == 0 {
                // Fully covered at this edge: recorded now, contributes no
                // production demand.
                let output_per_run = producer.recipe.primary_product().quantity_per_run.max(1);
                acc.record_intermediate(
                    BoundaryRecord {
                        build_id,
                        graph_node_id: &graph_node_id,
                        tree_path: &child_path,
                        type_id: line.type_id,
                        type_name: &line.type_name,
                        scope: edge.fulfillment_scope,
                        required: line.total_quantity,
                        allocated,
                        remaining,
                        dependency_id: Some(&edge.dependency_id),
                        producer_build_id: Some(producer_id),
                    },
                    resolution,
                    ProductionEvidence::pruned(output_per_run),
                    boundary_verification,
                );
                worklist.arrive(producer_id, None);
                continue;
            }
            worklist.arrive(
                producer_id,
                Some(PendingDemand {
                    dependency_id: edge.dependency_id.clone(),
                    consumer: build_id,
                    consumer_graph_node_id: graph_node_id.clone(),
                    consumer_op_index: op_index,
                    boundary_tree_path: child_path,
                    type_id: line.type_id,
                    type_name: line.type_name.clone(),
                    scope: edge.fulfillment_scope,
                    required: line.total_quantity,
                    allocated,
                    remaining,
                    resolution,
                    verification: boundary_verification,
                }),
            );
        }
    }
}
