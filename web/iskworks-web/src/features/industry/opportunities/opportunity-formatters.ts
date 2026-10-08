import type { MarketScope } from "../../../api/industry";
import type { OpportunityCandidate, OpportunityValuation } from "../../../api/opportunities";

export type ValuationMode = "sellSide" | "immediateLiquidation";

// A cheap, synchronous scope label -- no region/location name lookup, since
// nothing else here needs one badly enough to justify an extra fetch.
export function describeMarketScope(scope: MarketScope): string {
  return scope.locationId ? `Region ${scope.regionId} · Location ${scope.locationId}` : `Region ${scope.regionId} (all locations)`;
}

export function valuationOf(candidate: OpportunityCandidate, mode: ValuationMode): OpportunityValuation {
  return candidate.valuations[mode];
}

// Opportunity percent fields (grossMarginPercent, etc.) arrive as full-
// precision decimal strings (e.g. "92.72512929070730066666666667"), unlike
// the pre-rounded values other features' formatPercent expects -- round for
// display here rather than changing that shared helper's contract.
export function formatOpportunityPercent(value: string | null): string {
  if (value === null) return "Unavailable";
  const rounded = Number(value);
  if (!Number.isFinite(rounded)) return "Unavailable";
  return `${rounded.toFixed(2)}%`;
}

export function formatEvidenceAge(ageSeconds: number | null): string {
  if (ageSeconds === null) return "unknown";
  if (ageSeconds < 60) return "just now";
  const minutes = Math.floor(ageSeconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  const remainingMinutes = minutes % 60;
  return remainingMinutes > 0 ? `${hours}h ${remainingMinutes}m ago` : `${hours}h ago`;
}
