import { json, request } from "./industry/request";
import type { MarketScope } from "./industry/market";
import type { Money } from "./industry/shared";

// Opaque server-owned identifier -- the full set of supported scopes is
// discovered from GET /api/opportunities/scopes, never hardcoded here.
export type ProfitabilityScopeId = string;

export type OpportunityRecipeKind = "manufacturing" | "reaction";

export interface ProfitabilityScopeDefinition {
  id: ProfitabilityScopeId;
  label: string;
  family: string;
  description: string;
  limitations: string[];
  recipeKind: OpportunityRecipeKind;
}

export interface EvaluateOpportunitiesCommand {
  scopeId: ProfitabilityScopeId;
  facilityProfileId: string;
  // Required for a manufacturing scope, must be omitted/null for a reaction
  // scope -- reaction formulas have no material/time efficiency concept.
  materialEfficiency: number | null;
  timeEfficiency: number | null;
  marketScope: MarketScope;
}

export type MarketPricingPolicy =
  | "lowestSell"
  | "highestBuy"
  | "acquireQuantityFromSellOrders"
  | "liquidateQuantityIntoBuyOrders";

export interface OpportunityEvaluationContext {
  scopeId: ProfitabilityScopeId;
  facilityProfileId: string;
  facilityRevision: number;
  marketRegionId: number;
  marketLocationId: number | null;
  materialEfficiency: number | null;
  timeEfficiency: number | null;
  runs: number;
  materialPricingPolicy: MarketPricingPolicy;
  outputPricingPolicy: MarketPricingPolicy;
  inventoryReuseEnabled: boolean;
  recursiveComponentExpansionEnabled: boolean;
}

export type OpportunityEvidenceState = "fresh" | "stale" | "missing" | "pending" | "failed";

export interface OpportunityEvidenceStatus {
  state: OpportunityEvidenceState;
  usable: boolean;
  observedAt: string | null;
  ageSeconds: number | null;
  refreshPending: boolean;
  lastRefreshError: string | null;
}

export type OpportunitySystemIndexSource = "manual" | "observed" | "missing";

export interface OpportunityReadiness {
  registeredAt: string;
  localReadAt: string;
  requiredMaterialTypeCount: number;
  requiredOutputTypeCount: number;
  marketFreshCount: number;
  marketStaleCount: number;
  marketMissingCount: number;
  marketPendingCount: number;
  marketFailedCount: number;
  oldestMarketObservedAt: string | null;
  newestMarketObservedAt: string | null;
  adjustedPrices: OpportunityEvidenceStatus;
  systemIndexSource: OpportunitySystemIndexSource;
  systemIndex: OpportunityEvidenceStatus;
  staleButCompleteCount: number;
  refreshPending: boolean;
}

export type OpportunityCompleteness = "complete" | "incomplete";

export type OpportunityWarningKind =
  | "missingMaterialPrice"
  | "missingOutputPrice"
  | "insufficientMarketDepth"
  | "incompleteInstallationCost"
  | "multipleOutputsUnsupported"
  | "staleMarketEvidence"
  | "thinOutputBook"
  | "thinInputBook"
  | "incompleteEivBasis";

export interface OpportunityMissingMaterial {
  typeId: number;
  typeName: string;
}

export type OpportunityThinBookReason =
  | "bestLevelAtOrBelowOutputQuantity"
  | "outputAtOrAboveTenPercentOfVisibleVolume"
  | "visibleVolumeBelowTwentyRunEquivalents";

export type OpportunityWarningDetails =
  | {
      type: "staleMarketEvidence";
      observedAt: string;
      ageSeconds: number;
      freshnessTargetSeconds: number;
      refreshState: OpportunityEvidenceState;
    }
  | { type: "thinBook"; reasons: OpportunityThinBookReason[] }
  | { type: "incompleteEivBasis"; missingMaterials: OpportunityMissingMaterial[] };

export interface OpportunityWarning {
  kind: OpportunityWarningKind;
  message: string;
  typeIds: number[];
  details?: OpportunityWarningDetails;
}

export interface OpportunityMetrics {
  materialCost: Money | null;
  installationCost: Money | null;
  totalEstimatedManufacturingCost: Money | null;
  estimatedOutputValue: Money | null;
  estimatedGrossProfit: Money | null;
  grossMarginPercent: string | null;
  estimatedGrossProfitPerUnit: Money | null;
  estimatedGrossProfitPerRun: Money | null;
  estimatedGrossProfitPerManufacturingHour: string | null;
  capitalRequired: Money | null;
}

export interface OpportunityValuation {
  revenue: Money | null;
  grossProfit: Money | null;
  grossMarginPercent: string | null;
  grossProfitPerManufacturingHour: string | null;
  completeness: OpportunityCompleteness;
}

export interface OpportunityValuations {
  sellSide: OpportunityValuation;
  immediateLiquidation: OpportunityValuation;
}

export interface OpportunityOutputMarketEvidence {
  bestSellUnitPrice: Money | null;
  bestSellLevelQuantity: number | null;
  totalVisibleSellQuantity: number;
  sellOrderCount: number;
  sellConsumedFraction: string | null;
  bestBuyUnitPrice: Money | null;
  bestBuyLevelQuantity: number | null;
  totalVisibleBuyQuantity: number;
  buyOrderCount: number;
  buyConsumedFraction: string | null;
  status: OpportunityEvidenceStatus;
  observationBatchId: string;
  importBatchId: string | null;
  importedFileId: string | null;
}

export interface OpportunityEivBasis {
  complete: boolean;
  requiredMaterialCount: number;
  observedMaterialCount: number;
  missingMaterials: OpportunityMissingMaterial[];
}

export interface OpportunityExclusionReason {
  code: string;
  message: string;
  marketGroupId: number;
  marketGroupName: string;
}

export type OpportunityEligibilityStatus = "eligible" | "eligibleWithWarnings" | "excludedFromDefaultRanking";

export interface OpportunityEligibility {
  status: OpportunityEligibilityStatus;
  exclusionReasons: OpportunityExclusionReason[];
}

export type OpportunityEvidenceQuality = "strong" | "qualified" | "weak";

export interface OpportunityQuality {
  evidenceQuality: OpportunityEvidenceQuality;
}

// Mirrors RecipeSelection/BuildRecipe's tagged-union shape exactly, so a
// candidate's `recipe` can be passed straight to createBuild() unchanged.
export type OpportunityRecipeIdentity =
  | { kind: "manufacturing"; blueprintTypeId: number; blueprintName: string }
  | { kind: "reaction"; reactionFormulaTypeId: number; reactionFormulaName: string };

export interface OpportunityCandidate {
  scopeId: ProfitabilityScopeId;
  productTypeId: number;
  productName: string;
  recipe: OpportunityRecipeIdentity;
  sdeVersion: string;
  recipeFingerprint: string;
  runs: number;
  outputQuantity: number;
  baseDurationSeconds: number | null;
  effectiveDurationSeconds: number | null;
  // null for a reaction candidate -- reaction formulas have no material/time
  // efficiency concept in EVE.
  materialEfficiency: number | null;
  timeEfficiency: number | null;
  facilityProfileId: string;
  facilityRevision: number;
  metrics: OpportunityMetrics;
  completeness: OpportunityCompleteness;
  warnings: OpportunityWarning[];
  missingPriceTypeIds: number[];
  valuations: OpportunityValuations;
  outputMarketEvidence: OpportunityOutputMarketEvidence | null;
  eivBasis: OpportunityEivBasis;
  eligibility: OpportunityEligibility;
  quality: OpportunityQuality;
}

export interface OpportunityRankingEntry {
  productTypeId: number;
  // The candidate's recipe type id (blueprint or reaction formula). Ranking
  // matches candidates by productTypeId alone; this is secondary identity
  // metadata, not used for lookups.
  recipeTypeId: number;
}

export interface OpportunityRankings {
  sellSideGrossProfit: OpportunityRankingEntry[];
  immediateLiquidationGrossProfit: OpportunityRankingEntry[];
  sellSideGrossMargin: OpportunityRankingEntry[];
  immediateLiquidationGrossMargin: OpportunityRankingEntry[];
  sellSideGrossProfitPerManufacturingHour: OpportunityRankingEntry[];
  immediateLiquidationGrossProfitPerManufacturingHour: OpportunityRankingEntry[];
}

export interface OpportunityExcludedCost {
  code: string;
  message: string;
}

export interface OpportunityEvaluation {
  context: OpportunityEvaluationContext;
  calculatedAt: string;
  elapsedMilliseconds: number;
  candidateCount: number;
  completeCount: number;
  incompleteCount: number;
  defaultRankingEligibleCount: number;
  excludedCount: number;
  strongEvidenceCount: number;
  qualifiedEvidenceCount: number;
  weakEvidenceCount: number;
  readiness: OpportunityReadiness;
  candidates: OpportunityCandidate[];
  rankings: OpportunityRankings;
  excludedCosts: OpportunityExcludedCost[];
  warnings: OpportunityWarning[];
  assumptions: string[];
  exclusions: string[];
}

export type OpportunityRefreshDisposition = "accepted" | "alreadyPending" | "notRequired";

export interface OpportunityRefreshAcceptance {
  scopeId: ProfitabilityScopeId;
  market: OpportunityRefreshDisposition;
  adjustedPrices: OpportunityRefreshDisposition;
  systemIndex: OpportunityRefreshDisposition;
  acceptedAt: string;
}

export function listOpportunityScopes(): Promise<ProfitabilityScopeDefinition[]> {
  return request("/api/opportunities/scopes");
}

export function evaluateOpportunities(
  command: EvaluateOpportunitiesCommand,
  signal?: AbortSignal,
): Promise<OpportunityEvaluation> {
  return request("/api/opportunities/evaluate", { ...json("POST", command), signal });
}

export function requestOpportunityRefresh(
  command: EvaluateOpportunitiesCommand,
): Promise<OpportunityRefreshAcceptance> {
  return request("/api/opportunities/refresh", json("POST", command));
}
