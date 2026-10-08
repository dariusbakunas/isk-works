import type { OpportunityCandidate, OpportunityRankings } from "../../../api/opportunities";
import type { ValuationMode } from "./opportunity-formatters";

export type SortField = "profit" | "margin" | "profitPerHour" | "cost" | "duration";
export type SortDirection = "asc" | "desc";

// The three metrics the backend already ranks (per valuation mode) --
// reusing its order avoids re-deriving the same tie-break rules
// (complete-first, then descending value, then name, then type ID)
// client-side. Cost/Duration aren't ranked by the backend since they're
// plain shared fields, not per-valuation metrics -- those sort client-side.
const RANKING_KEY: Record<ValuationMode, Partial<Record<SortField, keyof OpportunityRankings>>> = {
  sellSide: {
    profit: "sellSideGrossProfit",
    margin: "sellSideGrossMargin",
    profitPerHour: "sellSideGrossProfitPerManufacturingHour",
  },
  immediateLiquidation: {
    profit: "immediateLiquidationGrossProfit",
    margin: "immediateLiquidationGrossMargin",
    profitPerHour: "immediateLiquidationGrossProfitPerManufacturingHour",
  },
};

export function sortCandidates(
  candidates: OpportunityCandidate[],
  rankings: OpportunityRankings,
  mode: ValuationMode,
  field: SortField,
  direction: SortDirection,
): OpportunityCandidate[] {
  const rankingKey = RANKING_KEY[mode][field];
  const byTypeId = new Map(candidates.map((candidate) => [candidate.productTypeId, candidate]));

  if (rankingKey) {
    const ordered = rankings[rankingKey]
      .map((entry) => byTypeId.get(entry.productTypeId))
      .filter((candidate): candidate is OpportunityCandidate => candidate !== undefined);
    // Candidates the backend's ranking omitted outright (shouldn't happen
    // for an already-ranked-only list, but keep them visible rather than
    // silently dropping a row) sort after the ranked ones.
    const ordinaryIds = new Set(ordered.map((candidate) => candidate.productTypeId));
    const unranked = candidates.filter((candidate) => !ordinaryIds.has(candidate.productTypeId));
    const combined = [...ordered, ...unranked];
    return direction === "desc" ? combined : combined.slice().reverse();
  }

  const value = field === "cost"
    ? (candidate: OpportunityCandidate) => numberOrNull(candidate.metrics.totalEstimatedManufacturingCost)
    : (candidate: OpportunityCandidate) => candidate.effectiveDurationSeconds;

  return candidates.slice().sort((left, right) => {
    const leftValue = value(left);
    const rightValue = value(right);
    if (leftValue === null && rightValue === null) return 0;
    if (leftValue === null) return 1;
    if (rightValue === null) return -1;
    return direction === "desc" ? rightValue - leftValue : leftValue - rightValue;
  });
}

function numberOrNull(value: string | null): number | null {
  return value === null ? null : Number(value);
}
