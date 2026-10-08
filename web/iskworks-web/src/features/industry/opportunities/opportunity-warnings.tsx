import type { OpportunityThinBookReason, OpportunityWarning, OpportunityWarningKind } from "../../../api/opportunities";
import { formatDuration } from "../shared/formatting";
import { formatEvidenceAge } from "./opportunity-formatters";

// Kept in sync with the backend's derive_evidence_quality tiering (crates/iskworks-core/src/opportunity_quality.rs):
// incompleteInstallationCost and multipleOutputsUnsupported never move the evidenceQuality tier, even though they
// render here -- that's real backend behavior, not something to route around client-side.
const WARNING_TITLES: Record<OpportunityWarningKind, string> = {
  missingMaterialPrice: "No material price",
  missingOutputPrice: "No output valuation",
  insufficientMarketDepth: "Insufficient depth",
  incompleteInstallationCost: "Incomplete installation cost",
  multipleOutputsUnsupported: "Multiple outputs",
  staleMarketEvidence: "Stale market evidence",
  thinOutputBook: "Thin output book",
  thinInputBook: "Thin input book",
  incompleteEivBasis: "Incomplete EIV basis",
};

const THIN_BOOK_REASON_LABELS: Record<OpportunityThinBookReason, string> = {
  bestLevelAtOrBelowOutputQuantity: "Best price level doesn't cover the output quantity",
  outputAtOrAboveTenPercentOfVisibleVolume: "Output is 10%+ of visible volume",
  visibleVolumeBelowTwentyRunEquivalents: "Visible volume is below 20 run-equivalents",
};

export function warningTitle(kind: OpportunityWarningKind): string {
  return WARNING_TITLES[kind];
}

export function WarningCard({ warning }: { warning: OpportunityWarning }) {
  return (
    <li className="rounded-md border border-border bg-panel-strong p-2 text-xs">
      <p className="font-semibold">{warningTitle(warning.kind)}</p>
      <WarningBody warning={warning} />
    </li>
  );
}

function WarningBody({ warning }: { warning: OpportunityWarning }) {
  const details = warning.details;
  if (!details) return <p className="mt-0.5 text-muted">{warning.message}</p>;

  if (details.type === "staleMarketEvidence") {
    return (
      <p className="mt-0.5 text-muted">
        {formatEvidenceAge(details.ageSeconds)} · {formatDuration(details.freshnessTargetSeconds)} target
      </p>
    );
  }

  if (details.type === "thinBook") {
    return (
      <ul className="mt-0.5 list-inside list-disc text-muted">
        {details.reasons.map((reason) => (
          <li key={reason}>{THIN_BOOK_REASON_LABELS[reason]}</li>
        ))}
      </ul>
    );
  }

  return (
    <p className="mt-0.5 text-muted">
      Missing: {details.missingMaterials.map((material) => material.typeName).join(", ")}
    </p>
  );
}
