import type { DayNet } from "../../../api/finance-analytics";
import { formatIskSummary } from "../../../components/money";
import { Private } from "../../../observability/private";
import { heatmapCellStyle, layoutHeatmap } from "./heatmap-layout";

const CELL = 13;
const GAP = 2;
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

function dayLabel(date: string): string {
  return `${MONTHS[Number(date.slice(5, 7)) - 1]} ${Number(date.slice(8, 10))}`;
}

function swatch(color: "income" | "expense", alpha: number): string {
  return `color-mix(in srgb, var(--color-${color}) ${Math.round(alpha * 100)}%, transparent)`;
}

/** Fixed square cells, one column per week, rows aligned to the real weekday. */
export function DailyNetHeatmap({ days }: { days: DayNet[] }) {
  const { weeks, monthLabels, maxAbs } = layoutHeatmap(days);
  const columns = `repeat(${weeks.length}, ${CELL}px)`;
  return (
    <div className="overflow-x-auto">
      <div className="inline-block">
        <div className="mb-1 ml-5 grid text-[0.5625rem] text-muted" style={{ gridTemplateColumns: columns, columnGap: GAP }}>
          {monthLabels.map((label, column) => (
            <span className="overflow-visible whitespace-nowrap" key={column}>{label}</span>
          ))}
        </div>
        <div className="flex gap-1">
          <div className="grid w-4 text-[0.5625rem] text-muted" style={{ gridTemplateRows: `repeat(7, ${CELL}px)`, rowGap: GAP }}>
            {["M", "", "W", "", "F", "", ""].map((label, row) => (
              <span className="leading-[13px]" key={row}>{label}</span>
            ))}
          </div>
          <Private
            as="div"
            className="grid"
            data-testid="heatmap-grid"
            style={{ gridAutoFlow: "column", gridTemplateColumns: columns, gridTemplateRows: `repeat(7, ${CELL}px)`, gap: GAP }}
          >
            {weeks.flatMap((week, column) =>
              week.map((cell, row) => {
                if (!cell) return <span aria-hidden="true" key={`${column}-${row}`} />;
                const style = heatmapCellStyle(cell.net, maxAbs);
                const text = `${dayLabel(cell.date)}: ${formatIskSummary(cell.raw, { signDisplay: "always" })}`;
                return (
                  <span
                    aria-label={text}
                    className="rounded-[2px] border border-border/60"
                    data-testid="heatmap-cell"
                    key={cell.date}
                    role="img"
                    style={{ width: CELL, height: CELL, background: style.color === "none" ? "var(--color-panel-strong)" : swatch(style.color, style.alpha) }}
                    title={text}
                  />
                );
              }),
            )}
          </Private>
        </div>
        <div className="ml-5 mt-2 flex items-center gap-1 text-[0.5625rem] text-muted">
          <span>Net −</span>
          {[0.9, 0.55, 0.25].map((alpha) => (
            <span aria-hidden="true" className="h-2.5 w-2.5 rounded-[2px]" key={`r${alpha}`} style={{ background: swatch("expense", alpha) }} />
          ))}
          <span aria-hidden="true" className="h-2.5 w-2.5 rounded-[2px] border border-border/60 bg-panel-strong" />
          {[0.25, 0.55, 0.9].map((alpha) => (
            <span aria-hidden="true" className="h-2.5 w-2.5 rounded-[2px]" key={`g${alpha}`} style={{ background: swatch("income", alpha) }} />
          ))}
          <span>Net +</span>
        </div>
      </div>
    </div>
  );
}
