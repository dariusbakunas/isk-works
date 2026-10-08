import type { TopItem } from "../../../api/finance-analytics";
import { MoneyAmount } from "../../../components/money";
import { Private } from "../../../observability/private";
import { numeric } from "./analytics-format";
import { NoData } from "./category-donuts";
import { Sparkline } from "./sparkline";

const GRID = "grid grid-cols-[minmax(8rem,1fr)_6.5rem_4.5rem_5.5rem_6.5rem_4rem_3.5rem] items-center gap-2";

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
    <div className="overflow-x-auto">
      <div className="min-w-[42rem]">
        <div className={`${GRID} border-b border-border pb-1 text-[0.625rem] font-semibold uppercase tracking-wider text-muted`}>
          <span>Item</span>
          <span>Category</span>
          <span className="text-right">Qty</span>
          <span className="text-right">Avg price (ISK)</span>
          <span className="text-right">Total (ISK)</span>
          <span className="text-right">{kind === "expense" ? "% of spend" : "% of income"}</span>
          <span className="text-right">Trend</span>
        </div>
        <ul>
          {rows.map((row) => {
            const color = colors.get(row.category) ?? "var(--color-faint)";
            return (
              <li className={`${GRID} border-b border-border/50 py-1.5 last:border-0`} key={row.typeId}>
                <button
                  className="block max-w-full truncate text-left text-[0.6875rem] text-foreground underline-offset-2 hover:text-primary hover:underline"
                  onClick={() => onOpen(row)}
                  title={`View transactions for ${row.typeName}`}
                  type="button"
                >
                  {row.typeName}
                </button>
                <span
                  className="w-fit max-w-full truncate rounded-[2px] border px-1.5 py-px text-[0.5625rem]"
                  style={{ background: `color-mix(in srgb, ${color} 12%, transparent)`, borderColor: `color-mix(in srgb, ${color} 30%, transparent)`, color }}
                >
                  {row.category}
                </span>
                <span className="text-right font-mono text-[0.6875rem] tabular-nums text-muted">{row.quantity.toLocaleString()}</span>
                <Private className="text-right font-mono text-[0.6875rem] tabular-nums text-muted" title="ISK per unit">
                  <MoneyAmount maximumSummaryFractionDigits={2} showCurrency={false} value={row.averageUnitPrice} />
                </Private>
                <Private className={`text-right font-mono text-[0.6875rem] font-bold tabular-nums ${tone}`}>
                  <MoneyAmount maximumSummaryFractionDigits={0} showCurrency={false} value={row.total} />
                </Private>
                <span className="text-right font-mono text-[0.6875rem] tabular-nums text-muted">
                  {row.sharePercent === null ? "—" : `${row.sharePercent.toFixed(1)}%`}
                </span>
                <span className="flex justify-end">
                  <Sparkline color={sparkColor} height={18} values={row.trend.map(numeric)} width={48} />
                </span>
              </li>
            );
          })}
        </ul>
      </div>
    </div>
  );
}
