//! **Canonical producers**: the `ProductionDependency` demand-edge
//! read model over a root Build plan, plus canonicalization-conflict analysis.
//!
//! ## Three distinct concepts (never collapsed)
//!
//! * **Producer** ([`ProducerConfiguration`]) -- a persisted `Build`: the
//!   configured *method* for producing one item inside one root plan (recipe
//!   / formula, facility, blueprint ME/TE, and its **own** outgoing
//!   dependencies' sourcing). Configuration only; no quantities.
//! * **ProductionOperation** -- a *derived* planning result (aggregate
//!   demand, inventory allocation, runs, output, surplus, cost). Produced by
//!   the planner (`IndustryService::project_build_materials`), never stored
//!   here. [`ProducerDemand`] / [`ProducerSizing`] are the pure shape of how
//!   one producer's operation is sized from many incoming demand edges.
//! * **ExecutionJob** -- future physical EVE job/BPC/character/timing. Out of
//!   scope.
//!
//! ## Demand edge vs producer ownership
//!
//! A [`ProductionDependency`] is one *requirement* of one consuming producer
//! for one component type. The **edge** owns the consumer, the component,
//! the Buy-vs-Produce decision, the fulfillment scope, and a reference to the
//! producer that satisfies it. The **producer** owns everything about *how*
//! the item is made -- including its own outgoing edges. Several edges may
//! reference the same producer ("one producer, many consumers"); the model is
//! a DAG keyed by producer, not a tree keyed by position.
//!
//! ## Persistence
//!
//! Each edge is a `production_dependencies` row and each producer a Build of
//! the root plan (`plan_root_build_id`). [`RootPlanDependencyGraph::from_persisted`]
//! projects what `IndustryRepository::load_root_plan` loads onto this model,
//! pure and with no reinterpretation.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::build_materials::{allocate, pooled_production_runs, PlanningInventory};
use crate::{
    recipe_selection_of, BlueprintKind, BlueprintSelection, Build, BuildId, BuildRecipeKind,
    FacilityProfileId, FulfillmentScope, RecipeSelection,
};

// ---------------------------------------------------------------------------
// Identities
// ---------------------------------------------------------------------------

/// Stable identity of one demand edge: *which producer* requires *which
/// component*. Deliberately **not** tree-position based (`build:{id}` /
/// `tree_path`): once one producer serves many consumers, a position no
/// longer identifies a requirement, but "consumer producer + component type"
/// still does -- a producer has exactly one configuration, hence exactly one
/// requirement per component type.
///
/// A persisted edge carries its database id ([`Self::persisted`]); the
/// planner derives one deterministically ([`Self::derived`]) for an edge it
/// has not written yet.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct ProductionDependencyId(String);

impl ProductionDependencyId {
    /// Deterministic identity for an edge not yet persisted:
    /// `dep:<consumer build uuid>:<component type id>`. Unique because a
    /// consumer has at most one demand edge per component type (a captured
    /// recipe lists each material type once).
    #[must_use]
    pub fn derived(consumer: BuildId, component_type_id: i64) -> Self {
        Self(format!("dep:{}:{component_type_id}", consumer.0))
    }

    /// The identity of a persisted `production_dependencies` row:
    /// `pd:<row uuid>`. Bridged to [`Self::derived`] through the edge's
    /// natural key `(consumer, component_type_id)`, which is unique in both
    /// representations (`production_dependencies_edge_unique`).
    #[must_use]
    pub fn persisted(id: Uuid) -> Self {
        Self(format!("pd:{id}"))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The selected production method. Mirrors [`RecipeSelection`] but is
/// orderable so it can participate in a map key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ProductionMethod {
    Manufacturing { blueprint_type_id: i64 },
    Reaction { reaction_formula_type_id: i64 },
}

impl From<RecipeSelection> for ProductionMethod {
    fn from(selection: RecipeSelection) -> Self {
        match selection {
            RecipeSelection::Manufacturing { blueprint_type_id } => {
                Self::Manufacturing { blueprint_type_id }
            }
            RecipeSelection::Reaction {
                reaction_formula_type_id,
            } => Self::Reaction {
                reaction_formula_type_id,
            },
        }
    }
}

/// What "one producer" means inside one root plan: the produced item plus
/// the selected production method (activity + blueprint / formula).
///
/// **Invariant (target):** within one root plan, at most one producer exists
/// per `CanonicalProducerKey`; every demand edge that produces this item by
/// this method references it. Two keys for the same `output_type_id` (a
/// genuinely alternative method) are distinct producers and never
/// conflated.
///
/// This is intentionally **not** Shared Production's quantity-compatibility
/// key: facility, ME/TE and descendant sourcing are *configuration of* the
/// producer, not part of its identity. Two candidates that differ there are
/// the same producer configured inconsistently -- a conflict to resolve,
/// not two legitimate producers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalProducerKey {
    pub output_type_id: i64,
    pub method: ProductionMethod,
}

// ---------------------------------------------------------------------------
// Read model
// ---------------------------------------------------------------------------

/// Buy vs Produce -- owned by the demand edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(
    tag = "strategy",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DependencySourcing {
    Buy,
    /// Produce with `method`. The satisfying producer is
    /// [`ProductionDependency::producer`]; `None` there means no producer
    /// exists yet.
    Produce {
        method: ProductionMethod,
    },
}

/// One demand edge: `consumer` requires `component_type_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionDependency {
    pub id: ProductionDependencyId,
    /// The producer (a Build -- root or descendant) whose recipe requires
    /// the component.
    pub consumer: BuildId,
    pub component_type_id: i64,
    pub component_type_name: String,
    /// The consumer recipe's raw per-run quantity (pre ME/facility).
    /// Informational -- exact requirements come from the planner's preview.
    pub base_quantity_per_run: u64,
    pub fulfillment_scope: FulfillmentScope,
    pub sourcing: DependencySourcing,
    /// The producer satisfying this edge when `sourcing` is `Produce`.
    pub producer: Option<BuildId>,
    /// **Consumer-stored producer configuration**: the
    /// `ComponentResolution.facility_override` / `.blueprint_selection` the
    /// consumer carries for this component. These belong to the producer --
    /// the planner reads the producer Build's own draft instead -- and are
    /// surfaced only to show where a consumer still sets them.
    pub legacy_consumer_overrides: Option<LegacyConsumerOverrides>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyConsumerOverrides {
    pub facility_profile_id: Option<FacilityProfileId>,
    pub blueprint_selection: Option<BlueprintSelection>,
}

/// A producer's effective blueprint configuration (manufacturing only),
/// normalized for comparison: free-text notes are excluded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProducerBlueprint {
    pub source: BlueprintSource,
    /// `None` only for an observed-asset selection whose effective
    /// configuration was never captured (resolving it needs I/O).
    pub effective_me: Option<u8>,
    pub effective_te: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "source",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum BlueprintSource {
    Manual {
        kind: BlueprintKind,
        licensed_runs: Option<u64>,
    },
    ObservedAsset {
        observation_id: Uuid,
        kind: BlueprintKind,
    },
    /// No blueprint selection: ME/TE come from the facility command's
    /// blueprint ME/TE (`0/0` without one).
    Legacy,
}

impl ProducerBlueprint {
    fn from_draft(
        selection: Option<&BlueprintSelection>,
        legacy: Option<&crate::FacilityPreviewCommand>,
    ) -> Self {
        match selection {
            Some(BlueprintSelection::Manual {
                kind,
                material_efficiency,
                time_efficiency,
                licensed_runs,
                ..
            }) => Self {
                source: BlueprintSource::Manual {
                    kind: *kind,
                    licensed_runs: *licensed_runs,
                },
                effective_me: Some(*material_efficiency),
                effective_te: Some(*time_efficiency),
            },
            Some(BlueprintSelection::ObservedAsset {
                observation_id,
                kind,
                material_efficiency,
                time_efficiency,
                ..
            }) => {
                let captured = *kind != BlueprintKind::Unknown;
                Self {
                    source: BlueprintSource::ObservedAsset {
                        observation_id: *observation_id,
                        kind: *kind,
                    },
                    effective_me: captured.then_some(*material_efficiency),
                    effective_te: captured.then_some(*time_efficiency),
                }
            }
            None => Self {
                source: BlueprintSource::Legacy,
                effective_me: Some(legacy.map_or(0, |command| command.blueprint_me)),
                effective_te: Some(legacy.map_or(0, |command| command.blueprint_te)),
            },
        }
    }
}

/// One producer: a persisted Build's production configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProducerConfiguration {
    pub producer: BuildId,
    pub is_root: bool,
    pub key: CanonicalProducerKey,
    pub output_type_name: String,
    pub output_per_run: u64,
    /// The facility of the producer's own activity slot.
    pub facility_profile_id: Option<FacilityProfileId>,
    /// `None` for reactions.
    pub blueprint: Option<ProducerBlueprint>,
    /// The root's user-requested runs; `None` for every descendant, whose
    /// runs are derived from aggregate incoming demand.
    pub requested_runs: Option<u64>,
    /// `Build.runs` as persisted. Authoritative for the root only; for a
    /// descendant it is informational only (the allocating planner derives
    /// descendant runs from aggregate incoming demand).
    #[serde(rename = "legacyPersistedRuns")]
    pub persisted_runs: u64,
    /// Outgoing demand edges, in recipe order.
    pub dependencies: Vec<ProductionDependencyId>,
}

/// Non-fatal findings when building the graph from persisted rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "code",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AdapterDiagnostic {
    /// A dependency would reference a producer already on the current path
    /// (a cycle -- only reachable through corrupt data). The reference is
    /// dropped and the branch stops there.
    CyclicProducerReference {
        dependency: ProductionDependencyId,
        producer: BuildId,
    },
    /// The consumer's resolution names one method but the linked producer
    /// Build uses another. The producer's own recipe wins (as in the
    /// planner).
    ProducerMethodMismatch {
        dependency: ProductionDependencyId,
        requested: ProductionMethod,
        producer_method: ProductionMethod,
    },
    /// The producer's captured recipe is no longer the active SDE's. This
    /// read model derives requirements from the *captured* lines (pure, no
    /// SDE I/O) while the live planner previews the active SDE recipe, so
    /// this producer's edges may not match what the planner walks.
    StaleCapturedRecipe {
        producer: BuildId,
        recipe_currency: crate::RecipeCurrency,
    },
    /// Persisted graph only: a captured recipe requirement with no
    /// `production_dependencies` row (data drift).
    MissingPersistedEdge {
        consumer: BuildId,
        component_type_id: i64,
    },
    /// Persisted graph only: a row whose component is not a requirement of
    /// its consumer's captured recipe.
    UnmatchedPersistedEdge {
        dependency: ProductionDependencyId,
        consumer: BuildId,
        component_type_id: i64,
    },
    /// Persisted graph only: the edge names a producer outside the loaded
    /// root plan. Treated as unresolved.
    DanglingProducerReference {
        dependency: ProductionDependencyId,
        producer: BuildId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PersistedGraphError {
    #[error("build {0:?} appears twice in the root plan records")]
    DuplicateBuild(BuildId),
    #[error("two persisted edges for consumer {consumer:?} component {component_type_id}")]
    DuplicateEdge {
        consumer: BuildId,
        component_type_id: i64,
    },
}

/// One `production_dependencies` row. Edge-owned facts only -- never
/// producer configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedProductionDependency {
    pub id: Uuid,
    pub plan_root_build_id: BuildId,
    pub consumer_build_id: BuildId,
    pub component_type_id: i64,
    /// `Produce { method }` carries the consumer's *requested* method.
    pub sourcing: DependencySourcing,
    pub producer_build_id: Option<BuildId>,
    pub fulfillment_scope: FulfillmentScope,
    /// Change counter of the derived row. Concurrency is enforced by the
    /// consumer Build's revision, not this one.
    pub revision: u64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Everything one root plan persists, loaded in a bounded number of queries
/// (`IndustryRepository::load_root_plan`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootPlanRecords {
    pub root: Build,
    /// Every other Build whose `plan_root_build_id` is `root` -- reachable
    /// or not.
    pub producers: Vec<Build>,
    pub dependencies: Vec<PersistedProductionDependency>,
    /// Producers retired by canonical reconciliation
    /// (`retired_producers`), ascending by id.
    pub retired_producers: Vec<BuildId>,
}

/// The demand-edge / producer graph of one root plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootPlanDependencyGraph {
    pub root: BuildId,
    /// Every reachable producer, in deterministic discovery (DFS pre-)order.
    producers: Vec<ProducerConfiguration>,
    dependencies: BTreeMap<ProductionDependencyId, ProductionDependency>,
    /// Producers of the plan **not** reachable from the root through active
    /// Produce edges (a retained producer after Build -> Buy, and anything
    /// only it references), in visit order.
    detached_producers: Vec<BuildId>,
    /// Producers retired by canonical reconciliation (a subset of
    /// `detached_producers`): never referenced again, never a candidate.
    retired_producers: Vec<BuildId>,
    pub diagnostics: Vec<AdapterDiagnostic>,
}

/// A visited producer's requirement edges: the persisted
/// `production_dependencies` rows and the plan's Builds, keyed for lookup.
struct EdgeSource<'a> {
    edges: HashMap<(Uuid, i64), &'a PersistedProductionDependency>,
    builds: HashMap<Uuid, &'a Build>,
}

/// One requirement line resolved against an [`EdgeSource`].
struct ResolvedLine<'a> {
    id: ProductionDependencyId,
    sourcing: DependencySourcing,
    producer: Option<&'a Build>,
    fulfillment_scope: FulfillmentScope,
}

impl RootPlanDependencyGraph {
    fn empty(root: BuildId) -> Self {
        Self {
            root,
            producers: Vec::new(),
            dependencies: BTreeMap::new(),
            detached_producers: Vec::new(),
            retired_producers: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    /// Build the graph from persisted `production_dependencies` rows and the
    /// root plan's Builds (`IndustryRepository::load_root_plan`). Pure.
    ///
    /// Producers reachable from the root are visited first, in the same
    /// deterministic recipe-order DFS; every other
    /// Build of the plan (a retained producer with no incoming Produce
    /// edge, and anything only it references) follows, ordered by id, and
    /// is listed in [`Self::detached_producers`] -- kept, never dropped.
    ///
    /// # Errors
    /// The same Build id twice in the records.
    pub fn from_persisted(records: &RootPlanRecords) -> Result<Self, PersistedGraphError> {
        let mut builds: HashMap<Uuid, &Build> = HashMap::with_capacity(records.producers.len() + 1);
        for build in std::iter::once(&records.root).chain(&records.producers) {
            if builds.insert(build.id.0, build).is_some() {
                return Err(PersistedGraphError::DuplicateBuild(build.id));
            }
        }
        let mut edges: HashMap<(Uuid, i64), &PersistedProductionDependency> =
            HashMap::with_capacity(records.dependencies.len());
        for edge in &records.dependencies {
            if edges
                .insert((edge.consumer_build_id.0, edge.component_type_id), edge)
                .is_some()
            {
                return Err(PersistedGraphError::DuplicateEdge {
                    consumer: edge.consumer_build_id,
                    component_type_id: edge.component_type_id,
                });
            }
        }

        let source = EdgeSource { edges, builds };
        let mut graph = Self::empty(records.root.id);
        graph
            .retired_producers
            .clone_from(&records.retired_producers);
        let mut visited: HashSet<Uuid> = HashSet::new();
        let mut on_path: HashSet<Uuid> = HashSet::new();
        let mut matched: HashSet<(Uuid, i64)> = HashSet::new();
        graph.visit(
            &records.root,
            true,
            &source,
            &mut visited,
            &mut on_path,
            &mut matched,
        );

        let mut rest: Vec<&Build> = records
            .producers
            .iter()
            .filter(|build| !visited.contains(&build.id.0))
            .collect();
        rest.sort_by_key(|build| build.id.0);
        for build in rest {
            if visited.contains(&build.id.0) {
                continue; // reached through an earlier detached producer
            }
            let before = graph.producers.len();
            graph.visit(
                build,
                false,
                &source,
                &mut visited,
                &mut on_path,
                &mut matched,
            );
            graph.detached_producers.extend(
                graph.producers[before..]
                    .iter()
                    .map(|producer| producer.producer),
            );
        }

        let mut unmatched: Vec<&PersistedProductionDependency> = records
            .dependencies
            .iter()
            .filter(|edge| !matched.contains(&(edge.consumer_build_id.0, edge.component_type_id)))
            .collect();
        unmatched.sort_by_key(|edge| (edge.consumer_build_id.0, edge.component_type_id));
        graph.diagnostics.extend(unmatched.into_iter().map(|edge| {
            AdapterDiagnostic::UnmatchedPersistedEdge {
                dependency: ProductionDependencyId::persisted(edge.id),
                consumer: edge.consumer_build_id,
                component_type_id: edge.component_type_id,
            }
        }));
        Ok(graph)
    }

    fn resolve_line<'a>(
        &mut self,
        source: &EdgeSource<'a>,
        build: &Build,
        type_id: i64,
        matched: &mut HashSet<(Uuid, i64)>,
    ) -> Option<ResolvedLine<'a>> {
        let Some(edge) = source.edges.get(&(build.id.0, type_id)).copied() else {
            self.diagnostics
                .push(AdapterDiagnostic::MissingPersistedEdge {
                    consumer: build.id,
                    component_type_id: type_id,
                });
            return None;
        };
        matched.insert((build.id.0, type_id));
        let id = ProductionDependencyId::persisted(edge.id);
        let (sourcing, producer) = match edge.sourcing {
            DependencySourcing::Buy => (DependencySourcing::Buy, None),
            DependencySourcing::Produce { method: requested } => {
                let producer = edge.producer_build_id.and_then(|producer_id| {
                    let found = source.builds.get(&producer_id.0).copied();
                    if found.is_none() {
                        self.diagnostics
                            .push(AdapterDiagnostic::DanglingProducerReference {
                                dependency: id.clone(),
                                producer: producer_id,
                            });
                    }
                    found
                });
                let method = self.producer_method(&id, requested, producer);
                (DependencySourcing::Produce { method }, producer)
            }
        };
        Some(ResolvedLine {
            id,
            sourcing,
            producer,
            fulfillment_scope: edge.fulfillment_scope,
        })
    }

    /// The producer's own recipe wins over the consumer's requested method
    /// (as in the planner); a disagreement is reported.
    fn producer_method(
        &mut self,
        dependency: &ProductionDependencyId,
        requested: ProductionMethod,
        producer: Option<&Build>,
    ) -> ProductionMethod {
        let Some(producer) = producer else {
            return requested;
        };
        let producer_method = ProductionMethod::from(recipe_selection_of(&producer.recipe));
        if producer_method != requested {
            self.diagnostics
                .push(AdapterDiagnostic::ProducerMethodMismatch {
                    dependency: dependency.clone(),
                    requested,
                    producer_method,
                });
        }
        producer_method
    }

    fn visit(
        &mut self,
        build: &Build,
        is_root: bool,
        source: &EdgeSource<'_>,
        visited: &mut HashSet<Uuid>,
        on_path: &mut HashSet<Uuid>,
        matched: &mut HashSet<(Uuid, i64)>,
    ) {
        visited.insert(build.id.0);
        on_path.insert(build.id.0);

        let draft = build
            .draft_planning
            .as_ref()
            .map(|snapshot| &snapshot.input);
        let resolutions: BTreeMap<i64, &crate::ComponentResolution> = draft
            .map(|input| {
                input
                    .component_resolutions
                    .iter()
                    .map(|resolution| (resolution.type_id, resolution))
                    .collect()
            })
            .unwrap_or_default();
        let kind = build.recipe.kind();
        let product = build.recipe.primary_product();
        let (facility_profile_id, blueprint) = match kind {
            BuildRecipeKind::Manufacturing => (
                draft
                    .and_then(|input| input.manufacturing_facility.as_ref())
                    .map(|command| command.facility_profile_id),
                Some(ProducerBlueprint::from_draft(
                    draft.and_then(|input| input.blueprint_selection.as_ref()),
                    draft.and_then(|input| input.manufacturing_facility.as_ref()),
                )),
            ),
            BuildRecipeKind::Reaction => (
                draft
                    .and_then(|input| input.reaction_facility.as_ref())
                    .map(|command| command.facility_profile_id),
                None,
            ),
        };

        if build.recipe_currency != crate::RecipeCurrency::Current {
            self.diagnostics
                .push(AdapterDiagnostic::StaleCapturedRecipe {
                    producer: build.id,
                    recipe_currency: build.recipe_currency,
                });
        }

        let producer_index = self.producers.len();
        self.producers.push(ProducerConfiguration {
            producer: build.id,
            is_root,
            key: CanonicalProducerKey {
                output_type_id: product.type_id,
                method: recipe_selection_of(&build.recipe).into(),
            },
            output_type_name: product.type_name.clone(),
            output_per_run: product.quantity_per_run.max(1),
            facility_profile_id,
            blueprint,
            requested_runs: is_root.then_some(build.runs),
            persisted_runs: build.runs,
            dependencies: Vec::new(),
        });

        let mut seen_types: BTreeSet<i64> = BTreeSet::new();
        for line in build.recipe.materials() {
            if !seen_types.insert(line.type_id) {
                continue;
            }
            let resolution = resolutions.get(&line.type_id).copied();
            let Some(ResolvedLine {
                id,
                sourcing,
                producer,
                fulfillment_scope,
            }) = self.resolve_line(source, build, line.type_id, matched)
            else {
                continue;
            };

            let mut producer_id = producer.map(|child| child.id);
            if let Some(child) = producer {
                if on_path.contains(&child.id.0) {
                    self.diagnostics
                        .push(AdapterDiagnostic::CyclicProducerReference {
                            dependency: id.clone(),
                            producer: child.id,
                        });
                    producer_id = None;
                } else if !visited.contains(&child.id.0) {
                    self.visit(child, false, source, visited, on_path, matched);
                }
                // Already visited and not on the path: a shared producer
                // reference -- representable, never re-walked.
            }

            self.producers[producer_index].dependencies.push(id.clone());
            self.dependencies.insert(
                id.clone(),
                ProductionDependency {
                    id,
                    consumer: build.id,
                    component_type_id: line.type_id,
                    component_type_name: line.type_name.clone(),
                    base_quantity_per_run: line.quantity_per_run,
                    fulfillment_scope,
                    sourcing,
                    producer: producer_id,
                    legacy_consumer_overrides: resolution.and_then(|resolution| {
                        (resolution.facility_override.is_some()
                            || resolution.blueprint_selection.is_some())
                        .then(|| LegacyConsumerOverrides {
                            facility_profile_id: resolution
                                .facility_override
                                .as_ref()
                                .map(|value| value.facility_profile_id),
                            blueprint_selection: resolution.blueprint_selection.clone(),
                        })
                    }),
                },
            );
        }

        on_path.remove(&build.id.0);
    }

    /// Assemble a graph directly (target-shape fixtures and a future
    /// dependency-table loader). Producers are kept in the given order.
    #[must_use]
    pub fn from_parts(
        root: BuildId,
        producers: Vec<ProducerConfiguration>,
        dependencies: Vec<ProductionDependency>,
    ) -> Self {
        Self {
            root,
            producers,
            dependencies: dependencies
                .into_iter()
                .map(|dependency| (dependency.id.clone(), dependency))
                .collect(),
            detached_producers: Vec::new(),
            retired_producers: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    #[must_use]
    pub fn is_retired(&self, producer: BuildId) -> bool {
        self.retired_producers.contains(&producer)
    }

    /// See the field doc. These producers have no incoming Produce edge
    /// from the root plan's active topology; they are retained, not
    /// deleted.
    #[must_use]
    pub fn detached_producers(&self) -> &[BuildId] {
        &self.detached_producers
    }

    pub(crate) fn is_detached(&self, producer: BuildId) -> bool {
        self.detached_producers.contains(&producer)
    }

    #[must_use]
    pub fn producers(&self) -> &[ProducerConfiguration] {
        &self.producers
    }

    #[must_use]
    pub fn producer(&self, id: BuildId) -> Option<&ProducerConfiguration> {
        self.producers
            .iter()
            .find(|producer| producer.producer == id)
    }

    #[must_use]
    pub fn dependency(&self, id: &ProductionDependencyId) -> Option<&ProductionDependency> {
        self.dependencies.get(id)
    }

    /// A producer's own outgoing edges, in recipe order.
    #[must_use]
    pub fn outgoing(&self, producer: BuildId) -> Vec<&ProductionDependency> {
        self.producer(producer)
            .map(|config| {
                config
                    .dependencies
                    .iter()
                    .filter_map(|id| self.dependencies.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Every edge satisfied by `producer` -- its consumers. Ordered by edge id.
    #[must_use]
    pub fn incoming(&self, producer: BuildId) -> Vec<&ProductionDependency> {
        self.dependencies
            .values()
            .filter(|dependency| dependency.producer == Some(producer))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Target semantics: many demand edges -> one producer sizing
// ---------------------------------------------------------------------------

/// One edge's exact requirement, as computed by the consumer's own
/// authoritative preview. Callers pass these in the planner's deterministic
/// allocation order; inventory is drawn in exactly that order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemandEdgeRequirement {
    pub dependency: ProductionDependencyId,
    pub required_quantity: u64,
}

/// Per-consumer attribution inside one producer's aggregate demand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeDemand {
    pub dependency: ProductionDependencyId,
    pub consumer: BuildId,
    pub fulfillment_scope: FulfillmentScope,
    pub required_quantity: u64,
    pub allocated_inventory: u64,
    pub production_demand: u64,
}

/// A producer's aggregate production demand: inventory is allocated per
/// edge first (by that edge's own fulfillment scope), and only the
/// remaining production demand is aggregated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProducerDemand {
    pub producer: BuildId,
    pub edges: Vec<EdgeDemand>,
    pub total_required: u64,
    pub total_allocated_inventory: u64,
    pub total_production_demand: u64,
}

/// One sizing of one producer from its aggregate demand -- never a sum of
/// independently sized runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProducerSizing {
    pub runs: u64,
    pub output_per_run: u64,
    pub produced_quantity: u64,
    pub surplus_quantity: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DemandAggregationError {
    #[error("unknown producer {0:?}")]
    UnknownProducer(BuildId),
    #[error("unknown dependency {0:?}")]
    UnknownDependency(ProductionDependencyId),
    #[error("dependency {dependency:?} is not satisfied by producer {producer:?}")]
    NotSatisfiedByProducer {
        dependency: ProductionDependencyId,
        producer: BuildId,
    },
    #[error("dependency {0:?} was supplied more than once")]
    DuplicateRequirement(ProductionDependencyId),
}

/// Aggregate `requirements` (each an incoming edge of `producer`) into one
/// production demand, allocating `inventory` per edge in the given order
/// with the existing `build_materials::allocate` (`Full` never draws,
/// `Missing` draws what remains).
///
/// # Errors
/// A requirement for an unknown edge, an edge not satisfied by `producer`,
/// or the same edge twice.
pub fn aggregate_producer_demand(
    graph: &RootPlanDependencyGraph,
    producer: BuildId,
    requirements: &[DemandEdgeRequirement],
    inventory: &mut PlanningInventory,
) -> Result<ProducerDemand, DemandAggregationError> {
    if graph.producer(producer).is_none() {
        return Err(DemandAggregationError::UnknownProducer(producer));
    }
    let mut seen: HashSet<&ProductionDependencyId> = HashSet::new();
    let mut edges = Vec::with_capacity(requirements.len());
    for requirement in requirements {
        let dependency = graph.dependency(&requirement.dependency).ok_or_else(|| {
            DemandAggregationError::UnknownDependency(requirement.dependency.clone())
        })?;
        if dependency.producer != Some(producer) {
            return Err(DemandAggregationError::NotSatisfiedByProducer {
                dependency: dependency.id.clone(),
                producer,
            });
        }
        if !seen.insert(&requirement.dependency) {
            return Err(DemandAggregationError::DuplicateRequirement(
                requirement.dependency.clone(),
            ));
        }
        let (allocated, remaining) = allocate(
            inventory,
            dependency.component_type_id,
            requirement.required_quantity,
            dependency.fulfillment_scope,
        );
        edges.push(EdgeDemand {
            dependency: dependency.id.clone(),
            consumer: dependency.consumer,
            fulfillment_scope: dependency.fulfillment_scope,
            required_quantity: requirement.required_quantity,
            allocated_inventory: allocated,
            production_demand: remaining,
        });
    }
    let sum = |f: fn(&EdgeDemand) -> u64| edges.iter().map(f).fold(0u64, u64::saturating_add);
    Ok(ProducerDemand {
        producer,
        total_required: sum(|e| e.required_quantity),
        total_allocated_inventory: sum(|e| e.allocated_inventory),
        total_production_demand: sum(|e| e.production_demand),
        edges,
    })
}

impl ProducerDemand {
    /// Size the producer **once** from its aggregate production demand with
    /// the same run rule the planner uses for pooled groups
    /// ([`pooled_production_runs`]). Exact material requirements at these
    /// runs still come from the planner's authoritative preview -- this
    /// never re-implements EVE quantity formulas.
    #[must_use]
    pub fn size(&self, output_per_run: u64) -> ProducerSizing {
        let output_per_run = output_per_run.max(1);
        let runs = pooled_production_runs(self.total_production_demand, output_per_run);
        let produced_quantity = runs.saturating_mul(output_per_run);
        ProducerSizing {
            runs,
            output_per_run,
            produced_quantity,
            surplus_quantity: produced_quantity.saturating_sub(self.total_production_demand),
        }
    }
}

// ---------------------------------------------------------------------------
// Cycle safety for the producer DAG
// ---------------------------------------------------------------------------

/// A cycle in the producer graph: each producer consumes the next one's
/// output, and the last consumes the first's.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("production dependency cycle through {0:?}")]
pub struct ProducerCycle(pub Vec<BuildId>);

impl RootPlanDependencyGraph {
    /// Would adding (or retargeting) an edge `consumer -> producer` make the
    /// producer graph cyclic? True when `producer` is `consumer` itself or
    /// already (transitively) depends on `consumer`'s output. The write-side
    /// guard every future edge insert/retarget must pass, inside the same
    /// transaction as the write.
    #[must_use]
    pub fn would_create_cycle(&self, consumer: BuildId, producer: BuildId) -> bool {
        if consumer == producer {
            return true;
        }
        let mut stack = vec![producer];
        let mut seen: HashSet<Uuid> = HashSet::new();
        while let Some(current) = stack.pop() {
            if !seen.insert(current.0) {
                continue;
            }
            for edge in self.outgoing(current) {
                if let Some(next) = edge.producer {
                    if next == consumer {
                        return true;
                    }
                    stack.push(next);
                }
            }
        }
        false
    }

    /// Verify the whole producer graph is acyclic (a DAG: shared producers
    /// and diamonds are fine).
    ///
    /// # Errors
    /// The first cycle found, in deterministic producer order.
    pub fn validate_acyclic(&self) -> Result<(), ProducerCycle> {
        #[derive(Clone, Copy, PartialEq)]
        enum Mark {
            Visiting,
            Done,
        }
        fn dfs(
            graph: &RootPlanDependencyGraph,
            node: BuildId,
            marks: &mut HashMap<Uuid, Mark>,
            path: &mut Vec<BuildId>,
        ) -> Result<(), ProducerCycle> {
            match marks.get(&node.0) {
                Some(Mark::Done) => return Ok(()),
                Some(Mark::Visiting) => {
                    let start = path.iter().position(|id| *id == node).unwrap_or(0);
                    return Err(ProducerCycle(path[start..].to_vec()));
                }
                None => {}
            }
            marks.insert(node.0, Mark::Visiting);
            path.push(node);
            for edge in graph.outgoing(node) {
                if let Some(next) = edge.producer {
                    dfs(graph, next, marks, path)?;
                }
            }
            path.pop();
            marks.insert(node.0, Mark::Done);
            Ok(())
        }
        let mut marks: HashMap<Uuid, Mark> = HashMap::new();
        for producer in &self.producers {
            dfs(self, producer.producer, &mut marks, &mut Vec::new())?;
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests;
