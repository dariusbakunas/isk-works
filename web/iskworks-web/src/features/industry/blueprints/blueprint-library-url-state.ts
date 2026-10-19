import type {
  BlueprintLibraryActivity,
  BlueprintLibraryKind,
  BlueprintLibraryOrder,
  BlueprintLibraryQuery,
  BlueprintLibrarySort,
} from "../../../api/industry";

export type BlueprintGrouping = "category" | "location" | "none";

/** Everything the page's filters hold; lives in the URL so views can be linked. */
export interface BlueprintLibraryView {
  search: string;
  kinds: BlueprintLibraryKind[];
  categoryIds: number[];
  groupIds: number[];
  connectionIds: string[];
  locationIds: number[];
  minMe: number | null;
  minTe: number | null;
  usableOnly: boolean;
  activity: BlueprintLibraryActivity | null;
  sort: BlueprintLibrarySort;
  order: BlueprintLibraryOrder;
  groupBy: BlueprintGrouping;
}

export const DEFAULT_VIEW: BlueprintLibraryView = {
  search: "",
  kinds: [],
  categoryIds: [],
  groupIds: [],
  connectionIds: [],
  locationIds: [],
  minMe: null,
  minTe: null,
  usableOnly: false,
  activity: null,
  sort: "blueprint",
  order: "asc",
  groupBy: "category",
};

const KINDS: readonly BlueprintLibraryKind[] = ["original", "copy", "unknown"];
const ACTIVITIES: readonly BlueprintLibraryActivity[] = ["manufacturing", "reaction"];
const SORTS: readonly BlueprintLibrarySort[] = [
  "blueprint", "product", "kind", "me", "te", "runs", "character", "location", "observed",
];
const GROUPINGS: readonly BlueprintGrouping[] = ["category", "location", "none"];
const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

function ids(value: string | null): number[] {
  return [...new Set((value ?? "").split(",").filter((entry) => /^\d+$/.test(entry)).map(Number))]
    .filter((id) => Number.isSafeInteger(id) && id > 0);
}

function bounded(value: string | null, max: number): number | null {
  if (value === null || !/^\d+$/.test(value)) return null;
  const number = Number(value);
  return number <= max ? number : null;
}

function oneOf<T extends string>(value: string | null, allowed: readonly T[]): T | null {
  return value !== null && (allowed as readonly string[]).includes(value) ? (value as T) : null;
}

/** Malformed or unknown values fall back to the defaults rather than erroring. */
export function parseBlueprintLibraryView(params: URLSearchParams): BlueprintLibraryView {
  return {
    search: params.get("search")?.trim() ?? "",
    kinds: [...new Set((params.get("kinds") ?? "").split(","))]
      .filter((kind): kind is BlueprintLibraryKind => (KINDS as readonly string[]).includes(kind)),
    categoryIds: ids(params.get("categoryIds")),
    groupIds: ids(params.get("groupIds")),
    connectionIds: [...new Set((params.get("connectionIds") ?? "").split(","))]
      .filter((id) => UUID_PATTERN.test(id)),
    locationIds: ids(params.get("locationIds")),
    minMe: bounded(params.get("minMe"), 10),
    minTe: bounded(params.get("minTe"), 20),
    usableOnly: params.get("usableOnly") === "true",
    activity: oneOf(params.get("activity"), ACTIVITIES),
    sort: oneOf(params.get("sort"), SORTS) ?? DEFAULT_VIEW.sort,
    order: params.get("order") === "desc" ? "desc" : "asc",
    groupBy: oneOf(params.get("groupBy"), GROUPINGS) ?? DEFAULT_VIEW.groupBy,
  };
}

/** Only non-default values are written, so a plain /builds/blueprints stays clean. */
export function blueprintLibraryViewParams(view: BlueprintLibraryView): URLSearchParams {
  const params = new URLSearchParams();
  const list = (key: string, values: readonly (string | number)[]) => {
    if (values.length > 0) params.set(key, values.join(","));
  };
  if (view.search.trim()) params.set("search", view.search.trim());
  list("kinds", view.kinds);
  list("categoryIds", view.categoryIds);
  list("groupIds", view.groupIds);
  list("connectionIds", view.connectionIds);
  list("locationIds", view.locationIds);
  if (view.minMe !== null) params.set("minMe", String(view.minMe));
  if (view.minTe !== null) params.set("minTe", String(view.minTe));
  if (view.usableOnly) params.set("usableOnly", "true");
  if (view.activity) params.set("activity", view.activity);
  if (view.sort !== DEFAULT_VIEW.sort) params.set("sort", view.sort);
  if (view.order !== DEFAULT_VIEW.order) params.set("order", view.order);
  if (view.groupBy !== DEFAULT_VIEW.groupBy) params.set("groupBy", view.groupBy);
  return params;
}

/** The API query for a view (grouping is client-side only). */
export function blueprintLibraryQuery(view: BlueprintLibraryView): BlueprintLibraryQuery {
  const { groupBy: _groupBy, ...query } = view;
  return query;
}

export function hasActiveFilters(view: BlueprintLibraryView): boolean {
  return (
    view.search.trim() !== ""
    || view.kinds.length > 0
    || view.categoryIds.length > 0
    || view.groupIds.length > 0
    || view.connectionIds.length > 0
    || view.locationIds.length > 0
    || view.minMe !== null
    || view.minTe !== null
    || view.usableOnly
    || view.activity !== null
  );
}
