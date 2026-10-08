import { ApiError, type ApiErrorBody } from "../workspace";
import { json, request } from "./request";
import type { Money } from "./shared";
import type { BlueprintSnapshot, PreviewBuildPlanCommand } from "./builds";
import type { FacilityProfile, InstallationCostBreakdown } from "./facilities";

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

// One frozen requirement's live inventory state in its Epic. `freeAvailable`
// is the type's free stock, shared by every line of that type.
export interface EpicCoverageLine {
  requirementId: string;
  reserved: number;
  consumed: number;
  remainingNeed: number;
  freeAvailable: number;
  freeCoverable: number;
}

export interface EpicCoverage {
  orderId: string;
  lines: EpicCoverageLine[];
}

export function getOrderCoverage(id: string): Promise<EpicCoverage> {
  return request(`/api/orders/${id}/coverage`);
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

// The order::Ticket domain -- a standalone execution work item. Its
// requirement/prerequisite *fulfillment* relationships (which Order(s) it
// contributes to) still go through a join, independent of `orderId`
// below, which answers a narrower question: explicit organizational Epic
// membership -- optional, and never inferred from fulfillment links.
export type TicketKind = "acquisition" | "manufacturing" | "reaction" | "generic";
// Purely organizational, purely user-controlled -- every transition is
// legal in both directions. Dependency state is a separate, derived
// concept (`TicketSummary.blockedBy`); a ticket can be `inProgress` with
// blockers, or `todo` with none.
export type TicketStatus = "todo" | "inProgress" | "complete" | "canceled";

// The blueprint/facility/duration/cost a Manufacturing/Reaction ticket's
// own job assumed, frozen at ticket-creation time -- not a second pricing
// calculation, the same data the Build worksheet already computes, just
// retained. `null` for Acquisition tickets.
export interface TaskExecutionSnapshot {
  runs: number;
  blueprint: BlueprintSnapshot | null;
  facility: FacilityProfile | null;
  durationSeconds: number | null;
  installationCost: InstallationCostBreakdown | null;
  materialValue: Money | null;
}

export interface Ticket {
  id: string;
  workspaceId: string;
  ownerId: string;
  displayId: string;
  kind: TicketKind;
  // `null` only for `generic` -- every other kind has an item.
  typeId: number | null;
  // This ticket's title -- an item/product name for a generated ticket,
  // or a user-chosen free-form title for a manually created one (most
  // naturally so for `generic`, which has no item to name it after).
  // Always required and non-empty.
  capturedName: string;
  // `null` only for `generic` -- every other kind has an output quantity.
  quantity: number | null;
  // Explicit organizational Epic membership -- `null` for a standalone
  // ticket, otherwise the Order/Epic this ticket was generated as work
  // for. The authoritative answer to "which Epic contains this ticket?" --
  // independent of `sourceBuildId` (which Build this ticket executes) and
  // of requirement-fulfillment linking (which frozen requirement(s) this
  // ticket contributes to, possibly for a *different* Epic).
  orderId: string | null;
  sourceBuildId: string | null;
  // Free-form work notes -- always a string, never null (an unset note is
  // just "").
  notes: string;
  // The connected EVE character (see api/characters.ts's `connectionId`)
  // this ticket is assigned to -- `null` when unassigned, purely
  // organizational metadata.
  assigneeCharacterId: string | null;
  status: TicketStatus;
  estimatedUnitCost: Money | null;
  estimatedLineTotal: Money | null;
  actualUnitCost: Money | null;
  actualLineTotal: Money | null;
  // The market scope this ticket was priced from, frozen at creation time
  // from the owning Order's material lines -- the real Acquisition Run
  // batching-compatibility key (grouping by this, not by Order, is what
  // lets tickets from different Orders share one shopping trip when their
  // scope matches). `priceSourceId` is the same freeze for a ticket priced
  // from a Manual Price List instead, where scope doesn't apply.
  marketRegionId: number | null;
  marketLocationId: number | null;
  priceSourceId: string | null;
  acquisitionRunId: string | null;
  acquiredQuantity: number | null;
  executionSnapshot: TaskExecutionSnapshot | null;
  createdAt: string;
  updatedAt: string;
  archivedAt: string | null;
  // This ticket's own frozen operation identity -- `null` for a ticket from
  // a version-1 Epic, a standalone, or an Acquisition/Generic ticket.
  // `parentTicketId` is the parent operation's ticket (`null` for the
  // root). Optional (in addition to nullable) so test fixtures may omit it.
  occurrenceKey?: string | null;
  parentTicketId?: string | null;
  // This ticket's own full job output at freeze time (never the
  // parent-consumed portion `quantity` already carries for an
  // Acquisition ticket).
  producedQuantity?: number | null;
  materialComponentCost?: Money | null;
  ownInstallationCost?: Money | null;
  totalProductionCost?: Money | null;
}

// The canonical manual Ticket creation contract -- one request shape,
// discriminated by `kind`, matching the single `POST /api/tickets`
// endpoint. `orderId`/`assigneeCharacterId` are purely organizational on
// every variant: selecting either only sets that relationship, never
// creates a requirement fulfillment, reserves inventory, or derives
// status.
//
// `manufacturing`/`reaction` carry only `buildId` and an optional `runs`
// override -- per the product rule ("Build owns the production plan;
// Ticket freezes that plan as intended work"), the client never sends
// recipe/ME/TE/facility/material data. The server derives the title,
// output quantity, execution snapshot and prerequisites from the selected
// Build, and rejects a `buildId` whose recipe doesn't match `kind`.
export type CreateTicketInput =
  | {
      kind: "generic";
      capturedName: string;
      notes?: string;
      orderId?: string;
      assigneeCharacterId?: string;
    }
  | {
      kind: "acquisition";
      capturedName: string;
      typeId: number;
      quantity: number;
      notes?: string;
      orderId?: string;
      assigneeCharacterId?: string;
      priceSourceId?: string;
    }
  | {
      kind: "manufacturing" | "reaction";
      buildId: string;
      /** Defaults to the Build's own `runs` when omitted. Never clamped --
       * a value outside the planner's bounds is rejected, not silently
       * capped. */
      runs?: number;
      notes?: string;
      orderId?: string;
      assigneeCharacterId?: string;
    };

export function createTicket(input: CreateTicketInput): Promise<Ticket> {
  return request("/api/tickets", json("POST", input));
}

// A read-only preview of the production plan a Manufacturing/Reaction
// ticket would freeze if created now from this Build, at `runs` (defaults
// to the Build's own). Powers the TicketEditor's "plan to freeze" panel --
// the exact same calculation `POST /api/tickets` uses at creation time, so
// the preview never drifts from what actually gets frozen. Never mutates
// the Build.
export interface TicketPlanPreviewPrerequisite {
  typeId: number;
  capturedName: string;
  kind: RequirementKind;
  requiredQuantity: number;
  estimatedUnitCost: Money | null;
  estimatedLineTotal: Money | null;
}

export interface TicketPlanPreview {
  kind: "manufacturing" | "reaction";
  buildId: string;
  runs: number;
  typeId: number;
  capturedName: string;
  quantity: number;
  executionSnapshot: TaskExecutionSnapshot;
  prerequisites: TicketPlanPreviewPrerequisite[];
}

export function previewTicketPlan(buildId: string, runs?: number): Promise<TicketPlanPreview> {
  const query = runs === undefined ? "" : `?runs=${runs}`;
  return request(`/api/builds/${buildId}/ticket-preview${query}`);
}

// Organizational metadata only -- title, notes, Epic membership,
// assignee. Every field is optional and independently three-valued on the
// wire: omit it (JSON.stringify drops an `undefined` value) to leave it
// untouched, pass `null` to clear `orderId`/`assigneeCharacterId`, or pass
// a value to set it. Never touches status, recording, execution snapshot,
// source Build, requirement fulfillments, or AcquisitionRun membership --
// see `PATCH /api/tickets/:id`'s own doc.
export interface UpdateTicketMetadataInput {
  capturedName?: string;
  notes?: string;
  orderId?: string | null;
  assigneeCharacterId?: string | null;
}

export function updateTicketMetadata(ticketId: string, input: UpdateTicketMetadataInput): Promise<Ticket> {
  return request(`/api/tickets/${ticketId}`, json("PATCH", input));
}

export function createTicketForRequirement(orderId: string, requirementId: string): Promise<Ticket> {
  return request(`/api/orders/${orderId}/requirements/${requirementId}/tickets`, { method: "POST" });
}

export function bulkCreateTickets(orderId: string, requirementIds: string[]): Promise<Ticket[]> {
  return request(`/api/orders/${orderId}/tickets/bulk`, json("POST", { requirementIds }));
}

// A bare workflow-status write
// with ZERO domain side effects -- no inventory event, no allocation, no
// dependent-ticket cascade, no Order/Run/Build mutation. This is the ONLY
// call a Board lane drag makes; it never touches /start, /complete, or
// /cancel. Any persisted status to any other, both directions.
export function updateTicketStatus(ticketId: string, status: TicketStatus): Promise<Ticket> {
  return request(`/api/tickets/${ticketId}`, json("PATCH", { status }));
}

export function startTicket(ticketId: string): Promise<Ticket> {
  return request(`/api/tickets/${ticketId}/start`, { method: "POST" });
}

// Workflow-only: sets status to Complete and nothing else -- no inventory
// posting, no recording, no dependent-ticket cascade. Explicit recording
// (recordTicketAcquisition/recordTicketProduction) is the only accounting
// path, and works identically before or after this call.
export function completeTicket(ticketId: string): Promise<Ticket> {
  return request(`/api/tickets/${ticketId}/complete`, { method: "POST" });
}

// Workflow-only: sets status to Canceled and nothing else -- no inventory
// release, no recording change, no dependent-ticket cascade. Valid from
// Blocked/Ready/InProgress.
export function cancelTicket(ticketId: string): Promise<Ticket> {
  return request(`/api/tickets/${ticketId}/cancel`, { method: "POST" });
}

// Permanently removes the ticket -- including an acquisition ("shopping
// trip") ticket -- with the rows that pin it (recordings, fulfillments,
// prerequisites, a solely-owned acquisition run). Unlike cancel/archive
// this deletes the row.
export function deleteTicket(ticketId: string): Promise<void> {
  return request(`/api/tickets/${ticketId}`, { method: "DELETE" });
}

// A Manufacturing/Reaction ticket's own frozen, single-level material need
// -- same shape as OrderRequirement, one level down. Empty for Acquisition
// tickets (never have prerequisite rows).
export interface TicketPrerequisite {
  id: string;
  ticketId: string;
  typeId: number;
  capturedName: string;
  kind: RequirementKind;
  sourceBuildId: string | null;
  requiredQuantity: number;
  // Frozen scope -- "full" for a child ticket's prerequisites; the Epic's
  // own scope for the root ticket's prerequisites (Model B).
  fulfillmentScope: "missing" | "full";
  reusedQuantity: number;
  freshQuantity: number;
  estimatedUnitCost: Money | null;
  estimatedLineTotal: Money | null;
  reusedLineTotal: Money | null;
  // See the matching fields on `OrderRequirement`. Optional so test
  // fixtures may omit them.
  operationOccurrenceKey?: string | null;
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

// One unsatisfied prerequisite behind a Blocked ticket. `representative*`
// fields name a single non-canceled fulfillment for compact display when
// more than one exists -- not the complete fulfillment relationship.
export interface TicketBlockerRef {
  prerequisiteId: string;
  kind: RequirementKind;
  typeId: number;
  capturedName: string;
  outstandingQuantity: number;
  representativeFulfillingTicketId: string | null;
  representativeFulfillingTicketDisplayId: string | null;
  representativeFulfillingTicketStatus: TicketStatus | null;
}

// Derived accounting/execution state from a ticket's explicit inventory
// recordings -- orthogonal to workflow `status` (a `complete` ticket may
// still be `notRecorded`, a `ready` ticket may be `recorded`). Populated
// for Acquisition, Manufacturing and Reaction tickets; the backend derives
// it, never the client.
export type RecordingState = "notRecorded" | "partiallyRecorded" | "recorded";

export interface TicketRecordingSummary {
  state: RecordingState;
  requestedQuantity: number;
  recordedQuantity: number;
  remainingQuantity: number;
  surplusQuantity: number;
}

// One immutable "I actually acquired / produced this" ledger row. Never
// updated or deleted -- accounting provenance for the inventory events it
// posted. `recordedQuantity` is set only for `acquisition`; the
// `runsCompleted` / `installationCost` / `outputTypeId` / `outputQuantity`
// fields only for `production`.
export interface TicketInventoryRecording {
  id: string;
  ticketId: string;
  kind: "acquisition" | "production";
  recordedQuantity: number | null;
  runsCompleted: number | null;
  installationCost: Money | null;
  outputTypeId: number | null;
  outputQuantity: number | null;
  locationNote: string;
  note: string;
  recordedAt: string;
  revertedAt: string | null;
  status: "recorded" | "reversed";
  effects: TicketInventoryEffect[];
}

export interface TicketInventoryEffect {
  eventId: string;
  kind: "purchase" | "consumption" | "productionOutput";
  typeId: number;
  capturedName: string;
  quantityDelta: number;
  totalCostDelta: string;
}

export interface RecordAcquisitionResponse {
  recording: TicketInventoryRecording;
  summary: TicketRecordingSummary;
}

export interface RecordProductionResponse {
  recording: TicketInventoryRecording;
  summary: TicketRecordingSummary;
}

export interface TicketSummary extends Ticket {
  blockedBy: TicketBlockerRef[];
  prerequisites: TicketPrerequisite[];
  recording: TicketRecordingSummary | null;
  recordings?: TicketInventoryRecording[];
}

export function listTickets(): Promise<TicketSummary[]> {
  return request("/api/tickets");
}

// Explicit inventory recording action -- posts exactly one Purchase and
// records immutable provenance, in one transaction. Never changes the
// ticket's workflow status. Idempotent on `idempotencyKey`.
export function recordTicketAcquisition(
  ticketId: string,
  body: {
    idempotencyKey: string;
    quantity: number;
    unitCost?: string;
    locationNote?: string;
    note?: string;
    effectiveAt?: string;
  },
): Promise<RecordAcquisitionResponse> {
  return request(`/api/tickets/${ticketId}/record-acquisition`, json("POST", body));
}

// Explicit production recording for a Manufacturing/Reaction ticket --
// posts one Consumption per input plus one ProductionOutput (basis =
// consumed basis + installationCost), in one transaction. Never changes
// the ticket's workflow status. Idempotent on `idempotencyKey`.
export function recordTicketProduction(
  ticketId: string,
  body: {
    idempotencyKey: string;
    runsCompleted: number;
    output: { typeId: number; quantity: number };
    inputs: { typeId: number; quantity: number }[];
    installationCost: string;
    locationNote?: string;
    note?: string;
    effectiveAt?: string;
  },
): Promise<RecordProductionResponse> {
  return request(`/api/tickets/${ticketId}/record-production`, json("POST", body));
}

export interface RevertTicketInventoryRecordingResponse {
  recording: TicketInventoryRecording;
  summary: TicketRecordingSummary;
  recordings: TicketInventoryRecording[];
}

export function revertTicketInventoryRecording(
  ticketId: string,
  recordingId: string,
): Promise<RevertTicketInventoryRecordingResponse> {
  return request(
    `/api/tickets/${ticketId}/recordings/${recordingId}/revert`,
    json("POST", {}),
  );
}

// The first (and only) kind of Execution Batch: groups compatible,
// already-Ready Acquisition tickets for one shopping trip, without
// merging or destroying the source tickets.
export type AcquisitionRunStatus = "ready" | "inProgress" | "complete";

export interface AcquisitionRun {
  id: string;
  workspaceId: string;
  ownerId: string;
  displayId: string;
  name: string;
  kind: "acquisition";
  status: AcquisitionRunStatus;
  marketRegionId: number | null;
  marketLocationId: number | null;
  priceSourceId: string | null;
  createdAt: string;
  updatedAt: string;
  startedAt: string | null;
  completedAt: string | null;
}

// The Run's real, uncapped per-type acquired total -- may exceed the
// matching tickets' summed `quantity` (over-acquisition is valid). Ticket
// `acquiredQuantity` stays capped to that ticket's own demand; this is the
// actual "Acquired" number to show in the drawer.
export interface AcquisitionRunItem {
  typeId: number;
  acquiredQuantity: number;
}

export interface AcquisitionRunDetail extends AcquisitionRun {
  tickets: Ticket[];
  items: AcquisitionRunItem[];
}

export function createAcquisitionRun(input: { name?: string; ticketIds: string[] }): Promise<AcquisitionRun> {
  return request("/api/acquisition-runs", json("POST", input));
}

export function listAcquisitionRuns(): Promise<AcquisitionRun[]> {
  return request("/api/acquisition-runs");
}

export function getAcquisitionRun(runId: string): Promise<AcquisitionRunDetail> {
  return request(`/api/acquisition-runs/${runId}`);
}

// No preview/confirm step -- an Order's status is fully derived, so
// there's no lock-transition side effect to warn about before starting.
export function startAcquisitionRun(runId: string): Promise<AcquisitionRun> {
  return request(`/api/acquisition-runs/${runId}/start`, { method: "POST" });
}

export interface AcquisitionProgressItem {
  typeId: number;
  acquiredQuantity: number;
}

export function recordAcquisitionProgress(
  runId: string,
  items: AcquisitionProgressItem[],
): Promise<AcquisitionRun> {
  return request(`/api/acquisition-runs/${runId}/items`, json("PATCH", { items }));
}

export function completeAcquisitionRun(runId: string): Promise<AcquisitionRun> {
  return request(`/api/acquisition-runs/${runId}/complete`, { method: "POST" });
}
