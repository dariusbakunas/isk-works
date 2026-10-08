import type { ReactNode } from "react";

import type { FacilityProfile, MarketScope } from "../../../api/industry";
import type { OpportunityCandidate, OpportunityExcludedCost, OpportunityValuation } from "../../../api/opportunities";
import { formatIskCompact, formatIskSummary } from "../../../components/money";
import { PlannerInspectorShell } from "../../../components/planner-inspector-shell";
import { formatDuration } from "../shared/formatting";
import { EvidenceQualityBadge } from "./opportunity-badges";
import { describeMarketScope, formatEvidenceAge, formatOpportunityPercent } from "./opportunity-formatters";
import { WarningCard } from "./opportunity-warnings";

export function OpportunityInspector({
  candidate,
  creatingBuild,
  excludedCosts,
  facility,
  marketScope,
  onClose,
  onCreateBuild,
}: {
  candidate: OpportunityCandidate;
  creatingBuild: boolean;
  excludedCosts: OpportunityExcludedCost[];
  facility: FacilityProfile | null;
  marketScope: MarketScope;
  onClose: () => void;
  onCreateBuild: () => void;
}) {
  const evidence = candidate.outputMarketEvidence;
  const recipeName = candidate.recipe.kind === "manufacturing" ? candidate.recipe.blueprintName : candidate.recipe.reactionFormulaName;
  return (
    <PlannerInspectorShell
      eyebrow="Opportunity"
      onClose={onClose}
      open
      returnFocusRowKey={String(candidate.productTypeId)}
      title={candidate.productName}
    >
      <div className="px-3 pb-3">
        <div className="flex items-center gap-2 pt-1">
          <EvidenceQualityBadge quality={candidate.quality.evidenceQuality} />
          <span className="truncate text-xs text-muted">{recipeName}</span>
        </div>

        <button className="iw-button-primary mt-2 w-full" disabled={creatingBuild} onClick={onCreateBuild} type="button">
          {creatingBuild ? "Creating Build..." : "Create Build"}
        </button>

        <InspectorSection label="Overview">
          <InspectorLine label="Facility" value={facility?.name ?? "Unknown"} />
          <InspectorLine label="Runs" value={String(candidate.runs)} />
          <InspectorLine label="Duration" value={formatDuration(candidate.effectiveDurationSeconds)} />
          {candidate.materialEfficiency !== null && candidate.timeEfficiency !== null ? (
            <InspectorLine label="ME / TE" value={`ME${candidate.materialEfficiency} / TE${candidate.timeEfficiency}`} />
          ) : null}
        </InspectorSection>

        <InspectorSection label="Cost">
          <InspectorLine label="Material cost" value={money(candidate.metrics.materialCost)} />
          <InspectorLine
            label="Installation cost"
            value={candidate.eivBasis.complete ? money(candidate.metrics.installationCost) : `${money(candidate.metrics.installationCost)} · Incomplete`}
          />
          <InspectorLine label="Total / capital required" value={money(candidate.metrics.totalEstimatedManufacturingCost)} />
        </InspectorSection>

        <InspectorSection label="Valuation">
          <ValuationBlock label="Sell-side" valuation={candidate.valuations.sellSide} />
          <ValuationBlock label="Liquidation" valuation={candidate.valuations.immediateLiquidation} />
        </InspectorSection>

        <InspectorSection label="Market Evidence">
          {evidence ? (
            <>
              <InspectorLine label="Location" value={describeMarketScope(marketScope)} />
              <InspectorLine
                label="Best sell"
                value={`${money(evidence.bestSellUnitPrice)} × ${evidence.bestSellLevelQuantity?.toLocaleString() ?? "—"}`}
              />
              <InspectorLine
                label="Visible sell depth"
                value={`${evidence.totalVisibleSellQuantity.toLocaleString()} (${evidence.sellOrderCount} orders)`}
              />
              <InspectorLine
                label="Best buy"
                value={`${money(evidence.bestBuyUnitPrice)} × ${evidence.bestBuyLevelQuantity?.toLocaleString() ?? "—"}`}
              />
              <InspectorLine
                label="Visible buy depth"
                value={`${evidence.totalVisibleBuyQuantity.toLocaleString()} (${evidence.buyOrderCount} orders)`}
              />
              <InspectorLine label="Observed" value={formatEvidenceAge(evidence.status.ageSeconds)} />
            </>
          ) : (
            <p className="text-xs text-muted">No market evidence for this candidate.</p>
          )}
        </InspectorSection>

        <InspectorSection label="Evidence Quality">
          {candidate.warnings.length === 0 ? (
            <p className="text-xs text-muted">No evidence warnings for this candidate.</p>
          ) : (
            <ul className="grid gap-1.5">
              {candidate.warnings.map((warning, index) => (
                <WarningCard key={`${warning.kind}-${index}`} warning={warning} />
              ))}
            </ul>
          )}
        </InspectorSection>

        {!candidate.eivBasis.complete ? (
          <InspectorSection label="EIV Basis">
            <InspectorLine
              label="Materials observed"
              value={`${candidate.eivBasis.observedMaterialCount} / ${candidate.eivBasis.requiredMaterialCount}`}
            />
            {candidate.eivBasis.missingMaterials.length > 0 ? (
              <p className="mt-1 text-xs text-muted">
                Missing: {candidate.eivBasis.missingMaterials.map((material) => material.typeName).join(", ")}
              </p>
            ) : null}
          </InspectorSection>
        ) : null}

        {excludedCosts.length > 0 ? (
          <InspectorSection label="Not Included in Estimate">
            <ul className="grid gap-1 text-xs text-muted">
              {excludedCosts.map((cost) => (
                <li key={cost.code}>{cost.message}</li>
              ))}
            </ul>
          </InspectorSection>
        ) : null}
      </div>
    </PlannerInspectorShell>
  );
}

function ValuationBlock({ label, valuation }: { label: string; valuation: OpportunityValuation }) {
  return (
    <div className="mb-2 last:mb-0">
      <p className="mb-1 text-[10px] font-semibold uppercase text-muted">{label}</p>
      <InspectorLine label="Revenue" value={money(valuation.revenue)} />
      <InspectorLine label="Gross profit" value={money(valuation.grossProfit)} />
      <InspectorLine label="Margin" value={formatOpportunityPercent(valuation.grossMarginPercent)} />
      <InspectorLine
        label="Profit / hour"
        value={valuation.grossProfitPerManufacturingHour === null ? "Incomplete" : `${formatIskCompact(valuation.grossProfitPerManufacturingHour)}/h`}
      />
    </div>
  );
}

function InspectorSection({ children, label }: { children: ReactNode; label: string }) {
  return (
    <section aria-label={label} className="border-b border-border py-2">
      <h3 className="mb-1 text-[10px] font-semibold uppercase text-muted">{label}</h3>
      {children}
    </section>
  );
}

function InspectorLine({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-start justify-between gap-3 py-1 text-xs">
      <span className="text-muted">{label}</span>
      <strong className="text-right font-mono">{value}</strong>
    </div>
  );
}

function money(value: string | null): string {
  return value === null ? "Incomplete" : formatIskSummary(value);
}
