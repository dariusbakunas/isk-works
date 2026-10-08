import { jsonRequest } from "./json-request";

export type CostInputQuality = "known" | "estimated" | "zeroCost";

export interface InventoryBalance {
  key: { workspaceId: string; ownerId: string; typeId: number };
  typeName: string;
  quantity: number;
  totalHistoricalCost: string;
  averageUnitCost: string | null;
  revision: number;
  lastActivityAt: string | null;
}

export interface InventoryEvent {
  id: string;
  kind: "openingBalance" | "purchase" | "consumption" | "productionOutput" | "reversal" | "adjustment";
  quantityDelta: number;
  totalCostDelta: string;
  unitCost: string | null;
  costQuality: CostInputQuality;
  sourceReference: string;
  note: string;
  effectiveAt: string;
  recordedAt: string;
  sequence: number;
  reversesEventId: string | null;
  reversedByEventId: string | null;
  buildId: string | null;
  buildCompletionId: string | null;
  resultingBalance: InventoryBalance;
}

export interface InventoryItem {
  balance: InventoryBalance;
  groupName: string | null;
  packagedVolumeM3: string | null;
  totalVolumeM3: string | null;
  reservedQuantity: number;
  availableQuantity: number;
  costQuality: CostInputQuality;
  currentPrice: string | null;
  currentValue: string | null;
  historicalDifference: string | null;
  historicalComparisonComplete: boolean;
  // Set only when pricing came from an explicitly-selected Manual Price
  // List. Mutually exclusive with marketRegionId/marketLocationId, which
  // are set instead when priced against the workspace's default market
  // scope (no PriceSource identity in that case).
  priceSourceId: string | null;
  priceSourceName: string | null;
  marketRegionId: number | null;
  marketLocationId: number | null;
  priceSourceUpdatedAt: string | null;
  /**
   * `null` means no ESI observation exists for this type -- never treat
   * as an observed zero. ESI never emits a zero-quantity asset row, so
   * "not observed" and "observed nothing" are different facts.
   */
  esiObservedQuantity: number | null;
  ignoredEsiQuantity?: number | null;
  includedEsiQuantity?: number | null;
  esiObservedAt: string | null;
  reconciliationDifference: number | null;
  warnings: string[];
}

export type OrderReservationStatus = "notStarted" | "inProgress" | "complete" | "canceled";
export type TicketReservationStatus = "todo" | "inProgress" | "complete" | "canceled";

export type InventoryReservationSource =
  | { kind: "order"; orderId: string; displayName: string; status: OrderReservationStatus }
  | { kind: "ticket"; ticketId: string; displayId: string; status: TicketReservationStatus };

export interface InventoryReservation {
  allocationId: string;
  quantity: number;
  createdAt: string;
  source: InventoryReservationSource;
}

export interface EsiHoldingContributor {
  connectionId: string;
  eveCharacterId: number;
  characterName: string;
  locationId: number;
  /** `null` when the location isn't in the locally cached location-name
   * table yet -- most often because it's a container/ship item id, not a
   * resolvable station or structure. Render "Unknown location {id}". */
  locationName: string | null;
  locationFlag: string;
  quantity: number;
  ignoredForReconciliation: boolean;
}

export interface EsiHoldings {
  typeId: number;
  observedQuantity: number;
  ignoredQuantity: number;
  includedQuantity: number;
  observedAt: string | null;
  contributors: EsiHoldingContributor[];
}

export function getEsiHoldings(typeId: number): Promise<EsiHoldings> {
  return request(`/api/inventory/${typeId}/esi-holdings`);
}

export function setEsiHoldingIncluded(
  typeId: number,
  eveCharacterId: number,
  effectiveLocationId: number,
  included: boolean,
): Promise<EsiHoldings> {
  return request(
    `/api/inventory/${typeId}/esi-holdings/reconciliation-inclusion`,
    json("PUT", { eveCharacterId, effectiveLocationId, included }),
  );
}

export interface InventoryDetail extends InventoryItem {
  events: InventoryEvent[];
  reservations: InventoryReservation[];
}

export interface InventoryPostingInput {
  typeId: number;
  typeName: string;
  quantity: number;
  unitCost: string | null;
  costQuality: CostInputQuality;
  sourceReference: string;
  note: string;
  effectiveAt: string;
  expectedRevision: number;
  acknowledgeZeroCost: boolean;
}

export interface InventoryAdjustmentInput {
  typeId: number;
  typeName: string;
  quantityDelta: number;
  unitCost: string | null;
  sourceReference: string;
  note: string;
  expectedRevision: number;
}

export interface InventoryPreview {
  current: InventoryBalance;
  posting: {
    kind: "openingBalance" | "purchase" | "adjustment";
    quantityDelta: number;
    totalCostDelta: string;
    unitCost: string | null;
    costQuality: CostInputQuality;
  };
  resulting: InventoryBalance;
  warnings: string[];
}

export type InventoryListScope = "tracked" | "untracked";

/**
 * `scope` defaults to "tracked" (only types with an accounting balance) --
 * omitted from the query string entirely in that case, matching the API's
 * own default. "untracked" asks for ESI-observed types with no balance;
 * the server only does the extra SDE-name/market-price work those rows
 * need when this is explicitly requested.
 */
export function listInventory(priceSourceId?: string, scope: InventoryListScope = "tracked"): Promise<InventoryItem[]> {
  const params = new URLSearchParams();
  if (priceSourceId) params.set("priceSourceId", priceSourceId);
  if (scope !== "tracked") params.set("scope", scope);
  const query = params.toString();
  return request(`/api/inventory${query ? `?${query}` : ""}`);
}

export function getInventoryItem(typeId: number, priceSourceId?: string): Promise<InventoryDetail> {
  return request(`/api/inventory/${typeId}${priceSourceId ? `?priceSourceId=${priceSourceId}` : ""}`);
}

export function previewInventory(
  kind: "opening" | "purchase",
  input: InventoryPostingInput,
): Promise<InventoryPreview> {
  const path = kind === "opening" ? "opening-balance" : "purchases";
  return request(`/api/inventory/${path}/preview`, json("POST", input));
}

export function postInventory(
  kind: "opening" | "purchase",
  input: InventoryPostingInput,
): Promise<{ balance: InventoryBalance; events: InventoryEvent[] }> {
  const path = kind === "opening" ? "opening-balance" : "purchases";
  return request(`/api/inventory/${path}`, json("POST", input));
}

export function previewAdjustment(input: InventoryAdjustmentInput): Promise<InventoryPreview> {
  return request("/api/inventory/adjustments/preview", json("POST", input));
}

export function postAdjustment(
  input: InventoryAdjustmentInput,
): Promise<{ balance: InventoryBalance; events: InventoryEvent[] }> {
  return request("/api/inventory/adjustments", json("POST", input));
}

export interface InventoryExportItem {
  typeId: number;
  typeName: string;
  quantity: number;
  averageUnitCost: string | null;
}

export interface InventoryExport {
  exportedAt: string;
  items: InventoryExportItem[];
}

export interface InventoryImportItemResult {
  typeId: number;
  typeName: string;
  imported: boolean;
  message: string | null;
}

export interface InventoryImportResponse {
  results: InventoryImportItemResult[];
}

export function exportInventory(): Promise<InventoryExport> {
  return request("/api/inventory/export");
}

export function importInventory(items: InventoryExportItem[]): Promise<InventoryImportResponse> {
  return request("/api/inventory/import", json("POST", { items }));
}

export function reverseInventoryEvent(
  typeId: number,
  eventId: string,
  expectedRevision: number,
  reason: string,
): Promise<{ balance: InventoryBalance; events: InventoryEvent[] }> {
  return request(
    `/api/inventory/${typeId}/events/${eventId}/reverse`,
    json("POST", { expectedRevision, reason }),
  );
}

function json(method: string, body: unknown): RequestInit {
  return {
    method,
    headers: { "content-type": "application/json", Accept: "application/json" },
    body: JSON.stringify(body),
  };
}

const request = jsonRequest({
  unavailable: "ISK Works API is unavailable. Check that the backend is running.",
  failed: "Inventory request failed.",
});
