// Build Graph projection -- the read-only recursive view of a saved
// Build's linked-build hierarchy under the editor's live planning overlay.
// Mirrors `crates/iskworks-core/src/build_graph.rs` exactly.
//
// Two discriminants that look alike and must NOT be conflated:
//   - `GraphChild.nodeKind`   -> production | acquisition | unresolvedBuild |
//                                producerReference
//   - `ProductionNode.kind`   -> rootManufacturing | rootReaction |
//                                manufacturing | reaction
//
// Graph node ids:
//   root:<rootBuildId>                     the root production node
//   build:<linkedBuildId>                  a linked production node, any depth
//   buy:<parentBuildId>:<componentTypeId>  a direct Buy/acquisition requirement
//                                          of that Build (buildable or raw),
//                                          and the Unresolved slot it becomes
//                                          when switched to Build before its
//                                          linked Build exists

import type { PreviewBuildPlanCommand, RecipeSelection } from "./builds";
import type { MarketScopeEvidence } from "./market";
import { json, request } from "./request";
import type { Money, RecipeCurrency } from "./shared";

export type { MarketScopeEvidence } from "./market";

export type ProductionKind =
  | "rootManufacturing"
  | "rootReaction"
  | "manufacturing"
  | "reaction";

export type CostState =
  | "notComputed"
  | "known"
  | "incomplete"
  | "stale"
  | "unresolved";

export type GraphWarningCode =
  | "marketPriceUnavailable"
  | "linkedBuildUnresolved"
  | "staleRecipe"
  | "runsDiverged"
  | "costIncomplete"
  | "staleMarketEvidence";

export interface GraphWarning {
  graphNodeId: string;
  code: GraphWarningCode;
  message: string;
}

export interface ProductionNode {
  graphNodeId: string;
  buildId: string;
  parentBuildId: string | null;
  parentComponentTypeId: number | null;
  typeId: number;
  typeName: string;
  kind: ProductionKind;
  recipe: RecipeSelection;
  /** The PROJECTED (dynamic) run count this plan currently
   * requires -- never the persisted `Build.runs` (see `persistedRuns`).
   * Opening this linked Build on its own still shows its own persisted
   * runs; this field is this graph's live plan only. */
  runs: number;
  /** This node's own persisted `Build.runs` -- purely informational
   * (see the `runsDiverged` warning), never the quantity/cost authority. */
  persistedRuns: number;
  /** For a canonical producer serving several demand edges these are the
   * AGGREGATE over every incoming edge (its sizing basis), so `surplus` is
   * the operation's one surplus; each consumer's own share is in
   * `incomingDemands`. */
  requiredQuantity: number | null;
  netRequiredQuantity: number | null;
  /** Canonical producers: every demand edge this operation serves when it
   * serves more than one (absent otherwise). */
  incomingDemands?: GraphIncomingDemand[];
  producingQuantity: number;
  surplus: number;
  /** Total production cost (material + this node's own installation),
   * from `OperationCostProjection.total_production_cost` -- `null` unless
   * BOTH `materialComponentCost` and `ownInstallationCost` are known. */
  estimatedCost: Money | null;
  /** This node's own material cost only (consumed inventory + fresh Buy +
   * consumed child production cost) -- `null` if any of its own boundaries,
   * or a Build/Reaction child it consumes, has an incomplete cost. Additive
   * with `ownInstallationCost`, never double-counted with descendants. */
  materialComponentCost: Money | null;
  /** This node's own installation/job cost only (EIV-based), excluding
   * every descendant's. `null` when incomplete (e.g. no facility, no
   * adjusted price, no system cost index). */
  ownInstallationCost: Money | null;
  costState: CostState;
  recipeCurrency: RecipeCurrency;
  /** Effective blueprint material efficiency of this linked Build's own
   * preview (the same the worksheet uses -- `Manual` value, or the owned
   * blueprint's ME for an `observedAsset` selection). `null` for a
   * reaction node, the root, or when the per-node snapshot failed. */
  effectiveMe: number | null;
  /** Effective blueprint time efficiency, same provenance. Always `null`
   * for a reaction node. */
  effectiveTe: number | null;
  /** Every direct requirement of this operation: a linked `production`
   * node, an `unresolvedBuild` slot, or an `acquisition` node (buildable or
   * raw). A component is never both a production child and an acquisition
   * child. Collapsed by default for non-root nodes -- see `useBuildGraph`. */
  children: GraphChild[];
}

/** A direct BUY/acquisition dependency of a Production node -- first-class
 * whether or not it is itself buildable. `buildableRecipe` is the only
 * capability flag: `null` -> terminal (no "Switch to BUILD"). */
export interface AcquisitionNode {
  graphNodeId: string;
  /** The Build that owns this requirement -- the target of BUY->BUILD. */
  parentBuildId: string;
  typeId: number;
  typeName: string;
  requiredQuantity: number;
  missingQuantity: number;
  buildableRecipe: RecipeSelection | null;
  estimatedCost: Money | null;
  costState: CostState;
  warning: GraphWarning | null;
}

export interface UnresolvedBuildNode {
  graphNodeId: string;
  parentBuildId: string;
  typeId: number;
  typeName: string;
  recipe: RecipeSelection;
  requiredQuantity: number;
  netRequiredQuantity: number;
}

/** One consumer's demand edge into a shared canonical producer. */
export interface GraphIncomingDemand {
  dependencyId: string;
  consumerBuildId: string;
  requiredQuantity: number;
  netRequiredQuantity: number;
}

/** Canonical producers: this consumer's demand edge is served by a producer
 * drawn once, in full, under another consumer. Same `graphNodeId` /
 * `buildId` as that one production node -- an alias carrying only this
 * edge's own requirement, never the operation's runs/output/surplus/cost. */
export interface ProducerReferenceNode {
  graphNodeId: string;
  buildId: string;
  parentBuildId: string;
  dependencyId: string;
  typeId: number;
  typeName: string;
  kind: ProductionKind;
  requiredQuantity: number;
  netRequiredQuantity: number;
}

export type GraphChild =
  | ({ nodeKind: "production" } & ProductionNode)
  | ({ nodeKind: "acquisition" } & AcquisitionNode)
  | ({ nodeKind: "unresolvedBuild" } & UnresolvedBuildNode)
  | ({ nodeKind: "producerReference" } & ProducerReferenceNode);

export interface BuildGraphProjection {
  root: ProductionNode;
  warnings: GraphWarning[];
  generatedAt: string;
  /** May be empty when no scope had any observed/imported market data. */
  marketEvidence: MarketScopeEvidence[];
}

/**
 * `POST /api/builds/:build_id/graph`. Read-only -- never creates or mutates
 * a Build. Send the same planning overlay a preview sends (the editor's
 * `previewKey`, parsed). The path id is authoritative; `command.buildId` is
 * ignored server-side.
 */
export function previewBuildGraph(
  buildId: string,
  command: PreviewBuildPlanCommand,
  signal?: AbortSignal,
): Promise<BuildGraphProjection> {
  return request(`/api/builds/${buildId}/graph`, {
    ...json("POST", command),
    signal,
  });
}
