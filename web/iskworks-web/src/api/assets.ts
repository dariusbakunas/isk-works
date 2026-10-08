import { ApiError, requestJson } from "./workspace";

export interface AssetSyncState {
  connectionId: string;
  characterName: string;
  connectionStatus: string;
  snapshotStatus: string | null;
  observedAt: string | null;
  rowCount: number | null;
}

export interface AssetBrowserSummary {
  locationCount: number;
  characterCount: number;
  stackCount: number;
  totalQuantity: number;
  totalPackagedVolume: string;
  latestObservedAt: string | null;
  unresolvedLocationCount: number;
  syncStates: AssetSyncState[];
}

export type AssetSortColumn = "item" | "quantity" | "packagedVolume" | "character" | "location" | "container" | "group" | "status" | "observed";
export type AssetSortDirection = "asc" | "desc";
export type AssetKindFilter = "blueprint" | "material" | "ship" | "container" | "other";
export type AssetBlueprintKindFilter = "original" | "copy" | "unknown";
export type AssetReconciliationFilter = "matched" | "difference" | "noAccountingRecord";

export interface AssetBrowserFacets {
  characters: AssetFilterOption[];
  locations: AssetFilterOption[];
  assetKinds: AssetFilterOption[];
  groups: AssetFilterOption[];
  blueprintKinds: AssetFilterOption[];
  reconciliationStates: AssetFilterOption[];
}

export interface FlatAssetRow {
  eveItemId: number;
  typeId: number;
  typeName: string | null;
  quantity: number;
  packagedVolume: string | null;
  totalPackagedVolume: string | null;
  ownerId: string;
  ownerName: string;
  connectionId: string;
  characterId: number;
  characterName: string;
  locationId: number;
  locationName: string | null;
  locationFlag: string;
  containerItemId: number | null;
  containerName: string | null;
  groupId: number | null;
  groupName: string | null;
  assetKind: string;
  observedAt: string;
  blueprint: AssetBrowserItem["blueprint"];
  reconciliation: AssetBrowserItem["reconciliation"];
  isContainer: boolean;
}

/** Summary and facets are workspace-wide, so only the first page (no cursor) carries them. */
export interface FlatAssetPage {
  rows: FlatAssetRow[];
  total: number;
  nextCursor: string | null;
  summary: AssetBrowserSummary | null;
  facets: AssetBrowserFacets | null;
}

export interface FlatAssetQuery {
  search?: string;
  connectionIds?: string[];
  locationIds?: number[];
  assetKinds?: AssetKindFilter[];
  groupIds?: number[];
  blueprintKinds?: AssetBlueprintKindFilter[];
  reconciliationStates?: AssetReconciliationFilter[];
  sort?: AssetSortColumn;
  order?: AssetSortDirection;
  cursor?: string;
  limit?: number;
}

export type AssetColumn = AssetSortColumn;

export interface AssetSyncOutcome {
  connectionId: string;
  characterName: string;
  succeeded: boolean;
  runs: Array<{ summary: string; cacheExpiresAt: string | null }>;
  error: string | null;
}

export interface AssetFilterOption {
  value: string;
  label: string;
  count: number;
}

export interface AssetBrowserItem {
  eveItemId: number;
  typeId: number;
  typeName: string;
  quantity: number;
  ownerId: string;
  ownerName: string;
  connectionId: string;
  characterId: number;
  characterName: string;
  locationId: number;
  locationFlag: string;
  parentItemId: number | null;
  groupId: number | null;
  groupName: string | null;
  assetKind: string;
  observedAt: string;
  blueprint: {
    kind: string;
    materialEfficiency: number;
    timeEfficiency: number;
    licensedRuns: number | null;
    observedAt: string;
  } | null;
  reconciliation: {
    state: string;
    observedOwnerTypeQuantity: number;
    accountedOwnerTypeQuantity: number;
    scope: "ownerType";
  };
  isContainer: boolean;
}

export function queryAssets(query: FlatAssetQuery, signal?: AbortSignal): Promise<FlatAssetPage> {
  return requestJson(`/api/assets${flatParams(query)}`, { signal });
}

export async function exportAssets(query: FlatAssetQuery, columns: AssetColumn[]): Promise<Blob> {
  const baseUrl = (import.meta.env.VITE_API_BASE_URL ?? "").replace(/\/$/, "");
  const queryString = flatParams(query);
  const values = new URLSearchParams(queryString.startsWith("?") ? queryString.slice(1) : queryString);
  values.set("columns", columns.join(","));
  const response = await fetch(`${baseUrl}/api/assets/export?${values}`, {
    headers: { Accept: "text/csv" },
    credentials: "include",
  });
  if (!response.ok) {
    const payload = await response.json().catch(() => null);
    throw new ApiError(response.status, payload?.error ?? {
      code: "asset_export_failed",
      message: "ISK Works could not export synchronized assets.",
    });
  }
  return response.blob();
}

export function syncAssets(connectionIds: string[]): Promise<AssetSyncOutcome[]> {
  return requestJson("/api/assets/sync", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ connectionIds }),
  });
}

function flatParams(query: FlatAssetQuery): string {
  const values = new URLSearchParams();
  Object.entries(query).forEach(([key, value]) => {
    if (Array.isArray(value) && value.length > 0) values.set(key, value.join(","));
    else if (value !== undefined && value !== "") values.set(key, String(value));
  });
  const encoded = values.toString();
  return encoded ? `?${encoded}` : "";
}
