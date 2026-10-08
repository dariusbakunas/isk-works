//! **Canonical producers**: the pure domain of the canonical
//! dependency-graph planner.
//!
//! Within one root plan, **one** canonical producer configuration per output
//! and production method serves **every** demand edge for it:
//!
//! * `production_dependencies` rows are the persisted sourcing (topology).
//! * the planner (`IndustryService::project_build_materials`) walks the
//!   persisted producer DAG ([`CanonicalPlanTopology`]), allocates inventory
//!   per incoming demand edge, sizes each producer **once** from its
//!   aggregate remaining demand and previews it **once** at those runs.
//! * descendant `Build.runs` is informational; only the root's runs are user
//!   input.
//!
//! This module holds everything that can be decided without I/O:
//!
//! * [`plan_canonical_edge_writes`] -- the canonical Buy/Produce/method
//!   change write rule (reuse the canonical producer, or create it exactly
//!   once; never mutate an incompatible producer; cycle-safe).
//! * [`CanonicalPlanTopology`] -- the validated, deterministic producer DAG
//!   the planner walks.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use serde::Serialize;
use uuid::Uuid;

use crate::plan_state::PlanBuildState;
use crate::production_dependency::{
    AdapterDiagnostic, CanonicalProducerKey, DependencySourcing, PersistedProductionDependency,
    ProductionDependency, ProductionDependencyId, ProductionMethod, RootPlanDependencyGraph,
    RootPlanRecords,
};
use crate::{BuildId, FulfillmentScope};

// ---------------------------------------------------------------------------
// Canonical producer writes
// ---------------------------------------------------------------------------

/// A producer the canonical write path creates -- exactly once per identity
/// in its transaction. `build.plan_root_build_id` is set to the root and it
/// has no parent link: one producer serves every consumer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCanonicalProducer {
    pub key: CanonicalProducerKey,
    pub build: crate::Build,
}

/// One canonical consumer write, applied atomically by
/// `IndustryRepository::apply_canonical_consumer_write`: the consumer's own
/// draft/runs/recipe update (revision-checked), the producers to create,
/// and the demand-edge writes -- after re-verifying under lock that the
/// plan is still exactly `expected_plan_state` (the state
/// [`plan_canonical_edge_writes`] planned against, including its cycle and
/// reuse checks) and still canonical-authoritative.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalConsumerWrite {
    pub root: BuildId,
    pub expected_plan_state: Vec<PlanBuildState>,
    pub consumer: BuildId,
    pub update: crate::DraftUpdate,
    pub new_producers: Vec<NewCanonicalProducer>,
    pub edge_writes: Vec<PlannedEdgeWrite>,
}

/// A cycle reachable from `start` under `next`, as the producers on it.
fn find_cycle(start: BuildId, next: &dyn Fn(BuildId) -> Vec<BuildId>) -> Option<Vec<BuildId>> {
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        Visiting,
        Done,
    }
    fn dfs(
        node: BuildId,
        next: &dyn Fn(BuildId) -> Vec<BuildId>,
        marks: &mut HashMap<Uuid, Mark>,
        path: &mut Vec<BuildId>,
    ) -> Option<Vec<BuildId>> {
        match marks.get(&node.0) {
            Some(Mark::Done) => return None,
            Some(Mark::Visiting) => {
                let start = path.iter().position(|id| *id == node).unwrap_or(0);
                return Some(path[start..].to_vec());
            }
            None => {}
        }
        marks.insert(node.0, Mark::Visiting);
        path.push(node);
        for child in next(node) {
            if let Some(cycle) = dfs(child, next, marks, path) {
                return Some(cycle);
            }
        }
        path.pop();
        marks.insert(node.0, Mark::Done);
        None
    }
    dfs(start, next, &mut HashMap::new(), &mut Vec::new())
}

// ---------------------------------------------------------------------------
// Canonical edge writes
// ---------------------------------------------------------------------------

/// The consumer's intended sourcing of one component, derived from its new
/// draft (`component_resolutions` / `fulfillment_scopes`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanonicalEdgeIntent {
    pub component_type_id: i64,
    /// `None` = Buy; `Some(method)` = Produce by `method`.
    pub produce: Option<ProductionMethod>,
    pub fulfillment_scope: FulfillmentScope,
}

/// Which producer a written Produce edge references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PlannedProducer {
    /// Buy: no producer.
    None,
    /// Reference an existing canonical producer of this root plan.
    Existing { producer: BuildId },
    /// No producer of this identity exists: create exactly one.
    Create { key: CanonicalProducerKey },
}

/// One changed demand edge of a canonical write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedEdgeWrite {
    pub dependency_id: Uuid,
    pub component_type_id: i64,
    pub sourcing: DependencySourcing,
    pub producer: PlannedProducer,
    pub fulfillment_scope: FulfillmentScope,
    pub previous_producer: Option<BuildId>,
    /// The previous producer still serves another demand edge afterwards.
    /// It is kept either way: Build -> Buy / a method change never deletes
    /// a producer; one left with no incoming edge is simply detached.
    pub previous_producer_still_referenced: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize)]
#[serde(
    tag = "code",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CanonicalWriteError {
    #[error("component {component_type_id} is not a requirement of this build")]
    UnknownComponent { component_type_id: i64 },
    #[error("several producers of {key:?} exist and none is canonical")]
    AmbiguousProducer {
        key: CanonicalProducerKey,
        candidates: Vec<BuildId>,
    },
    #[error("producing this component with {producer:?} would create a production cycle")]
    WouldCreateCycle {
        consumer: BuildId,
        producer: BuildId,
    },
    #[error("build {build:?} is a retired producer and cannot be edited as a consumer")]
    RetiredConsumer { build: BuildId },
    /// A descendant producer's own recipe never changes in place: that would
    /// silently change what every consumer receives. Change the consuming
    /// demand edge's method instead (it reuses or creates the producer of
    /// the new identity).
    #[error(
        "a producer's recipe cannot be replaced in place; change the consumer's method instead"
    )]
    ProducerRecipeChange { producer: BuildId },
    /// The plan changed between planning a canonical write and applying it
    /// under lock (a concurrent edit); nothing was written.
    #[error("the root plan changed since this change was prepared")]
    PlanChanged,
    /// A write path that writes no demand edges (the plain `update_draft`)
    /// reached a Build with a sourcing change it cannot express; nothing was
    /// written.
    #[error(
        "this build belongs to a canonical plan; its sourcing is changed through demand edges"
    )]
    CanonicalWriteRequired { build: BuildId },
    #[error("the persisted canonical producer graph is invalid: {detail}")]
    CanonicalGraphCorrupt { detail: String },
}

/// Plan the edge writes that bring `consumer`'s persisted demand edges in
/// line with `intents` on a **canonical** root plan. Pure; the repository
/// applies the result inside one transaction after re-verifying, under lock,
/// the exact plan state this was planned against.
///
/// * Buy: the edge becomes Buy; its previous producer is kept (detached when
///   nothing else references it).
/// * Produce, already referencing a producer of that identity: unchanged.
/// * Produce otherwise (Buy -> Produce, or a method change): reference the
///   root plan's canonical producer of the new identity, or create it
///   exactly once (the `select_producer_target` rule over the
///   canonical graph, retired producers excluded). The previous producer is
///   never mutated -- in particular never re-recipe'd in place.
/// * Every referenced producer is cycle-checked.
///
/// Only edges that change are returned.
///
/// # Errors
/// See [`CanonicalWriteError`].
pub fn plan_canonical_edge_writes(
    graph: &RootPlanDependencyGraph,
    records: &RootPlanRecords,
    consumer: BuildId,
    intents: &[CanonicalEdgeIntent],
) -> Result<Vec<PlannedEdgeWrite>, CanonicalWriteError> {
    if graph.is_retired(consumer) {
        return Err(CanonicalWriteError::RetiredConsumer { build: consumer });
    }
    let rows: HashMap<i64, &PersistedProductionDependency> = records
        .dependencies
        .iter()
        .filter(|edge| edge.consumer_build_id == consumer)
        .map(|edge| (edge.component_type_id, edge))
        .collect();
    let builds: HashMap<Uuid, &crate::Build> = std::iter::once(&records.root)
        .chain(&records.producers)
        .map(|build| (build.id.0, build))
        .collect();
    let key_of_build = |producer: BuildId| -> Option<CanonicalProducerKey> {
        builds.get(&producer.0).map(|build| CanonicalProducerKey {
            output_type_id: build.recipe.primary_product().type_id,
            method: crate::recipe_selection_of(&build.recipe).into(),
        })
    };

    let mut writes = Vec::new();
    for intent in intents {
        let row = rows.get(&intent.component_type_id).copied().ok_or(
            CanonicalWriteError::UnknownComponent {
                component_type_id: intent.component_type_id,
            },
        )?;
        let current_method = match row.sourcing {
            DependencySourcing::Produce { method } => Some(method),
            DependencySourcing::Buy => None,
        };
        let (sourcing, producer) = match intent.produce {
            None => (DependencySourcing::Buy, PlannedProducer::None),
            Some(method) => {
                let key = CanonicalProducerKey {
                    output_type_id: intent.component_type_id,
                    method,
                };
                let keeps_producer = current_method == Some(method)
                    && row.producer_build_id.is_some_and(|producer| {
                        !graph.is_retired(producer) && key_of_build(producer) == Some(key)
                    });
                let producer = if keeps_producer {
                    PlannedProducer::Existing {
                        producer: row.producer_build_id.expect("checked above"),
                    }
                } else {
                    let (active, detached) = canonical_candidates(graph, key);
                    match select_producer_target(&active, &detached) {
                        Ok(Some(target)) => {
                            if graph.would_create_cycle(consumer, target) {
                                return Err(CanonicalWriteError::WouldCreateCycle {
                                    consumer,
                                    producer: target,
                                });
                            }
                            PlannedProducer::Existing { producer: target }
                        }
                        Ok(None) => PlannedProducer::Create { key },
                        Err(candidates) => {
                            return Err(CanonicalWriteError::AmbiguousProducer { key, candidates });
                        }
                    }
                };
                (DependencySourcing::Produce { method }, producer)
            }
        };

        let new_producer = match producer {
            PlannedProducer::Existing { producer } => Some(producer),
            PlannedProducer::None | PlannedProducer::Create { .. } => None,
        };
        let unchanged = row.sourcing == sourcing
            && row.fulfillment_scope == intent.fulfillment_scope
            && !matches!(producer, PlannedProducer::Create { .. })
            && row.producer_build_id == new_producer;
        if unchanged {
            continue;
        }
        let previous_producer = row.producer_build_id;
        let previous_producer_still_referenced = previous_producer.is_some_and(|previous| {
            new_producer == Some(previous)
                || graph.incoming(previous).iter().any(|other| {
                    !(other.consumer == consumer
                        && other.component_type_id == intent.component_type_id)
                })
        });
        writes.push(PlannedEdgeWrite {
            dependency_id: row.id,
            component_type_id: intent.component_type_id,
            sourcing,
            producer,
            fulfillment_scope: intent.fulfillment_scope,
            previous_producer,
            previous_producer_still_referenced,
        });
    }
    Ok(writes)
}

/// The non-retired producers of `key` in a canonical root plan: reachable
/// (active) ones, then detached ones.
#[must_use]
pub fn canonical_candidates(
    graph: &RootPlanDependencyGraph,
    key: CanonicalProducerKey,
) -> (Vec<BuildId>, Vec<BuildId>) {
    let mut active = Vec::new();
    let mut detached = Vec::new();
    for producer in graph.producers() {
        if producer.is_root || producer.key != key || graph.is_retired(producer.producer) {
            continue;
        }
        if graph.is_detached(producer.producer) {
            detached.push(producer.producer);
        } else {
            active.push(producer.producer);
        }
    }
    (active, detached)
}

// ---------------------------------------------------------------------------
// Planner topology
// ---------------------------------------------------------------------------

/// Why a canonical root's persisted graph cannot be planned. Always a hard,
/// typed failure of the projection -- never a silent partial plan.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize)]
#[serde(
    tag = "code",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CanonicalGraphError {
    #[error("production dependency cycle through {producers:?}")]
    Cycle { producers: Vec<BuildId> },
    #[error("demand edge {dependency} references a retired producer {producer:?}")]
    RetiredProducerReferenced {
        dependency: String,
        producer: BuildId,
    },
    #[error("two active producers of {key:?}: {producers:?}")]
    DuplicateActiveProducer {
        key: CanonicalProducerKey,
        producers: Vec<BuildId>,
    },
    #[error("persisted canonical graph diagnostic: {detail}")]
    Diagnostic { detail: String },
}

/// One demand edge as the canonical planner walks it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanningEdge {
    /// See [`crate::build_materials::NodeMaterialAllocation::dependency_id`].
    pub dependency_id: String,
    pub component_type_id: i64,
    pub fulfillment_scope: FulfillmentScope,
    /// `None` = Buy; `Some(method)` = Produce.
    pub produce: Option<ProductionMethod>,
    /// The producer satisfying a Produce edge; `None` = unresolved
    /// (provisional demand, recipe inputs not expanded).
    pub producer: Option<BuildId>,
}

impl PlanningEdge {
    #[must_use]
    pub fn from_dependency(edge: &ProductionDependency) -> Self {
        Self {
            dependency_id: dependency_evidence_id(edge),
            component_type_id: edge.component_type_id,
            fulfillment_scope: edge.fulfillment_scope,
            produce: match edge.sourcing {
                DependencySourcing::Produce { method } => Some(method),
                DependencySourcing::Buy => None,
            },
            producer: active_producer(edge),
        }
    }
}

/// The validated producer DAG one canonical projection walks, from `start`
/// (the plan root, or any producer of the plan viewed on its own): every
/// producer reachable through Produce edges that name a producer, each
/// node's edges, and for each producer the number of demand edges that can
/// reach it from reachable consumers -- its topological in-degree over
/// edges. A producer is sized only once all of them have arrived.
#[derive(Debug, Clone)]
pub struct CanonicalPlanTopology {
    pub start: BuildId,
    /// Reachable nodes (`start` first), breadth-first in edge order.
    pub reachable: Vec<BuildId>,
    edges: HashMap<Uuid, Vec<PlanningEdge>>,
    pub incoming_edge_count: HashMap<Uuid, usize>,
}

impl CanonicalPlanTopology {
    /// Validate and index `graph` from `start`. `start_edges` replaces
    /// `start`'s persisted edges (the live, unsaved overlay of the Build
    /// being projected); every other node uses its persisted edges.
    ///
    /// # Errors
    /// A cycle, a reference to a retired producer, two active producers of
    /// one identity, or a structural persisted-graph diagnostic (dangling or
    /// cyclic reference, missing/unmatched edge, producer method mismatch)
    /// on a reachable node.
    pub fn new(
        graph: &RootPlanDependencyGraph,
        start: BuildId,
        start_edges: Option<Vec<PlanningEdge>>,
    ) -> Result<Self, CanonicalGraphError> {
        let mut edges: HashMap<Uuid, Vec<PlanningEdge>> = HashMap::new();
        let mut start_edges = start_edges;
        let mut edges_of = |node: BuildId| -> Vec<PlanningEdge> {
            if node == start {
                if let Some(overlay) = start_edges.take() {
                    return overlay;
                }
            }
            let mut list: Vec<PlanningEdge> = graph
                .outgoing(node)
                .into_iter()
                .map(PlanningEdge::from_dependency)
                .collect();
            list.sort_by_key(|edge| edge.component_type_id);
            list
        };

        let mut reachable = Vec::new();
        let mut seen: HashSet<Uuid> = HashSet::new();
        let mut incoming_edge_count: HashMap<Uuid, usize> = HashMap::new();
        let mut queue = VecDeque::from([start]);
        seen.insert(start.0);
        while let Some(current) = queue.pop_front() {
            reachable.push(current);
            let list = edges_of(current);
            for edge in &list {
                let Some(producer) = edge.producer else {
                    continue;
                };
                if graph.is_retired(producer) {
                    return Err(CanonicalGraphError::RetiredProducerReferenced {
                        dependency: edge.dependency_id.clone(),
                        producer,
                    });
                }
                if graph.producer(producer).is_none() {
                    return Err(CanonicalGraphError::Diagnostic {
                        detail: format!(
                            "demand edge {} references unknown producer {:?}",
                            edge.dependency_id, producer
                        ),
                    });
                }
                *incoming_edge_count.entry(producer.0).or_insert(0) += 1;
                if seen.insert(producer.0) {
                    queue.push_back(producer);
                }
            }
            edges.insert(current.0, list);
        }

        // Structural diagnostics only matter where the plan actually walks.
        let reachable_set: HashSet<Uuid> = reachable.iter().map(|id| id.0).collect();
        let consumer_of = |dependency: &ProductionDependencyId| {
            graph.dependency(dependency).map(|edge| edge.consumer)
        };
        for diagnostic in &graph.diagnostics {
            let consumer = match diagnostic {
                AdapterDiagnostic::StaleCapturedRecipe { .. } => continue,
                AdapterDiagnostic::CyclicProducerReference { dependency, .. }
                | AdapterDiagnostic::ProducerMethodMismatch { dependency, .. }
                | AdapterDiagnostic::DanglingProducerReference { dependency, .. } => {
                    consumer_of(dependency)
                }
                AdapterDiagnostic::MissingPersistedEdge { consumer, .. }
                | AdapterDiagnostic::UnmatchedPersistedEdge { consumer, .. } => Some(*consumer),
            };
            if consumer.map_or(true, |consumer| reachable_set.contains(&consumer.0)) {
                return Err(match diagnostic {
                    AdapterDiagnostic::CyclicProducerReference { producer, .. } => {
                        CanonicalGraphError::Cycle {
                            producers: vec![*producer],
                        }
                    }
                    other => CanonicalGraphError::Diagnostic {
                        detail: serde_json::to_string(other).unwrap_or_default(),
                    },
                });
            }
        }
        if let Some(cycle) = find_cycle(start, &|node: BuildId| {
            edges
                .get(&node.0)
                .map(|list| list.iter().filter_map(|edge| edge.producer).collect())
                .unwrap_or_default()
        }) {
            return Err(CanonicalGraphError::Cycle { producers: cycle });
        }

        let mut by_key: BTreeMap<CanonicalProducerKey, Vec<BuildId>> = BTreeMap::new();
        for producer in &reachable {
            if *producer == start {
                continue;
            }
            if let Some(config) = graph.producer(*producer) {
                by_key.entry(config.key).or_default().push(*producer);
            }
        }
        if let Some((key, producers)) = by_key.into_iter().find(|(_, list)| list.len() > 1) {
            return Err(CanonicalGraphError::DuplicateActiveProducer { key, producers });
        }
        Ok(Self {
            start,
            reachable,
            edges,
            incoming_edge_count,
        })
    }

    /// `node`'s edges, ascending by component type.
    #[must_use]
    pub fn edges_of(&self, node: BuildId) -> &[PlanningEdge] {
        self.edges.get(&node.0).map_or(&[], Vec::as_slice)
    }
}

/// The producer a Produce edge references, if any (Buy and unresolved
/// Produce edges reference none).
#[must_use]
pub fn active_producer(edge: &ProductionDependency) -> Option<BuildId> {
    match edge.sourcing {
        DependencySourcing::Produce { .. } => edge.producer,
        DependencySourcing::Buy => None,
    }
}

/// The identity carried by every canonical demand edge in planner evidence.
#[must_use]
pub fn dependency_evidence_id(edge: &ProductionDependency) -> String {
    edge.id.as_str().to_string()
}

/// The evidence id for a boundary whose edge is unknown to the persisted
/// graph (e.g. an unsaved root overlay change): the edge's
/// natural key, in the same `dep:` scheme [`ProductionDependencyId::derived`]
/// uses.
#[must_use]
pub fn natural_dependency_id(consumer: BuildId, component_type_id: i64) -> String {
    ProductionDependencyId::derived(consumer, component_type_id)
        .as_str()
        .to_string()
}

/// Which existing producer of one canonical identity a Produce edge should
/// reference: a single active candidate, else a single retained (detached)
/// one; with none, `Ok(None)` (create exactly one). Several candidates is
/// ambiguous -- never resolved by guessing -- and returns every candidate.
fn select_producer_target(
    active: &[BuildId],
    detached: &[BuildId],
) -> Result<Option<BuildId>, Vec<BuildId>> {
    match (active, detached) {
        ([only], _) => Ok(Some(*only)),
        ([], [only]) => Ok(Some(*only)),
        ([], []) => Ok(None),
        _ => {
            let mut all = active.to_vec();
            all.extend(detached.iter().copied());
            Err(all)
        }
    }
}

#[cfg(test)]
mod tests;
