import { ApiError, type ApiErrorBody } from "../workspace";
import type { PreviewBuildPlanCommand } from "./builds";
import type { ExecutionPlanProjection } from "./execution-plan";
import { json, request } from "./request";
import type { Money } from "./shared";
import type { TicketStatus } from "./tickets";

export type OrderStatus = "blocked" | "ready" | "inProgress" | "complete" | "canceled";

// BUY/BUILD/RXN in the dependency badges -- never
// "INV" (inventory coverage is a fulfillment *state*, not a sourcing
// kind; see RequirementFulfillmentState below).
export type RequirementKind = "buy" | "build" | "react";

// Derived, never stored -- see order::RequirementFulfillmentState.
// InventorySatisfied: fully covered by stock at Order-creation time, no
// ticket needed ("✓ Inventory"). Satisfied: a linked ticket completed
// enough of it. Linked: a non-canceled ticket exists but isn't done yet.
// NeedsAction: nothing tracking this requirement yet.
export type RequirementFulfillmentState = "inventorySatisfied" | "satisfied" | "linked" | "needsAction";

// Every non-canceled ticket fulfilling a requirement -- the data behind a
// line like "Isogen x1,400 -- Executing via ACQ-0042". Usually 0 or 1
// entries.
export interface LinkedTicketRef {
  id: string;
  displayId: string;
  status: TicketStatus;
  allocatedQuantity: number;
}

export interface OrderRequirement {
  id: string;
  orderId: string;
  typeId: number;
  capturedName: string;
  kind: RequirementKind;
  // The specific linked Build selected at Order-creation time, frozen
  // alongside `kind` -- null for "buy". A later ticket-creation action
  // reads this directly rather than re-resolving it against the Order's
  // current build graph, which may have changed since.
  sourceBuildId: string | null;
  requiredQuantity: number;
  // The scope this requirement was sourced under, frozen at Epic-creation
  // time (Model B). "missing" -- inventory coverage was netted, so
  // `reusedQuantity` may be > 0 and `freshQuantity` is the shortage.
  // "full" -- sourced entirely fresh, `reusedQuantity === 0`.
  fulfillmentScope: "missing" | "full";
  // How much of `requiredQuantity` this Epic intended to draw from existing
  // stock, frozen at creation. Planning evidence, NOT a reservation -- two
  // Epics may plan against the same units.
  reusedQuantity: number;
  // `requiredQuantity - reusedQuantity` -- the portion every generated
  // ticket is sized from. `0` => InventorySatisfied, no ticket.
  freshQuantity: number;
  estimatedUnitCost: Money | null;
  // Blended: reused (inventory) portion + fresh portion's own market/build
  // cost.
  estimatedLineTotal: Money | null;
  // Expected cost of just the reused portion, at the inventory weighted
  // average frozen at creation. null for a "full"-scoped requirement or one
  // with nothing reused.
  reusedLineTotal: Money | null;
  state: RequirementFulfillmentState;
  linkedTickets: LinkedTicketRef[];
  // Which frozen operation this requirement belongs to -- `null` for a
  // version-1 Epic's requirement (reads as "the Order's own root").
  // Optional here (in addition to nullable) so test fixtures may omit a
  // field the server always sends.
  operationOccurrenceKey?: string | null;
  // For a `build`/`react` row whose production is an active frozen
  // operation, that operation's own occurrence key -- `null` for a `buy`
  // row, a fully-covered row, or an unresolved slot.
  childOccurrenceKey?: string | null;
  inventoryUnitBasis?: Money | null;
  childProducedQuantity?: number | null;
  childConsumedQuantity?: number | null;
  childSurplusQuantity?: number | null;
  childSurplusRetainedBasis?: Money | null;
  // Version-3 Epics only: this row's own frozen share of the producer
  // operation's cost, and its demand-edge identity. Several rows may name
  // the same `childOccurrenceKey` (one operation serving many
  // requirements). null for version-1/2 rows.
  childConsumedCost?: Money | null;
  dependencyId?: string | null;
}

export interface OrderRequirementRollup {
  satisfied: number;
  needsAction: number;
  inProgress: number;
  total: number;
  // Of the needs-action items, how many can be started now: buys, and steps
  // whose own inputs are all covered -- the Epic inspector's "can start
  // now". Optional so fixtures may omit it.
  readyToStart?: number;
}

// "Execute this version of this Build" -- an immutable snapshot: there is
// no Draft/Committed distinction and no regenerate action. If the source
// Build changes, create another Order.
export interface Order {
  id: string;
  workspaceId: string;
  ownerId: string;
  sourceBuildId: string | null;
  sourceBuildRevision: number;
  displayName: string;
  runs: number;
  recipeFingerprint: string;
  priceSnapshotId: string;
  estimatedMaterialCost: Money;
  expectedRevenue: Money | null;
  estimatedMargin: Money | null;
  missingPriceCount: number;
  createdAt: string;
  updatedAt: string;
  startedAt: string | null;
  completedAt: string | null;
  canceledAt: string | null;
  archivedAt: string | null;
  // 1 = legacy, root-only freeze. 2 = whole-tree freeze (every
  // active Build/Reaction operation has its own frozen row, requirements
  // carry `operationOccurrenceKey`). Never backfilled for an existing Epic.
  // 3 = canonical-producer freeze: one frozen operation (and one ticket)
  // may serve several requirements; operations with more than one consumer
  // have no single parent (`parentOccurrenceKey`/`parentTicketId` null).
  // Optional so test fixtures may omit it.
  planningSnapshotVersion?: number;
}

// Whole-tree (version 2/3) Epics: one frozen production operation.
export interface PlanOperationView {
  id: string;
  occurrenceKey: string;
  // null for the root, and for a version-3 operation serving several
  // consuming operations -- see `productionPlan.dependencies` instead.
  parentOccurrenceKey: string | null;
  buildId: string | null;
  productTypeId: number;
  productName: string;
  runs: number;
  producedQuantity: number;
  materialComponentCost: Money | null;
  ownInstallationCost: Money | null;
  totalProductionCost: Money | null;
  complete: boolean;
  // Version 3 only (null otherwise, and for the root): aggregate
  // consumption across every served requirement and the one surplus.
  consumedQuantity: number | null;
  surplusQuantity: number | null;
  surplusRetainedBasis: Money | null;
  // Longest producer chain beneath this operation (0 = consumes nothing
  // produced). Ordering evidence only, not readiness.
  stage: number;
  ticketId: string | null;
  ticketDisplayId: string | null;
  ticketStatus: TicketStatus | null;
  servedRequirementIds: string[];
}

export interface OperationDependency {
  producerOccurrenceKey: string;
  consumerOccurrenceKey: string;
  dependencyIds: string[];
}

export interface ProductionPlan {
  rootOccurrenceKey: string;
  // Deterministic build order: by stage, then occurrence key.
  operations: PlanOperationView[];
  dependencies: OperationDependency[];
}

export interface OrderDetail extends Order {
  status: OrderStatus;
  rollup: OrderRequirementRollup;
  requirements: OrderRequirement[];
  // Absent for a version-1 Epic.
  productionPlan?: ProductionPlan;
  // Create Epic only: types the new Epic reserved more of than the dialog
  // previewed (stock arrived in between).
  reuseIncreased?: ReuseChange[];
  // Version-3 Epics: how much of the frozen reuse the Epic holds and used.
  inventory?: EpicInventorySummary;
}

export interface EpicInventorySummary {
  // Units, summed across items -- not comparable between items.
  plannedReuse: number;
  reserved: number;
  used: number;
  // Items (requirements) the Epic planned to take from stock, and how many
  // of them it holds (or has used) in full.
  itemsPlanned: number;
  itemsHeld: number;
}

export interface ReserveInventoryResult {
  reserved: { typeId: number; typeName: string; quantity: number }[];
  shortfalls: ReservationShortfall[];
}

// The Epic's "Reserve inventory": reserves what free stock allows toward
// its frozen reuse and reports what's still short.
export function reserveOrderInventory(id: string): Promise<ReserveInventoryResult> {
  return request(`/api/orders/${id}/reserve`, { method: "POST" });
}

// Board-scoped listing: same derived status/rollup as `OrderDetail`
// (every card needs its lane and progress), without the full
// requirements/linkedTickets detail only the Order detail page needs.
export interface OrderSummary extends Order {
  status: OrderStatus;
  rollup: OrderRequirementRollup;
}

// The Epic freezes the *live* overlay (the same
// `PreviewBuildPlanCommand` Materials/Graph/Worksheet already send), not the
// Build's last-saved configuration -- send the editor's current overlay.
//
// `reservation` reserves the Epic's planned inventory reuse, confirming the
// per-type reuse the user previewed (`previewOrder`). Without it the Epic
// reserves nothing.
export function createOrder(
  buildId: string,
  command: PreviewBuildPlanCommand,
  reservation?: CreateOrderReservation,
): Promise<OrderDetail> {
  return request(`/api/builds/${buildId}/orders`, json("POST", reservation ? { ...command, reservation } : command));
}

// What an Epic created from this overlay right now would reuse from free
// inventory, per type. Persists nothing.
export function previewOrder(buildId: string, command: PreviewBuildPlanCommand): Promise<EpicReusePreview> {
  return request(`/api/builds/${buildId}/orders/preview`, json("POST", command));
}

export interface EpicReuseLine {
  typeId: number;
  typeName: string;
  quantity: number;
}

export interface EpicReusePreview {
  reuse: EpicReuseLine[];
}

export interface CreateOrderReservation {
  expectedReuse: { typeId: number; quantity: number }[];
}

export interface ReuseChange {
  typeId: number;
  expected: number;
  now: number;
}

export interface ReservationShortfall {
  typeId: number;
  wanted: number;
  free: number;
}

// 409 from Create Epic: free inventory changed since the preview. Carries
// the fresh preview, so the dialog can refresh without another request.
export interface ReservationDrift extends ApiErrorBody {
  code: "reservation_drift";
  preview: EpicReusePreview;
  decreased: ReuseChange[];
  shortfalls: ReservationShortfall[];
}

export function reservationDrift(error: unknown): ReservationDrift | null {
  return error instanceof ApiError && error.body.code === "reservation_drift"
    ? (error.body as ReservationDrift)
    : null;
}

// Board filtering rule -- default `active` (archivedAt IS NULL). Filtered server-side (in-memory, small dataset),
// same convention as `listTickets`'s own `showBatched` param.
export type ArchivedFilter = "active" | "archived" | "all";

export function listOrders(archived: ArchivedFilter = "active"): Promise<OrderSummary[]> {
  const query = archived === "active" ? "" : `?archived=${archived}`;
  return request(`/api/orders${query}`);
}

export function getOrder(id: string): Promise<OrderDetail> {
  return request(`/api/orders/${id}`);
}

export function startOrder(id: string): Promise<OrderDetail> {
  return request(`/api/orders/${id}/start`, { method: "POST" });
}

export function completeOrder(id: string): Promise<OrderDetail> {
  return request(`/api/orders/${id}/complete`, { method: "POST" });
}

// Valid any time before completion; releases (never consumes) this
// Order's own inventory allocations but never touches linked tickets --
// a shared ticket might still be needed by another Order.
export function cancelOrder(id: string): Promise<OrderDetail> {
  return request(`/api/orders/${id}/cancel`, { method: "POST" });
}

// Orthogonal to workflow status -- never affects allocations.
export function archiveOrder(id: string): Promise<OrderDetail> {
  return request(`/api/orders/${id}/archive`, { method: "POST" });
}

export function restoreOrder(id: string): Promise<OrderDetail> {
  return request(`/api/orders/${id}/restore`, { method: "POST" });
}

// Permanently removes the Epic and everything planned under it (requirements
// + linked tickets, with their recordings / fulfillments / solely-owned
// acquisition runs). Unlike cancel/archive this frees the source Build.
export function deleteOrder(id: string): Promise<void> {
  return request(`/api/orders/${id}`, { method: "DELETE" });
}

// What an Epic holds and has done against one line of its frozen plan.
export interface EpicStockProgress {
  reserved: number;
  consumed: number;
  remainingNeed: number;
}

export interface EpicNodeProgress {
  ticketId: string | null;
  ticketDisplayId: string | null;
  ticketStatus: TicketStatus | null;
  // Stock held for (and used by) the steps this one feeds.
  output: EpicStockProgress;
}

export interface EpicPlanOverlay {
  orderId: string;
  displayName: string;
  sourceBuildRevision: number;
  // By ExecutionNode id.
  nodes: Record<string, EpicNodeProgress>;
  // By acquisition type id.
  acquisitions: Record<string, EpicStockProgress>;
}

// A version-3 Epic's frozen plan in the Build Plan view's own shape, plus
// the Epic overlay. Selecting an Epic swaps the Plan's data, not its layout.
export interface EpicExecutionPlan {
  plan: ExecutionPlanProjection;
  epic: EpicPlanOverlay;
}

export function getOrderExecutionPlan(id: string): Promise<EpicExecutionPlan> {
  return request(`/api/orders/${id}/execution-plan`);
}
