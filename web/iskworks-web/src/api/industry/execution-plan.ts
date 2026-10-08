// Execution Plan projection -- a staged, occurrence-preserving production
// dependency graph derived from the same authoritative allocation-aware
// evidence Materials/Graph already use. Mirrors
// `crates/iskworks-core/src/execution_plan.rs` exactly. Read this module's doc comments alongside that file's module
// doc for the full "why" -- the short version:
//
//   - `stage` is DEPENDENCY ORDER, never readiness. Stage 1 does not mean
//     "ready now" -- there is no reservation/blueprint-availability/execution
//     -access evidence behind it yet.
//   - `ExecutionNode` may represent MORE THAN ONE authoritative production
//     occurrence (`occurrenceIds.length > 1`) when the backend judged them
//     compatible AND mutually unrelated (never an ancestor/descendant pair).
//     Every quantity/cost field on a grouped node is a plain SUM of its
//     member occurrences' own already-computed values -- never a
//     recomputation. In particular `projectedRuns` on a grouped node is
//     `Σ occurrence.projectedRuns`, NEVER `ceil(totalDemand / outputPerRun)`.
//     Do not recompute anything client-side; render exactly what the
//     endpoint returns.
//   - `occurrenceIds` / `occurrences[].id` (`graphNodeId`-shaped: `root:<uuid>`
//     / `build:<uuid>`) are the durable per-render identity. `ExecutionNode.id`
//     is a *display* id (the group's smallest-occurrence anchor) -- stable
//     within one render, never a persisted identity.
//   - `acquisitions` is a Buy-only external-shortage rollup. It is
//     deliberately NOT the same thing as `AggregateMaterialLine` (Materials'
//     whole-tree rollup, which also folds in Build/Reaction-resolved
//     contributions of the same type and can be `mixed`) -- a `mixed`
//     `sourceStrategy` here means "this type is ALSO produced elsewhere in
//     the tree", never a claim about the acquisition amount itself.

import type { Build, BlueprintSelection, RecipeSelection, PreviewBuildPlanCommand } from "./builds";
import type { MaterialRowStrategy } from "./build-materials";
import { json, request } from "./request";
import type { Money } from "./shared";

export type MaterialActivity = "manufacturing" | "reaction";
export type RequirementResolution = "buy" | "build" | "reaction" | "unresolved";

export interface ExecutionRequirement {
  typeId: number;
  typeName: string;
  requiredQuantity: number;
  plannedInventoryQuantity: number;
  shortageQuantity: number;
  fulfillmentScope: "missing" | "full";
  resolution: RequirementResolution;
  dependencyId: string;
  producerBuildId: string | null;
  producerNodeId: string | null;
}

/** Reuses `crates/iskworks-core/src/build_cost.rs`'s `CostWarning` verbatim
 * (internally tagged on `code`) -- Execution Plan does not invent a second
 * warning taxonomy. */
export type CostWarning =
  | { code: "missingFreshPrice"; opIndex: number; traversalIndex: number; typeId: number }
  | { code: "missingInventoryBasis"; opIndex: number; traversalIndex: number; typeId: number }
  | { code: "missingAdjustedPrice"; opIndex: number; typeIds: number[] }
  | { code: "missingSystemCostIndex"; opIndex: number }
  | { code: "noFacilitySelected"; opIndex: number }
  | { code: "unresolvedBuild"; opIndex: number; traversalIndex: number; typeId: number }
  | { code: "childCostIncomplete"; opIndex: number; traversalIndex: number; childOpIndex: number }
  | { code: "staleFreshPrice"; opIndex: number; traversalIndex: number; typeId: number }
  | { code: "arithmeticOverflow"; opIndex: number };

export interface ExecutionStage {
  index: number;
  /** `ExecutionNode.id`s at this stage, already in the endpoint's
   * deterministic order. No ordering within a stage implies execution
   * priority -- same-stage nodes are topologically parallel. */
  nodeIds: string[];
}

/** One display node: either a single authoritative production occurrence,
 * or several compatible, mutually-unrelated occurrences grouped for
 * readability. Every quantity/cost field is a straight sum of the
 * authoritative values already computed for `occurrenceIds` -- never
 * re-derived, re-rounded, or reroutable client-side. */
export interface ExecutionNode {
  /** Deterministic within one render (the group's smallest-occurrence
   * anchor's `graphNodeId`). Never a persisted identity. */
  id: string;
  outputTypeId: number;
  outputTypeName: string;
  activity: MaterialActivity;
  /** Dependency order only -- see this module's own doc comment. */
  stage: number;
  /** Every member occurrence's `id`, ascending by the backend's own walk
   * order. Drill-down evidence -- look these up in
   * `ExecutionPlanProjection.occurrences`. */
  occurrenceIds: string[];
  facilityId: string | null;
  facilityName: string | null;
  effectiveMe: number | null;
  effectiveTe: number | null;
  /** Σ member `requiredQuantity`. */
  requiredQuantity: number;
  /** Σ member `plannedInventoryQuantity`. */
  plannedInventoryQuantity: number;
  /** Σ member `productionDemand`. */
  productionDemand: number;
  /** Σ member `projectedOutput` -- display evidence only. */
  projectedOutput: number;
  /** Σ member `projectedRuns` -- display evidence only. NEVER
   * `ceil(totalDemand / outputPerRun)`; never recompute this. */
  projectedRuns: number;
  /** Σ member `retainedSurplusQuantity`. Known independent of cost
   * completeness -- see `retainedSurplusCost`. */
  retainedSurplusQuantity: number;
  /** `null` whenever `costComplete` is `false` (never a partially-summed
   * figure with a silent gap) -- distinct from `retainedSurplusQuantity`,
   * which is quantity truth and always known. */
  retainedSurplusCost: Money | null;
  materialComponentCost: Money | null;
  ownInstallationCost: Money | null;
  totalProductionCost: Money | null;
  /** `true` iff every member occurrence's own cost evidence is complete.
   * `false` forces every `Money | null` field above to `null`. */
  costComplete: boolean;
  /** Every occurrence-edge OUT of this node to a consuming occurrence --
   * "Used By". Not collapsed by quantity: two distinct consumers (or the
   * same consumer node reached via two distinct consuming occurrences)
   * each keep their own entry. */
  consumers: ExecutionConsumerRef[];
  /** Every published production recipe for
   * `outputTypeId` (manufacturing and/or reaction) -- the methods a consumer
   * may switch this component between. */
  productionMethods: RecipeSelection[];
  /** `total / output` for a single-occurrence row (display evidence only;
   * `null` for a grouped row or incomplete cost). */
  unitProductionCost: Money | null;
  /** The output type's whole-tree starting stock (`0` if not a material). */
  availableQuantity: number;
}

export interface ExecutionConsumerRef {
  /** The consuming occurrence's own `ExecutionNode.id`. */
  nodeId: string;
  /** The specific consuming occurrence's id -- never lost to node-level
   * grouping even when several occurrences share `nodeId`. */
  occurrenceId: string;
  /** The quantity that specific consuming occurrence drew from this
   * node's specific producing occurrence -- the edge's production demand. */
  quantity: number;
  /** The demand edge -- the consuming Build whose
   * sourcing a Plan change targets, and the edge's own requirement. */
  buildId: string;
  dependencyId: string;
  fulfillmentScope: "missing" | "full";
  requiredQuantity: number;
  plannedInventoryQuantity: number;
}

/** A deduplicated prerequisite -> consumer edge between two display nodes.
 * Per-occurrence quantities are never lost here -- see each node's own
 * `consumers`. */
export interface ExecutionEdge {
  from: string;
  to: string;
}

/** One authoritative production occurrence -- never replaced or altered by
 * grouping. This is the drill-down evidence behind every grouped
 * `ExecutionNode`. */
export interface ExecutionOccurrence {
  /** The durable per-render identity (`graphNodeId`-shaped). Never an
   * ephemeral index. */
  id: string;
  /** The `ExecutionNode.id` this occurrence currently belongs to. */
  nodeId: string;
  buildId: string;
  isRoot: boolean;
  stage: number;
  activity: MaterialActivity;
  outputTypeId: number;
  outputTypeName: string;
  blueprintOrFormulaTypeId: number;
  /** The blueprint/formula's own display name -- the Stages descendant-
   * configuration editor's own identity display. */
  blueprintOrFormulaName: string;
  facilityId: string | null;
  facilityName: string | null;
  effectiveMe: number | null;
  effectiveTe: number | null;
  /** The raw, persisted blueprint selection (mode/kind/licensed runs/
   * notes) -- not just the already-resolved `effectiveMe`/`effectiveTe`
   * above. `null` for a reaction or an unresearched manufacturing Build.
   * The Stages descendant-configuration editor's own edit-seed evidence:
   * an ME/TE-only edit must never silently drop a field (e.g. a BPC's
   * `licensedRuns`) it never had a chance to show. */
  blueprintSelection: BlueprintSelection | null;
  /** `buildId`'s own current
   * `Build.revision` -- the optimistic-concurrency token a descendant-
   * configuration edit must echo back per member (see
   * `updateDescendantProductionConfiguration`). */
  revision: number;
  /** This occurrence's own projected runs. Display evidence only. */
  projectedRuns: number;
  /** This occurrence's own projected output. */
  projectedOutput: number;
  /** The full requirement its parent recorded for it (`0` for the root --
   * nothing external consumes it). */
  requiredQuantity: number;
  /** Inventory reused against that requirement (`0` for the root). */
  plannedInventoryQuantity: number;
  /** Production demand handed to this occurrence by its parent (`0` for
   * the root). */
  productionDemand: number;
  /** `projectedOutput - productionDemand`, read verbatim from the parent's
   * own evidence (`0` for the root -- no consumer, no surplus concept).
   * Known independent of cost completeness. */
  retainedSurplusQuantity: number;
  /** The cost BASIS of that surplus -- `null` for the root (not unknown,
   * simply not applicable) and `null` whenever `costComplete` is `false`. */
  retainedSurplusCost: Money | null;
  materialComponentCost: Money | null;
  ownInstallationCost: Money | null;
  totalProductionCost: Money | null;
  /** Display evidence only. */
  unitProductionCost: Money | null;
  costComplete: boolean;
  /** Direct requirements owned by this occurrence, never producer-wide aggregates. */
  requirements: ExecutionRequirement[];
}

/** One production occurrence's own contribution to an `AcquisitionLine`'s
 * shortage -- "which occurrence needs this purchase, and how much." A
 * straight re-key of the same `NodeMaterialAllocation` evidence every other
 * consumer edge already uses (never a second planning walk, a recipe
 * re-expansion, or a reallocation). `quantity` is that occurrence's own
 * `shortageQuantity` (its external shortfall for this type) -- NOT its
 * gross requirement -- because that is the one figure that provably
 * reconciles: `Σ consumers[].quantity == AcquisitionLine.shortageQuantity`
 * by construction. No proportional split is invented anywhere. */
export interface AcquisitionConsumerRef {
  /** The consuming occurrence's own `ExecutionNode.id`. */
  nodeId: string;
  /** The specific consuming occurrence's id -- never lost to node-level
   * grouping even when several occurrences share `nodeId`. */
  occurrenceId: string;
  /** This occurrence's own shortage contribution -- see this field's own
   * doc comment above for why this, not gross required quantity. */
  quantity: number;
  /** The demand edge itself -- the consuming Build a
   * sourcing change targets, and that edge's own requirement evidence. */
  buildId: string;
  dependencyId: string;
  fulfillmentScope: "missing" | "full";
  requiredQuantity: number;
  plannedInventoryQuantity: number;
  /** This edge's fresh (to-buy) cost and unit price,
   * `null` when unpriced. */
  freshCost: Money | null;
  freshUnitPrice: Money | null;
}

/** One external (`Buy`-resolution) shortage row. See this module's own doc
 * comment for why this is not `AggregateMaterialLine`. */
export interface AcquisitionLine {
  typeId: number;
  typeName: string;
  requiredQuantity: number;
  plannedInventoryQuantity: number;
  /** Always `> 0` -- a fully inventory-covered row never appears here. */
  shortageQuantity: number;
  /** The type's whole-tree starting inventory, read through from the
   * canonical Materials rollup -- identical regardless of source strategy. */
  availableQuantity: number;
  /** Informational only. `mixed` means this type is ALSO produced
   * elsewhere in the tree -- the quantities above are always Buy-only
   * regardless of this value. */
  sourceStrategy: MaterialRowStrategy;
  /** Every occurrence that contributes to this line's shortage, ascending
   * by `(nodeId, occurrenceId)`. "Used By" evidence for the Stages
   * acquisition inspector -- see `AcquisitionConsumerRef`'s own doc
   * comment for quantity semantics. */
  consumers: AcquisitionConsumerRef[];
  /** The production methods this Buy requirement may
   * switch to (empty: buy-only). */
  productionMethods: RecipeSelection[];
  /** The estimated fresh (to-buy) cost of the whole
   * shortage -- `null` when any consumer's price is unknown (never partial). */
  freshCost: Money | null;
  /** The unit price when every priced consumer uses the same one. */
  freshUnitPrice: Money | null;
  freshPriceStale: boolean;
}

/** A `Build`/`Reaction`-intended component with no linked `Build` yet --
 * a truthful placeholder, never a fabricated descendant. */
export interface ExecutionUnresolvedPrerequisite {
  /** The `ExecutionNode.id` of the occurrence that owns this requirement. */
  owningNodeId: string;
  owningOccurrenceId: string;
  typeId: number;
  typeName: string;
  intendedRecipe: RecipeSelection | null;
  requiredQuantity: number;
  netRequiredQuantity: number;
}

export interface ExecutionPlanProjection {
  /** The `ExecutionNode.id` of the group containing the root operation. */
  rootNodeId: string;
  /** Ascending, SPARSE -- a stage index with no node at it is omitted
   * rather than emitted empty. */
  stages: ExecutionStage[];
  /** Every display node, in the endpoint's deterministic order. */
  nodes: ExecutionNode[];
  /** Deduplicated prerequisite -> consumer edges between display nodes. */
  edges: ExecutionEdge[];
  /** Every authoritative production occurrence, ascending by the backend's
   * own walk order. */
  occurrences: ExecutionOccurrence[];
  /** Only `shortageQuantity > 0` rows appear. */
  acquisitions: AcquisitionLine[];
  unresolved: ExecutionUnresolvedPrerequisite[];
  /** Mirrors the underlying cost projection's own completeness verbatim. */
  complete: boolean;
  warnings: CostWarning[];
  generatedAt: string;
  /** The facility-aware Logistics plan over the same
   * walk's allocations (`crates/iskworks-core/src/logistics.rs`). */
  logistics: LogisticsPlan;
}

export type LogisticsSourceKind = "acquire" | "produced" | "unresolved";

export interface LogisticsConsumerRef {
  /** The consuming operation's `graphNodeId`. */
  operationId: string;
  buildId: string;
  outputTypeName: string;
  source: LogisticsSourceKind;
  fulfillmentScope: "missing" | "full";
  requiredQuantity: number;
  plannedInventoryQuantity: number;
  shortageQuantity: number;
  dependencyId: string;
}

export interface LogisticsProducerRef {
  operationId: string;
  facilityId: string | null;
  facilityName: string | null;
  solarSystem: string | null;
  quantity: number;
}

/** One `(destination, type)` requirement. `quantity` is everything needed
 * at the destination; `shortageQuantity = quantity - plannedInventoryQuantity`
 * splits into `acquireQuantity` / `producedQuantity` / `unresolvedQuantity`.
 * `totalVolumeM3 = quantity x unitVolumeM3` (SDE packaged volume), `null`
 * when the SDE has no volume. */
export interface LogisticsLine {
  typeId: number;
  typeName: string;
  quantity: number;
  plannedInventoryQuantity: number;
  shortageQuantity: number;
  acquireQuantity: number;
  producedQuantity: number;
  unresolvedQuantity: number;
  unitVolumeM3: string | null;
  totalVolumeM3: string | null;
  consumers: LogisticsConsumerRef[];
  producers: LogisticsProducerRef[];
}

/** Everything needed at one facility -- the natural unit of a future
 * hauling ticket. Destination = the consuming operation's facility. */
export interface LogisticsDestination {
  /** `facility:<uuid>` or `unassigned`. */
  key: string;
  facilityId: string | null;
  facilityName: string | null;
  solarSystem: string | null;
  operationIds: string[];
  lines: LogisticsLine[];
  /** Σ known line volumes. */
  totalVolumeM3: string;
  /** `false` when a line's unit volume is unknown (total understates). */
  volumeComplete: boolean;
}

export interface LogisticsPlan {
  destinations: LogisticsDestination[];
  totalVolumeM3: string;
  volumeComplete: boolean;
}

/**
 * `POST /api/builds/:build_id/execution-plan`. Read-only -- never creates or
 * mutates a Build, and (like Materials) makes no `ProductionRepository`
 * calls. Send the same planning overlay the Worksheet / Materials / Graph
 * send (the editor's `previewKey`, parsed). The path id is authoritative;
 * `command.buildId` is ignored server-side.
 *
 * Ordinary incomplete cost evidence is never an HTTP failure -- the
 * response succeeds with `complete: false` and typed `warnings`.
 */
export function postBuildExecutionPlan(
  buildId: string,
  command: PreviewBuildPlanCommand,
  signal?: AbortSignal,
): Promise<ExecutionPlanProjection> {
  return request(`/api/builds/${buildId}/execution-plan`, {
    ...json("POST", command),
    signal,
  });
}

/** One canonical descendant Build a production operation represents, with
 * the revision last observed for it -- echo back `ExecutionOccurrence.buildId`
 * / `.revision` pairs exactly, never a client-invented id list. */
export interface DescendantConfigurationMember {
  buildId: string;
  expectedRevision: number;
}

export type DescendantProductionConfigurationRequest =
  | { kind: "facility"; facilityProfileId: string | null; estimatedItemValue?: string | null }
  | { kind: "blueprintSelection"; blueprintSelection: BlueprintSelection | null };

/**
 * Stages inspector's atomic, multi-Build descendant-configuration edit:
 * `PATCH /api/builds/:build_id/descendant-production-configuration`.
 * `buildId` is the ROOT Build (path-authoritative, mirrors
 * `postBuildExecutionPlan`) -- `members` must be descendant Builds only,
 * never the root itself.
 *
 * Applies `request` to every listed member in one transaction, after the
 * server re-validates that they still form exactly one current production
 * operation under `command`'s live overlay (the same body
 * `postBuildExecutionPlan` takes) -- a stale/changed membership set is
 * rejected as a curated 409 (`descendant_operation_membership_stale`)
 * rather than silently applied to whatever subset the client happened to
 * send. Returns every updated Build; the caller must re-fetch the plan
 * (`postBuildExecutionPlan`) separately -- this endpoint never returns a
 * fresh plan itself, and the response must never be used to locally patch
 * runs/output/surplus/materials/cost.
 */
export function updateDescendantProductionConfiguration(
  rootBuildId: string,
  input: {
    command: PreviewBuildPlanCommand;
    members: DescendantConfigurationMember[];
    request: DescendantProductionConfigurationRequest;
  },
): Promise<Build[]> {
  const { command, members, request: patch } = input;
  return request(
    `/api/builds/${rootBuildId}/descendant-production-configuration`,
    json("PATCH", { command, members, ...patch }),
  );
}
