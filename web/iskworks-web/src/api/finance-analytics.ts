import { ApiError } from "./workspace";
import type { FinanceCharacter } from "./finance";

export type AnalyticsGranularity = "day" | "week" | "month";

/** Money is always a decimal string; never do arithmetic on it in floats. */
export interface AnalyticsRange {
  dateFrom: string;
  dateTo: string;
  previousDateFrom: string | null;
  previousDateTo: string | null;
  granularity: AnalyticsGranularity;
}

export interface AnalyticsDelta {
  previous: string;
  /** `null` when there is no baseline to divide by. */
  percent: number | null;
  /** Previous was zero and there is something now. */
  isNew: boolean;
}

export interface AnalyticsKpiValue {
  value: string;
  delta: AnalyticsDelta | null;
  sparkline: string[];
}

export interface FeesKpi {
  value: string;
  /** Withheld when the journal does not cover the whole previous window. */
  delta: AnalyticsDelta | null;
  sparkline: string[];
  brokersFee: string;
  transactionTax: string;
  marketProviderTax: string;
  /** First day the wallet journal has, so partial coverage can be called out. */
  availableFrom: string | null;
}

export interface AnalyticsKpis {
  income: AnalyticsKpiValue;
  expenses: AnalyticsKpiValue;
  net: AnalyticsKpiValue;
  margin: { percent: number | null; previousPercent: number | null; sparkline: number[] };
  walletBalance: { value: string | null; delta: AnalyticsDelta | null; sparkline: string[] };
  /** `null` without journal data, or while a category is selected. */
  fees: FeesKpi | null;
  transactionCount: number;
}

export interface CashFlowBucket {
  start: string;
  income: string;
  expenses: string;
  net: string;
  cumulativeNet: string;
}

export interface CategoryTotal {
  category: string;
  total: string;
  previous: string | null;
}

export interface CharacterTotal {
  connectionId: string;
  characterName: string;
  income: string;
  expenses: string;
  net: string;
}

export interface LocationTotal {
  locationId: number;
  locationName: string;
  regionName: string | null;
  income: string;
  expenses: string;
  net: string;
  transactionCount: number;
}

export interface TopItem {
  typeId: number;
  typeName: string;
  category: string;
  quantity: number;
  averageUnitPrice: string;
  total: string;
  sharePercent: number | null;
  trend: string[];
}

export interface DayNet {
  date: string;
  net: string;
}

export type InsightKind = "categoryChange" | "locationConcentration" | "characterConcentration" | "feeBurden";

export interface Insight {
  kind: InsightKind;
  side: "spending" | "income" | null;
  subject: string;
  amount: string;
  previous: string | null;
  total: string | null;
  sharePercent: number | null;
  changePercent: number | null;
}

export interface FinanceAnalytics {
  range: AnalyticsRange;
  earliestObservedAt: string | null;
  availableCharacters: FinanceCharacter[];
  excludedIntraAccount: { transactionCount: number; totalIsk: string };
  excludedInventoryBuys: { transactionCount: number; totalIsk: string };
  kpis: AnalyticsKpis;
  cashFlow: CashFlowBucket[];
  spendingByCategory: CategoryTotal[];
  incomeByCategory: CategoryTotal[];
  byCharacter: CharacterTotal[];
  byLocation: LocationTotal[];
  topExpenses: TopItem[];
  topEarners: TopItem[];
  heatmap: DayNet[];
  insights: Insight[];
}

export type AnalyticsSection =
  | "kpis"
  | "cashFlow"
  | "spending"
  | "income"
  | "flow"
  | "characters"
  | "locations"
  | "topExpenses"
  | "topEarners"
  | "heatmap";

/** The filters the server aggregates by. Dates are inclusive `YYYY-MM-DD`. */
export interface AnalyticsQuery {
  connectionIds: string[];
  dateFrom: string;
  dateTo: string;
  granularity: AnalyticsGranularity;
  comparePrevious: boolean;
  category: string | null;
  /** Leave out buys already recorded into Inventory: build inputs, not spend. */
  excludeInventoryBuys: boolean;
}

export function getFinanceAnalytics(query: AnalyticsQuery, signal?: AbortSignal): Promise<FinanceAnalytics> {
  return request(`/api/finance/analytics?${analyticsQueryString(query)}`, signal);
}

export async function exportFinanceAnalytics(query: AnalyticsQuery, section: AnalyticsSection): Promise<Blob> {
  const params = analyticsQueryString(query);
  params.set("section", section);
  const response = await fetchApi(`/api/finance/analytics/export?${params}`, { Accept: "text/csv" });
  return response.blob();
}

function analyticsQueryString(query: AnalyticsQuery): URLSearchParams {
  const params = new URLSearchParams();
  if (query.connectionIds.length > 0) params.set("connectionIds", query.connectionIds.join(","));
  params.set("dateFrom", query.dateFrom);
  params.set("dateTo", query.dateTo);
  params.set("granularity", query.granularity);
  params.set("comparePrevious", String(query.comparePrevious));
  if (query.category) params.set("category", query.category);
  if (query.excludeInventoryBuys) params.set("excludeInventoryBuys", "true");
  return params;
}

async function request<T>(path: string, signal?: AbortSignal): Promise<T> {
  const response = await fetchApi(path, { Accept: "application/json" }, signal);
  return response.json() as Promise<T>;
}

async function fetchApi(path: string, headers: Record<string, string>, signal?: AbortSignal): Promise<Response> {
  const baseUrl = (import.meta.env.VITE_API_BASE_URL ?? "").replace(/\/$/, "");
  let response: Response;
  try {
    response = await fetch(`${baseUrl}${path}`, { headers, credentials: "include", signal });
  } catch (error) {
    if (error instanceof DOMException && error.name === "AbortError") throw error;
    throw new ApiError(0, { code: "api_unavailable", message: "ISK Works API is unavailable." });
  }
  if (!response.ok) {
    const payload = await response.json().catch(() => null);
    throw new ApiError(
      response.status,
      payload?.error ?? { code: "api_error", message: "Finance analytics request failed." },
    );
  }
  return response;
}
