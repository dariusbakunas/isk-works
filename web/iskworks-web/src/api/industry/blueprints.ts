import { request } from "./request";

// Mirrors `routes/blueprint_library.rs`: every synced character blueprint,
// classified by the item it makes.

export type BlueprintLibraryKind = "original" | "copy" | "unknown";
export type BlueprintLibraryActivity = "manufacturing" | "reaction";
export type BlueprintLibrarySort =
  | "blueprint"
  | "product"
  | "kind"
  | "me"
  | "te"
  | "runs"
  | "character"
  | "location"
  | "observed";
export type BlueprintLibraryOrder = "asc" | "desc";

export interface BlueprintLibraryBuildRef {
  buildId: string;
  name: string;
}

export interface BlueprintLibraryRow {
  observationId: string;
  eveItemId: number;
  blueprintTypeId: number;
  blueprintName: string;
  activity: BlueprintLibraryActivity;
  productTypeId: number | null;
  productName: string | null;
  productGroupId: number | null;
  productGroupName: string | null;
  productCategoryId: number | null;
  productCategoryName: string | null;
  kind: BlueprintLibraryKind;
  materialEfficiency: number;
  timeEfficiency: number;
  /** `null` for originals (unlimited) and exhausted copies. */
  licensedRuns: number | null;
  connectionId: string | null;
  characterName: string | null;
  locationId: number;
  locationName: string | null;
  locationFlag: string;
  observedAt: string;
  usedByBuilds: BlueprintLibraryBuildRef[];
}

export interface BlueprintLibrarySummary {
  originals: number;
  copies: number;
  characters: number;
  latestObservedAt: string | null;
}

export interface BlueprintLibraryPage {
  rows: BlueprintLibraryRow[];
  total: number;
  summary: BlueprintLibrarySummary;
}

export interface BlueprintFilterOption {
  value: string;
  label: string;
  count: number;
}

export interface BlueprintGroupFacet extends BlueprintFilterOption {
  categoryId: number | null;
}

export interface BlueprintLibraryFacets {
  categories: BlueprintFilterOption[];
  groups: BlueprintGroupFacet[];
  characters: BlueprintFilterOption[];
  locations: BlueprintFilterOption[];
}

export interface BlueprintLibraryQuery {
  search?: string;
  kinds?: BlueprintLibraryKind[];
  categoryIds?: number[];
  groupIds?: number[];
  connectionIds?: string[];
  locationIds?: number[];
  minMe?: number | null;
  minTe?: number | null;
  usableOnly?: boolean;
  activity?: BlueprintLibraryActivity | null;
  sort?: BlueprintLibrarySort;
  order?: BlueprintLibraryOrder;
  limit?: number;
  offset?: number;
}

export function blueprintLibraryParams(query: BlueprintLibraryQuery): URLSearchParams {
  const params = new URLSearchParams();
  const list = (key: string, values: readonly (string | number)[] | undefined) => {
    if (values && values.length > 0) params.set(key, values.join(","));
  };
  if (query.search?.trim()) params.set("search", query.search.trim());
  list("kinds", query.kinds);
  list("categoryIds", query.categoryIds);
  list("groupIds", query.groupIds);
  list("connectionIds", query.connectionIds);
  list("locationIds", query.locationIds);
  if (query.minMe != null) params.set("minMe", String(query.minMe));
  if (query.minTe != null) params.set("minTe", String(query.minTe));
  if (query.usableOnly) params.set("usableOnly", "true");
  if (query.activity) params.set("activity", query.activity);
  if (query.sort) params.set("sort", query.sort);
  if (query.order) params.set("order", query.order);
  if (query.limit != null) params.set("limit", String(query.limit));
  if (query.offset) params.set("offset", String(query.offset));
  return params;
}

export function listBlueprintLibrary(
  query: BlueprintLibraryQuery,
  init?: RequestInit,
): Promise<BlueprintLibraryPage> {
  const params = blueprintLibraryParams(query).toString();
  return request(`/api/industry/blueprints${params ? `?${params}` : ""}`, init);
}

export function getBlueprintLibraryFacets(): Promise<BlueprintLibraryFacets> {
  return request("/api/industry/blueprints/filters");
}
