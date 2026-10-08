// Build Materials aggregate -- the whole production plan's external-input
// demand, allocated **once** against current inventory by the backend's
// PlanningInventory. Mirrors `BuildMaterialsSummary`
// (`crates/iskworks-app/src/build_materials.rs`) and the allocator
// types (`crates/iskworks-core/src/build_materials.rs`).
//
// The frontend displays these quantities verbatim -- it never re-derives
// allocation (no `min(required, inventory)`, no `required - inventory`, no
// redistribution). The backend performs one coherent Build-tree allocation.

import type { FulfillmentScope, PreviewBuildPlanCommand } from "./builds";
import { json, request } from "./request";

/** How a row's demand is sourced across its contributing nodes. `mixed` when
 * the same type is Buy at one node and Build/Reaction at another -- the
 * per-node truth is on `nodeAllocations`. */
export type MaterialRowStrategy = "buy" | "build" | "reaction" | "mixed";

/** One aggregate row: **every** requirement boundary the plan touches for one
 * `typeId` -- covered, partial, or short; Buy leaf, Build/Reaction
 * intermediate, or provisional. Materials is the whole active plan's
 * participating items, not a shortage-only list. Invariants: `requiredQuantity
 * === allocatedQuantity + shortageQuantity`, `allocatedQuantity <=
 * availableQuantity`, `fullyCovered === (shortageQuantity === 0)`. */
export interface AggregateMaterialLine {
  typeId: number;
  typeName: string;
  requiredQuantity: number;
  availableQuantity: number;
  allocatedQuantity: number;
  shortageQuantity: number;
  /** `shortageQuantity === 0`. */
  fullyCovered: boolean;
  strategy: MaterialRowStrategy;
  /** True only when every contributing boundary is a provisional
   * (unresolved Build/Reaction) slot. */
  provisional: boolean;
}

/** How one component-requirement boundary is sourced. */
export type MaterialBoundaryResolution = "buy" | "build" | "reaction" | "unresolved";

/** Per production node, per type: how one boundary was allocated against
 * planning inventory. Every boundary gets one -- Buy leaves, unresolved
 * slots, **and** intermediate Build/Reaction boundaries (with their
 * `childRuns` / `producedQuantity` / `surplusQuantity`). Retained for a
 * future drill-down; not rendered yet. `allocatedQuantity` is planned use of
 * pre-existing inventory -- never a reservation. */
export interface NodeMaterialAllocation {
  buildId: string;
  /** `root:<uuid>` / `build:<uuid>` -- matches the Build Graph's node ids. */
  graphNodeId: string;
  /** `parentComponentTypeId` chain from the root to this boundary's node. */
  treePath: number[];
  typeId: number;
  typeName: string;
  requiredQuantity: number;
  allocatedQuantity: number;
  /** `requiredQuantity - allocatedQuantity`. External shortfall for
   * `buy`/`unresolved`; production demand handed to the child for
   * `build`/`reaction` (`0` => the subtree was pruned). */
  shortageQuantity: number;
  scope: FulfillmentScope;
  resolution: MaterialBoundaryResolution;
  /** `resolution === "unresolved"`. */
  provisional: boolean;
  /** `build`/`reaction` only: runs the child was dynamically projected at
   * (`0` for a pruned or non-production boundary). */
  childRuns: number;
  /** `build`/`reaction` only: the linked child's captured-recipe
   * primary-product `quantity_per_run` -- the authoritative per-run yield
   * `childRuns` is sized from. `0` for a `buy`/`unresolved` leaf. */
  outputPerRun: number;
  producedQuantity: number;
  /** `producedQuantity - shortageQuantity` -- discrete-run overproduction,
   * recorded only; never reused by another branch. For a canonical producer
   * serving several demand edges, the operation's one surplus sits on one
   * edge and the others report `0`. */
  surplusQuantity: number;
  /** The demand edge this row is: `pd:<id>` on a canonical plan,
   * `dep:<consumerBuildId>:<typeId>` otherwise. */
  dependencyId?: string;
  /** The producer Build satisfying this edge (build/reaction only). */
  producerBuildId?: string | null;
}

/** One leaf demand contribution -- provenance for the same future
 * drill-down. `graphNodeId` matches the Build Graph's node ids. */
export interface MaterialSource {
  buildId: string;
  graphNodeId: string;
  typeId: number;
  typeName: string;
  requiredQuantity: number;
  allocatedQuantity: number;
  shortageQuantity: number;
  scope: FulfillmentScope;
  /** True when this demand is for a Build/Reaction slot whose linked Build
   * does not exist yet -- provisional external demand, not a chosen
   * acquisition. */
  provisional: boolean;
  /** `parentComponentTypeId` chain from the root to the owning node. */
  treePath: number[];
}

export type MaterialsAggregateWarningCode = "unresolvedBuild";

export interface MaterialsAggregateWarning {
  code: MaterialsAggregateWarningCode;
  buildId: string;
  typeId: number | null;
  message: string;
}

export interface BuildMaterialsSummary {
  buildId: string;
  generatedAt: string;
  /** Aggregate demand, one row per type, in the backend's deterministic
   * (ascending `typeId`) order -- render as received. */
  rows: AggregateMaterialLine[];
  nodeAllocations: NodeMaterialAllocation[];
  sources: MaterialSource[];
  warnings: MaterialsAggregateWarning[];
}

/**
 * `POST /api/builds/:build_id/materials`. Read-only -- never creates or
 * mutates a Build, and (unlike the Graph) makes no `ProductionRepository`
 * calls. Send the same planning overlay a preview / the Build Graph sends
 * (the editor's `previewKey`, parsed). The path id is authoritative;
 * `command.buildId` is ignored server-side.
 *
 * A `422` with `code: "build_materials_incomplete"` means authoritative
 * per-node quantities could not be projected for the complete tree -- render
 * the curated error, never a partial table.
 */
export function postBuildMaterials(
  buildId: string,
  command: PreviewBuildPlanCommand,
  signal?: AbortSignal,
): Promise<BuildMaterialsSummary> {
  return request(`/api/builds/${buildId}/materials`, {
    ...json("POST", command),
    signal,
  });
}
