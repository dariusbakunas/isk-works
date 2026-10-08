import type { OpportunityCandidate, OpportunityEvidenceQuality } from "../../../api/opportunities";
import { valuationOf, type ValuationMode } from "./opportunity-formatters";

export interface OpportunityFilters {
  evidenceQuality: Set<OpportunityEvidenceQuality>;
  hideWeak: boolean;
  minGrossMarginPercent: number;
  maxCapitalRequired: number | null;
  maxDurationSeconds: number | null;
}

export const DEFAULT_FILTERS: OpportunityFilters = {
  evidenceQuality: new Set(["strong", "qualified", "weak"]),
  hideWeak: false,
  minGrossMarginPercent: 0,
  maxCapitalRequired: null,
  maxDurationSeconds: null,
};

export function applyFilters(
  candidates: OpportunityCandidate[],
  filters: OpportunityFilters,
  mode: ValuationMode,
): OpportunityCandidate[] {
  return candidates.filter((candidate) => {
    const quality = candidate.quality.evidenceQuality;
    if (!filters.evidenceQuality.has(quality)) return false;
    if (filters.hideWeak && quality === "weak") return false;

    if (filters.minGrossMarginPercent > 0) {
      const margin = valuationOf(candidate, mode).grossMarginPercent;
      if (margin === null || Number(margin) < filters.minGrossMarginPercent) return false;
    }

    if (filters.maxCapitalRequired !== null) {
      const capital = candidate.metrics.capitalRequired;
      if (capital === null || Number(capital) > filters.maxCapitalRequired) return false;
    }

    if (filters.maxDurationSeconds !== null) {
      const duration = candidate.effectiveDurationSeconds;
      if (duration === null || duration > filters.maxDurationSeconds) return false;
    }

    return true;
  });
}
