import { useState } from "react";

import {
  requestOpportunityRefresh,
  type EvaluateOpportunitiesCommand,
  type OpportunityCandidate,
  type OpportunityEvaluation,
  type OpportunityRefreshAcceptance,
  type OpportunityRefreshDisposition,
} from "../../../api/opportunities";
import { InlineAlert } from "../../../components/primitives";
import { formatDuration } from "../shared/formatting";
import { formatEvidenceAge } from "./opportunity-formatters";
import { apiMessage } from "./shared";

type RefreshState =
  | { status: "idle" }
  | { status: "loading" }
  | { status: "done"; acceptance: OpportunityRefreshAcceptance }
  | { status: "error"; message: string };

export function FreshnessBanner({
  evaluation,
  requestCommand,
}: {
  evaluation: OpportunityEvaluation;
  requestCommand: EvaluateOpportunitiesCommand;
}) {
  const [refreshState, setRefreshState] = useState<RefreshState>({ status: "idle" });
  const readiness = evaluation.readiness;

  const ageSeconds = readiness.oldestMarketObservedAt
    ? Math.max(0, Math.floor((Date.now() - new Date(readiness.oldestMarketObservedAt).getTime()) / 1000))
    : null;
  const freshnessTargetSeconds = findFreshnessTargetSeconds(evaluation.candidates);
  // Old evidence stays usable (readiness.*.usable) even when a background refresh attempt failed --
  // this is an informational note, never a blocking error, since the evaluation itself succeeded.
  const backgroundRefreshErrors = [readiness.adjustedPrices.lastRefreshError, readiness.systemIndex.lastRefreshError].filter(
    (message): message is string => message !== null,
  );

  async function refresh() {
    setRefreshState({ status: "loading" });
    try {
      const acceptance = await requestOpportunityRefresh(requestCommand);
      setRefreshState({ status: "done", acceptance });
    } catch (error) {
      setRefreshState({ status: "error", message: apiMessage(error) });
    }
  }

  return (
    <div className="mb-2 rounded-md border border-border bg-panel px-3 py-1.5 text-xs">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <span className="text-muted">
          Market data updated {ageSeconds === null ? "unknown" : formatEvidenceAge(ageSeconds)}
          {freshnessTargetSeconds !== null ? ` · ${formatDuration(freshnessTargetSeconds)} target` : ""}
          {readiness.refreshPending ? " · Refresh pending" : ""}
          {readiness.marketStaleCount > 0 ? ` · ${readiness.marketStaleCount} stale` : ""}
          {readiness.marketMissingCount > 0 ? ` · ${readiness.marketMissingCount} missing` : ""}
        </span>
        <button className="iw-button-secondary" disabled={refreshState.status === "loading"} onClick={refresh} type="button">
          {refreshState.status === "loading" ? "Refreshing..." : "Refresh"}
        </button>
      </div>
      {backgroundRefreshErrors.length > 0 ? (
        <p className="mt-1 text-warning">
          Last background refresh failed ({backgroundRefreshErrors.join("; ")}) -- showing the last usable data.
        </p>
      ) : null}
      {refreshState.status === "done" ? (
        <div className="mt-1.5">
          <InlineAlert title="Refresh requested" tone="info">
            Market {dispositionLabel(refreshState.acceptance.market)} · Adjusted prices{" "}
            {dispositionLabel(refreshState.acceptance.adjustedPrices)} · System index{" "}
            {dispositionLabel(refreshState.acceptance.systemIndex)}
          </InlineAlert>
        </div>
      ) : null}
      {refreshState.status === "error" ? (
        <div className="mt-1.5">
          <InlineAlert title="Refresh request failed">{refreshState.message}</InlineAlert>
        </div>
      ) : null}
    </div>
  );
}

function findFreshnessTargetSeconds(candidates: OpportunityCandidate[]): number | null {
  for (const candidate of candidates) {
    for (const warning of candidate.warnings) {
      if (warning.details?.type === "staleMarketEvidence") return warning.details.freshnessTargetSeconds;
    }
  }
  return null;
}

function dispositionLabel(disposition: OpportunityRefreshDisposition): string {
  return disposition === "accepted" ? "refresh accepted" : disposition === "alreadyPending" ? "already pending" : "not required";
}
