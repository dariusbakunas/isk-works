import type { LocationTotal } from "../../../api/finance-analytics";
import { MoneyAmount } from "../../../components/money";
import { Private } from "../../../observability/private";
import { numeric } from "./analytics-format";
import { NoData } from "./category-donuts";

const GRID = "grid grid-cols-[minmax(9rem,1fr)_6.5rem_6.5rem_6.5rem_2.75rem_4rem] items-center gap-2";

export function LocationsTable({ rows, onOpen }: { rows: LocationTotal[]; onOpen: (row: LocationTotal) => void }) {
  if (rows.length === 0) return <NoData>No location activity in this period.</NoData>;
  const volume = (row: LocationTotal) => numeric(row.income) + numeric(row.expenses);
  const maxVolume = Math.max(...rows.map(volume), 1);
  return (
    <div className="overflow-x-auto">
      <div className="min-w-[38rem]">
        <div className={`${GRID} border-b border-border pb-1 text-[0.625rem] font-semibold uppercase tracking-wider text-muted`}>
          <span>Location</span>
          <span className="text-right">Income (ISK)</span>
          <span className="text-right">Expenses (ISK)</span>
          <span className="text-right">Net (ISK)</span>
          <span className="text-right">Txns</span>
          <span>Vol</span>
        </div>
        <ul>
          {rows.map((row) => {
            const net = numeric(row.net);
            return (
              <li className={`${GRID} border-b border-border/50 py-1.5 last:border-0`} key={row.locationId}>
                <div className="min-w-0">
                  <button
                    className="block max-w-full truncate text-left text-[0.6875rem] text-foreground underline-offset-2 hover:text-primary hover:underline"
                    onClick={() => onOpen(row)}
                    title={`View transactions at ${row.locationName}`}
                    type="button"
                  >
                    <Private>{row.locationName}</Private>
                  </button>
                  {row.regionName ? <div className="truncate text-[0.5625rem] text-muted">{row.regionName}</div> : null}
                </div>
                <Private className="text-right font-mono text-[0.6875rem] tabular-nums text-income"><MoneyAmount maximumSummaryFractionDigits={0} showCurrency={false} value={row.income} /></Private>
                <Private className="text-right font-mono text-[0.6875rem] tabular-nums text-expense"><MoneyAmount maximumSummaryFractionDigits={0} showCurrency={false} value={row.expenses} /></Private>
                <Private className={`text-right font-mono text-[0.6875rem] tabular-nums ${net >= 0 ? "text-income" : "text-expense"}`}>
                  <MoneyAmount maximumSummaryFractionDigits={0} showCurrency={false} signDisplay="always" value={row.net} />
                </Private>
                <span className="text-right font-mono text-[0.6875rem] tabular-nums text-muted">{row.transactionCount.toLocaleString()}</span>
                <span aria-hidden="true" className="block h-2 rounded-[1px] bg-net/50" style={{ width: `${(volume(row) / maxVolume) * 100}%` }} />
              </li>
            );
          })}
        </ul>
      </div>
    </div>
  );
}
