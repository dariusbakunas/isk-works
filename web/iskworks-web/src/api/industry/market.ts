import { json, request } from "./request";
import type { Money } from "./shared";

export interface PriceSourceItem {
  typeId: number;
  typeName: string;
  price: Money;
  note: string;
  updatedAt: string;
}

export interface PriceSource {
  id: string;
  workspaceId: string;
  name: string;
  description: string;
  kind: "manual" | "eveClientMarketExport" | "esiMarketOrders";
  revision: number;
  itemCount: number;
  recentBuildCount: number;
  items: PriceSourceItem[];
  createdAt: string;
  updatedAt: string;
}

export type MarketPricingPolicy =
  | "lowestSell"
  | "highestBuy"
  | "acquireQuantityFromSellOrders"
  | "liquidateQuantityIntoBuyOrders";

export interface MarketImportProblem {
  code: string;
  message: string;
  row: number | null;
  column: string | null;
}

export interface MarketImportPreviewFile {
  filename: string;
  fileChecksum: string;
  normalizedChecksum: string;
  fileSizeBytes: number;
  typeId: number | null;
  typeName: string | null;
  locationId: number | null;
  locationName: string | null;
  solarSystemId: number | null;
  solarSystemName: string | null;
  regionId: number | null;
  regionName: string | null;
  observedAt: string | null;
  timestampSource: "filename" | "userSupplied" | "importTime" | null;
  rowCount: number;
  buyOrderCount: number;
  sellOrderCount: number;
  lowestSell: Money | null;
  highestBuy: Money | null;
  totalBuyVolume: number;
  totalSellVolume: number;
  duplicateOrderCount: number;
  alreadyImported: boolean;
  canImport: boolean;
  warnings: string[];
  errors: MarketImportProblem[];
}

export interface MarketImportPreview {
  files: MarketImportPreviewFile[];
  totalFiles: number;
  validFiles: number;
  invalidFiles: number;
  duplicateFiles: number;
  itemCount: number;
  locationCount: number;
  totalRows: number;
  oldestObservationAt: string | null;
  newestObservationAt: string | null;
}

export interface ImportedMarketFile {
  id: string;
  batchId: string;
  filename: string;
  fileChecksum: string;
  normalizedChecksum: string;
  fileSizeBytes: number;
  observedAt: string;
  timestampSource: "filename" | "userSupplied" | "importTime";
  typeId: number;
  typeName: string;
  locationId: number;
  locationName: string;
  solarSystemId: number;
  solarSystemName: string | null;
  regionId: number;
  regionName: string | null;
  rowCount: number;
  buyOrderCount: number;
  sellOrderCount: number;
  importedAt: string;
}

export interface MarketImportBatch {
  id: string;
  workspaceId: string;
  observedAtMin: string;
  observedAtMax: string;
  importedAt: string;
  fileCount: number;
  itemCount: number;
  locationCount: number;
  observationCount: number;
  skippedDuplicateFileCount: number;
  warnings: string[];
  files: ImportedMarketFile[];
}

export interface MarketImportResult {
  batch: MarketImportBatch | null;
  importedFiles: number;
  skippedDuplicateFiles: number;
  failedFiles: MarketImportPreviewFile[];
  importedObservations: number;
  warnings: string[];
}

export interface MarketLocationResolution {
  configured: boolean;
  resolved: Array<{
    locationId: number;
    locationName: string;
  }>;
  unresolvedLocationIds: number[];
  eligibleCharacterCount: number;
  needsReconnection: boolean;
  warnings: string[];
}

export function listPriceSources(): Promise<PriceSource[]> {
  return request("/api/price-sources");
}

export function getPriceSource(id: string): Promise<PriceSource> {
  return request(`/api/price-sources/${id}`);
}

export function createPriceSource(input: {
  name: string;
  description: string;
}): Promise<PriceSource> {
  return request("/api/price-sources", json("POST", input));
}

export function updatePriceSource(id: string, input: {
  expectedRevision: number;
  name: string;
  description: string;
}): Promise<PriceSource> {
  return request(`/api/price-sources/${id}`, json("PUT", input));
}

export function upsertPriceItem(
  sourceId: string,
  typeId: number,
  input: { expectedRevision: number; typeName: string; price: string; note: string },
): Promise<PriceSource> {
  return request(`/api/price-sources/${sourceId}/items/${typeId}`, json("PUT", input));
}

export function removePriceItem(sourceId: string, typeId: number, expectedRevision: number): Promise<PriceSource> {
  return request(
    `/api/price-sources/${sourceId}/items/${typeId}?expectedRevision=${expectedRevision}`,
    { method: "DELETE" },
  );
}

export function deletePriceSource(id: string, expectedRevision: number): Promise<void> {
  return request(`/api/price-sources/${id}?expectedRevision=${expectedRevision}`, { method: "DELETE" });
}

function marketFilesForm(files: File[], observedAt?: string): FormData {
  const form = new FormData();
  files.forEach((file) => form.append("files", file, file.name));
  if (observedAt) form.append("observedAt", observedAt);
  return form;
}

export function previewMarketExports(files: File[], observedAt?: string): Promise<MarketImportPreview> {
  return request("/api/industry/market-imports/preview", {
    method: "POST",
    body: marketFilesForm(files, observedAt),
  });
}

export function importMarketExports(files: File[], observedAt?: string): Promise<MarketImportResult> {
  return request("/api/industry/market-imports", {
    method: "POST",
    body: marketFilesForm(files, observedAt),
  });
}

export function resolveMarketLocations(locationIds: number[]): Promise<MarketLocationResolution> {
  return request(
    "/api/industry/market-locations/resolve",
    json("POST", { locationIds }),
  );
}

// Which connected character (if any) can read a structure's market -- a
// separate ESI scope/check from name resolution above, so a structure can
// be named but still have no confirmed market access.
export type StructureMarketAccess =
  | { access: "confirmed"; characterName: string }
  | { access: "noEligibleCharacter" }
  | { access: "denied" };

export function verifyStructureMarketAccess(locationId: number): Promise<StructureMarketAccess> {
  return request(
    `/api/industry/market-locations/${locationId}/verify-access`,
    { method: "POST" },
  );
}

export function listMarketImports(): Promise<MarketImportBatch[]> {
  return request("/api/industry/market-imports");
}

// ─── Market Browser ─────────────────────────────────────────────────────
//
// Reads for the /api/market/* reference-data and item endpoints.
// Deliberately independent of `PriceSource` above -- these
// take a plain region/location scope, not a source id.

export interface MarketRegion {
  regionId: number;
  regionName: string;
}

export interface MarketLocation {
  locationId: number;
  locationName: string;
  kind: "npcStation" | "structure";
  solarSystemId: number;
  solarSystemName: string;
  stationTypeId: number | null;
  stationTypeName: string | null;
  securityClass: string | null;
  structureTypeId: number | null;
  freshness: ScopeFreshness;
}

// Deliberately not a single "last updated" timestamp -- coverage is
// tracked per type_id, and a scope can have many tracked types registered
// at very different times. A bare mostRecentObservedAt would report only
// the single freshest tracked item, silently implying the whole scope is
// that fresh even when most of it is older or never fetched. Render
// trackedTypeCount/observedTypeCount alongside it (e.g. "12 of 340 items
// observed, most recently 3m ago") -- never mostRecentObservedAt alone.
export interface ScopeFreshness {
  trackedTypeCount: number;
  observedTypeCount: number;
  mostRecentObservedAt: string | null;
}

export type MarketAccessState = "unknown" | "confirmed" | "expired";

export interface MarketHubSummary {
  locationId: number;
  locationName: string;
  shortName: string;
  solarSystemId: number;
  solarSystemName: string;
  regionId: number;
  regionName: string;
  freshness: ScopeFreshness;
}

export interface MarketStructureSummary {
  locationId: number;
  locationName: string;
  structureTypeId: number | null;
  structureTypeName: string | null;
  solarSystemId: number;
  solarSystemName: string | null;
  regionId: number | null;
  regionName: string | null;
  securityClass: string;
  accessState: MarketAccessState;
  accessCharacterName: string | null;
  accessCheckedAt: string | null;
  freshness: ScopeFreshness;
}

// Internally tagged on `kind`, mirroring the backend's
// MarketLocationSearchResult -- a Hub and an NpcStation share the same
// underlying source and are only distinguished by `kind`, same for every
// other pair of fields that overlaps between variants.
export type MarketLocationSearchResult =
  | {
      kind: "hub";
      locationId: number;
      displayName: string;
      solarSystemId: number;
      solarSystemName: string;
      regionId: number;
      regionName: string;
      freshness: ScopeFreshness;
    }
  | {
      kind: "npcStation";
      locationId: number;
      displayName: string;
      solarSystemId: number;
      solarSystemName: string;
      regionId: number;
      regionName: string;
      securityClass: string;
      freshness: ScopeFreshness;
    }
  | {
      kind: "structure";
      locationId: number;
      displayName: string;
      structureTypeId: number | null;
      structureTypeName: string | null;
      solarSystemId: number;
      solarSystemName: string | null;
      regionId: number | null;
      regionName: string | null;
      securityClass: string;
      accessState: MarketAccessState;
      accessCharacterName: string | null;
      freshness: ScopeFreshness;
    }
  | {
      kind: "region";
      regionId: number;
      displayName: string;
    };

export interface MarketCategoryNode {
  marketGroupId: number;
  name: string;
  itemCount: number;
  children: MarketCategoryNode[];
}

export interface MarketItemSummary {
  typeId: number;
  typeName: string;
  bestSell: Money | null;
  bestBuy: Money | null;
  spread: Money | null;
  sellOrderCount: number;
  buyOrderCount: number;
  observedAt: string | null;
}

export interface MarketItemPage {
  rows: MarketItemSummary[];
  totalCount: number;
  page: number;
  pageSize: number;
}

export interface MarketOrderRow {
  price: Money;
  quantity: number;
  minQuantity: number;
  locationId: number;
  // Resolved station/structure name (NPC stations from the SDE, including
  // security status; player structures from workspace-resolved names) --
  // falls back to a "Location {id}" string server-side when neither source
  // has it, so this is always safe to render directly.
  locationName: string;
  // ESI's order range as a short label ("Station"/"System"/"Region"/"N
  // jumps") -- only meaningful for buy orders.
  orderRange: string;
  observedAt: string;
  // issued_at + duration_days, computed server-side.
  expiresAt: string;
}

export interface MarketItemMarketData {
  bestSell: Money | null;
  bestBuy: Money | null;
  spread: Money | null;
  sellOrderCount: number;
  buyOrderCount: number;
  sellVolume: number;
  observedAt: string | null;
}

export interface MarketItemOrders {
  typeId: number;
  typeName: string | null;
  marketGroupId: number | null;
  summary: MarketItemMarketData;
  sellOrders: MarketOrderRow[];
  buyOrders: MarketOrderRow[];
}

export function listMarketRegions(): Promise<MarketRegion[]> {
  return request("/api/market/regions");
}

export function listMarketRegionLocations(regionId: number): Promise<MarketLocation[]> {
  return request(`/api/market/regions/${regionId}/locations`);
}

export function listMarketHubs(): Promise<MarketHubSummary[]> {
  return request("/api/market/hubs");
}

export function listMarketStructures(): Promise<MarketStructureSummary[]> {
  return request("/api/market/structures");
}

export function searchMarketLocations(query: string): Promise<MarketLocationSearchResult[]> {
  return request(`/api/market/locations/search?q=${encodeURIComponent(query)}`);
}

export function listMarketCategories(): Promise<MarketCategoryNode[]> {
  return request("/api/market/categories");
}

export interface MarketItemsQuery {
  regionId: number;
  locationId?: number;
  marketGroupId?: number;
  search?: string;
  page?: number;
  pageSize?: number;
}

export function listMarketItems(query: MarketItemsQuery): Promise<MarketItemPage> {
  const params = new URLSearchParams({ regionId: String(query.regionId) });
  if (query.locationId !== undefined) params.set("locationId", String(query.locationId));
  if (query.marketGroupId !== undefined) params.set("marketGroupId", String(query.marketGroupId));
  if (query.search) params.set("search", query.search);
  if (query.page !== undefined) params.set("page", String(query.page));
  if (query.pageSize !== undefined) params.set("pageSize", String(query.pageSize));
  return request(`/api/market/items?${params.toString()}`);
}

export function getMarketItemOrders(
  typeId: number,
  regionId: number,
  locationId?: number,
): Promise<MarketItemOrders> {
  const params = new URLSearchParams({ regionId: String(regionId) });
  if (locationId !== undefined) params.set("locationId", String(locationId));
  return request(`/api/market/items/${typeId}/orders?${params.toString()}`);
}

// A region, optionally narrowed to one location within it -- the same
// value MarketScope plays server-side (crates/iskworks-core/src/market.rs).
// `locationId: undefined` means region-wide/"all locations".
export interface MarketScope {
  regionId: number;
  locationId?: number;
}

/** The market-evidence identity one valuation resolved prices against, for
 * one `MarketScope` -- the completed ESI batches + latest import batch it
 * pinned, the newest `observedAt` it saw, and the frozen `asOf` clock used
 * for freshness classification. Returned per scope on `BuildGraphProjection`
 * and accepted back (echoed) on a `candidate-preview` command to price it
 * from the identical snapshot. */
export interface MarketScopeEvidence {
  scope: MarketScope;
  observationBatchIds: string[];
  importBatchId: string | null;
  observedAt: string | null;
  asOf: string;
}

export interface RequestMarketDataResult {
  requested: boolean;
  sourcesNotified: number;
}

export function requestMarketData(typeId: number, scope: MarketScope): Promise<RequestMarketDataResult> {
  const params = new URLSearchParams({ regionId: String(scope.regionId) });
  if (scope.locationId !== undefined) params.set("locationId", String(scope.locationId));
  return request(`/api/market/items/${typeId}/request?${params.toString()}`, json("POST", {}));
}

export interface MarketItemsFreshness {
  mostRecentUpdatedAt: string | null;
}

// Cheap "has anything I'm looking at changed" check the Market Browser
// polls instead of re-running listMarketItems -- typeIds should always be
// exactly the currently-rendered page's ids.
export function getMarketItemsFreshness(typeIds: number[], scope: MarketScope): Promise<MarketItemsFreshness> {
  const params = new URLSearchParams({
    regionId: String(scope.regionId),
    typeIds: typeIds.join(","),
  });
  if (scope.locationId !== undefined) params.set("locationId", String(scope.locationId));
  return request(`/api/market/items/freshness?${params.toString()}`);
}
