//! Optimize build sourcing: choose Buy vs Produce for every component of a
//! root plan by which gives the lower plan cost (higher margin).
//!
//! 1. [`CandidatePlan`] -- the plan with every eligible bought component
//!    switched to Produce, held only in memory. New producers are *virtual*:
//!    `Build` values made by the same template the canonical write uses, so
//!    the candidate is costed exactly like the plan Apply would create.
//! 2. The caller projects and costs the candidate
//!    ([`IndustryService::project_plan_records`] + `project_build_cost`).
//! 3. [`decide_sourcing`] -- bottom-up over the candidate's cost projection,
//!    one decision per component type for the whole plan.
//! 4. [`chosen_records`] -- the plan under those decisions; the caller
//!    projects it once more so the reported margin is exact
//!    ([`build_proposal`]).
//!
//! Steps 1, 3 and 4 are pure; I/O lives in the service methods at the end.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::BuildMaterialsProjection;
use super::{recipe_selection_of, IndustryError, IndustryService};
use crate::build_cost::{BoundaryCostKind, BoundaryCostProjection, BuildCostProjection};
use crate::build_materials::PlanningInventory;
use crate::production_dependency::{
    CanonicalProducerKey, DependencySourcing, PersistedProductionDependency, ProductionMethod,
    RootPlanDependencyGraph, RootPlanRecords,
};
use crate::{
    BlueprintSelection, Build, BuildId, ComponentResolution, FulfillmentScope, Money, OwnerId,
    RecipeSelection, WorkspaceId,
};

#[cfg(test)]
mod tests;

/// What the user asked the optimizer to consider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcingOptions {
    /// A switch away from the current sourcing must save at least this
    /// share of the current choice's cost (percent, `0` = any saving).
    pub min_saving_percent: Decimal,
    /// Manufacturing normally needs a synced blueprint; `true` lifts that.
    pub include_without_blueprint: bool,
    /// Component types that keep their current sourcing.
    pub exclude: BTreeSet<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourcingWarningKind {
    /// Manufacturable, but the owner has no synced blueprint.
    NoBlueprint,
    /// No market price to compare a produced component against.
    NoBuyPrice,
    /// The produce side of a component could not be fully costed.
    ProduceCostIncomplete,
    /// The component's recipe could not be captured from the SDE.
    RecipeUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcingWarning {
    pub type_id: i64,
    pub type_name: String,
    pub kind: SourcingWarningKind,
}

/// A component type still bought somewhere in the candidate plan that the
/// optimizer has not considered yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingComponent {
    pub type_id: i64,
    pub type_name: String,
    /// The first reachable consumer buying it -- the one a new producer's
    /// template is prepopulated from (as a manual Buy -> Build switch would).
    pub consumer: BuildId,
}

/// How a pending component was resolved in the candidate plan.
#[derive(Debug, Clone)]
pub enum CandidateProducer {
    /// A producer the plan already has (attached or detached).
    Existing(BuildId),
    /// A virtual producer: never persisted.
    New(Box<Build>),
    /// Not producible here; stays Buy. `Some` records why.
    Skip(Option<SourcingWarningKind>),
}

/// The plan with every eligible bought component switched to Produce.
#[derive(Debug, Clone)]
pub struct CandidatePlan {
    pub current: RootPlanRecords,
    pub records: RootPlanRecords,
    pub virtual_producers: HashSet<BuildId>,
    pub warnings: Vec<SourcingWarning>,
    exclude: BTreeSet<i64>,
    checked: BTreeSet<i64>,
}

impl CandidatePlan {
    #[must_use]
    pub fn new(current: RootPlanRecords, options: &SourcingOptions) -> Self {
        Self {
            records: current.clone(),
            current,
            virtual_producers: HashSet::new(),
            warnings: Vec::new(),
            exclude: options.exclude.clone(),
            checked: options.exclude.clone(),
        }
    }

    #[must_use]
    pub fn build(&self, id: BuildId) -> Option<&Build> {
        std::iter::once(&self.records.root)
            .chain(&self.records.producers)
            .find(|build| build.id == id)
    }

    /// Bought components (reachable consumers only) not yet considered,
    /// ascending by type.
    pub fn pending(&self) -> Result<Vec<PendingComponent>, IndustryError> {
        let reachable = reachable_builds(&self.records)?;
        let mut pending: BTreeMap<i64, PendingComponent> = BTreeMap::new();
        for edge in &self.records.dependencies {
            if !matches!(edge.sourcing, DependencySourcing::Buy)
                || self.checked.contains(&edge.component_type_id)
                || !reachable.contains(&edge.consumer_build_id)
                || pending.contains_key(&edge.component_type_id)
            {
                continue;
            }
            let type_name = self
                .build(edge.consumer_build_id)
                .and_then(|build| {
                    build
                        .recipe
                        .materials()
                        .iter()
                        .find(|line| line.type_id == edge.component_type_id)
                })
                .map_or_else(String::new, |line| line.type_name.clone());
            pending.insert(
                edge.component_type_id,
                PendingComponent {
                    type_id: edge.component_type_id,
                    type_name,
                    consumer: edge.consumer_build_id,
                },
            );
        }
        Ok(pending.into_values().collect())
    }

    /// A producer of `type_id` the plan already has: an attached one first,
    /// then a detached one (as the canonical write would reuse it).
    pub fn existing_producer(&self, type_id: i64) -> Result<Option<BuildId>, IndustryError> {
        let reachable = reachable_builds(&self.records)?;
        let mut producers: Vec<&Build> = self
            .records
            .producers
            .iter()
            .filter(|build| {
                build.recipe.primary_product().type_id == type_id
                    && !self.records.retired_producers.contains(&build.id)
            })
            .collect();
        producers.sort_by_key(|build| (!reachable.contains(&build.id), build.id.0));
        Ok(producers.first().map(|build| build.id))
    }

    /// Resolve one pending component: switch every reachable Buy edge of
    /// `type_id` to Produce by `producer` (or leave it Buy on `Skip`).
    pub fn attach(&mut self, pending: &PendingComponent, producer: CandidateProducer) {
        self.checked.insert(pending.type_id);
        let producer_id = match producer {
            CandidateProducer::Skip(warning) => {
                if let Some(kind) = warning {
                    self.warnings.push(SourcingWarning {
                        type_id: pending.type_id,
                        type_name: pending.type_name.clone(),
                        kind,
                    });
                }
                return;
            }
            CandidateProducer::Existing(id) => id,
            CandidateProducer::New(build) => {
                let id = build.id;
                self.add_virtual_producer(*build);
                id
            }
        };
        let Some(method) = self
            .build(producer_id)
            .map(|build| ProductionMethod::from(recipe_selection_of(&build.recipe)))
        else {
            return;
        };
        let reachable = reachable_builds(&self.records).unwrap_or_default();
        for edge in &mut self.records.dependencies {
            if edge.component_type_id == pending.type_id
                && matches!(edge.sourcing, DependencySourcing::Buy)
                && reachable.contains(&edge.consumer_build_id)
            {
                edge.sourcing = DependencySourcing::Produce { method };
                edge.producer_build_id = Some(producer_id);
            }
        }
        sync_resolutions(&mut self.records, None);
    }

    fn add_virtual_producer(&mut self, build: Build) {
        let mut seen = BTreeSet::new();
        for line in build.recipe.materials() {
            if !seen.insert(line.type_id) {
                continue;
            }
            self.records
                .dependencies
                .push(PersistedProductionDependency {
                    id: Uuid::new_v4(),
                    plan_root_build_id: self.records.root.id,
                    consumer_build_id: build.id,
                    component_type_id: line.type_id,
                    sourcing: DependencySourcing::Buy,
                    producer_build_id: None,
                    fulfillment_scope: FulfillmentScope::Missing,
                    revision: 1,
                    created_at: build.created_at,
                    updated_at: build.created_at,
                });
        }
        self.virtual_producers.insert(build.id);
        self.records.producers.push(build);
    }

    /// The current sourcing of `type_id` across the plan's reachable
    /// consumers (`None` when no current consumer has it).
    fn current_sourcing(&self, type_id: i64) -> Option<CurrentSourcing> {
        let reachable = reachable_builds(&self.current).ok()?;
        let mut produce = false;
        let mut buy = false;
        for edge in &self.current.dependencies {
            if edge.component_type_id == type_id && reachable.contains(&edge.consumer_build_id) {
                match edge.sourcing {
                    DependencySourcing::Buy => buy = true,
                    DependencySourcing::Produce { .. } => produce = true,
                }
            }
        }
        match (buy, produce) {
            (false, false) => None,
            (true, false) => Some(CurrentSourcing::Buy),
            (false, true) => Some(CurrentSourcing::Produce),
            (true, true) => Some(CurrentSourcing::Mixed),
        }
    }

    fn current_edge(
        &self,
        consumer: BuildId,
        type_id: i64,
    ) -> Option<&PersistedProductionDependency> {
        self.current
            .dependencies
            .iter()
            .find(|edge| edge.consumer_build_id == consumer && edge.component_type_id == type_id)
    }
}

/// Root plus every producer reachable from it through active Produce edges.
fn reachable_builds(records: &RootPlanRecords) -> Result<HashSet<BuildId>, IndustryError> {
    let graph = RootPlanDependencyGraph::from_persisted(records)
        .map_err(|error| IndustryError::Persistence(error.to_string()))?;
    Ok(std::iter::once(records.root.id)
        .chain(
            graph
                .producers()
                .iter()
                .map(|producer| producer.producer)
                .filter(|id| !graph.is_detached(*id)),
        )
        .collect())
}

/// Make each Build's draft `component_resolutions` agree with its edges:
/// a Produce edge has a resolution (the original one when `original` had
/// it, else a plain one for the producer's recipe), a Buy edge has none.
fn sync_resolutions(records: &mut RootPlanRecords, original: Option<&RootPlanRecords>) {
    let recipes: HashMap<Uuid, RecipeSelection> = std::iter::once(&records.root)
        .chain(&records.producers)
        .map(|build| (build.id.0, recipe_selection_of(&build.recipe)))
        .collect();
    let mut produce: HashMap<Uuid, BTreeMap<i64, RecipeSelection>> = HashMap::new();
    let mut buy: HashMap<Uuid, BTreeSet<i64>> = HashMap::new();
    for edge in &records.dependencies {
        match (edge.sourcing, edge.producer_build_id) {
            (DependencySourcing::Produce { method }, producer) => {
                let recipe = producer
                    .and_then(|id| recipes.get(&id.0).copied())
                    .unwrap_or_else(|| method_selection(method));
                produce
                    .entry(edge.consumer_build_id.0)
                    .or_default()
                    .insert(edge.component_type_id, recipe);
            }
            (DependencySourcing::Buy, _) => {
                buy.entry(edge.consumer_build_id.0)
                    .or_default()
                    .insert(edge.component_type_id);
            }
        }
    }
    let original_resolution = |build: BuildId, type_id: i64| -> Option<ComponentResolution> {
        let original = original?;
        std::iter::once(&original.root)
            .chain(&original.producers)
            .find(|candidate| candidate.id == build)?
            .draft_planning
            .as_ref()?
            .input
            .component_resolutions
            .iter()
            .find(|resolution| resolution.type_id == type_id)
            .cloned()
    };
    for build in std::iter::once(&mut records.root).chain(records.producers.iter_mut()) {
        let id = build.id;
        let Some(draft) = build.draft_planning.as_mut() else {
            continue;
        };
        let bought = buy.get(&id.0);
        let produced = produce.get(&id.0);
        let resolutions = &mut draft.input.component_resolutions;
        resolutions
            .retain(|resolution| !bought.is_some_and(|set| set.contains(&resolution.type_id)));
        for (type_id, recipe) in produced.into_iter().flatten() {
            if resolutions
                .iter()
                .any(|resolution| resolution.type_id == *type_id)
            {
                continue;
            }
            resolutions.push(
                original_resolution(id, *type_id).unwrap_or(ComponentResolution {
                    type_id: *type_id,
                    recipe: *recipe,
                    facility_override: None,
                    blueprint_selection: None,
                }),
            );
        }
        resolutions.sort_by_key(|resolution| resolution.type_id);
    }
}

fn method_selection(method: ProductionMethod) -> RecipeSelection {
    match method {
        ProductionMethod::Manufacturing { blueprint_type_id } => {
            RecipeSelection::Manufacturing { blueprint_type_id }
        }
        ProductionMethod::Reaction {
            reaction_formula_type_id,
        } => RecipeSelection::Reaction {
            reaction_formula_type_id,
        },
    }
}

/// Whether a new producer may be proposed: reactions always; manufacturing
/// only with a synced blueprint unless the user lifted that rule.
#[must_use]
pub fn producer_eligible(template: &Build, options: &SourcingOptions) -> bool {
    if options.include_without_blueprint {
        return true;
    }
    match recipe_selection_of(&template.recipe) {
        RecipeSelection::Reaction { .. } => true,
        RecipeSelection::Manufacturing { .. } => template
            .draft_planning
            .as_ref()
            .and_then(|draft| draft.input.blueprint_selection.as_ref())
            .is_some_and(|selection| matches!(selection, BlueprintSelection::ObservedAsset { .. })),
    }
}

// ---------------------------------------------------------------------------
// Decision
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CurrentSourcing {
    Buy,
    Produce,
    /// Bought by some consumers, produced for others.
    Mixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourcingDecision {
    Produce,
    Buy,
    /// Leave every edge as it is now (no data, or excluded).
    Keep,
}

/// One component type's comparison, at the quantity the candidate plan
/// produces for it (after inventory).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentEvaluation {
    pub type_id: i64,
    pub type_name: String,
    pub recipe: RecipeSelection,
    pub quantity: u64,
    pub buy_unit: Option<Money>,
    pub produce_unit: Option<Money>,
    pub buy_total: Option<Decimal>,
    pub produce_total: Option<Decimal>,
    pub current: Option<CurrentSourcing>,
    pub decision: SourcingDecision,
    /// Producer depth below the root (the root's direct components are 1).
    pub depth: u32,
}

#[derive(Debug, Clone, Default)]
pub struct SourcingDecisions {
    pub decisions: BTreeMap<i64, SourcingDecision>,
    pub evaluations: Vec<ComponentEvaluation>,
    pub warnings: Vec<SourcingWarning>,
}

/// Decide Buy vs Produce bottom-up over the candidate plan's cost
/// projection. Each producer operation is costed with its own inputs
/// already decided, then compared, for all its consumers at once, against
/// what those consumers would pay to buy it.
#[must_use]
pub fn decide_sourcing(
    candidate: &CandidatePlan,
    cost: &BuildCostProjection,
    options: &SourcingOptions,
) -> SourcingDecisions {
    let mut state = DecideState {
        candidate,
        options,
        ops: cost.operations.iter().map(|op| (op.op_index, op)).collect(),
        boundaries: BTreeMap::new(),
        consumers_of: BTreeMap::new(),
        op_total: BTreeMap::new(),
        unit: BTreeMap::new(),
        depth: BTreeMap::new(),
        out: SourcingDecisions::default(),
    };
    for boundary in &cost.boundaries {
        state
            .boundaries
            .entry(boundary.op_index)
            .or_default()
            .push(boundary);
        if let Some(child) = boundary.child_op_index {
            state.consumers_of.entry(child).or_default().push(boundary);
        }
    }
    state.assign_depths();
    if state.ops.contains_key(&0) {
        state.total(0);
    }
    state
        .out
        .evaluations
        .sort_by_key(|evaluation| (evaluation.depth, evaluation.type_id));
    state.out
}

struct DecideState<'a> {
    candidate: &'a CandidatePlan,
    options: &'a SourcingOptions,
    ops: BTreeMap<u32, &'a crate::build_cost::OperationCostProjection>,
    boundaries: BTreeMap<u32, Vec<&'a BoundaryCostProjection>>,
    consumers_of: BTreeMap<u32, Vec<&'a BoundaryCostProjection>>,
    /// Each operation's total cost under the decisions below it.
    op_total: BTreeMap<u32, Option<Decimal>>,
    /// Per producer operation: its decided unit cost for consumers whose
    /// edge follows the decision (`Keep` resolves per edge, see `unit_for`).
    unit: BTreeMap<u32, (SourcingDecision, Option<Decimal>)>,
    depth: BTreeMap<u32, u32>,
    out: SourcingDecisions,
}

impl DecideState<'_> {
    fn assign_depths(&mut self) {
        let mut queue = std::collections::VecDeque::from([(0u32, 0u32)]);
        while let Some((op, depth)) = queue.pop_front() {
            if self.depth.contains_key(&op) {
                continue;
            }
            self.depth.insert(op, depth);
            for boundary in self.boundaries.get(&op).into_iter().flatten() {
                if let Some(child) = boundary.child_op_index {
                    queue.push_back((child, depth + 1));
                }
            }
        }
    }

    /// The operation's total cost with its inputs decided (memoized).
    fn total(&mut self, op_index: u32) -> Option<Decimal> {
        if let Some(total) = self.op_total.get(&op_index) {
            return *total;
        }
        // Guard re-entry on a malformed (cyclic) projection.
        self.op_total.insert(op_index, None);
        let op = self.ops.get(&op_index).copied()?;
        let mut total = op.own_installation.total.map(|money| money.0);
        let boundaries = self.boundaries.get(&op_index).cloned().unwrap_or_default();
        for boundary in boundaries {
            let cost = match boundary.kind {
                BoundaryCostKind::Buy | BoundaryCostKind::FullyCovered => {
                    boundary.requirement_cost.map(|money| money.0)
                }
                BoundaryCostKind::Unresolved => None,
                BoundaryCostKind::Build | BoundaryCostKind::Reaction => {
                    let child = boundary.child_op_index;
                    if let Some(child) = child {
                        self.decide(child);
                    }
                    let unit = child.and_then(|child| self.unit_for(child, op.build_id, boundary));
                    let inventory = boundary
                        .inventory_cost
                        .map_or(Decimal::ZERO, |money| money.0);
                    unit.map(|unit| {
                        inventory + unit * Decimal::from(boundary.child_consumed_quantity)
                    })
                }
            };
            total = match (total, cost) {
                (Some(total), Some(cost)) => Some(total + cost),
                _ => None,
            };
        }
        self.op_total.insert(op_index, total);
        total
    }

    /// The unit cost one consumer edge pays for the child's output.
    fn unit_for(
        &self,
        child: u32,
        consumer: BuildId,
        boundary: &BoundaryCostProjection,
    ) -> Option<Decimal> {
        let (decision, produce_unit) = *self.unit.get(&child)?;
        let buy_unit = boundary.market_unit_price.map(|money| money.0);
        match decision {
            SourcingDecision::Produce => produce_unit,
            SourcingDecision::Buy => buy_unit,
            SourcingDecision::Keep => {
                match self
                    .candidate
                    .current_edge(consumer, boundary.type_id)
                    .map(|edge| edge.sourcing)
                {
                    Some(DependencySourcing::Produce { .. }) => produce_unit,
                    _ => buy_unit,
                }
            }
        }
    }

    fn decide(&mut self, child: u32) {
        if self.unit.contains_key(&child) {
            return;
        }
        let Some(op) = self.ops.get(&child).copied() else {
            return;
        };
        let total = self.total(child);
        let produce_unit = match (total, op.produced_quantity) {
            (Some(total), produced) if produced > 0 => Some(total / Decimal::from(produced)),
            _ => None,
        };
        let consumers = self.consumers_of.get(&child).cloned().unwrap_or_default();
        let type_id = op.product_type_id;
        let type_name = consumers
            .first()
            .map_or_else(String::new, |boundary| boundary.type_name.clone());
        let quantity: u64 = consumers
            .iter()
            .map(|boundary| boundary.child_consumed_quantity)
            .sum();
        let buy_total = consumers.iter().try_fold(Decimal::ZERO, |sum, boundary| {
            boundary
                .market_unit_price
                .map(|price| sum + price.0 * Decimal::from(boundary.child_consumed_quantity))
        });
        let produce_total = produce_unit.map(|unit| unit * Decimal::from(quantity));
        let current = self.candidate.current_sourcing(type_id);
        let warn = |kind| SourcingWarning {
            type_id,
            type_name: type_name.clone(),
            kind,
        };
        let decision = if self.options.exclude.contains(&type_id)
            || self.candidate.exclude.contains(&type_id)
        {
            SourcingDecision::Keep
        } else {
            match (buy_total, produce_total) {
                (None, _) => {
                    self.out
                        .warnings
                        .push(warn(SourcingWarningKind::NoBuyPrice));
                    SourcingDecision::Keep
                }
                (Some(_), None) => {
                    self.out
                        .warnings
                        .push(warn(SourcingWarningKind::ProduceCostIncomplete));
                    SourcingDecision::Keep
                }
                (Some(buy), Some(produce)) => {
                    let keep_share =
                        Decimal::ONE - self.options.min_saving_percent / Decimal::ONE_HUNDRED;
                    match current {
                        Some(CurrentSourcing::Produce) => {
                            if buy <= produce * keep_share && buy < produce {
                                SourcingDecision::Buy
                            } else {
                                SourcingDecision::Produce
                            }
                        }
                        Some(CurrentSourcing::Mixed) => {
                            if produce <= buy {
                                SourcingDecision::Produce
                            } else {
                                SourcingDecision::Buy
                            }
                        }
                        // Bought now, or only reached through a new producer.
                        Some(CurrentSourcing::Buy) | None => {
                            if produce <= buy * keep_share && produce < buy {
                                SourcingDecision::Produce
                            } else {
                                SourcingDecision::Buy
                            }
                        }
                    }
                }
            }
        };
        self.unit.insert(child, (decision, produce_unit));
        self.out.decisions.insert(type_id, decision);
        let recipe = self.candidate.build(op.build_id).map_or_else(
            || method_of_activity(op),
            |build| recipe_selection_of(&build.recipe),
        );
        self.out.evaluations.push(ComponentEvaluation {
            type_id,
            type_name,
            recipe,
            quantity,
            buy_unit: (quantity > 0)
                .then(|| buy_total.map(|total| Money(total / Decimal::from(quantity))))
                .flatten(),
            produce_unit: produce_unit.map(Money),
            buy_total,
            produce_total,
            current,
            decision,
            depth: self.depth.get(&child).copied().unwrap_or(0),
        });
    }
}

fn method_of_activity(op: &crate::build_cost::OperationCostProjection) -> RecipeSelection {
    // Only reached for an operation whose Build is unknown to the candidate
    // (a malformed projection); the identity is then informational.
    match op.activity {
        crate::build_materials::MaterialActivity::Reaction => RecipeSelection::Reaction {
            reaction_formula_type_id: 0,
        },
        crate::build_materials::MaterialActivity::Manufacturing => RecipeSelection::Manufacturing {
            blueprint_type_id: 0,
        },
    }
}

/// The plan under `decisions`: decided types follow the decision on every
/// edge, `Keep` (and undecided) types return to their current sourcing, and
/// virtual producers no longer reached are dropped.
#[must_use]
pub fn chosen_records(
    candidate: &CandidatePlan,
    decisions: &BTreeMap<i64, SourcingDecision>,
) -> RootPlanRecords {
    let mut records = candidate.records.clone();
    for edge in &mut records.dependencies {
        let decision = decisions
            .get(&edge.component_type_id)
            .copied()
            .unwrap_or(SourcingDecision::Keep);
        match decision {
            SourcingDecision::Produce => {}
            SourcingDecision::Buy => {
                edge.sourcing = DependencySourcing::Buy;
                edge.producer_build_id = None;
            }
            SourcingDecision::Keep => {
                match candidate
                    .current
                    .dependencies
                    .iter()
                    .find(|current| current.id == edge.id)
                {
                    Some(current) => {
                        edge.sourcing = current.sourcing;
                        edge.producer_build_id = current.producer_build_id;
                    }
                    None => {
                        edge.sourcing = DependencySourcing::Buy;
                        edge.producer_build_id = None;
                    }
                }
            }
        }
    }
    // Drop virtual producers nothing reaches any more, with their edges.
    if let Ok(reachable) = reachable_builds(&records) {
        let dropped: HashSet<BuildId> = candidate
            .virtual_producers
            .iter()
            .filter(|id| !reachable.contains(id))
            .copied()
            .collect();
        records
            .producers
            .retain(|build| !dropped.contains(&build.id));
        records
            .dependencies
            .retain(|edge| !dropped.contains(&edge.consumer_build_id));
    }
    sync_resolutions(&mut records, Some(&candidate.current));
    records
}

// ---------------------------------------------------------------------------
// Proposal
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ProposedSourcing {
    Buy,
    Produce { recipe: RecipeSelection },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcingChange {
    pub type_id: i64,
    pub type_name: String,
    pub quantity: u64,
    /// `None`: the component is only reached through a new producer.
    pub from: Option<CurrentSourcing>,
    pub to: ProposedSourcing,
    pub buy_unit: Option<Money>,
    pub produce_unit: Option<Money>,
    /// What the chosen side saves against the other, at `quantity`
    /// (estimate from the candidate plan; the plan totals are exact).
    pub saving: Option<Money>,
    pub depth: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcingProposal {
    /// Identifies the plan state this proposal was computed from; Apply
    /// refuses a plan that changed since.
    pub fingerprint: String,
    pub expected_revenue: Option<Money>,
    pub current_cost: Option<Money>,
    pub proposed_cost: Option<Money>,
    pub current_margin: Option<Money>,
    pub proposed_margin: Option<Money>,
    /// `current_cost - proposed_cost`: the margin gained.
    pub delta: Option<Money>,
    pub changes: Vec<SourcingChange>,
    pub warnings: Vec<SourcingWarning>,
    /// Why nothing could be proposed (e.g. the candidate plan could not be
    /// fully projected), when that is not simply "already optimal".
    pub notice: Option<String>,
}

/// Assemble the proposal from the exact current and chosen plan costs. A
/// chosen plan that is not strictly cheaper proposes nothing.
#[must_use]
pub fn build_proposal(
    candidate: &CandidatePlan,
    decided: &SourcingDecisions,
    chosen: &RootPlanRecords,
    current_cost: Option<Money>,
    proposed_cost: Option<Money>,
    expected_revenue: Option<Money>,
) -> SourcingProposal {
    let mut warnings = candidate.warnings.clone();
    warnings.extend(decided.warnings.iter().cloned());
    warnings.sort_by_key(|warning| (warning.type_id, warning.kind as u8));
    warnings.dedup();

    let improves = matches!((current_cost, proposed_cost), (Some(current), Some(proposed)) if proposed.0 < current.0);
    let reachable = reachable_builds(chosen).unwrap_or_default();
    let in_chosen = |type_id: i64| {
        chosen.dependencies.iter().any(|edge| {
            edge.component_type_id == type_id && reachable.contains(&edge.consumer_build_id)
        })
    };
    let changes = if improves {
        decided
            .evaluations
            .iter()
            .filter(|evaluation| in_chosen(evaluation.type_id))
            .filter_map(|evaluation| {
                let to = match (evaluation.decision, evaluation.current) {
                    (SourcingDecision::Keep, _)
                    | (SourcingDecision::Produce, Some(CurrentSourcing::Produce))
                    | (SourcingDecision::Buy, Some(CurrentSourcing::Buy) | None) => return None,
                    (SourcingDecision::Produce, _) => ProposedSourcing::Produce {
                        recipe: evaluation.recipe,
                    },
                    (SourcingDecision::Buy, _) => ProposedSourcing::Buy,
                };
                let saving = match (&to, evaluation.buy_total, evaluation.produce_total) {
                    (ProposedSourcing::Buy, Some(buy), Some(produce)) => Some(Money(produce - buy)),
                    (ProposedSourcing::Produce { .. }, Some(buy), Some(produce)) => {
                        Some(Money(buy - produce))
                    }
                    _ => None,
                };
                Some(SourcingChange {
                    type_id: evaluation.type_id,
                    type_name: evaluation.type_name.clone(),
                    quantity: evaluation.quantity,
                    from: evaluation.current,
                    to,
                    buy_unit: evaluation.buy_unit.map(|unit| round_money(unit, 4)),
                    produce_unit: evaluation.produce_unit.map(|unit| round_money(unit, 4)),
                    saving: saving.map(|saving| round_money(saving, 2)),
                    depth: evaluation.depth,
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    let margin = |cost: Option<Money>| match (expected_revenue, cost) {
        (Some(revenue), Some(cost)) => Some(Money(revenue.0 - cost.0)),
        _ => None,
    };
    SourcingProposal {
        fingerprint: plan_fingerprint(&candidate.current),
        expected_revenue,
        current_cost,
        proposed_cost: if improves {
            proposed_cost
        } else {
            current_cost
        },
        current_margin: margin(current_cost),
        proposed_margin: margin(if improves {
            proposed_cost
        } else {
            current_cost
        }),
        delta: match (current_cost, proposed_cost) {
            (Some(current), Some(proposed)) if improves => Some(Money(current.0 - proposed.0)),
            (Some(_), Some(_)) => Some(Money::zero()),
            _ => None,
        },
        changes,
        warnings,
        notice: None,
    }
}

/// Display rounding for derived per-unit figures (plan totals stay exact).
fn round_money(value: Money, decimal_places: u32) -> Money {
    Money(value.0.round_dp(decimal_places))
}

/// A stable digest of the root's and every producer's `(id, revision)`.
#[must_use]
pub fn plan_fingerprint(records: &RootPlanRecords) -> String {
    let mut builds: Vec<(Uuid, u64)> = std::iter::once(&records.root)
        .chain(&records.producers)
        .map(|build| (build.id.0, build.revision))
        .collect();
    builds.sort_unstable();
    let mut hasher = Sha256::new();
    for (id, revision) in builds {
        hasher.update(id.as_bytes());
        hasher.update(revision.to_le_bytes());
    }
    hasher
        .finalize()
        .iter()
        .take(16)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The producer key a new producer of `type_id` would get.
#[must_use]
pub fn producer_key(type_id: i64, recipe: RecipeSelection) -> CanonicalProducerKey {
    CanonicalProducerKey {
        output_type_id: type_id,
        method: recipe.into(),
    }
}

// ---------------------------------------------------------------------------
// Apply
// ---------------------------------------------------------------------------

/// One component's chosen sourcing, as Apply receives it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcingApplyChange {
    pub type_id: i64,
    pub sourcing: ProposedSourcing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcingApplyFailure {
    pub type_id: i64,
    pub build_id: BuildId,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcingApplyOutcome {
    pub applied: Vec<i64>,
    pub failed: Vec<SourcingApplyFailure>,
    pub root_revision: u64,
}

/// The edits one Build needs: its distinct recipe materials whose current
/// draft sourcing differs from `wanted`.
fn consumer_edits(
    build: &Build,
    wanted: &BTreeMap<i64, Option<RecipeSelection>>,
) -> Vec<(i64, Option<RecipeSelection>)> {
    let current: BTreeMap<i64, RecipeSelection> = build
        .draft_planning
        .as_ref()
        .map(|draft| {
            draft
                .input
                .component_resolutions
                .iter()
                .map(|resolution| (resolution.type_id, resolution.recipe))
                .collect()
        })
        .unwrap_or_default();
    let mut seen = BTreeSet::new();
    build
        .recipe
        .materials()
        .iter()
        .filter(|line| seen.insert(line.type_id))
        .filter_map(|line| {
            let target = wanted.get(&line.type_id)?;
            (current.get(&line.type_id) != target.as_ref()).then_some((line.type_id, *target))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

impl IndustryService {
    /// Build the in-memory all-produce candidate of root plan `root`.
    pub async fn sourcing_candidate(
        &self,
        workspace_id: WorkspaceId,
        root: BuildId,
        options: &SourcingOptions,
    ) -> Result<CandidatePlan, IndustryError> {
        let repository = self.build_repository();
        match repository.plan_root_of(workspace_id, root).await? {
            Some(plan_root) if plan_root != root => {
                return Err(IndustryError::Validation(
                    "Sourcing can only be optimized from the plan's root Build.".to_string(),
                ));
            }
            _ => {}
        }
        let current = repository.load_root_plan(workspace_id, root).await?;
        let mut candidate = CandidatePlan::new(current, options);
        let defaults = self.load_facility_defaults(workspace_id).await?;
        loop {
            let pending = candidate.pending()?;
            if pending.is_empty() {
                break;
            }
            let type_ids: Vec<i64> = pending.iter().map(|component| component.type_id).collect();
            let recipes = self
                .sde_repository
                .production_recipes_for_products(&type_ids)
                .await
                .map_err(|error| IndustryError::StaticData(error.to_string()))?;
            for component in pending {
                if let Some(existing) = candidate.existing_producer(component.type_id)? {
                    candidate.attach(&component, CandidateProducer::Existing(existing));
                    continue;
                }
                let method = recipes.get(&component.type_id).and_then(|refs| {
                    refs.blueprint_type_id
                        .map(|blueprint_type_id| RecipeSelection::Manufacturing {
                            blueprint_type_id,
                        })
                        .or_else(|| {
                            refs.reaction_formula_type_id
                                .map(|reaction_formula_type_id| RecipeSelection::Reaction {
                                    reaction_formula_type_id,
                                })
                        })
                });
                let Some(method) = method else {
                    candidate.attach(&component, CandidateProducer::Skip(None));
                    continue;
                };
                let Some(consumer) = candidate.build(component.consumer).cloned() else {
                    candidate.attach(&component, CandidateProducer::Skip(None));
                    continue;
                };
                let draft = consumer.draft_planning.as_ref().map(|draft| &draft.input);
                let template = match self
                    .new_producer_template(
                        &consumer,
                        draft,
                        producer_key(component.type_id, method),
                        Some(&defaults),
                    )
                    .await
                {
                    Ok(template) => template,
                    Err(_) => {
                        candidate.attach(
                            &component,
                            CandidateProducer::Skip(Some(SourcingWarningKind::RecipeUnavailable)),
                        );
                        continue;
                    }
                };
                let producer = if producer_eligible(&template, options) {
                    CandidateProducer::New(Box::new(template))
                } else {
                    CandidateProducer::Skip(Some(SourcingWarningKind::NoBlueprint))
                };
                candidate.attach(&component, producer);
            }
        }
        Ok(candidate)
    }

    /// Project a plan held in `records` (persisted, candidate or chosen)
    /// from its root, as the Build plan views do.
    pub async fn project_plan_records(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        records: &RootPlanRecords,
        inventory: &mut PlanningInventory,
    ) -> Result<BuildMaterialsProjection, IndustryError> {
        let command = self
            .reconstruct_preview_command(workspace_id, &records.root)
            .await?
            .ok_or_else(|| {
                IndustryError::Validation(
                    "The plan's root Build has no planning input.".to_string(),
                )
            })?;
        let start = self.counters().snapshot();
        let mut projection = self
            .project_canonical_records(workspace_id, owner_id, &command, inventory, true, records)
            .await?;
        projection.metrics = self.counters().snapshot().since(start);
        Ok(projection)
    }

    /// Apply chosen sourcing to root plan `root`, top-down: the root's
    /// edges first, then each producer reachable after that write,
    /// breadth-first -- so a producer created by a Buy -> Produce switch
    /// gets its own inputs' sourcing in the same Apply. Each Build is one
    /// atomic canonical write; a failed write is reported, not rolled back
    /// (every committed step is a valid plan). Refuses with
    /// `RevisionConflict` when the plan changed since `fingerprint`.
    pub async fn apply_sourcing_changes(
        &self,
        workspace_id: WorkspaceId,
        owner_id: OwnerId,
        root: BuildId,
        fingerprint: &str,
        changes: &[SourcingApplyChange],
    ) -> Result<SourcingApplyOutcome, IndustryError> {
        let repository = self.build_repository();
        if let Some(plan_root) = repository.plan_root_of(workspace_id, root).await? {
            if plan_root != root {
                return Err(IndustryError::Validation(
                    "Sourcing can only be optimized from the plan's root Build.".to_string(),
                ));
            }
        }
        let records = repository.load_root_plan(workspace_id, root).await?;
        if records.root.owner_id != owner_id {
            return Err(IndustryError::BuildNotFound);
        }
        if plan_fingerprint(&records) != fingerprint {
            return Err(IndustryError::RevisionConflict);
        }
        let wanted: BTreeMap<i64, Option<RecipeSelection>> = changes
            .iter()
            .map(|change| {
                let recipe = match change.sourcing {
                    ProposedSourcing::Buy => None,
                    ProposedSourcing::Produce { recipe } => Some(recipe),
                };
                (change.type_id, recipe)
            })
            .collect();

        let mut applied: BTreeSet<i64> = BTreeSet::new();
        let mut failed: Vec<SourcingApplyFailure> = Vec::new();
        let mut visited: HashSet<BuildId> = HashSet::new();
        let mut queue = std::collections::VecDeque::from([root]);
        let mut records = records;
        while let Some(build_id) = queue.pop_front() {
            if !visited.insert(build_id) {
                continue;
            }
            let Some(build) = std::iter::once(&records.root)
                .chain(&records.producers)
                .find(|build| build.id == build_id)
                .cloned()
            else {
                continue;
            };
            let edits = consumer_edits(&build, &wanted);
            if !edits.is_empty() {
                match self
                    .set_component_sourcing(workspace_id, build_id, build.revision, &edits)
                    .await
                {
                    Ok(_) => applied.extend(edits.iter().map(|(type_id, _)| *type_id)),
                    Err(error) => {
                        failed.extend(edits.iter().map(|(type_id, _)| SourcingApplyFailure {
                            type_id: *type_id,
                            build_id,
                            error: error.to_string(),
                        }))
                    }
                }
                records = repository.load_root_plan(workspace_id, root).await?;
            }
            let mut producers: Vec<BuildId> = records
                .dependencies
                .iter()
                .filter(|edge| {
                    edge.consumer_build_id == build_id
                        && matches!(edge.sourcing, DependencySourcing::Produce { .. })
                })
                .filter_map(|edge| edge.producer_build_id)
                .collect();
            producers.sort_by_key(|id| id.0);
            queue.extend(producers);
        }
        for failure in &failed {
            applied.remove(&failure.type_id);
        }
        Ok(SourcingApplyOutcome {
            applied: applied.into_iter().collect(),
            failed,
            root_revision: records.root.revision,
        })
    }
}
