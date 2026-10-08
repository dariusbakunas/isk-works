import type { CharacterTotal } from "../../../api/finance-analytics";
import { formatIskAbbreviated, formatIskSummary } from "../../../components/money";
import { CharacterName, Private } from "../../../observability/private";
import { characterColor } from "./analytics-header";
import { numeric } from "./analytics-format";
import { NoData } from "./category-donuts";

/** Income and expense bars per character, both scaled to the same maximum. */
export function CharacterBars({ rows, roster }: { rows: CharacterTotal[]; roster: string[] }) {
  if (rows.length === 0) return <NoData>No character activity in this period.</NoData>;
  const max = Math.max(...rows.flatMap((row) => [numeric(row.income), numeric(row.expenses)]), 1);
  return (
    <div>
      <ul className="space-y-2">
        {rows.map((row) => {
          const net = numeric(row.net);
          const color = characterColor(Math.max(roster.indexOf(row.connectionId), 0));
          return (
            <li className="flex items-center gap-2.5" key={row.connectionId}>
              <span
                aria-hidden="true"
                className="grid h-6 w-6 shrink-0 place-items-center rounded-full border text-[0.625rem] font-bold"
                style={{ background: `color-mix(in srgb, ${color} 20%, transparent)`, borderColor: `color-mix(in srgb, ${color} 45%, transparent)`, color }}
              >
                {row.characterName.charAt(0)}
              </span>
              <CharacterName className="w-28 shrink-0 truncate text-[0.6875rem] text-foreground" name={row.characterName} />
              <div className="min-w-0 flex-1 space-y-0.5" role="img" aria-label={`${row.characterName}: income ${formatIskAbbreviated(row.income, { currency: true })}, expenses ${formatIskAbbreviated(row.expenses, { currency: true })}`}>
                <div className="h-2 rounded-[1px] bg-income/80" style={{ width: `${(numeric(row.income) / max) * 100}%` }} />
                <div className="h-2 rounded-[1px] bg-expense/80" style={{ width: `${(numeric(row.expenses) / max) * 100}%` }} />
              </div>
              <Private
                className={`w-16 shrink-0 text-right font-mono text-[0.6875rem] tabular-nums ${net >= 0 ? "text-income" : "text-expense"}`}
                title={formatIskSummary(row.net, { signDisplay: "always" })}
              >
                {formatIskAbbreviated(row.net, { signDisplay: "always" })}
              </Private>
            </li>
          );
        })}
      </ul>
      <div className="mt-3 flex items-center gap-4 text-[0.625rem] text-muted">
        <span className="flex items-center gap-1.5"><span aria-hidden="true" className="h-2 w-2 rounded-[1px] bg-income" />Income</span>
        <span className="flex items-center gap-1.5"><span aria-hidden="true" className="h-2 w-2 rounded-[1px] bg-expense" />Expenses</span>
      </div>
    </div>
  );
}
