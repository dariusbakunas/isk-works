import type { InventoryBalance } from "../inventory";
import { ApiError, type ApiErrorBody } from "../workspace";
import { json, request } from "./request";
import type { Money } from "./shared";

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

// What a recording would do to inventory, per touched item, before and
// after -- the server runs the recording and rolls it back, so this is
// exactly what recording would post. Added items (the output or acquired
// item) come first, then consumed inputs.
export interface RecordingPreviewLine {
  typeId: number;
  typeName: string;
  effect: "added" | "consumed";
  current: InventoryBalance;
  resulting: InventoryBalance;
}

export interface RecordingPreview {
  lines: RecordingPreviewLine[];
}

export type RecordAcquisitionBody = Parameters<typeof recordTicketAcquisition>[1];
export type RecordProductionBody = Parameters<typeof recordTicketProduction>[1];

export function previewTicketAcquisition(ticketId: string, body: RecordAcquisitionBody): Promise<RecordingPreview> {
  return request(`/api/tickets/${ticketId}/record-acquisition/preview`, json("POST", body));
}

// Answers the same 409 `insufficient_available` as recording when other
// Epics hold the inputs.
export function previewTicketProduction(ticketId: string, body: RecordProductionBody): Promise<RecordingPreview> {
  return request(`/api/tickets/${ticketId}/record-production/preview`, json("POST", body));
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
    // Epics the user agreed to take reserved stock from, after a 409
    // `insufficient_available`.
    takeFrom?: { orderId: string; typeId: number }[];
  },
): Promise<RecordProductionResponse> {
  return request(`/api/tickets/${ticketId}/record-production`, json("POST", body));
}

export interface ReservationHolderView {
  orderId: string;
  displayName: string;
  quantity: number;
}

// One input recording can't cover without another Epic's reserved stock.
export interface AvailabilityShortage {
  typeId: number;
  typeName: string;
  needed: number;
  // Held by this ticket's own Epic.
  own: number;
  free: number;
  holders: ReservationHolderView[];
}

// 409 from record-production: other Epics reserved stock it needs.
export interface InsufficientAvailable extends ApiErrorBody {
  code: "insufficient_available";
  shortages: AvailabilityShortage[];
}

export function insufficientAvailable(error: unknown): InsufficientAvailable | null {
  return error instanceof ApiError && error.body.code === "insufficient_available"
    ? (error.body as InsufficientAvailable)
    : null;
}

/** What to take, per holder in the order the server listed them, to cover
 * each shortage: `needed - own - free`, capped at each holder's quantity. */
export function proposedTakes(
  shortages: AvailabilityShortage[],
): { typeId: number; typeName: string; orderId: string; displayName: string; quantity: number }[] {
  const takes = [];
  for (const shortage of shortages) {
    let missing = Math.max(0, shortage.needed - shortage.own - shortage.free);
    for (const holder of shortage.holders) {
      if (missing === 0) break;
      const quantity = Math.min(missing, holder.quantity);
      takes.push({
        typeId: shortage.typeId,
        typeName: shortage.typeName,
        orderId: holder.orderId,
        displayName: holder.displayName,
        quantity,
      });
      missing -= quantity;
    }
  }
  return takes;
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
