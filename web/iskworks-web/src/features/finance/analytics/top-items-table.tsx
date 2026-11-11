import type { TopItem } from "../../../api/finance-analytics";
import { MoneyAmount } from "../../../components/money";
import { OperationalTable, OperationalTableRow, type OperationalColumn } from "../../../components/operational-table";
import { Private } from "../../../observability/private";
import { numeric } from "./analytics-format";
import { NoData } from "./category-donuts";
import { Sparkline } from "../../../components/sparkline";

// Sized to sit side by side in two half-width cards at 2xl (~45rem each).
function columns(kind: "expense" | "income"): OperationalColumn[] {
  return [
    { key: "item", label: "Item", width: "minmax(7.5rem,1fr)" },
    { key: "category", label: "Category", width: "6.5rem" },
    { key: "quantity", label: "Qty", width: "5rem", align: "right", numeric: true },
    { key: "averagePrice", label: "Avg price", title: "ISK per unit", width: "6.5rem", align: "right", numeric: true },
    { key: "total", label: "Total", title: "ISK", width: "6.5rem", align: "right", numeric: true },
    { key: "share", label: kind === "expense" ? "% of spend" : "% of income", width: "5.75rem", align: "right", numeric: true },
    { key: "trend", label: "Trend", width: "4rem", align: "right" },
  ];
}

// The rows don't select: the item name is the only action.
const noSelection = () => {};

/** Top items on one side. The share column is of that side's own total. */
export function TopTable({
  rows,
  kind,
  colors,
  onOpen,
}: {
  rows: TopItem[];
  kind: "expense" | "income";
  colors: Map<string, string>;
  onOpen: (row: TopItem) => void;
}) {
  if (rows.length === 0) return <NoData>{kind === "expense" ? "No purchases in this period." : "No sales in this period."}</NoData>;
  const tone = kind === "expense" ? "text-expense" : "text-income";
  const sparkColor = kind === "expense" ? "var(--color-expense)" : "var(--color-income)";
  return (
    <OperationalTable
      ariaLabel={kind === "expense" ? "Top expenses" : "Top earners"}
      columns={columns(kind)}
      onSelectRow={noSelection}
      selectedRowKey={null}
    >
      <tbody>
        {rows.map((row) => {
          const color = colors.get(row.category) ?? "var(--color-faint)";
          return (
            <OperationalTableRow
              cells={{
                // `w-0 min-w-full`: fill the column but add nothing to the
                // table's own width, so a long name truncates instead of
                // widening the flexible Item column past the card.
                item: (
                  <button
                    className="block w-0 min-w-full truncate text-left text-xs text-foreground underline-offset-2 hover:text-primary hover:underline"
                    onClick={() => onOpen(row)}
                    title={`View transactions for ${row.typeName}`}
                    type="button"
                  >
                    {row.typeName}
                  </button>
                ),
                category: (
                  <span
                    className="w-fit max-w-full truncate rounded-[2px] border px-1.5 py-px text-[0.625rem]"
                    style={{ background: `color-mix(in srgb, ${color} 12%, transparent)`, borderColor: `color-mix(in srgb, ${color} 30%, transparent)`, color }}
                    title={row.category}
                  >
                    {row.category}
                  </span>
                ),
                quantity: <span className="truncate text-xs text-muted" title={row.quantity.toLocaleString()}>{row.quantity.toLocaleString()}</span>,
                averagePrice: (
                  <Private className="truncate text-xs text-muted" title="ISK per unit">
                    <MoneyAmount maximumSummaryFractionDigits={2} showCurrency={false} value={row.averageUnitPrice} />
                  </Private>
                ),
                total: (
                  <Private className={`truncate text-xs font-bold ${tone}`}>
                    <MoneyAmount maximumSummaryFractionDigits={0} showCurrency={false} value={row.total} />
                  </Private>
                ),
                share: <span className="text-xs text-muted">{row.sharePercent === null ? "—" : `${row.sharePercent.toFixed(1)}%`}</span>,
                trend: <Sparkline color={sparkColor} height={18} values={row.trend.map(numeric)} width={48} />,
              }}
              interactive={false}
              key={row.typeId}
              rowKey={String(row.typeId)}
            />
          );
        })}
      </tbody>
    </OperationalTable>
  );
}
