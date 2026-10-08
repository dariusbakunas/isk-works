import type { BuildWorksheetProjection, BuildWorksheetRow, MarketPricingPolicy, WorksheetPricingEvidence } from "../../../../api/industry";
import { EveTypeImage } from "../../../../components/eve-type-image";
import { formatIskCompact, formatIskSummary } from "../../../../components/money";
import { OperationalTable, OperationalTableGroup, OperationalTableRow } from "../../../../components/operational-table";
import { WORKSHEET_COLUMNS } from "./worksheet-columns";
import { targetForRow } from "./worksheet-selection";

const quantities = new Intl.NumberFormat("en-US");
const ignoreSelection = () => {};

export function BuildWorksheetTable({ onOpenOutput, onSelectRow, selectedRowKey = null, worksheet }: { onOpenOutput?: () => void; onSelectRow?: (row: BuildWorksheetRow) => void; selectedRowKey?: string | null; worksheet: BuildWorksheetProjection }) {
  return (
    <OperationalTable ariaLabel="Production Worksheet" columns={WORKSHEET_COLUMNS} onSelectRow={ignoreSelection} selectedRowKey={selectedRowKey}>
      {worksheet.groups.map((group) => (
        <OperationalTableGroup defaultExpanded groupKey={group.key} itemCount={group.rowCount} key={group.key} label={group.label} status={groupStatus(group.rows)}>
          {group.rows.map((row) => <WorksheetRow key={row.id} onSelectRow={onSelectRow} row={row} />)}
        </OperationalTableGroup>
      ))}
      <OperationalTableGroup groupKey="output" itemCount={1} label="Output" status="positive">
        <OperationalTableRow interactive={Boolean(onOpenOutput)} onActivate={onOpenOutput} rowKey="output" status="positive" cells={{
          item: <ItemCell status="positive" typeId={worksheet.output.typeId} typeName={worksheet.output.typeName} />,
          sourcing: <span className="text-xs text-muted">—</span>, required: <Quantity value={worksheet.output.quantity} />, covered: <Placeholder />, shortage: <Placeholder />, coverage: <Placeholder />,
          pricing: <span className="whitespace-nowrap text-xs text-muted">—</span>,
          unitCost: <Money state={worksheet.output.evidenceState} value={worksheet.output.unitValue} />,
          totalValue: <Money className="text-positive" state={worksheet.output.evidenceState} value={worksheet.output.totalValue} />,
        }} />
      </OperationalTableGroup>
    </OperationalTable>
  );
}

function WorksheetRow({ onSelectRow, row }: { onSelectRow?: (row: BuildWorksheetRow) => void; row: BuildWorksheetRow }) {
  const status = rowStatus(row);
  const cells = {
    item: <ItemCell status={status} typeId={row.typeId} typeName={row.typeName} />,
    sourcing: <span className={`whitespace-nowrap text-xs font-medium ${coveredFromInventory(row) ? "text-positive" : row.sourcing === "buy" ? "text-muted" : "text-primary"}`}>{sourcingLabel(row)}</span>,
    required: <Quantity value={row.requiredQuantity} />,
    covered: <Quantity className={(row.coveredQuantity ?? 0) > 0 ? "text-positive" : ""} value={row.coveredQuantity} />,
    shortage: row.shortageQuantity == null
      ? <Placeholder />
      : row.shortageQuantity > 0
        ? <Quantity className="text-destructive" dataShortage value={row.shortageQuantity} />
        : <span className="whitespace-nowrap text-muted" data-shortage="false">—</span>,
    coverage: <Coverage row={row} />,
    pricing: <Pricing evidence={row.pricing} />,
    unitCost: <Money state={row.pricing.state} value={row.unitCost} />,
    totalValue: <Money state={row.evidenceState} value={row.totalValue} />,
  };
  const interactive = Boolean(onSelectRow && targetForRow(row));
  return <OperationalTableRow cells={cells} interactive={interactive} onActivate={interactive ? () => onSelectRow?.(row) : undefined} rowKey={row.id} status={status} />;
}

function evidenceLabel(state: BuildWorksheetRow["evidenceState"]): string {
  return state === "unpriced" ? "Unpriced" : state === "incomplete" ? "Incomplete" : state === "notApplicable" ? "—" : "Complete";
}
function ItemCell({ status, typeId, typeName }: { status: "neutral" | "positive" | "warning" | "blocking"; typeId: number; typeName: string }) {
  return <span className="flex min-w-0 items-center gap-2" title={typeName}>
    <span aria-hidden="true" className={`h-1.5 w-1.5 shrink-0 rounded-full ${statusDot(status)}`} data-row-status-dot={status} />
    <EveTypeImage size={24} typeId={typeId} typeName={typeName} />
    <span className="min-w-0 truncate font-medium">{typeName}</span>
    <span className="sr-only">{statusLabel(status)}</span>
  </span>;
}

function Quantity({ className = "", dataShortage = false, value }: { className?: string; dataShortage?: boolean; value: number | null }) {
  if (value == null) return <Placeholder />;
  const full = quantities.format(value);
  return <span className={`whitespace-nowrap font-mono tabular-nums ${className}`} data-shortage={dataShortage ? "true" : undefined} title={full}>
    <span className="lg:hidden">{formatQuantityCompact(value)}</span>
    <span className="hidden lg:inline">{full}</span>
  </span>;
}

function Money({ className = "", state, value }: { className?: string; state: BuildWorksheetRow["evidenceState"]; value: string | null }) {
  if (value == null) return <span className={`whitespace-nowrap text-xs ${state === "notApplicable" ? "text-muted" : "text-warning"}`}>{evidenceLabel(state)}</span>;
  return <span className={`whitespace-nowrap font-mono tabular-nums ${className}`} title={formatIskSummary(value)}>{formatIskCompact(value)}</span>;
}

function Coverage({ row }: { row: BuildWorksheetRow }) {
  if (row.coveragePercentage == null) return <Placeholder />;
  const percentage = Number(row.coveragePercentage);
  const complete = row.shortageQuantity === 0;
  const tone = complete ? "text-positive" : (row.coveredQuantity ?? 0) > 0 ? "text-warning" : "text-destructive";
  return <span className={`grid w-full grid-cols-[2rem_minmax(0,1fr)] items-center gap-1.5 whitespace-nowrap font-mono ${tone}`}>
    <span aria-label={`${row.coveragePercentage}% covered`} aria-valuemax={100} aria-valuemin={0} aria-valuenow={percentage} className="h-1 w-8 shrink-0 overflow-hidden bg-border" role="progressbar">
      <span className={`block h-full ${complete ? "bg-positive" : "bg-warning"}`} style={{ width: `${percentage}%` }} />
    </span>
    <span className="whitespace-nowrap text-right">{row.coveragePercentage}%</span>
  </span>;
}

function Pricing({ evidence }: { evidence: WorksheetPricingEvidence }) {
  return <span className={`whitespace-nowrap text-xs ${evidence.state === "complete" ? "text-muted" : "text-warning"}`} title={evidence.sourceNote ?? undefined}>{pricingLabel(evidence)}</span>;
}

function pricingLabel(evidence: WorksheetPricingEvidence): string {
  switch (evidence.classification) {
    case "production": return "Production";
    case "default": return "Default";
    case "manual": return "Manual";
    case "marketPolicy": return evidence.policy ? policyLabel(evidence.policy) : "Market policy";
    case "mixed": return "Mixed";
    case "unresolved": return evidence.state === "unpriced" ? "Unpriced" : "Unresolved";
  }
}

function policyLabel(policy: MarketPricingPolicy): string {
  return {
    highestBuy: "Highest buy",
    lowestSell: "Lowest sell",
    acquireQuantityFromSellOrders: "Buy from sells",
    liquidateQuantityIntoBuyOrders: "Sell into buys",
  }[policy];
}

// A Buy row whose whole requirement comes out of inventory isn't bought at
// all -- say so, like the graph's "Inventory" nodes. Partly covered rows stay
// "Buy" (the remainder is bought; the coverage bar shows how much).
function coveredFromInventory(row: BuildWorksheetRow): boolean {
  return (
    row.sourcing === "buy" &&
    (row.requiredQuantity ?? 0) > 0 &&
    (row.shortageQuantity ?? 0) === 0 &&
    (row.coveredQuantity ?? 0) >= (row.requiredQuantity ?? 0)
  );
}

function sourcingLabel(row: BuildWorksheetRow): string {
  if (coveredFromInventory(row)) return "Inventory";
  return row.sourcing === "manufacturing" ? "Build" : row.sourcing === "reaction" ? "Reaction" : row.sourcing === "buy" ? "Buy" : "Unresolved";
}

function rowStatus(row: BuildWorksheetRow): "positive" | "warning" | "blocking" {
  if ((row.shortageQuantity ?? 0) > 0) return (row.coveredQuantity ?? 0) > 0 ? "warning" : "blocking";
  if (row.evidenceState !== "complete" || row.pricing.state !== "complete") return "warning";
  return "positive";
}

function groupStatus(rows: BuildWorksheetRow[]): "positive" | "warning" | "blocking" {
  const statuses = rows.map(rowStatus);
  if (statuses.includes("blocking")) return "blocking";
  if (statuses.includes("warning")) return "warning";
  return "positive";
}

function Placeholder() { return <span className="whitespace-nowrap text-muted">—</span>; }

function formatQuantityCompact(value: number): string {
  if (Math.abs(value) < 10_000) return quantities.format(value);
  return new Intl.NumberFormat("en-US", { notation: "compact", maximumFractionDigits: 1 }).format(value);
}

function statusDot(status: "neutral" | "positive" | "warning" | "blocking") {
  return status === "positive" ? "bg-positive" : status === "warning" ? "bg-warning" : status === "blocking" ? "bg-destructive" : "bg-muted";
}

function statusLabel(status: "neutral" | "positive" | "warning" | "blocking") {
  return status === "positive" ? "Covered" : status === "warning" ? "Partially covered or incomplete" : status === "blocking" ? "Missing" : "Status unavailable";
}
