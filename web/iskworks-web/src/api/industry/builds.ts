import type {
  FacilityPlanPreview,
  InstallationCostBreakdown,
  ManufacturingFacilityCommand,
  ReactionFacilityCommand,
} from "./facilities";
import type { MarketPricingPolicy, MarketScope } from "./market";
import { json, request } from "./request";
import type { BuildRecipe, Money, RecipeCurrency } from "./shared";
import { ApiError } from "../workspace";

export interface SnapshotLine {
  typeId: number;
  typeName: string;
  itemRole?: "material" | "output";
  selectionKind?: "default" | "market_policy" | "manual";
  manualUnitPrice?: Money | null;
  price: Money | null;
  pricingPolicy: MarketPricingPolicy | null;
  missing: boolean;
  sourceNote: string;
  sortOrder: number;
  marketRegionId?: number | null;
  marketLocationId?: number | null;
}

export interface PlannedMaterialLine {
  typeId: number;
  typeName: string;
  quantityPerRun: number;
  totalQuantity: number;
  unitPrice: Money | null;
  lineTotal: Money | null;
  missing: boolean;
  isBuildResolved: boolean;
  installationCost: InstallationCostBreakdown | null;
  reusedQuantity?: number | null;
  missingQuantity?: number | null;
  reusedLineTotal?: Money | null;
  /** Present only for a Build/Reaction-resolved row under the
   * allocation-aware planning-cost model: optional evidence for *why* this
   * row's cost is less than the child job's full total (a surplus-producing
   * child) -- never presented as if the full child job were charged here.
   * See `childConsumedCost` for the number actually reflected in
   * `lineTotal`. */
  planningEvidence?: PlanningChildEvidence | null;
}

/** Evidence for a Build/Reaction-resolved row's planning cost -- how much of
 * the child operation's production this row actually consumes, versus what
 * the child produced and retained as unavoidable surplus. */
export interface PlanningChildEvidence {
  childOpIndex: number;
  childProducedQuantity: number;
  childConsumedQuantity: number;
  childUnitProductionCost: Money | null;
  childConsumedCost: Money | null;
  childSurplusQuantity: number;
  childSurplusRetainedBasis: Money | null;
}

export interface BuildPlanRevision {
  id: string;
  revision: number;
  runs: number;
  recipeFingerprint: string;
  snapshot: {
    id: string;
    priceSourceId: string | null;
    sourceName: string;
    sourceRevision: number;
    createdAt: string;
    items: SnapshotLine[];
  };
  pricingComplete: boolean;
  estimatedMaterialCost: Money;
  expectedRevenue: Money | null;
  estimatedMargin: Money | null;
  missingPriceCount: number;
  active: boolean;
  plannedAt: string;
  supersededAt: string | null;
  materialLines: PlannedMaterialLine[];
  manufacturingFacility: FacilityPlanPreview | null;
  reactionFacility: FacilityPlanPreview | null;
  blueprint: BlueprintSnapshot | null;
}

// Exactly one of the two slots is ever the root job's own facility --
// whichever matches the Build's recipe kind. The other slot exists only
// for a build-resolved sub-component of that other kind.
export function rootFacility(
  plan: { manufacturingFacility: FacilityPlanPreview | null; reactionFacility: FacilityPlanPreview | null },
): FacilityPlanPreview | null {
  return plan.manufacturingFacility ?? plan.reactionFacility;
}

export type BlueprintKind = "original" | "copy" | "unknown";
export type BlueprintSelection =
  | { mode: "manual"; kind: "original" | "copy"; materialEfficiency: number; timeEfficiency: number; licensedRuns: number | null; notes: string }
  // `kind`/`materialEfficiency`/`timeEfficiency` are the durable effective
  // configuration the server captures once, when the observed blueprint is
  // selected -- omit them (or send `kind: "unknown"`) to ask the server to
  // resolve and capture them from `observationId`; once captured, they're
  // returned on every read and should be sent back unchanged on save (the
  // server never re-resolves an already-captured selection).
  | {
      mode: "observedAsset";
      observationId: string;
      kind?: BlueprintKind;
      materialEfficiency?: number;
      timeEfficiency?: number;
      /** Frozen with kind/ME/TE: a copy's licensed runs, the per-job run
       * limit the planner splits runs by. Absent for an original or a
       * selection captured before it was frozen. */
      licensedRuns?: number | null;
    };

export type RecipeSelection =
  | { mode: "manufacturing"; blueprintTypeId: number }
  | { mode: "reaction"; reactionFormulaTypeId: number };

export interface ComponentFacilityOverride {
  facilityProfileId: string;
}

export interface ComponentResolution {
  typeId: number;
  recipe: RecipeSelection;
  facilityOverride?: ComponentFacilityOverride;
  blueprintSelection?: BlueprintSelection;
}

export type FulfillmentScope = "missing" | "full";

export interface FulfillmentScopeOverride {
  typeId: number;
  scope: FulfillmentScope;
}

export interface BlueprintSnapshot {
  id: string;
  buildId: string;
  sourceMode: "manual" | "observedAsset" | "legacyMigration";
  blueprintTypeId: number;
  blueprintName: string;
  kind: BlueprintKind;
  materialEfficiency: number;
  timeEfficiency: number;
  licensedRuns: number | null;
  requestedRuns: number;
  sourceObservationId: string | null;
  sourceEveItemId: number | null;
  sourceOwnerId: string | null;
  sourceOwnerName: string | null;
  sourceLocationId: number | null;
  sourceLocationName: string | null;
  observedAt: string | null;
  importedAt: string | null;
  manualNotes: string | null;
  plannedDurationSeconds: number | null;
  formulaVersion: string;
  capturedAt: string;
}

export interface BlueprintObservation {
  id: string;
  workspaceId: string;
  ownerId: string;
  ownerName: string;
  eveItemId: number;
  blueprintTypeId: number;
  blueprintName: string;
  kind: BlueprintKind;
  materialEfficiency: number;
  timeEfficiency: number;
  licensedRuns: number | null;
  locationId: number;
  locationFlag: string;
  locationName: string | null;
  observedAt: string;
  importedAt: string;
}

// Build is a stateless, always-editable configuration -- no status/
// lifecycle fields. Executing a Build happens through an Epic (an Order
// frozen from the Build, see `createOrder` in ./orders.ts) and its Board
// tickets.
export interface Build {
  id: string;
  workspaceId: string;
  ownerId: string;
  name: string;
  recipe: BuildRecipe;
  runs: number;
  notes: string;
  revision: number;
  createdAt: string;
  updatedAt: string;
  draftPlanning: DraftPlanningSnapshot | null;
  recipeCurrency: RecipeCurrency;
  activeSdeVersion: string | null;
  /** Broad EVE classification of the output item (its `invCategories`
   * category, e.g. "Ship"), resolved from the active SDE at read time.
   * `null` when the SDE has no classification for the product. */
  productCategoryName: string | null;
  /** The output item's `invGroups` group (e.g. "Heavy Assault Cruiser"). */
  productGroupName: string | null;
  /** Origin of the blueprint actually selected on this Build -- `"original"`
   * (BPO) or `"copy"` (BPC). `null` when no concrete blueprint is selected,
   * for a reaction Build, or when the source observation is gone. Never
   * inferred from the output item. */
  selectedBlueprintOrigin: BlueprintKind | null;
  /** True when this workspace holds a current blueprint observation for
   * this Build's blueprint (BPO or BPC on hand). Always `false` for a
   * reaction Build. */
  hasOwnedBlueprint: boolean;
  /** The top-level plan owning this Build (`GET /api/builds/:id` only). */
  planRootBuildId?: string | null;
  /** Display name of the top-level plan. */
  planRootBuildName?: string | null;
}

export interface DraftPlanningInput {
  materialScope: MarketScope;
  outputScope: MarketScope;
  manualPriceListId: string | null;
  expectedManualPriceListRevision: number | null;
  materialPricingPolicy: MarketPricingPolicy;
  outputPricingPolicy: MarketPricingPolicy;
  pricingSelections: PlannerPricingSelection[];
  blueprintSelection: BlueprintSelection | null;
  manufacturingFacility: ManufacturingFacilityCommand | null;
  reactionFacility: ReactionFacilityCommand | null;
  facilityEivManual: boolean;
  componentResolutions?: ComponentResolution[];
  fulfillmentScopes?: FulfillmentScopeOverride[];
}

export interface DraftPlanningSnapshot {
  input: DraftPlanningInput;
  updatedAt: string;
}

export type MaterialCostQuality = "known" | "estimated" | "zeroCost" | "unresolved";

export interface MaterialCoverage {
  typeId: number;
  typeName: string;
  sortOrder: number;
  requiredQuantity: number;
  accountedOwnedQuantity: number;
  reservedForThisBuild: number;
  reservedByOtherBuilds: number;
  unreservedAvailableQuantity: number;
  availableToThisBuild: number;
  reservableAdditionalQuantity: number;
  coveredQuantity: number;
  missingQuantity: number;
  averageHistoricalUnitCost: Money | null;
  projectedHistoricalCost: Money | null;
  costQuality: MaterialCostQuality;
  quantityCoverageState: "noInventory" | "missing" | "partiallyCovered" | "covered" | "reservedPartially" | "reserved";
  esiObservedQuantity: number | null;
  esiReconciliationDifference: number | null;
  esiObservedAt: string | null;
  explanation: string;
  warnings: string[];
  inventoryRevision: number;
}

export interface BuildCoverageReport {
  buildId: string;
  ownerId: string;
  buildRevision: number;
  recipeFingerprint: string;
  runs: number;
  completeQuantityCoverage: boolean;
  completeCostCoverage: boolean;
  materialLines: MaterialCoverage[];
  warnings: string[];
}

export interface ProductionWorksheet {
  groups: WorksheetGroup[];
  output: WorksheetGroup;
  summary: {
    materialCost: Money;
    /** The root operation's *own* installation only -- never root +
     * descendant installation. A descendant's installation is already
     * folded into `materialCost` through each Build/Reaction row's
     * consumed-child-cost share. */
    installationCost: Money | null;
    totalCost: Money | null;
    expectedRevenue: Money | null;
    /** Compatibility alias: under the allocation-aware planning-cost model
     * this is the planning margin (`expectedRevenue -
     * planningTotalProductionCost`), the same value as `planningMargin`. */
    estimatedMargin: Money | null;
    /** Present when this worksheet was built from the allocation-aware
     * planning-cost model. Identical to `estimatedMargin`. */
    planningMargin?: Money | null;
    planningCostComplete?: boolean | null;
    /** Total spent on fresh (non-inventory) acquisition across the whole
     * tree -- explanatory only, already included in `totalCost`. */
    totalFreshOutlay?: Money | null;
    /** Total historical-basis value retained in unavoidable child-job
     * surplus across the whole tree -- explanatory only, never added to
     * `totalCost`. */
    totalSurplusRetainedBasis?: Money | null;
    pricingComplete: boolean;
    quantityCoverageComplete: boolean;
    costCoverageComplete: boolean;
    warnings: Array<{ code: string; message: string }>;
  };
}

export interface WorksheetGroup {
  key: string;
  label: string;
  items: WorksheetItem[];
}

export interface WorksheetItem {
  typeId: number;
  typeName: string;
  role: "material" | "output";
  requiredQuantity: number;
  availableQuantity: number;
  coveredQuantity: number;
  missingQuantity: number;
  coveragePercentage: string;
  projectedInventoryCost: Money | null;
  pricing: {
    selectionKind: "default" | "market_policy" | "manual";
    effectivePolicy: MarketPricingPolicy | null;
    unitPrice: Money | null;
    manualUnitPrice: Money | null;
    missing: boolean;
    sourceNote: string;
  };
  lineTotal: Money | null;
  contributions: MaterialContribution[];
  isBuildResolved: boolean;
  installationCost: Money | null;
  /** How much of this row is being covered from inventory under the
   * current fulfillment scope -- absent unless a `Missing` scope override
   * applies to this row. Missing/bought-or-built quantity isn't repeated:
   * it's `requiredQuantity - reusedQuantity`. */
  reusedQuantity?: number | null;
  reusedLineTotal?: Money | null;
  /** Same shape and meaning as `PlannedMaterialLine.planningEvidence`. */
  planningEvidence?: PlanningChildEvidence | null;
}

export interface MaterialContribution {
  parentTypeId: number | null;
  parentTypeName: string;
  quantity: number;
}

export interface CreateBuildPlanPreview {
  candidateFingerprint: string;
  canPlan: boolean;
  candidate: BuildPlanRevision;
  coverage: BuildCoverageReport;
  projectedInventoryCost: Money | null;
  decision: {
    headline: string;
    supportingText: string;
    tone: "positive" | "warning" | "blocking";
  };
  validation: {
    fields: Array<{ field: string; code: string; message: string }>;
    blockers: Array<{ code: string; message: string }>;
  };
  warnings: Array<{ code: string; message: string }>;
  completeness: {
    materials: "complete" | "incomplete" | "unavailable" | "notConfigured";
    duration: "complete" | "incomplete" | "unavailable" | "notConfigured";
    pricing: "complete" | "incomplete" | "unavailable" | "notConfigured";
    installation: "complete" | "incomplete" | "unavailable" | "notConfigured";
    inventoryCost: "complete" | "incomplete" | "unavailable" | "notConfigured";
    profitability: "complete" | "qualified" | "unavailable";
  };
  profitabilityBasis: {
    includedCosts: string[];
    excludedCosts: string[];
  };
  calculationEvidence: CalculationEvidenceProjection;
  worksheet: ProductionWorksheet;
}

export interface CalculationEvidenceProjection {
  profitMarginPercent: string | null;
  systemCostIndexPercent: string | null;
  materialCost: {
    complete: boolean;
    baseMarketValue: Money | null;
    afterBlueprintMe: Money | null;
    afterStructure: Money | null;
    adjustedMaterialCost: Money;
    blueprintMultiplier: string;
    structureMultiplier: string;
    rigMultiplier: string;
    requirementTraces: string[];
  };
  durationSteps: FacilityPlanPreview["durationSteps"];
}

export type PlannerPricingSelection = {
  typeId: number;
  role: "material" | "output";
  selection:
    | { kind: "default" }
    | { kind: "market_policy"; policy: MarketPricingPolicy }
    | { kind: "manual"; unit_price: string };
};

export function listBuilds(): Promise<Build[]> {
  return request("/api/builds");
}

export function getBuild(id: string, signal?: AbortSignal): Promise<Build> {
  return request(`/api/builds/${id}`, { signal });
}

export function createBuild(input: {
  name: string;
  recipe: RecipeSelection;
  runs: number;
  notes: string;
  draftPlanning?: DraftPlanningInput | null;
}): Promise<Build> {
  return request("/api/builds", json("POST", input));
}

export function updateBuild(id: string, input: {
  expectedRevision: number;
  name: string;
  recipe: RecipeSelection;
  runs: number;
  notes: string;
  draftPlanning?: DraftPlanningInput | null;
}): Promise<Build> {
  return request(`/api/builds/${id}`, json("PUT", input));
}

export function renameBuild(id: string, input: {
  name: string;
}): Promise<Build> {
  return request(`/api/builds/${id}/name`, json("PATCH", input));
}

export function createLinkedBuild(buildId: string, input: { componentTypeId: number }): Promise<Build> {
  return request(`/api/builds/${buildId}/linked-builds`, json("POST", input));
}

/**
 * Build-ID-addressable sourcing mutation (Graph nested BUY -> BUILD). Sets
 * one component's resolution on `buildId` -- the Build that *owns* the
 * requirement, never the root. Follow with `createLinkedBuild(buildId, ...)`
 * to resolve the producer Build.
 */
export function setComponentResolution(
  buildId: string,
  input: { componentTypeId: number; recipe: RecipeSelection; expectedRevision: number },
): Promise<Build> {
  return request(`/api/builds/${buildId}/component-resolutions`, json("POST", input));
}

/** Graph nested BUILD -> BUY on `buildId`. The retained linked Build stays
 * inactive for reuse (no delete/recreate). */
export function clearComponentResolution(
  buildId: string,
  componentTypeId: number,
  expectedRevision: number,
): Promise<Build> {
  return request(
    `/api/builds/${buildId}/component-resolutions/${componentTypeId}?expectedRevision=${expectedRevision}`,
    { method: "DELETE" },
  );
}

/**
 * Build-ID-addressable "common settings" patches -- the narrow,
 * revision-checked counterparts of `updateBuild` that mutate exactly one
 * facet of an arbitrary Build's planning input. The unified inspector uses
 * these to edit a *linked* Build's own blueprint / facility / pricing in
 * place, without the single-Build worksheet editor. Each returns the updated
 * Build; the server follows every one with a best-effort descendant resync.
 */
export function setBuildBlueprintSelection(
  buildId: string,
  input: { expectedRevision: number; blueprintSelection: BlueprintSelection | null },
): Promise<Build> {
  return request(`/api/builds/${buildId}/blueprint-selection`, json("PATCH", input));
}

export function setBuildFacility(
  buildId: string,
  input: {
    expectedRevision: number;
    facilityProfileId: string | null;
    estimatedItemValue?: string | null;
  },
): Promise<Build> {
  return request(`/api/builds/${buildId}/facility`, json("PATCH", input));
}

export function setBuildPricing(
  buildId: string,
  input: {
    expectedRevision: number;
    materialScope: MarketScope;
    outputScope: MarketScope;
    materialPricingPolicy: MarketPricingPolicy;
    outputPricingPolicy: MarketPricingPolicy;
    manualPriceListId?: string | null;
    expectedManualPriceListRevision?: number | null;
    facilityEivManual?: boolean;
  },
): Promise<Build> {
  return request(`/api/builds/${buildId}/pricing`, json("PATCH", input));
}

/**
 * The live, unsaved build-plan planning overlay. One shape shared by every
 * request that sends "the current recipe planning state" -- the two
 * `/api/build-plans/*` previews and `POST /api/builds/:id/graph`.
 * The editor's `previewKey` is a `JSON.stringify` of exactly this.
 */
export interface PreviewBuildPlanCommand {
  recipe: RecipeSelection;
  runs: number;
  materialScope: MarketScope;
  outputScope: MarketScope;
  manualPriceListId?: string | null;
  expectedManualPriceListRevision?: number | null;
  pricingSelections: PlannerPricingSelection[];
  blueprintSelection?: BlueprintSelection;
  manufacturingFacility?: ManufacturingFacilityCommand | null;
  reactionFacility?: ReactionFacilityCommand | null;
  componentResolutions?: ComponentResolution[];
  fulfillmentScopes?: FulfillmentScopeOverride[];
  /** The saved build this overlay edits; omitted for the create flow. */
  buildId?: string;
}

export function previewCreateBuildCandidate(
  input: PreviewBuildPlanCommand,
  signal?: AbortSignal,
): Promise<CreateBuildPlanPreview> {
  return request("/api/build-plans/candidate-preview", {
    ...json("POST", input),
    signal,
  });
}

/**
 * `POST /api/builds/:id/export-verification` -- an engineering / audit
 * `.xlsx` workbook that re-checks this Build's calculations with plain Excel
 * formulas. Send the same planning overlay a preview / the Materials view
 * sends (the editor's `previewKey`, parsed). Read-only: never saves the
 * overlay, creates linked Builds, or mutates inventory / orders.
 *
 * Returns the raw `Blob` (an OOXML spreadsheet) plus the server-suggested
 * download filename parsed from `Content-Disposition`.
 */
export async function exportBuildVerificationWorkbook(
  buildId: string,
  command: PreviewBuildPlanCommand,
): Promise<{ blob: Blob; filename: string }> {
  const baseUrl = (import.meta.env.VITE_API_BASE_URL ?? "").replace(/\/$/, "");
  let response: Response;
  try {
    response = await fetch(`${baseUrl}/api/builds/${buildId}/export-verification`, {
      method: "POST",
      credentials: "include",
      headers: {
        "content-type": "application/json",
        Accept:
          "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
      },
      body: JSON.stringify(command),
    });
  } catch {
    throw new ApiError(0, {
      code: "api_unavailable",
      message: "ISK Works API is unavailable. Check that the backend is running.",
    });
  }
  if (!response.ok) {
    const payload = await response.json().catch(() => null);
    throw new ApiError(
      response.status,
      payload?.error ?? {
        code: "api_error",
        message: "The verification workbook could not be generated.",
      },
    );
  }
  const disposition = response.headers.get("Content-Disposition") ?? "";
  const match = /filename="?([^"]+)"?/i.exec(disposition);
  const filename = match?.[1] ?? "build-verification.xlsx";
  return { blob: await response.blob(), filename };
}

export function listBlueprintObservations(blueprintTypeId: number): Promise<BlueprintObservation[]> {
  return request(`/api/industry/blueprints/observations?blueprintTypeId=${blueprintTypeId}`);
}

// Deletes the live planning subtree. Frozen Epics and Board Tickets survive
// with their source Build references detached by the server.
export function deleteBuild(id: string, expectedRevision: number): Promise<void> {
  const query = `?expectedRevision=${expectedRevision}`;
  return request(`/api/builds/${id}${query}`, { method: "DELETE" });
}

