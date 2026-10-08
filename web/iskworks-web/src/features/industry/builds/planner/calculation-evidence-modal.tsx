import { AlertTriangle, Info, X } from "lucide-react";
import { useEffect, useRef, type ReactNode } from "react";

import {
  rootFacility,
  type BuildPlanRevision,
  type CalculationEvidenceProjection,
} from "../../../../api/industry";
import { formatIskSummary } from "../../../../components/money";

export function CalculationEvidenceModal({
  evidence,
  plan,
  profitabilityBasis,
  onClose,
}: {
  evidence: CalculationEvidenceProjection;
  plan: BuildPlanRevision;
  profitabilityBasis: { includedCosts: string[]; excludedCosts: string[] };
  onClose: () => void;
}) {
  const closeButton = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    closeButton.current?.focus();
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [onClose]);

  const facility = rootFacility(plan);
  return (
    <div
      className="fixed inset-0 z-[80] grid place-items-center bg-black/70 p-2 sm:p-4"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
      role="presentation"
    >
      <section
        aria-labelledby="calculation-evidence-title"
        aria-modal="true"
        className="iw-dialog flex max-h-[calc(100vh-1rem)] w-[min(920px,100%)] flex-col overflow-hidden sm:max-h-[calc(100vh-2rem)]"
        role="dialog"
      >
        <header className="flex shrink-0 items-start justify-between gap-3 border-b border-border bg-panel-strong px-4 py-3">
          <div className="min-w-0">
            <h2 className="text-base font-semibold" id="calculation-evidence-title">
              Calculation Evidence - Revenue and Profit
            </h2>
            <p className="mt-1 truncate text-xs text-muted">
              {facility?.profile.name ?? "Facility not selected"}
              {facility?.profile.structureTypeName ? ` · ${facility.profile.structureTypeName}` : ""}
              {" · Candidate plan · not committed"}
            </p>
          </div>
          <button
            aria-label="Close calculation evidence"
            className="grid h-7 w-7 shrink-0 place-items-center rounded text-muted hover:bg-panel hover:text-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary"
            onClick={onClose}
            ref={closeButton}
            type="button"
          >
            <X aria-hidden="true" className="h-3.5 w-3.5" />
          </button>
        </header>

        <div className="min-h-0 flex-1 space-y-7 overflow-y-auto px-4 py-5 sm:px-6">
          <CalculationEvidenceProfit evidence={evidence} plan={plan} />
          <CalculationEvidenceAssumptions plan={plan} />
          <CalculationEvidenceMaterialCost evidence={evidence} plan={plan} />
          <CalculationEvidenceInstallation evidence={evidence} plan={plan} />
          <CalculationEvidenceDuration evidence={evidence} />
          <CalculationEvidenceExcludedCosts
            excludedCosts={profitabilityBasis.excludedCosts}
            installationComplete={Boolean(facility?.installationCost.complete)}
          />
          <CalculationEvidenceExactFormulas evidence={evidence} plan={plan} />
        </div>

        <footer className="flex shrink-0 items-center justify-between gap-3 border-t border-border bg-panel-strong px-4 py-2.5">
          <span className="inline-flex items-center gap-1.5 text-[11px] text-muted">
            <Info aria-hidden="true" className="h-3 w-3" />
            Preview only. No inventory, accounting, or market events have occurred.
          </span>
          <button className="iw-button-secondary" onClick={onClose} type="button">Close</button>
        </footer>
      </section>
    </div>
  );
}

function CalculationEvidenceProfit({
  evidence,
  plan,
}: {
  evidence: CalculationEvidenceProjection;
  plan: BuildPlanRevision;
}) {
  const installation = rootFacility(plan)?.installationCost.total ?? null;
  const complete = plan.expectedRevenue !== null
    && plan.pricingComplete
    && installation !== null
    && plan.estimatedMargin !== null;
  return (
    <EvidenceSection label="Profit Calculation">
      <div className="overflow-hidden rounded border border-border bg-panel-strong">
        <div className="grid grid-cols-[1.25rem_minmax(0,1fr)_minmax(9rem,auto)] border-b border-border px-3 py-2 text-[10px] uppercase text-muted">
          <span />
          <span>Component</span>
          <span className="text-right">Amount</span>
        </div>
        <div className="px-3">
          <EquationRow
            label="Expected revenue"
            note={outputPricingNote(plan)}
            tone="positive"
            value={plan.expectedRevenue}
          />
          <EquationRow
            label="Adjusted material cost"
            note={`${plan.materialLines.length} inputs · ${plan.snapshot.sourceName}`}
            operator="-"
            value={plan.pricingComplete ? plan.estimatedMaterialCost : null}
          />
          <EquationRow
            label="Installation cost"
            note={installationNote(plan)}
            operator="-"
            value={installation}
          />
          <EquationRow
            emphasis
            label="Estimated profit"
            operator="="
            tone={plan.estimatedMargin?.startsWith("-") ? "negative" : "positive"}
            value={complete ? plan.estimatedMargin : null}
          />
        </div>
        <div className="flex gap-6 border-t border-border bg-panel px-3 py-2 text-xs text-muted">
          <span>Margin <strong className="ml-1 font-mono text-foreground">{evidence.profitMarginPercent ? `${evidence.profitMarginPercent}%` : "Incomplete"}</strong></span>
          <span>Duration <strong className="ml-1 font-mono text-foreground">{formatDuration(plan.blueprint?.plannedDurationSeconds ?? null)}</strong></span>
        </div>
      </div>
      {!complete ? (
        <p className="mt-2 flex items-start gap-2 text-xs text-warning">
          <AlertTriangle aria-hidden="true" className="mt-0.5 h-3.5 w-3.5 shrink-0" />
          Revenue, all material prices, and installation cost must be available before profit is complete.
        </p>
      ) : null}
    </EvidenceSection>
  );
}

function CalculationEvidenceAssumptions({ plan }: { plan: BuildPlanRevision }) {
  const facility = rootFacility(plan);
  const blueprint = plan.blueprint;
  const materialPolicy = plan.snapshot.items.find((item) => item.itemRole === "material")?.pricingPolicy;
  return (
    <EvidenceSection label="Assumptions">
      <div className="grid gap-5 md:grid-cols-2">
        <div>
          <Subheading>Facility</Subheading>
          <KeyValue label="Name" value={facility?.profile.name ?? "Not selected"} />
          <KeyValue label="Structure type" value={facility?.profile.structureTypeName || "Unavailable"} />
          <KeyValue label="Solar system" value={facility?.profile.solarSystemName || "Unavailable"} />
        </div>
        <div>
          <Subheading>Blueprint</Subheading>
          <KeyValue label="Blueprint" value={blueprint?.blueprintName ?? "Unavailable"} />
          <KeyValue label="Kind" value={blueprint ? titleCase(blueprint.kind) : "Unavailable"} />
          <KeyValue label="Efficiency" value={blueprint ? `ME ${blueprint.materialEfficiency} · TE ${blueprint.timeEfficiency}` : "Unavailable"} />
          <KeyValue label="Runs" value={blueprint?.licensedRuns === null ? "Unlimited" : blueprint?.licensedRuns?.toLocaleString() ?? "Unknown"} />
        </div>
        <div>
          <Subheading>Pricing</Subheading>
          <KeyValue label="Material pricing" value={pricingPolicyLabel(materialPolicy)} />
          <KeyValue label="Output pricing" value={outputPricingNote(plan)} />
          <KeyValue label="Price source" value={plan.snapshot.sourceName} />
          <KeyValue label="Captured" value={new Date(plan.snapshot.createdAt).toLocaleString()} />
        </div>
      </div>
    </EvidenceSection>
  );
}

function CalculationEvidenceMaterialCost({
  evidence,
  plan,
}: {
  evidence: CalculationEvidenceProjection;
  plan: BuildPlanRevision;
}) {
  const material = evidence.materialCost;
  return (
    <EvidenceSection label="Material Cost">
      <p className="mb-3 text-xs leading-5 text-muted">
        Rust applies each modifier to per-item requirements and rounds final quantities according to the facility formula.
      </p>
      <StepTable amountLabel="Running market cost">
        <Step label="Base recipe requirements" note={`${plan.runs.toLocaleString()} runs before efficiency modifiers`} value={material.baseMarketValue} />
        <Step label={`Blueprint ME ${plan.blueprint?.materialEfficiency ?? 0}`} multiplier={material.blueprintMultiplier} value={material.afterBlueprintMe} />
        <Step label="Facility structure material bonus" multiplier={material.structureMultiplier} value={material.afterStructure} />
        <Step label="Facility rig material bonuses" multiplier={material.rigMultiplier} value={material.adjustedMaterialCost} />
        <Step emphasis label="Adjusted material cost" value={material.adjustedMaterialCost} />
      </StepTable>
    </EvidenceSection>
  );
}

function CalculationEvidenceInstallation({
  evidence,
  plan,
}: {
  evidence: CalculationEvidenceProjection;
  plan: BuildPlanRevision;
}) {
  const cost = rootFacility(plan)?.installationCost;
  return (
    <EvidenceSection label="Installation Cost">
      {!cost ? <Unavailable text="Select a facility to calculate installation cost." /> : (
        <>
          <p className="mb-3 text-xs leading-5 text-muted">
            EVE adjusted-price EIV is separate from market material cost. Taxes and surcharges remain explicit terms.
          </p>
          <div className="overflow-hidden rounded border border-border bg-panel-strong">
            <InstallationRow label="Adjusted-price EIV basis" value={cost.estimatedItemValue} />
            <InstallationRow label="Unmodified system-index cost" rate={evidence.systemCostIndexPercent ? `${evidence.systemCostIndexPercent}%` : "Unavailable"} value={cost.unmodifiedSystemIndexCost} />
            <InstallationRow label="Job-cost reduction" rate={`${cost.jobCostReductionPercent}%`} value={cost.systemIndexCost} />
            <InstallationRow label="SCC surcharge" rate={`${rootFacility(plan)?.profile.sccSurchargePercent}%`} value={cost.sccSurcharge} />
            <InstallationRow label="Facility tax" rate={`${rootFacility(plan)?.profile.facilityTaxPercent}%`} value={cost.facilityTax} />
            <InstallationRow label="Alliance surcharge" rate={`${rootFacility(plan)?.profile.allianceSurchargePercent}%`} value={cost.allianceSurcharge} />
            <InstallationRow label="Fixed supplemental cost" value={cost.fixedSupplementalCost} />
            <InstallationRow emphasis label="Total installation cost" value={cost.total} />
          </div>
          {cost.warnings.map((warning) => <p className="mt-2 text-xs text-warning" key={warning}>{warning}</p>)}
        </>
      )}
    </EvidenceSection>
  );
}

function CalculationEvidenceDuration({ evidence }: { evidence: CalculationEvidenceProjection }) {
  return (
    <EvidenceSection label="Duration">
      {evidence.durationSteps.length === 0 ? <Unavailable text="Duration steps are unavailable without a facility projection." /> : (
        <div className="overflow-hidden rounded border border-border bg-panel-strong">
          {evidence.durationSteps.map((step, index) => (
            <div className="grid gap-1 border-b border-border px-3 py-2 last:border-0 sm:grid-cols-[minmax(0,1fr)_7rem_10rem] sm:items-center" key={`${step.label}-${index}`}>
              <span className="text-xs"><strong className="block">{step.label}</strong><small className="text-muted">{step.detail}</small></span>
              <span className="text-right font-mono text-xs text-muted">× {step.multiplier}</span>
              <span className="text-right font-mono text-xs">{formatDuration(step.runningDurationSeconds)}</span>
            </div>
          ))}
        </div>
      )}
    </EvidenceSection>
  );
}

function CalculationEvidenceExcludedCosts({
  excludedCosts,
  installationComplete,
}: {
  excludedCosts: string[];
  installationComplete: boolean;
}) {
  const exclusions = excludedCosts.map(exclusionDescription);
  if (!installationComplete && !excludedCosts.includes("installationCost")) {
    exclusions.unshift(exclusionDescription("installationCost"));
  }
  return (
    <EvidenceSection label="Excluded From Estimated Profit">
      <p className="mb-2 text-xs leading-5 text-muted">
        These terms are not included because ISK Works does not calculate them yet, not because they are negligible.
      </p>
      <div className="divide-y divide-border">
        {exclusions.map((item) => (
          <div className="flex items-start gap-2 py-2" key={item.label}>
            <Info aria-hidden="true" className="mt-0.5 h-3.5 w-3.5 shrink-0 text-muted" />
            <span><strong className="block text-xs">{item.label}</strong><small className="text-muted">{item.detail}</small></span>
          </div>
        ))}
      </div>
    </EvidenceSection>
  );
}

function CalculationEvidenceExactFormulas({
  evidence,
  plan,
}: {
  evidence: CalculationEvidenceProjection;
  plan: BuildPlanRevision;
}) {
  return (
    <details className="overflow-hidden rounded border border-border">
      <summary className="cursor-pointer bg-panel-strong px-3 py-2.5 text-xs font-semibold">
        Exact formulas and provenance
      </summary>
      <div className="space-y-5 border-t border-border px-3 py-4">
        <div>
          <Subheading>Material requirement traces</Subheading>
          <pre className="overflow-x-auto whitespace-pre-wrap rounded border border-border bg-background p-3 font-mono text-[11px] leading-5 text-muted">
            {evidence.materialCost.requirementTraces.join("\n") || "No facility requirement traces are available."}
          </pre>
        </div>
        <div>
          <Subheading>Provenance and versions</Subheading>
          <KeyValue label="Price source" value={plan.snapshot.sourceName} />
          <KeyValue label="Price source revision" value={plan.snapshot.sourceRevision.toLocaleString()} />
          <KeyValue label="Price snapshot captured" value={new Date(plan.snapshot.createdAt).toLocaleString()} />
          <KeyValue label="Recipe fingerprint" value={plan.recipeFingerprint} />
          <KeyValue label="Blueprint type ID" value={plan.blueprint?.blueprintTypeId.toLocaleString() ?? "Unavailable"} />
          <KeyValue label="Blueprint formula" value={plan.blueprint?.formulaVersion ?? "Unavailable"} />
          <KeyValue label="Facility revision" value={rootFacility(plan)?.profile.revision.toLocaleString() ?? "Unavailable"} />
          <KeyValue label="Facility formula" value={rootFacility(plan)?.formulaVersion ?? "Unavailable"} />
          <KeyValue label="Installation formula" value={rootFacility(plan)?.installationCost.formulaVersion ?? "Unavailable"} />
        </div>
      </div>
    </details>
  );
}

function EvidenceSection({ children, label }: { children: ReactNode; label: string }) {
  return (
    <section aria-label={label}>
      <div className="mb-3 flex items-center gap-3">
        <h3 className="whitespace-nowrap text-[10px] font-semibold uppercase text-muted">{label}</h3>
        <span className="h-px flex-1 bg-border" />
      </div>
      {children}
    </section>
  );
}

function EquationRow({
  emphasis = false,
  label,
  note,
  operator = "",
  tone = "neutral",
  value,
}: {
  emphasis?: boolean;
  label: string;
  note?: string;
  operator?: string;
  tone?: "neutral" | "positive" | "negative";
  value: string | null;
}) {
  const color = tone === "positive" ? "text-positive" : tone === "negative" ? "text-destructive" : "";
  return (
    <div className={`grid grid-cols-[1.25rem_minmax(0,1fr)_minmax(9rem,auto)] items-center gap-2 border-b border-border py-2.5 last:border-0 ${emphasis ? "border-t-2" : ""}`}>
      <span className="text-right font-mono text-sm text-muted">{operator}</span>
      <span className="min-w-0 text-xs"><strong className="block">{label}</strong>{note ? <small className="block truncate text-muted" title={note}>{note}</small> : null}</span>
      <strong className={`text-right font-mono text-xs tabular-nums ${emphasis ? "text-sm" : ""} ${color}`}>
        {value ? formatIskSummary(value) : "Unavailable"}
      </strong>
    </div>
  );
}

function StepTable({ amountLabel, children }: { amountLabel: string; children: ReactNode }) {
  return (
    <div className="overflow-hidden rounded border border-border bg-panel-strong">
      <div className="grid grid-cols-[minmax(0,1fr)_7rem_minmax(9rem,auto)] border-b border-border px-3 py-2 text-[10px] uppercase text-muted">
        <span>Step</span><span className="text-right">Multiplier</span><span className="text-right">{amountLabel}</span>
      </div>
      {children}
    </div>
  );
}

function Step({
  emphasis = false,
  label,
  multiplier,
  note,
  value,
}: {
  emphasis?: boolean;
  label: string;
  multiplier?: string;
  note?: string;
  value: string | null;
}) {
  return (
    <div className={`grid grid-cols-[minmax(0,1fr)_7rem_minmax(9rem,auto)] items-center gap-2 border-b border-border px-3 py-2 last:border-0 ${emphasis ? "border-t-2" : ""}`}>
      <span className="text-xs"><strong className="block">{label}</strong>{note ? <small className="text-muted">{note}</small> : null}</span>
      <span className="text-right font-mono text-xs text-muted">{multiplier ? `× ${multiplier}` : ""}</span>
      <strong className="text-right font-mono text-xs tabular-nums">{value ? formatIskSummary(value) : "Unavailable"}</strong>
    </div>
  );
}

function InstallationRow({
  emphasis = false,
  label,
  rate,
  value,
}: {
  emphasis?: boolean;
  label: string;
  rate?: string;
  value: string | null;
}) {
  return (
    <div className={`grid grid-cols-[minmax(0,1fr)_6rem_minmax(9rem,auto)] items-center gap-2 border-b border-border px-3 py-2 last:border-0 ${emphasis ? "border-t-2" : ""}`}>
      <strong className="text-xs">{label}</strong>
      <span className="text-right font-mono text-xs text-muted">{rate ?? ""}</span>
      <strong className="text-right font-mono text-xs tabular-nums">{value ? formatIskSummary(value) : "Unavailable"}</strong>
    </div>
  );
}

function KeyValue({ label, value }: { label: string; value: string }) {
  return <div className="grid grid-cols-[8rem_minmax(0,1fr)] gap-2 py-1 text-xs"><span className="text-muted">{label}</span><strong className="min-w-0 break-words">{value}</strong></div>;
}

function Subheading({ children }: { children: ReactNode }) {
  return <h4 className="mb-1 text-[10px] font-semibold uppercase text-muted">{children}</h4>;
}

function Unavailable({ text }: { text: string }) {
  return <p className="rounded border border-warning/30 bg-warning/5 px-3 py-2 text-xs text-warning">{text}</p>;
}

function outputPricingNote(plan: BuildPlanRevision): string {
  const output = plan.snapshot.items.find((item) => item.itemRole === "output");
  if (!output || output.missing) return "Output price is unavailable";
  if (output.selectionKind === "manual") return `Manual · ${output.price ? formatIskSummary(output.price) : "Unavailable"}`;
  return `${plan.snapshot.sourceName} · ${pricingPolicyLabel(output.pricingPolicy)}`;
}

function installationNote(plan: BuildPlanRevision): string {
  const profile = rootFacility(plan)?.profile;
  if (!profile) return "Facility not selected";
  return `SCI ${rootFacility(plan)?.installationCost.systemCostIndex ?? "Unavailable"} · SCC ${profile.sccSurchargePercent}% · tax ${profile.facilityTaxPercent}%`;
}

function pricingPolicyLabel(policy: string | null | undefined): string {
  switch (policy) {
    case "highestBuy": return "Highest buy order";
    case "acquireQuantityFromSellOrders": return "Buy immediately";
    case "lowestSell": return "Lowest sell order";
    case "liquidateQuantityIntoBuyOrders": return "Sell immediately";
    default: return "Unavailable";
  }
}

function exclusionDescription(value: string): { label: string; detail: string } {
  switch (value) {
    case "marketFees": return { label: "Market broker fees", detail: "Broker fees depend on the sale method and character skills." };
    case "salesTax": return { label: "Sales tax", detail: "Transaction tax is assessed when the output is sold." };
    case "hauling": return { label: "Logistics and transport", detail: "Movement costs vary by route and method." };
    case "installationCost": return { label: "Installation cost", detail: "Installation is unavailable and is not treated as zero." };
    case "projectedInventoryCost": return { label: "Projected inventory cost", detail: "Historical inventory cost is not included in this market estimate." };
    default: return { label: titleCase(value), detail: "This backend profitability term is not included." };
  }
}

function formatDuration(seconds: number | null): string {
  if (seconds === null) return "Unavailable";
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const remaining = seconds % 60;
  return `${hours}h ${minutes}m ${remaining}s`;
}

function titleCase(value: string): string {
  return value.replace(/([a-z])([A-Z])/g, "$1 $2").replace(/^./, (letter) => letter.toUpperCase());
}
