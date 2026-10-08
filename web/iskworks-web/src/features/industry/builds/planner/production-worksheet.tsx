import type { ReactNode } from "react";
import { AlertTriangle, CheckCircle2 } from "lucide-react";

import type { MarketPricingPolicy, ProductionWorksheet as Worksheet, WorksheetItem } from "../../../../api/industry";
import {
  OperationalTable,
  OperationalTableGroup,
  OperationalTableRow,
} from "../../../../components/operational-table";
import { EveTypeImage } from "../../../../components/eve-type-image";
import { formatIskCompact, formatIskSummary } from "../../../../components/money";
import { effectiveUnitCost } from "../../shared/decimal";
import { PRODUCTION_WORKSHEET_COLUMNS } from "./worksheet-columns";

export function ProductionWorksheet({
  selectedRowKey,
  worksheet,
  onSelectRow,
}: {
  selectedRowKey: string | null;
  worksheet: Worksheet;
  onSelectRow: (key: string) => void;
}) {
  return (
    <section aria-labelledby="production-worksheet-title" className="@container min-w-0">
      <div className="flex min-h-8 items-center justify-between gap-2 border-b border-border px-2">
        <div className="flex items-baseline gap-2">
          <h2 className="text-sm font-semibold" id="production-worksheet-title">Production Worksheet</h2>
          <span className="text-xs text-muted">Build requirements</span>
        </div>
        <span className="inline-flex items-center gap-1.5 text-xs text-muted">
          {worksheet.summary.quantityCoverageComplete
            ? <CheckCircle2 aria-hidden="true" className="h-3.5 w-3.5 text-positive" />
            : <AlertTriangle aria-hidden="true" className="h-3.5 w-3.5 text-warning" />}
          {worksheet.summary.quantityCoverageComplete ? "Inventory covered" : "Materials missing"}
        </span>
      </div>
      <OperationalTable
        ariaLabel="Production worksheet"
        columns={PRODUCTION_WORKSHEET_COLUMNS}
        onSelectRow={onSelectRow}
        selectedRowKey={selectedRowKey}
      >
        {worksheet.groups.map((group) => (
          <OperationalTableGroup
            groupKey={group.key}
            itemCount={group.items.length}
            key={group.key}
            label={group.label}
            status={groupStatus(group.items)}
          >
            {group.items.map((item) => <WorksheetRow item={item} key={rowKey(item)} />)}
          </OperationalTableGroup>
        ))}
        <OperationalTableGroup
          groupKey={worksheet.output.key}
          itemCount={worksheet.output.items.length}
          label={worksheet.output.label}
          status={groupStatus(worksheet.output.items)}
        >
          {worksheet.output.items.map((item) => <WorksheetRow item={item} key={rowKey(item)} />)}
        </OperationalTableGroup>
      </OperationalTable>
    </section>
  );
}

function WorksheetRow({ item }: { item: WorksheetItem }) {
  const status = item.pricing.missing || item.missingQuantity > 0
    ? item.coveredQuantity > 0 ? "warning" : "blocking"
    : "positive";
  const coverageTone = item.missingQuantity === 0 ? "text-positive" : item.coveredQuantity > 0 ? "text-warning" : "text-destructive";
  const cells: Record<string, ReactNode> = {
    item: (
      <span className="flex min-w-0 items-center gap-2">
        <span className={`h-1.5 w-1.5 shrink-0 rounded-full ${statusDot(status)}`} aria-hidden="true" />
        <EveTypeImage size={24} typeId={item.typeId} typeName={item.typeName} />
        <span className="min-w-0 truncate font-medium">{item.typeName}</span>
        <span className="sr-only">{statusLabel(status)}</span>
      </span>
    ),
    sourcing: item.role === "material"
      ? (
        <span
          className={`text-xs font-medium ${item.isBuildResolved ? "text-primary" : "text-muted"}`}
          title={sourcingSentence(item) ?? undefined}
        >
          {sourcingLabel(item)}
        </span>
      )
      : <span className="text-xs text-muted">—</span>,
    required: <Quantity value={item.requiredQuantity} />,
    available: <Quantity value={item.availableQuantity} />,
    covered: <Quantity className={item.coveredQuantity > 0 ? "text-positive" : ""} value={item.coveredQuantity} />,
    shortage: item.missingQuantity > 0
      ? <Quantity className="text-destructive" value={item.missingQuantity} />
      : <span className="text-muted">—</span>,
    coverage: item.role === "output"
      ? <span className="text-muted">—</span>
      : (
        <span className={`grid w-full grid-cols-[2rem_minmax(0,1fr)] items-center gap-1.5 whitespace-nowrap font-mono ${coverageTone}`}>
          <span className="h-1 w-8 shrink-0 overflow-hidden bg-border">
            <span
              className={`block h-full ${item.missingQuantity === 0 ? "bg-positive" : "bg-warning"}`}
              style={{ width: `${item.coveragePercentage}%` }}
            />
          </span>
          <span className="text-right">{item.coveragePercentage}%</span>
        </span>
      ),
    pricing: <span className={`text-xs ${item.pricing.missing ? "text-warning" : "text-muted"}`}>{pricingLabel(item)}</span>,
    unitPrice: <UnitCostCell item={item} />,
    totalValue: item.lineTotal
      ? <Money className={item.role === "output" ? "text-positive" : ""} value={item.lineTotal} />
      : <span className="text-xs text-muted">Incomplete</span>,
  };
  return <OperationalTableRow cells={cells} rowKey={rowKey(item)} status={status} />;
}

function Quantity({ className = "", value }: { className?: string; value: number }) {
  const fullValue = value.toLocaleString();
  return (
    <span className={`whitespace-nowrap font-mono tabular-nums ${className}`} title={fullValue}>
      <span className="lg:hidden">{formatQuantityCompact(value)}</span>
      <span className="hidden lg:inline">{fullValue}</span>
    </span>
  );
}

function Money({ className = "", title, value }: { className?: string; title?: string; value: string }) {
  return (
    <span className={`whitespace-nowrap font-mono tabular-nums ${className}`} title={title ?? `${Number(value).toLocaleString()} ISK`}>
      {formatIskCompact(value)}
    </span>
  );
}

/**
 * The "Unit Cost" cell. `item.pricing.unitPrice` is the market-buy unit
 * price for a Buy row -- for a Build/Reaction-resolved row it is `None`
 * *by design* (a self-produced component has no per-unit market price;
 * see `PlannedMaterialLine.unit_price`'s doc comment). `unitPrice` being
 * absent must never be read as "pricing is missing" -- `item.pricing.missing`
 * is the authoritative completeness signal for that, entirely independent
 * of whether a market unit price exists at all.
 */
function UnitCostCell({ item }: { item: WorksheetItem }) {
  if (item.role === "output") {
    // Sale-side, not cost-side -- the one row where this column still means
    // "market unit price", here for the *output* rather than an input.
    const outputPrice = item.pricing.unitPrice;
    return outputPrice
      ? <Money title={`${formatIskSummary(outputPrice)} — expected sale unit price`} value={outputPrice} />
      : <span className="text-xs text-warning">Missing price</span>;
  }
  if (!item.isBuildResolved) {
    return item.pricing.unitPrice
      ? <Money value={item.pricing.unitPrice} />
      : <span className="text-xs text-warning">Missing price</span>;
  }
  if (item.pricing.missing) {
    // Distinct wording from the Buy case: what's actually missing here is
    // never a market price (this row was never priced from the market) --
    // it's some piece of the child's own cost evidence (inventory basis,
    // adjusted price, system cost index, facility, or an unresolved child).
    return <span className="text-xs text-warning">Missing cost</span>;
  }
  const cost = item.lineTotal ? effectiveUnitCost(item.lineTotal, item.requiredQuantity) : null;
  if (!cost) return <span className="text-muted">—</span>;
  return <Money title={buildCostEvidenceSentence(item, cost) ?? undefined} value={cost} />;
}

/**
 * Secondary evidence for a Build/Reaction row's Unit Cost, as a plain
 * `title` tooltip -- the same lightweight pattern `sourcingSentence` already
 * uses for the Sourcing cell, not a new tooltip component. Makes clear the
 * displayed number is this *row's* blended cost (inventory + consumed child
 * production), while the child's own production cost per unit -- a
 * different, smaller-scoped number when there's surplus or partial
 * inventory -- is named explicitly so the two are never confused.
 */
function buildCostEvidenceSentence(item: WorksheetItem, effectiveCost: string): string | undefined {
  const evidence = item.planningEvidence;
  const base = `${formatIskSummary(effectiveCost)}/unit — this row's blended cost.`;
  if (!evidence) return base;
  const parts = [base];
  if (evidence.childUnitProductionCost) {
    parts.push(`Production cost per unit: ${formatIskSummary(evidence.childUnitProductionCost)}.`);
  }
  parts.push(
    `Produced ${evidence.childProducedQuantity.toLocaleString()}, used by this Build ${evidence.childConsumedQuantity.toLocaleString()}.`,
  );
  if (evidence.childSurplusQuantity > 0 && evidence.childSurplusRetainedBasis) {
    parts.push(
      `Surplus ${evidence.childSurplusQuantity.toLocaleString()} retained at ${formatIskSummary(evidence.childSurplusRetainedBasis)}.`,
    );
  }
  return parts.join(" ");
}

function sourcingLabel(item: WorksheetItem): string {
  const base = item.isBuildResolved ? "Build" : "Buy";
  // `reusedQuantity` of 0 or absent both mean "nothing reused" -- showing
  // "Missing (5,625)" when that equals the full requirement anyway is
  // redundant noise, not new information.
  if (!item.reusedQuantity) return base;
  const missing = item.requiredQuantity - item.reusedQuantity;
  if (missing === 0) return "Use Inventory";
  return `${base} · Missing (${formatQuantityCompact(missing)})`;
}

function sourcingSentence(item: WorksheetItem): string | null {
  if (!item.reusedQuantity) return null;
  const missing = item.requiredQuantity - item.reusedQuantity;
  const reused = item.reusedQuantity.toLocaleString();
  if (missing === 0) return `Use ${reused} from inventory.`;
  const verb = item.isBuildResolved ? "build" : "buy";
  return `Use ${reused} from inventory and ${verb} ${missing.toLocaleString()}.`;
}

function pricingLabel(item: WorksheetItem) {
  // A self-produced Build/Reaction row was never priced by any market
  // policy -- its own cost basis is production, whatever policy its
  // *descendants'* Buy requirements might use. `selectionKind`/
  // `effectivePolicy` here are leftover recipe-capture metadata, not what
  // actually priced this row (see the InvalidRecipe/Worksheet-presentation
  // investigations).
  if (item.isBuildResolved) return "Production";
  if (item.pricing.selectionKind === "manual") return "Manual";
  if (item.pricing.selectionKind === "market_policy") {
    return policyLabel(item.pricing.effectivePolicy);
  }
  return "Default";
}

function formatQuantityCompact(value: number): string {
  if (Math.abs(value) < 10_000) return value.toLocaleString();
  return new Intl.NumberFormat("en-US", {
    notation: "compact",
    maximumFractionDigits: 1,
  }).format(value);
}

function policyLabel(policy: MarketPricingPolicy | null) {
  return {
    highestBuy: "Highest buy",
    lowestSell: "Lowest sell",
    acquireQuantityFromSellOrders: "Buy from sells",
    liquidateQuantityIntoBuyOrders: "Sell into buys",
  }[policy ?? "highestBuy"];
}

function groupStatus(items: WorksheetItem[]) {
  if (items.some((item) => item.missingQuantity > 0 || item.pricing.missing)) return "warning" as const;
  return "positive" as const;
}

function statusDot(status: "positive" | "warning" | "blocking") {
  return status === "positive" ? "bg-positive" : status === "warning" ? "bg-warning" : "bg-destructive";
}

function statusLabel(status: "positive" | "warning" | "blocking") {
  return status === "positive" ? "Covered" : status === "warning" ? "Partially covered" : "Missing";
}

export function rowKey(item: WorksheetItem): string {
  return `${item.role}:${item.typeId}`;
}
