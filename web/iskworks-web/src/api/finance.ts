import type { InventoryPreview } from "./inventory";
import { ApiError } from "./workspace";

export type FinanceTransactionType = "marketBuy" | "marketSell";
export type FinanceDirection = "all" | "income" | "expense";
export type FinanceSortColumn =
  | "time"
  | "character"
  | "transactionType"
  | "item"
  | "quantity"
  | "unitPrice"
  | "totalPrice"
  | "direction"
  | "counterparty"
  | "location"
  | "region";
export type SortDirection = "asc" | "desc";
export type FinanceColumn = FinanceSortColumn;
/** Columns the table can show. `inventory` is display-only: not sortable, not exported. */
export type FinanceDisplayColumn = FinanceColumn | "inventory";

export type FinanceInventoryState = "unrecorded" | "recorded" | "reverted" | "unavailable";

/** Inventory recording state of one Market Buy, exactly as the server derives it. */
export interface FinanceInventoryRecording {
  state: FinanceInventoryState;
  recordingId: string | null;
  recordedAt: string | null;
  revertedAt: string | null;
  quantity: number | null;
  totalBasis: string | null;
}

export interface FinanceFilter {
  connectionIds: string[];
  dateFrom: string | null;
  dateTo: string | null;
  search: string | null;
  transactionTypes: FinanceTransactionType[];
  direction: FinanceDirection;
  /** Analytics category label (server-defined); set by Analytics deep-links. */
  category?: string | null;
  locationId?: number | null;
  typeId?: number | null;
  /** Leave out buys currently recorded into Inventory. */
  excludeInventoryBuys?: boolean;
  page: number;
  pageSize: number;
}

export interface FinanceCharacter {
  connectionId: string;
  characterName: string;
  walletBalance: string | null;
  balanceObservedAt: string | null;
}

export interface FinanceTransaction {
  observationId: string;
  transactionId: number;
  connectionId: string;
  characterName: string;
  transactionType: FinanceTransactionType;
  typeId: number;
  typeName: string;
  quantity: number;
  unitPrice: string;
  totalPrice: string;
  transactedAt: string;
  counterpartyName: string | null;
  locationName: string | null;
  regionName: string | null;
  /** `null` for rows that can never be recorded (Market Sell). */
  inventoryRecording: FinanceInventoryRecording | null;
}

export interface FinanceSummary {
  walletBalance: string;
  income: string;
  expenses: string;
  netIsk: string;
  transactionCount: number;
  averageDailyIsk: string;
}

export interface FinanceTransactionPage {
  rows: FinanceTransaction[];
  summary: FinanceSummary;
  availableCharacters: FinanceCharacter[];
  totalCount: number;
  page: number;
  pageSize: number;
}

export interface SavedFinanceFilter {
  id: string;
  name: string;
  filter: FinanceFilter;
  createdAt: string;
  updatedAt: string;
}

export interface FinanceSyncOutcome {
  connectionId: string;
  characterName: string;
  succeeded: boolean;
  runs: Array<{ summary: string; cacheExpiresAt: string | null }>;
  error: string | null;
}

export interface FinanceQuery {
  filter: FinanceFilter;
  sort: FinanceSortColumn;
  order: SortDirection;
}

export function getFinanceTransactions(query: FinanceQuery, signal?: AbortSignal): Promise<FinanceTransactionPage> {
  return request(`/api/finance/transactions?${queryString(query)}`, { signal });
}

export async function exportFinanceTransactions(query: FinanceQuery, columns: FinanceColumn[]): Promise<Blob> {
  const baseUrl = (import.meta.env.VITE_API_BASE_URL ?? "").replace(/\/$/, "");
  const params = queryString(query);
  params.set("columns", columns.join(","));
  const response = await fetch(`${baseUrl}/api/finance/transactions/export?${params}`, {
    headers: { Accept: "text/csv" },
    credentials: "include",
  });
  if (!response.ok) throw await apiError(response);
  return response.blob();
}

export function syncFinance(connectionIds: string[]): Promise<FinanceSyncOutcome[]> {
  return request("/api/finance/sync", json("POST", { connectionIds }));
}

/**
 * What `recordFinanceInventory` would post right now -- derived server-side
 * exactly like the recording, against the owner's current balance. Writes
 * nothing.
 */
export function previewFinanceInventory(observationId: string): Promise<InventoryPreview> {
  return request(`/api/finance/transactions/${observationId}/inventory-recording/preview`, { method: "POST" });
}

/**
 * Adds one Market Buy to accounting Inventory. No body: the server derives
 * type, quantity, cost and owner from the stored transaction. Idempotent -- an
 * already-recorded transaction resolves to its existing recording.
 */
export function recordFinanceInventory(observationId: string): Promise<FinanceInventoryRecording> {
  return request(`/api/finance/transactions/${observationId}/inventory-recording`, { method: "POST" });
}

export function revertFinanceInventoryRecording(
  observationId: string,
  recordingId: string,
): Promise<FinanceInventoryRecording> {
  return request(
    `/api/finance/transactions/${observationId}/inventory-recording/${recordingId}/revert`,
    { method: "POST" },
  );
}

export function listSavedFinanceFilters(): Promise<SavedFinanceFilter[]> {
  return request("/api/finance/saved-filters");
}

export function saveFinanceFilter(name: string, filter: FinanceFilter): Promise<SavedFinanceFilter> {
  return request("/api/finance/saved-filters", json("POST", { name, filter }));
}

export function deleteSavedFinanceFilter(id: string): Promise<void> {
  return request(`/api/finance/saved-filters/${id}`, { method: "DELETE" });
}

function queryString({ filter, sort, order }: FinanceQuery) {
  const params = new URLSearchParams();
  if (filter.connectionIds.length > 0) params.set("connectionIds", filter.connectionIds.join(","));
  if (filter.dateFrom) params.set("dateFrom", filter.dateFrom);
  if (filter.dateTo) params.set("dateTo", filter.dateTo);
  if (filter.search) params.set("search", filter.search);
  params.set("transactionTypes", filter.transactionTypes.join(","));
  params.set("direction", filter.direction);
  if (filter.category) params.set("category", filter.category);
  if (filter.locationId != null) params.set("locationId", String(filter.locationId));
  if (filter.typeId != null) params.set("typeId", String(filter.typeId));
  if (filter.excludeInventoryBuys) params.set("excludeInventoryBuys", "true");
  params.set("page", String(filter.page));
  params.set("pageSize", String(filter.pageSize));
  params.set("sort", sort);
  params.set("order", order);
  return params;
}

function json(method: string, body: unknown): RequestInit {
  return {
    method,
    headers: { "content-type": "application/json", Accept: "application/json" },
    body: JSON.stringify(body),
  };
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const baseUrl = (import.meta.env.VITE_API_BASE_URL ?? "").replace(/\/$/, "");
  let response: Response;
  try {
    response = await fetch(`${baseUrl}${path}`, {
      headers: { Accept: "application/json", ...init?.headers },
      credentials: "include",
      ...init,
    });
  } catch {
    throw new ApiError(0, { code: "api_unavailable", message: "ISK Works API is unavailable." });
  }
  if (!response.ok) throw await apiError(response);
  if (response.status === 204) return undefined as T;
  return response.json() as Promise<T>;
}

async function apiError(response: Response) {
  const payload = await response.json().catch(() => null);
  return new ApiError(response.status, payload?.error ?? {
    code: "api_error",
    message: "Finance request failed.",
  });
}
