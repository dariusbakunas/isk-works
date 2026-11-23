import { ClipboardCopy } from "lucide-react";
import { useState, type ReactNode } from "react";

import type { InventionOption, InventionQuote, ScienceJobFee } from "../../../api/invention";
import { IskCompact } from "../../../components/money";
import { PlannerInspectorShell } from "../../../components/planner-inspector-shell";
import { formatDuration, formatPercent } from "../../../lib/formatting";
import { formatChance, gapLabel, optionKey, optionName, shoppingLines, shoppingListText } from "./invention-format";

/** One option in full: what an attempt costs, what it yields, and what to
 * have on hand for the target runs. */
export function InventionInspector({
  onClose,
  option,
  quote,
}: {
  onClose: () => void;
  option: InventionOption;
  quote: InventionQuote;
}) {
  const [copied, setCopied] = useState(false);
  const lines = shoppingLines(quote, option);
  const multiplier = option.decryptor?.probabilityMultiplier ?? "1";

  async function copyShoppingList() {
    await navigator.clipboard?.writeText(shoppingListText(quote, option));
    setCopied(true);
  }

  return (
    <PlannerInspectorShell
      closeLabel="Close option details"
      dismissLabel="Dismiss option details"
      eyebrow={`${quote.productName} · ${quote.targetRuns.toLocaleString()} target runs`}
      onClose={onClose}
      open
      returnFocusRowKey={optionKey(option)}
      title={optionName(option)}
      width="wide"
    >
      <div className="px-3 pb-3 text-xs">
        {option.gaps.length > 0 ? (
          <ul className="mb-2 space-y-0.5 border border-warning/50 bg-warning/10 px-2 py-1.5 text-warning">
            {option.gaps.map((gap) => <li key={JSON.stringify(gap)}>{gapLabel(gap, quote)}</li>)}
          </ul>
        ) : null}

        <Section label="Attempt cost">
          {quote.materials.map((material) => (
            <Line key={material.typeId} label={`${material.typeName} × ${material.quantity}`}>
              <Money value={lineTotal(material.unitPrice, material.quantity)} />
            </Line>
          ))}
          <Line label="Decryptor">
            {option.decryptor ? <Money value={option.decryptor.unitPrice} /> : <span className="text-muted">—</span>}
          </Line>
          <FeeLine fee={quote.jobFee} label="Invention job fee" />
          <FeeLine fee={quote.copyFee} label="T1 copy, one run" />
          <Line emphasis label="Total"><Money value={option.attemptCost} /></Line>
        </Section>

        <Section label="Outcome">
          <p className="font-mono text-base font-semibold tabular-nums">{formatChance(option.chance)} chance</p>
          <p className="text-[0.6875rem] text-muted">
            {formatChance(quote.baseChance)} base × {Number(quote.skillFactor).toFixed(3)} skills × {Number(multiplier).toFixed(1)} decryptor
          </p>
          <Line label="Copy">
            ME {option.copy.materialEfficiency} · TE {option.copy.timeEfficiency} · {option.copy.runs.toLocaleString()} runs
          </Line>
          <Line label="Expected cost per copy"><Money value={option.costPerCopy} /></Line>
          <Line label="Blueprint cost per run"><Money value={option.costPerRun} /></Line>
          <Line label="Manufacturing per run"><Money value={option.manufacturingCostPerRun} /></Line>
          <Line emphasis label="Unit cost"><Money value={option.unitCost} /></Line>
          {quote.inventionDurationSeconds !== null ? (
            <Line label="Invention time">
              <span title="With Advanced Industry only. Structure and rig time bonuses are not applied.">
                {formatDuration(quote.inventionDurationSeconds)}
              </span>
            </Line>
          ) : null}
        </Section>

        <Section label={`Plan for ${quote.targetRuns.toLocaleString()} runs`}>
          <p className="font-semibold">
            {option.copiesNeeded.toLocaleString()} {option.copiesNeeded === 1 ? "copy" : "copies"} needed
          </p>
          {option.attempts ? (
            <>
              <div className="grid grid-cols-2 gap-2">
                <Tile label="Expected" title="Copies needed divided by the chance: the long-run average." value={option.attempts.expected} />
                <Tile label="90% sure" title="The fewest attempts that yield the copies nine times out of ten." value={option.attempts.confident} />
              </div>
              <div className="border border-border">
                <div className="border-b border-border px-2 py-1 text-[0.625rem] font-semibold uppercase text-muted">
                  Shopping list · 90% confidence
                </div>
                <ul>
                  {lines.map((line) => (
                    <li className="border-b border-border px-2 py-1 font-mono tabular-nums" key={line.name}>
                      {line.quantity.toLocaleString()} × {line.name}
                    </li>
                  ))}
                </ul>
                <div className="flex items-center justify-between px-2 py-1 font-semibold">
                  <span>Total</span>
                  <Money value={option.shoppingCost} />
                </div>
              </div>
              <button className="iw-button-secondary min-h-7 w-full" onClick={() => void copyShoppingList()} type="button">
                <ClipboardCopy aria-hidden="true" className="mr-1.5 inline h-3.5 w-3.5" />
                {copied ? "Copied" : "Copy shopping list"}
              </button>
            </>
          ) : (
            <p className="text-muted">Attempts can't be planned at this chance.</p>
          )}
        </Section>
      </div>
    </PlannerInspectorShell>
  );
}

function lineTotal(unitPrice: string | null, quantity: number): string | null {
  return unitPrice === null ? null : String(Number(unitPrice) * quantity);
}

function Money({ value }: { value: string | null }) {
  return value === null ? <span className="text-muted" title="Unknown">—</span> : <IskCompact value={value} />;
}

/** A job fee that opens into its lines, as the industry window lists them. */
function FeeLine({ fee, label }: { fee: ScienceJobFee; label: string }) {
  const percent = fee.systemCostIndex === null ? "unknown" : formatPercent(Number(fee.systemCostIndex) * 100, 2);
  return (
    <details className="group">
      <summary className="grid cursor-pointer list-none grid-cols-[minmax(0,1fr)_auto] gap-2">
        <span className="text-muted group-open:text-foreground">{label}</span>
        <Money value={fee.total} />
      </summary>
      <dl className="mb-1 ml-2 mt-1 space-y-0.5 border-l border-border pl-2 text-[0.6875rem]">
        <Line label="Estimated item value"><Money value={fee.estimatedItemValue} /></Line>
        <Line label="Job cost base (2%)"><Money value={fee.jobCostBase} /></Line>
        <Line label={`System cost index (${percent})`}><Money value={fee.systemIndexCost} /></Line>
        <Line label="Facility tax"><Money value={fee.facilityTax} /></Line>
        <Line label="SCC surcharge"><Money value={fee.sccSurcharge} /></Line>
        {fee.allianceSurcharge !== null && Number(fee.allianceSurcharge) > 0 ? (
          <Line label="Alliance surcharge"><Money value={fee.allianceSurcharge} /></Line>
        ) : null}
      </dl>
    </details>
  );
}

function Section({ children, label }: { children: ReactNode; label: string }) {
  return (
    <section aria-label={label} className="space-y-1.5 border-t border-border py-3 first:border-t-0 first:pt-1">
      <h3 className="text-[0.625rem] font-semibold uppercase text-muted">{label}</h3>
      {children}
    </section>
  );
}

function Line({ children, emphasis = false, label }: { children: ReactNode; emphasis?: boolean; label: string }) {
  return (
    <div className={`grid grid-cols-[minmax(0,1fr)_auto] gap-2 ${emphasis ? "border-t border-border pt-1 font-semibold" : ""}`}>
      <span className={emphasis ? "" : "text-muted"}>{label}</span>
      <span className="text-right font-mono tabular-nums">{children}</span>
    </div>
  );
}

function Tile({ label, title, value }: { label: string; title: string; value: number }) {
  return (
    <div className="border border-border px-2 py-1.5" title={title}>
      <div className="text-[0.625rem] font-semibold uppercase text-muted">{label}</div>
      <div className="font-mono text-sm font-semibold tabular-nums">{value.toLocaleString()} attempts</div>
    </div>
  );
}
