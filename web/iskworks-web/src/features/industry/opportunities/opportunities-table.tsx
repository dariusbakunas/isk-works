import type { OpportunityCandidate } from "../../../api/opportunities";
import { formatIskCompact, formatIskSummary } from "../../../components/money";
import { EveTypeImage } from "../../../components/eve-type-image";
import { OperationalTable, OperationalTableRow, type OperationalColumn } from "../../../components/operational-table";
import { EvidenceQualityBadge } from "./opportunity-badges";
import { formatOpportunityPercent, valuationOf, type ValuationMode } from "./opportunity-formatters";
import type { SortDirection, SortField } from "./opportunity-sorting";
import { warningTitle } from "./opportunity-warnings";

const SORTABLE_COLUMN_FIELD: Partial<Record<string, SortField>> = {
  cost: "cost",
  profit: "profit",
  margin: "margin",
  profitPerHour: "profitPerHour",
};

function columns(
  mode: ValuationMode,
  sortField: SortField,
  sortDirection: SortDirection,
  onSort: (field: SortField) => void,
): OperationalColumn[] {
  function sortable(key: string) {
    const field = SORTABLE_COLUMN_FIELD[key];
    if (!field) return undefined;
    return { direction: sortField === field ? sortDirection : null, onToggle: () => onSort(field) };
  }
  return [
    { key: "item", label: "Item", width: "minmax(200px,1fr)", sticky: true },
    { key: "cost", label: "Cost", width: "100px", align: "right", numeric: true, sort: sortable("cost") },
    { key: "revenue", label: mode === "sellSide" ? "Revenue (Sell)" : "Revenue (Liq)", width: "110px", align: "right", numeric: true },
    { key: "profit", label: "Profit", width: "100px", align: "right", numeric: true, sort: sortable("profit") },
    { key: "margin", label: "Margin", width: "90px", align: "right", numeric: true, sort: sortable("margin") },
    { key: "profitPerHour", label: "Profit/h", width: "120px", align: "right", numeric: true, sort: sortable("profitPerHour") },
    { key: "evidence", label: "Evidence", width: "140px" },
  ];
}

export function OpportunitiesTable({
  candidates,
  creatingBuildFor,
  mode,
  onCreateBuild,
  onSelectCandidate,
  onSort,
  selectedProductTypeId,
  sortDirection,
  sortField,
}: {
  candidates: OpportunityCandidate[];
  creatingBuildFor: number | null;
  mode: ValuationMode;
  onCreateBuild: (candidate: OpportunityCandidate) => void;
  onSelectCandidate: (productTypeId: number) => void;
  onSort: (field: SortField) => void;
  selectedProductTypeId: number | null;
  sortDirection: SortDirection;
  sortField: SortField;
}) {
  return (
    <OperationalTable
      ariaLabel="Opportunities"
      columns={columns(mode, sortField, sortDirection, onSort)}
      onSelectRow={(key) => onSelectCandidate(Number(key))}
      selectedRowKey={selectedProductTypeId === null ? null : String(selectedProductTypeId)}
    >
      <tbody>
        {candidates.map((candidate) => (
          <CandidateRow
            candidate={candidate}
            creatingBuild={creatingBuildFor === candidate.productTypeId}
            isSelected={candidate.productTypeId === selectedProductTypeId}
            key={candidate.productTypeId}
            mode={mode}
            onCreateBuild={onCreateBuild}
          />
        ))}
      </tbody>
    </OperationalTable>
  );
}

function CandidateRow({
  candidate,
  creatingBuild,
  isSelected,
  mode,
  onCreateBuild,
}: {
  candidate: OpportunityCandidate;
  creatingBuild: boolean;
  isSelected: boolean;
  mode: ValuationMode;
  onCreateBuild: (candidate: OpportunityCandidate) => void;
}) {
  const valuation = valuationOf(candidate, mode);
  const warningCount = candidate.warnings.length;
  const status = candidate.completeness === "incomplete" ? ("warning" as const) : ("neutral" as const);
  const cells = {
    item: (
      <span className="flex min-w-0 items-center gap-2">
        <EveTypeImage size={24} typeId={candidate.productTypeId} typeName={candidate.productName} />
        <span className="min-w-0 truncate font-medium">{candidate.productName}</span>
        {isSelected ? (
          <button
            className="iw-button-secondary ml-auto shrink-0 px-1.5 py-0.5 text-[10px]"
            disabled={creatingBuild}
            onClick={(event) => {
              event.stopPropagation();
              onCreateBuild(candidate);
            }}
            type="button"
          >
            {creatingBuild ? "Creating..." : "Create Build"}
          </button>
        ) : null}
      </span>
    ),
    cost: <MoneyCell value={candidate.metrics.totalEstimatedManufacturingCost} />,
    revenue: <MoneyCell value={valuation.revenue} />,
    profit: <MoneyCell tone={profitTone(valuation.revenue !== null ? valuation.grossProfit : null)} value={valuation.grossProfit} />,
    margin: (
      <span className="whitespace-nowrap font-mono tabular-nums" style={{ color: marginColor(valuation.grossMarginPercent) }}>
        {formatOpportunityPercent(valuation.grossMarginPercent)}
      </span>
    ),
    profitPerHour:
      valuation.grossProfitPerManufacturingHour === null ? (
        <span className="text-xs text-warning" title="Incomplete">
          Incomplete
        </span>
      ) : (
        <span className="whitespace-nowrap font-mono font-semibold tabular-nums" style={{ color: profitTone(valuation.grossProfit) }}>
          {formatIskCompact(valuation.grossProfitPerManufacturingHour)}/h
        </span>
      ),
    evidence: (
      <span className="flex items-center gap-1.5">
        <EvidenceQualityBadge quality={candidate.quality.evidenceQuality} />
        {warningCount > 0 ? (
          <span
            className="text-xs font-semibold text-warning"
            title={candidate.warnings.map((warning) => warningTitle(warning.kind)).join(", ")}
          >
            ⚠ {warningCount}
          </span>
        ) : null}
      </span>
    ),
  };
  return <OperationalTableRow cells={cells} rowKey={String(candidate.productTypeId)} status={status} />;
}

function MoneyCell({ value, tone }: { value: string | null; tone?: string }) {
  if (value === null) {
    return (
      <span className="text-xs text-warning" title="Incomplete">
        Incomplete
      </span>
    );
  }
  return (
    <span className="whitespace-nowrap font-mono tabular-nums" style={tone ? { color: tone } : undefined} title={formatIskSummary(value)}>
      {formatIskCompact(value)}
    </span>
  );
}

function profitTone(profit: string | null): string | undefined {
  if (profit === null) return undefined;
  return Number(profit) < 0 ? "var(--color-danger)" : undefined;
}

function marginColor(marginPercent: string | null): string | undefined {
  if (marginPercent === null) return undefined;
  const value = Number(marginPercent);
  if (value < 0) return "var(--color-danger)";
  if (value > 20) return "var(--color-positive)";
  return undefined;
}
