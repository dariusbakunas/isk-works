import type { AnalyticsGranularity, AnalyticsQuery } from "../../../api/finance-analytics";
import type { FinanceDirection } from "../../../api/finance";

export type RangePreset = "7d" | "30d" | "90d" | "ytd" | "custom";

export interface AnalyticsUrlState {
  preset: RangePreset;
  /** Only meaningful for `custom`. */
  dateFrom: string | null;
  dateTo: string | null;
  compare: boolean;
  /** Empty means every character. */
  characters: string[];
  /** `null` picks a granularity that suits the range. */
  granularity: AnalyticsGranularity | null;
  category: string | null;
  /** Leave out buys already recorded into Inventory. */
  excludeInventory: boolean;
}

const PRESETS: readonly RangePreset[] = ["7d", "30d", "90d", "ytd", "custom"];
const GRANULARITIES: readonly AnalyticsGranularity[] = ["day", "week", "month"];
const DATE_PATTERN = /^\d{4}-(0[1-9]|1[0-2])-(0[1-9]|[12]\d|3[01])$/;
const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const PRESET_DAYS: Record<"7d" | "30d" | "90d", number> = { "7d": 7, "30d": 30, "90d": 90 };

function isoDate(date: Date): string {
  return date.toISOString().slice(0, 10);
}

function validDate(value: string | null): string | null {
  if (!value || !DATE_PATTERN.test(value)) return null;
  return isoDate(new Date(`${value}T00:00:00Z`)) === value ? value : null;
}

export function parseAnalyticsUrlState(params: URLSearchParams): AnalyticsUrlState {
  const requested = params.get("range") as RangePreset | null;
  const from = validDate(params.get("from"));
  const to = validDate(params.get("to"));
  const customOk = requested === "custom" && from !== null && to !== null && from <= to;
  const preset: RangePreset =
    requested === "custom" ? (customOk ? "custom" : "30d") : requested && PRESETS.includes(requested) ? requested : "30d";
  const granularity = params.get("granularity") as AnalyticsGranularity | null;
  return {
    preset,
    dateFrom: preset === "custom" ? from : null,
    dateTo: preset === "custom" ? to : null,
    compare: params.get("compare") !== "0",
    characters: [...new Set((params.get("characters") ?? "").split(",").filter((id) => UUID_PATTERN.test(id)))],
    granularity: granularity && GRANULARITIES.includes(granularity) ? granularity : null,
    category: params.get("category")?.trim() || null,
    excludeInventory: params.get("excludeInventory") === "1",
  };
}

/** Defaults are left out so a fresh page has a clean URL. */
export function serializeAnalyticsUrlState(state: AnalyticsUrlState): URLSearchParams {
  const params = new URLSearchParams();
  if (state.preset !== "30d") params.set("range", state.preset);
  if (state.preset === "custom" && state.dateFrom && state.dateTo) {
    params.set("from", state.dateFrom);
    params.set("to", state.dateTo);
  }
  if (!state.compare) params.set("compare", "0");
  if (state.characters.length > 0) params.set("characters", state.characters.join(","));
  if (state.granularity) params.set("granularity", state.granularity);
  if (state.category) params.set("category", state.category);
  if (state.excludeInventory) params.set("excludeInventory", "1");
  return params;
}

/** Inclusive UTC window ending today (or the custom dates). */
export function resolveDateRange(
  state: Pick<AnalyticsUrlState, "preset" | "dateFrom" | "dateTo">,
  now: Date,
): { dateFrom: string; dateTo: string } {
  if (state.preset === "custom" && state.dateFrom && state.dateTo) {
    return { dateFrom: state.dateFrom, dateTo: state.dateTo };
  }
  const dateTo = isoDate(now);
  if (state.preset === "ytd") return { dateFrom: `${now.getUTCFullYear()}-01-01`, dateTo };
  const days = PRESET_DAYS[state.preset === "custom" ? "30d" : state.preset];
  const start = new Date(now);
  start.setUTCDate(start.getUTCDate() - (days - 1));
  return { dateFrom: isoDate(start), dateTo };
}

/** Day for two weeks or less, week up to a year, month beyond. */
export function autoGranularity(dateFrom: string, dateTo: string): AnalyticsGranularity {
  const days = (Date.parse(`${dateTo}T00:00:00Z`) - Date.parse(`${dateFrom}T00:00:00Z`)) / 86_400_000 + 1;
  if (days <= 14) return "day";
  return days <= 366 ? "week" : "month";
}

export function toAnalyticsQuery(state: AnalyticsUrlState, now: Date): AnalyticsQuery {
  const { dateFrom, dateTo } = resolveDateRange(state, now);
  return {
    connectionIds: state.characters,
    dateFrom,
    dateTo,
    granularity: state.granularity ?? autoGranularity(dateFrom, dateTo),
    comparePrevious: state.compare,
    category: state.category,
    excludeInventoryBuys: state.excludeInventory,
  };
}

/**
 * Deep link into Finance > Transactions carrying this page's filters plus the
 * chart's own (`category: null` drops the page-level category for that link).
 */
export function transactionsLink(
  state: AnalyticsUrlState,
  now: Date,
  extra: {
    direction?: FinanceDirection;
    category?: string | null;
    locationId?: number;
    typeId?: number;
    /** Display-only names for the chips Transactions shows for those ids. */
    itemLabel?: string;
    locationLabel?: string;
    /** Replaces the page range, for cards with their own fixed window. */
    range?: { dateFrom: string; dateTo: string };
  } = {},
): string {
  const { dateFrom, dateTo } = extra.range ?? resolveDateRange(state, now);
  const params = new URLSearchParams();
  if (state.characters.length > 0) params.set("connectionIds", state.characters.join(","));
  params.set("dateFrom", dateFrom);
  params.set("dateTo", dateTo);
  const category = extra.category === undefined ? state.category : extra.category;
  if (category) params.set("category", category);
  if (extra.direction && extra.direction !== "all") params.set("direction", extra.direction);
  if (extra.locationId !== undefined) params.set("locationId", String(extra.locationId));
  if (extra.typeId !== undefined) params.set("typeId", String(extra.typeId));
  if (extra.itemLabel) params.set("itemLabel", extra.itemLabel);
  if (extra.locationLabel) params.set("locationLabel", extra.locationLabel);
  if (state.excludeInventory) params.set("excludeInventoryBuys", "true");
  return `/finance/transactions?${params}`;
}
