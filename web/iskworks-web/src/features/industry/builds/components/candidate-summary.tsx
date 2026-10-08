import { Info } from "lucide-react";
import { useRef, useState, type ReactNode } from "react";

import type { CreateBuildPlanPreview } from "../../../../api/industry";
import { WarningBadge } from "../../../../components/warning-badge";
import { CalculationEvidenceModal } from "../planner/calculation-evidence-modal";
import { formatMoney } from "../../shared/formatting";

type CandidatePreview = CreateBuildPlanPreview;

export function CandidateSummary({
  onShowLogistics,
  preview,
}: {
  /** When set, the decision line offers a jump to Logistics. */
  onShowLogistics?: () => void;
  preview: CandidatePreview | null;
}) {
  const [evidenceOpen, setEvidenceOpen] = useState(false);
  const evidenceTrigger = useRef<HTMLButtonElement>(null);
  const candidate = preview?.candidate;
  const createPreview = preview;
  const installationCost = preview?.worksheet?.summary.installationCost ?? null;
  const marginPercentage = preview?.calculationEvidence?.profitMarginPercent ?? null;
  // A missing material price leaves the material (and so total) cost
  // unknown -- the revision reports it as 0 plus a missing count, so show
  // "Incomplete" rather than a fabricated figure, as profit/margin already do.
  const materialsKnown = !candidate || (candidate.missingPriceCount ?? 0) === 0;
  const closeEvidence = () => {
    setEvidenceOpen(false);
    requestAnimationFrame(() => evidenceTrigger.current?.focus());
  };
  const statusTone = preview?.decision.tone === "blocking"
    ? "border-destructive text-destructive"
    : preview?.decision.tone === "warning"
      ? "border-warning text-warning"
      : "border-positive text-positive";

  return (
    <section
      aria-label="Candidate summary"
      className="min-w-0 border-t border-border pt-3 sm:border-l sm:border-t-0 sm:pl-4 sm:pt-0"
    >
      <div className="grid grid-cols-2 gap-x-4 gap-y-2 sm:grid-cols-3 xl:grid-cols-6">
        <CandidateMetric label="Material cost" value={formatMoney(materialsKnown ? candidate?.estimatedMaterialCost ?? null : null)} />
        <CandidateMetric label="Installation" value={formatMoney(installationCost)} />
        <CandidateMetric label="Total cost" value={formatMoney(materialsKnown ? preview?.worksheet?.summary.totalCost ?? null : null)} />
        <CandidateMetric
          action={candidate && createPreview?.calculationEvidence ? (
            <button
              aria-label="View revenue and profit calculation"
              className="grid h-5 w-5 place-items-center text-muted hover:text-primary"
              onClick={() => setEvidenceOpen(true)}
              ref={evidenceTrigger}
              title="View calculation"
              type="button"
            >
              <Info aria-hidden="true" className="h-3.5 w-3.5" />
            </button>
          ) : null}
          label="Revenue"
          value={formatMoney(candidate?.expectedRevenue ?? null)}
        />
        <CandidateMetric
          label="Estimated profit"
          tone={candidate?.estimatedMargin?.startsWith("-") ? "negative" : "neutral"}
          value={formatMoney(candidate?.estimatedMargin ?? null)}
        />
        <CandidateMetric
          label="Profit margin"
          tone={marginPercentage?.startsWith("-") ? "negative" : "neutral"}
          value={marginPercentage ? `${marginPercentage}%` : "Incomplete"}
        />
      </div>
      {preview ? (
        <div className={`mt-3 flex items-start justify-between gap-2 border-l-2 pl-2 text-xs ${statusTone}`} role="status">
          <div className="min-w-0">
            <strong className="block">{preview.decision.headline}</strong>
            <span className="mt-0.5 block text-muted">{preview.decision.supportingText}</span>
          </div>
          <span className="flex shrink-0 items-center gap-2">
            {onShowLogistics && preview.decision.tone !== "positive" ? (
              <button className="text-xs underline" onClick={onShowLogistics} type="button">
                View logistics
              </button>
            ) : null}
            <WarningBadge warnings={(preview.warnings ?? []).map((warning) => warning.message)} />
          </span>
        </div>
      ) : null}
      {createPreview?.calculationEvidence && evidenceOpen ? (
        <CalculationEvidenceModal
          evidence={createPreview.calculationEvidence}
          onClose={closeEvidence}
          plan={createPreview.candidate}
          profitabilityBasis={createPreview.profitabilityBasis}
        />
      ) : null}
    </section>
  );
}

function CandidateMetric({
  action,
  label,
  tone = "neutral",
  value,
}: {
  action?: ReactNode;
  label: string;
  tone?: "negative" | "neutral";
  value: string;
}) {
  return (
    <div className="min-w-0">
      <span className="flex min-h-5 items-center gap-1 text-[10px] font-semibold uppercase text-muted">
        {label}
        {action}
      </span>
      <strong
        className={`mt-0.5 block truncate font-mono text-xs tabular-nums ${tone === "negative" ? "text-destructive" : ""}`}
        title={value}
      >
        {value}
      </strong>
    </div>
  );
}
