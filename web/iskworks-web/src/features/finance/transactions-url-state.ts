import type { FinanceDirection, FinanceFilter } from "../../api/finance";

const DATE_PATTERN = /^\d{4}-(0[1-9]|1[0-2])-(0[1-9]|[12]\d|3[01])$/;
const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const DIRECTIONS: readonly FinanceDirection[] = ["all", "income", "expense"];

function positiveInteger(value: string | null): number | undefined {
  if (value === null || !/^\d+$/.test(value)) return undefined;
  const number = Number(value);
  return Number.isSafeInteger(number) && number > 0 ? number : undefined;
}

/**
 * The filters a link into Transactions can carry (Analytics' "View
 * transactions" uses this). Only what is present and well-formed is returned,
 * so the page keeps its own defaults for everything else. The parameter names
 * match the API's.
 */
export function parseTransactionsUrlFilter(params: URLSearchParams): Partial<FinanceFilter> {
  const filter: Partial<FinanceFilter> = {};
  const connectionIds = (params.get("connectionIds") ?? "")
    .split(",")
    .filter((id) => UUID_PATTERN.test(id));
  if (connectionIds.length > 0) filter.connectionIds = [...new Set(connectionIds)];
  const dateFrom = params.get("dateFrom");
  if (dateFrom && DATE_PATTERN.test(dateFrom)) filter.dateFrom = dateFrom;
  const dateTo = params.get("dateTo");
  if (dateTo && DATE_PATTERN.test(dateTo)) filter.dateTo = dateTo;
  const direction = params.get("direction") as FinanceDirection | null;
  if (direction && DIRECTIONS.includes(direction)) filter.direction = direction;
  const category = params.get("category")?.trim();
  if (category) filter.category = category;
  const locationId = positiveInteger(params.get("locationId"));
  if (locationId !== undefined) filter.locationId = locationId;
  const typeId = positiveInteger(params.get("typeId"));
  if (typeId !== undefined) filter.typeId = typeId;
  if (params.get("excludeInventoryBuys") === "true") filter.excludeInventoryBuys = true;
  return filter;
}

/**
 * Names Analytics passes along for the ids above, purely so the filter chips
 * can say "Tritanium" instead of "Item #34". They never reach the API.
 */
export function parseTransactionsUrlLabels(params: URLSearchParams): { item?: string; location?: string } {
  const labels: { item?: string; location?: string } = {};
  const item = params.get("itemLabel")?.trim();
  if (item) labels.item = item;
  const location = params.get("locationLabel")?.trim();
  if (location) labels.location = location;
  return labels;
}
