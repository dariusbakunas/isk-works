import type { RecipeSelection } from "./industry";
import { ApiError } from "./workspace";

export interface ImportCounts {
  types: number;
  blueprints: number;
  materialLines: number;
  productLines: number;
  skippedBlueprints: number;
}

export interface ActiveSde {
  importId: string;
  sourceVersion: string;
  sourceLabel: string;
  sourceChecksum: string;
  completedAt: string;
  counts: ImportCounts;
}

export interface SdeStatus {
  configured: boolean;
  active: ActiveSde | null;
}

export interface BlueprintSearchResult {
  blueprintTypeId: number;
  blueprintName: string;
  productTypeId: number;
  productName: string;
  groupName: string | null;
  published: boolean;
  manufacturingAvailable: boolean;
}

export interface TypeSearchResult {
  typeId: number;
  typeName: string;
  groupName: string | null;
  published: boolean;
}

export interface StructureManufacturingModifiers {
  typeId: number;
  materialReductionPercent: string;
  timeReductionPercent: string;
  jobCostReductionPercent: string;
}

/** One of EVE's `industryTargetFilters` -- a named category/group set a rig
 * bonus is restricted to. */
export interface IndustryTargetFilter {
  filterId: number;
  name: string;
  categoryIds: number[];
  groupIds: number[];
}

/** What a rig's bonuses affect, per activity. A `null` field is
 * unrestricted (the rig has no `filterID`, or the SDE lacks the dataset). */
export interface RigApplicability {
  material: IndustryTargetFilter | null;
  time: IndustryTargetFilter | null;
}

export interface RigManufacturingModifiers {
  typeId: number;
  materialReductionPercent: string;
  timeReductionPercent: string;
  compatibleWithStructure: boolean | null;
  appliesTo: RigApplicability;
}

export interface ReactionRigModifiers {
  typeId: number;
  materialReductionPercent: string;
  timeReductionPercent: string;
  compatibleWithStructure: boolean | null;
  appliesTo: RigApplicability;
}

export interface BuildPlanLine {
  typeId: number;
  typeName: string;
  quantityPerRun: number;
  totalQuantity: number;
}

export interface BuildPlan {
  blueprintTypeId: number;
  blueprintName: string;
  runs: number;
  durationSeconds: number | null;
  materials: BuildPlanLine[];
  products: BuildPlanLine[];
}

export interface ReactionFormulaSearchResult {
  reactionFormulaTypeId: number;
  reactionFormulaName: string;
  productTypeId: number;
  productName: string;
  groupName: string | null;
  published: boolean;
}

export interface ReactionPlan {
  reactionFormulaTypeId: number;
  reactionFormulaName: string;
  runs: number;
  durationSeconds: number | null;
  materials: BuildPlanLine[];
  products: BuildPlanLine[];
}

const apiBase = (import.meta.env.VITE_API_BASE_URL ?? "").replace(/\/$/, "");

export function getSdeStatus(): Promise<SdeStatus> {
  return request("/api/sde");
}

export function searchBlueprints(query: string): Promise<BlueprintSearchResult[]> {
  return request(`/api/blueprints/search?q=${encodeURIComponent(query)}&limit=20`);
}

export function searchTypes(query: string): Promise<TypeSearchResult[]> {
  return request(`/api/types/search?q=${encodeURIComponent(query)}&limit=20`);
}

export function searchStructureTypes(query: string): Promise<TypeSearchResult[]> {
  return request(`/api/structure-types/search?q=${encodeURIComponent(query)}&limit=250`);
}

export function searchStructureRigs(query: string, structureTypeId: number | null): Promise<TypeSearchResult[]> {
  const params = new URLSearchParams({ q: query, limit: "250" });
  if (structureTypeId) params.set("structureTypeId", String(structureTypeId));
  return request(`/api/structure-rigs/search?${params}`);
}

export function getStructureManufacturingModifiers(typeId: number): Promise<StructureManufacturingModifiers> {
  return request(`/api/structure-types/${typeId}/manufacturing-modifiers`);
}

export function getRigManufacturingModifiers(
  typeId: number,
  securityClass: string,
  structureTypeId: number | null,
): Promise<RigManufacturingModifiers> {
  const query = new URLSearchParams({ securityClass });
  if (structureTypeId) query.set("structureTypeId", String(structureTypeId));
  return request(`/api/structure-rigs/${typeId}/manufacturing-modifiers?${query}`);
}

export function searchReactionRigs(query: string, structureTypeId: number | null): Promise<TypeSearchResult[]> {
  const params = new URLSearchParams({ q: query, limit: "250" });
  if (structureTypeId) params.set("structureTypeId", String(structureTypeId));
  return request(`/api/reaction-rigs/search?${params}`);
}

export function getRigReactionModifiers(
  typeId: number,
  securityClass: string,
  structureTypeId: number | null,
): Promise<ReactionRigModifiers> {
  const query = new URLSearchParams({ securityClass });
  if (structureTypeId) query.set("structureTypeId", String(structureTypeId));
  return request(`/api/reaction-rigs/${typeId}/reaction-modifiers?${query}`);
}

export function planBuild(blueprintTypeId: number, runs: number): Promise<BuildPlan> {
  return request(`/api/blueprints/${blueprintTypeId}/plan?runs=${runs}`);
}

export function searchReactionFormulas(query: string): Promise<ReactionFormulaSearchResult[]> {
  return request(`/api/reaction-formulas/search?q=${encodeURIComponent(query)}&limit=20`);
}

export function planReaction(reactionFormulaTypeId: number, runs: number): Promise<ReactionPlan> {
  return request(`/api/reaction-formulas/${reactionFormulaTypeId}/plan?runs=${runs}`);
}

export function recipeForProduct(productTypeId: number): Promise<RecipeSelection | null> {
  return request(`/api/recipes/for-product/${productTypeId}`);
}

async function request<T>(path: string): Promise<T> {
  let response: Response;
  try {
    response = await fetch(`${apiBase}${path}`, {
      headers: { Accept: "application/json" },
      credentials: "include",
    });
  } catch {
    throw new ApiError(0, {
      code: "api_unavailable",
      message: "ISK Works API is unavailable. Check that the backend is running.",
    });
  }

  if (!response.ok) {
    const body = await response.json().catch(() => null);
    throw new ApiError(response.status, body?.error ?? {
      code: "api_error",
      message: "ISK Works could not complete the request.",
    });
  }
  return (await response.json()) as T;
}
