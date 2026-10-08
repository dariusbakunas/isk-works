import { json, request } from "./request";
import type { Money } from "./shared";

export type FacilityKind = "npcStation" | "upwellStructure" | "manual";
export type FacilityRole = "manufacturing" | "reaction";
export type SecurityClass = "highSec" | "lowSec" | "nullSec" | "wormhole" | "unknown";
export interface SolarSystemSearchResult {
  solarSystemId: number;
  solarSystemName: string;
  securityClass: SecurityClass;
}

export interface NpcStationSearchResult {
  stationId: number;
  stationName: string;
  stationTypeId: number;
  stationTypeName: string | null;
  solarSystemId: number;
  solarSystemName: string;
  securityClass: SecurityClass;
}

export interface SystemCostIndex {
  solarSystemId: number;
  manufacturing: string;
  reaction: string;
  fetchedAt: string;
  expiresAt: string;
}

export interface KnownStructure {
  structureId: number;
  structureName: string;
  structureTypeId: number | null;
  structureTypeName: string | null;
  solarSystemId: number;
  solarSystemName: string | null;
  securityClass: SecurityClass;
}

export interface FacilityProfile {
  id: string;
  workspaceId: string;
  name: string;
  kind: FacilityKind;
  role: FacilityRole;
  structureId: number | null;
  structureTypeId: number | null;
  structureTypeName: string;
  solarSystemId: number | null;
  solarSystemName: string;
  securityClass: SecurityClass;
  materialReductionPercent: string;
  timeReductionPercent: string;
  jobCostReductionPercent: string;
  facilityTaxPercent: string;
  sccSurchargePercent: string;
  allianceSurchargePercent: string;
  fixedSupplementalCost: Money;
  manualSystemCostIndex: string | null;
  notes: string;
  rigs: Array<{
    slotNumber: number;
    typeId: number;
    typeName: string;
    materialReductionPercent: string;
    timeReductionPercent: string;
  }>;
  archivedAt: string | null;
  revision: number;
  createdAt: string;
  updatedAt: string;
}

export interface FacilityPlanPreview {
  profile: FacilityProfile;
  blueprintMe: number;
  blueprintTe: number;
  requirements: Array<{
    typeId: number;
    typeName: string;
    sortOrder: number;
    baseQuantityPerRun: number;
    runs: number;
    baseExtendedQuantity: number;
    blueprintMe: number;
    finalRequiredQuantity: number;
    calculationTrace: string;
    formulaVersion: string;
    recipeFingerprint: string;
  }>;
  plannedDurationSeconds: number | null;
  durationSteps: Array<{
    label: string;
    detail: string;
    multiplier: string;
    runningDurationSeconds: number;
  }>;
  installationCost: InstallationCostBreakdown;
  warnings: string[];
  formulaVersion: string;
}

export interface InstallationCostBreakdown {
  complete: boolean;
  estimatedItemValue: Money | null;
  systemCostIndex: string | null;
  unmodifiedSystemIndexCost: Money | null;
  jobCostReductionPercent: string;
  systemIndexCost: Money | null;
  facilityTax: Money | null;
  sccSurcharge: Money | null;
  allianceSurcharge: Money | null;
  fixedSupplementalCost: Money;
  total: Money | null;
  warnings: string[];
  formulaVersion: string;
}

export interface AutomaticEiv {
  value: Money | null;
  missingTypeIds: number[];
  observedAt: string;
  expiresAt: string;
}

export interface ManufacturingFacilityCommand {
  facilityProfileId: string;
  blueprintMe: number;
  blueprintTe: number;
  estimatedItemValue: string | null;
}

// Reaction formulas have no blueprint ME/TE in EVE, so this has no
// blueprintMe/blueprintTe fields, unlike ManufacturingFacilityCommand.
export interface ReactionFacilityCommand {
  facilityProfileId: string;
  estimatedItemValue: string | null;
}

export interface FacilityInput {
  name: string;
  kind: FacilityKind;
  role: FacilityRole;
  structureId: number | null;
  structureTypeId: number | null;
  structureTypeName: string;
  solarSystemId: number | null;
  solarSystemName: string;
  securityClass: SecurityClass;
  materialReductionPercent: string;
  timeReductionPercent: string;
  jobCostReductionPercent: string;
  facilityTaxPercent: string;
  sccSurchargePercent: string;
  allianceSurchargePercent: string;
  fixedSupplementalCost: string;
  manualSystemCostIndex: string | null;
  notes: string;
  rigs: FacilityProfile["rigs"];
}

export function listFacilities(): Promise<FacilityProfile[]> {
  return request("/api/industry/facilities");
}

export function searchSolarSystems(query: string): Promise<SolarSystemSearchResult[]> {
  return request(`/api/universe/solar-systems/search?q=${encodeURIComponent(query)}&limit=12`);
}

export function searchNpcStations(query: string): Promise<NpcStationSearchResult[]> {
  return request(`/api/universe/npc-stations/search?q=${encodeURIComponent(query)}&limit=50`);
}

export function getSystemCostIndex(solarSystemId: number): Promise<SystemCostIndex> {
  return request(`/api/industry/systems/${solarSystemId}/cost-index`);
}

export function getBlueprintAutomaticEiv(
  blueprintTypeId: number,
  runs: number,
): Promise<AutomaticEiv> {
  return request(`/api/blueprints/${blueprintTypeId}/estimated-item-value?runs=${runs}`);
}

export function getReactionFormulaAutomaticEiv(
  reactionFormulaTypeId: number,
  runs: number,
): Promise<AutomaticEiv> {
  return request(`/api/reaction-formulas/${reactionFormulaTypeId}/estimated-item-value?runs=${runs}`);
}

export function searchKnownStructures(query: string): Promise<KnownStructure[]> {
  return request(`/api/industry/structures/search?q=${encodeURIComponent(query)}&limit=100`);
}

export interface StructureResolution {
  configured: boolean;
  structure: KnownStructure | null;
  needsReconnection: boolean;
  eligibleCharacterCount: number;
  warnings: string[];
}

/**
 * Resolves a single structure by ID via ESI, using any connected character
 * with docking access there -- works even if the workspace has never
 * observed the structure through assets/wallet/market activity before.
 */
export function resolveKnownStructure(structureId: number): Promise<StructureResolution> {
  return request("/api/industry/structures/resolve", json("POST", { structureId }));
}

export function createFacility(input: FacilityInput): Promise<FacilityProfile> {
  return request("/api/industry/facilities", json("POST", input));
}

export function updateFacility(
  id: string,
  expectedRevision: number,
  input: FacilityInput,
): Promise<FacilityProfile> {
  return request(`/api/industry/facilities/${id}`, json("PUT", { expectedRevision, ...input }));
}

export function deleteFacility(id: string, expectedRevision: number): Promise<void> {
  return request(`/api/industry/facilities/${id}`, json("DELETE", { expectedRevision }));
}

export interface FacilityExport {
  exportedAt: string;
  items: FacilityInput[];
}

export interface FacilityImportItemResult {
  name: string;
  status: "created" | "replaced" | "skipped" | "failed";
  message: string | null;
}

export interface FacilityImportResponse {
  results: FacilityImportItemResult[];
}

export function exportFacilities(): Promise<FacilityExport> {
  return request("/api/industry/facilities/export");
}

export type FacilityImportPreviewItem = {
  index: number;
  name: string;
  classification: "new" | "duplicate" | "invalid";
  existingId: string | null;
  existingName: string | null;
  existingRevision: number | null;
  matchBasis: "eveLocation" | "manualName" | null;
  message: string | null;
};

export interface FacilityImportPreviewResponse {
  items: FacilityImportPreviewItem[];
}

export type FacilityImportAction = {
  action: "create" | "skip" | "replace";
  item: FacilityInput;
  existingId?: string;
  expectedRevision?: number;
};

export function previewFacilityImport(items: FacilityInput[]): Promise<FacilityImportPreviewResponse> {
  return request("/api/industry/facilities/import/preview", json("POST", { items }));
}

export function importFacilities(actions: FacilityImportAction[]): Promise<FacilityImportResponse> {
  return request("/api/industry/facilities/import", json("POST", { actions }));
}

