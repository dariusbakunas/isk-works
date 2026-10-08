import { ArrowDown, ArrowUp, Minus } from "lucide-react";
import { Cell, Pie, PieChart, Tooltip } from "recharts";

import type { CategoryTotal } from "../../../api/finance-analytics";
import { formatIskAbbreviated, formatIskSummary } from "../../../components/money";
import { Private } from "../../../observability/private";
import { categoryDelta, deltaPresentation, numeric, shareOfTotal } from "./analytics-format";

const SIZE = 160;

export function NoData({ children }: { children: string }) {
  return <p className="py-8 text-center text-[0.6875rem] text-muted">{children}</p>;
}

function DonutTooltip({ active, payload, total }: { active?: boolean; payload?: Array<{ payload: CategoryTotal }>; total: string }) {
  const row = active ? payload?.[0]?.payload : undefined;
  if (!row) return null;
  const share = shareOfTotal(row.total, total);
  return (
    <Private as="div" className="iw-panel-strong border px-2.5 py-2 text-[0.6875rem] shadow-2xl">
      <div className="font-semibold text-foreground">{row.category}</div>
      <div className="font-mono tabular-nums">{formatIskSummary(row.total)}</div>
      {share !== null ? <div className="text-muted">{share.toFixed(1)}% of total</div> : null}
    </Private>
  );
}

/**
 * One donut plus its legend. Clicking a slice or a legend row filters the whole
 * page by that category; the others dim rather than disappear. "Other" is the
 * folded tail of small categories, so it cannot be a filter.
 */
export function DonutSection({
  rows,
  total,
  side,
  colors,
  selected,
  onSelect,
}: {
  rows: CategoryTotal[];
  total: string;
  side: "spending" | "income";
  colors: Map<string, string>;
  selected: string | null;
  onSelect: (category: string | null) => void;
}) {
  if (rows.length === 0) {
    return <NoData>{side === "spending" ? "No spending in this period." : "No income in this period."}</NoData>;
  }
  const chartRows = rows.map((row) => ({ ...row, value: numeric(row.total) }));
  const canSelect = (category: string) => category !== "Other";
  const toggle = (category: string) => canSelect(category) && onSelect(selected === category ? null : category);
  const opacity = (category: string) => (selected === null || selected === category ? 1 : 0.2);

  return (
    <div className="flex flex-wrap items-center gap-4">
      <div className="relative shrink-0" style={{ width: SIZE, height: SIZE }}>
        <PieChart height={SIZE} width={SIZE}>
          <Pie
            data={chartRows}
            dataKey="value"
            endAngle={-270}
            innerRadius={50}
            isAnimationActive={false}
            nameKey="category"
            outerRadius={76}
            startAngle={90}
            stroke="var(--color-panel)"
            strokeWidth={1}
          >
            {chartRows.map((row) => (
              <Cell
                cursor={canSelect(row.category) ? "pointer" : "default"}
                fill={colors.get(row.category) ?? "var(--color-faint)"}
                fillOpacity={opacity(row.category) * 0.9}
                key={row.category}
                onClick={() => toggle(row.category)}
              />
            ))}
          </Pie>
          <Tooltip content={<DonutTooltip total={total} />} isAnimationActive={false} />
        </PieChart>
        <div className="pointer-events-none absolute inset-0 grid place-content-center text-center">
          <Private className="font-mono text-sm font-bold tabular-nums text-foreground">{formatIskAbbreviated(total)}</Private>
          <div className="text-[0.5625rem] text-muted">ISK</div>
        </div>
      </div>
      <ul className="min-w-[14rem] flex-1 space-y-0.5" aria-label={side === "spending" ? "Spending categories" : "Income categories"}>
        {rows.map((row) => {
          const change = deltaPresentation(categoryDelta(row.total, row.previous), { inverse: side === "spending" });
          const Arrow = change?.arrow === "up" ? ArrowUp : change?.arrow === "down" ? ArrowDown : Minus;
          const tone = change?.tone === "good" ? "text-income" : change?.tone === "bad" ? "text-expense" : "text-muted";
          return (
            <li key={row.category}>
              <button
                aria-pressed={selected === row.category}
                className="flex w-full items-center gap-2 rounded-[2px] px-1 py-0.5 text-left transition hover:bg-panel-strong disabled:cursor-default disabled:hover:bg-transparent"
                disabled={!canSelect(row.category)}
                onClick={() => toggle(row.category)}
                style={{ opacity: opacity(row.category) === 1 ? 1 : 0.4 }}
                type="button"
              >
                <span aria-hidden="true" className="h-2 w-2 shrink-0 rounded-[1px]" style={{ background: colors.get(row.category) }} />
                <span className="min-w-0 flex-1 truncate text-[0.6875rem] text-foreground">{row.category}</span>
                <Private className="font-mono text-[0.6875rem] tabular-nums text-foreground" title={formatIskSummary(row.total)}>
                  {formatIskAbbreviated(row.total)}
                </Private>
                <span className={`flex w-16 items-center justify-end gap-0.5 font-mono text-[0.625rem] tabular-nums ${tone}`}>
                  {change ? (
                    <>
                      <Arrow aria-hidden="true" className="h-2.5 w-2.5" />
                      {change.text}
                    </>
                  ) : null}
                </span>
              </button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
