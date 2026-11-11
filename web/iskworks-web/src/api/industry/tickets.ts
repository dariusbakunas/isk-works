import type { IndustryJob } from "../characters";
import type { BlueprintSnapshot } from "./builds";
import type { FacilityProfile, InstallationCostBreakdown } from "./facilities";
import type { RequirementKind } from "./orders";
import { json, request } from "./request";
import type { Money } from "./shared";
import type { TicketInventoryRecording, TicketRecordingSummary } from "./ticket-recording";

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
  // When the ticket last moved into Complete; `null` unless it is Complete.
  completedAt: string | null;
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
  // The owning plan operation's frozen evidence -- `null` wherever
  // `occurrenceKey` is.
  planEvidence?: TicketPlanEvidence | null;
  // The frozen Epic steps this ticket executes: its own step for an Epic
  // step ticket, several for a combined ticket (which has no `orderId`),
  // none for acquisition/generic/standalone tickets. Optional so test
  // fixtures may omit it.
  operations?: TicketOperation[];
  // On a canceled member of a combined ticket: the combined ticket.
  mergedIntoTicketId?: string | null;
}

export interface TicketOperation {
  orderId: string;
  occurrenceKey: string;
}

// The subset of a frozen plan operation's evidence the web reads. The
// facility profile is referenced by id only (its name is not frozen), so
// it resolves against the live facility list and may be gone.
export interface TicketPlanEvidence {
  effectiveMe: number | null;
  effectiveTe: number | null;
  jobCount: number;
  installation: { facilityProfileId: string | null } | null;
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

// Organizational metadata only -- title, notes, assignee. Every field is
// optional: omit it (JSON.stringify drops an `undefined` value) to leave it
// untouched, pass `null` to clear `assigneeCharacterId`, or pass a value to
// set it. A ticket's Epic is fixed when it's created; the server refuses an
// `orderId` here. Never touches status, recording, execution snapshot,
// source Build, requirement fulfillments, or AcquisitionRun membership --
// see `PATCH /api/tickets/:id`'s own doc.
export interface UpdateTicketMetadataInput {
  capturedName?: string;
  notes?: string;
  assigneeCharacterId?: string | null;
}

export function updateTicketMetadata(ticketId: string, input: UpdateTicketMetadataInput): Promise<Ticket> {
  return request(`/api/tickets/${ticketId}`, json("PATCH", input));
}

export function createTicketForRequirement(orderId: string, requirementId: string): Promise<Ticket> {
  return request(`/api/orders/${orderId}/requirements/${requirementId}/tickets`, { method: "POST" });
}

// Creates the ticket for one frozen production step of a version-3 Epic
// (e.g. after its ticket was deleted). Returns the step's existing active
// ticket instead when it has one.
export function createOperationTicket(orderId: string, occurrenceKey: string): Promise<Ticket> {
  return request(`/api/orders/${orderId}/operations/${encodeURIComponent(occurrenceKey)}/ticket`, {
    method: "POST",
  });
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

// Combines unstarted production steps of the same item -- from one or more
// Epics -- into one ticket doing all of their work; the members are
// canceled and point at it (`mergedIntoTicketId`). The first id sets the
// assignee. A refused ticket answers 409 naming it.
export function combineTickets(ticketIds: string[]): Promise<Ticket> {
  return request("/api/tickets/combine", json("POST", { ticketIds }));
}

// Undoes a combine while nothing is recorded or linked on the combined
// ticket; answers the restored members.
export function splitTicket(ticketId: string): Promise<Ticket[]> {
  return request(`/api/tickets/${ticketId}/split`, { method: "POST" });
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
  // On a combined ticket's prerequisite: the Epic it serves (each member's
  // prerequisites are copied over, tagged). `null` means the ticket's own.
  orderId?: string | null;
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

export interface TicketSummary extends Ticket {
  blockedBy: TicketBlockerRef[];
  prerequisites: TicketPrerequisite[];
  recording: TicketRecordingSummary | null;
  recordings?: TicketInventoryRecording[];
}

export function listTickets(): Promise<TicketSummary[]> {
  return request("/api/tickets");
}

// In Progress Manufacturing/Reaction tickets matched to their assignee's
// running ESI jobs. Display only, recomputed server-side on every read;
// a ticket with no matching job is absent.
export interface TicketJobProgress {
  ticketId: string;
  jobs: IndustryJob[];
}

export function listTicketJobProgress(): Promise<TicketJobProgress[]> {
  return request("/api/tickets/job-progress");
}

// A job ESI reports now that could be linked to the ticket by hand -- the
// assignee's, for the ticket's activity and exact product -- and the ticket
// it is on now, if any (linking moves it).
export interface TicketJobChoice {
  job: IndustryJob;
  linkedTicketId: string | null;
  linkedTicketDisplayId: string | null;
}

export function listTicketJobChoices(ticketId: string): Promise<TicketJobChoice[]> {
  return request(`/api/tickets/${ticketId}/job-choices`);
}

// Links a job to the ticket by hand, moving it off any other ticket.
export function linkTicketJob(ticketId: string, jobId: number): Promise<void> {
  return request(`/api/tickets/${ticketId}/jobs`, json("POST", { jobId }));
}

// Unlinks a job; automatic matching won't link it to this ticket again.
export function unlinkTicketJob(ticketId: string, jobId: number): Promise<void> {
  return request(`/api/tickets/${ticketId}/jobs/${jobId}`, { method: "DELETE" });
}
