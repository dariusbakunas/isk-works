import { Bar, BarChart, CartesianGrid, Cell, Pie, PieChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";

import {
  MATERIAL_CATEGORIES,
  type ConsumptionBucket,
  type ConsumptionGranularity,
  type InventoryConsumption,
  type MaterialCategory,
} from "../../../../api/inventory-analytics";
import { formatIskAbbreviated, formatIskSummary } from "../../../../components/money";
import { categoryColor } from "../material-colors";
import { bucketRange, shortDate } from "./consumption-format";

const AXIS_TICK = { fill: "var(--color-muted)", fontSize: 10, fontFamily: "var(--font-mono)" };

type BucketRow = { label: string; raw: ConsumptionBucket } & Partial<Record<MaterialCategory, number>>;

function toRows(buckets: ConsumptionBucket[]): BucketRow[] {
  return buckets.map((bucket) => {
    const row: BucketRow = { label: shortDate(bucket.start), raw: bucket };
    // Plotting only: charts need numbers. Totals shown as text stay exact.
    for (const value of bucket.byCategory) row[value.category] = Number(value.value);
    return row;
  });
}

function BucketTooltip({
  active,
  payload,
  granularity,
}: {
  active?: boolean;
  payload?: Array<{ payload: BucketRow }>;
  granularity: ConsumptionGranularity;
}) {
  const row = active ? payload?.[0]?.payload : undefined;
  if (!row) return null;
  const { raw } = row;
  return (
    <div className="iw-panel-strong min-w-48 border px-2.5 py-2 text-[0.6875rem] shadow-2xl">
      <div className="mb-1 font-semibold uppercase tracking-wide text-foreground">
        {bucketRange(raw.start, granularity)}
        {raw.partial ? <span className="ml-1 font-normal normal-case text-muted">(so far)</span> : null}
      </div>
      {[...raw.byCategory].reverse().map((value) => (
        <div className="flex items-center justify-between gap-4" key={value.category}>
          <span className="flex items-center gap-1.5 text-muted">
            <span aria-hidden="true" className="h-2 w-2 rounded-[1px]" style={{ background: categoryColor(value.category) }} />
            {value.category}
          </span>
          <span className="font-mono tabular-nums">{formatIskSummary(value.value)}</span>
        </div>
      ))}
      <div className="mt-1 flex justify-between gap-4 border-t border-border pt-1">
        <span className="text-muted">Total</span>
        <span className="font-mono font-bold tabular-nums">{formatIskSummary(raw.total)}</span>
      </div>
    </div>
  );
}

/** Consumption value per bucket, stacked by category; the partial bucket is lighter. */
export function ConsumptionChart({ report }: { report: InventoryConsumption }) {
  const rows = toRows(report.buckets);
  const categories = MATERIAL_CATEGORIES.filter((category) =>
    report.buckets.some((bucket) => bucket.byCategory.some((value) => value.category === category)),
  );
  const summary = `Consumption by ${report.granularity}: ${rows.length} periods, ${formatIskAbbreviated(report.kpis.consumedValue, { currency: true })} in total.`;
  const partial = report.buckets.find((bucket) => bucket.partial);
  return (
    <div>
      <div aria-label={summary} className="h-56 w-full" role="img">
        <ResponsiveContainer height="100%" width="100%">
          <BarChart data={rows} margin={{ top: 8, right: 12, bottom: 0, left: 0 }}>
            <CartesianGrid stroke="var(--color-border)" strokeDasharray="3 3" vertical={false} />
            <XAxis axisLine={{ stroke: "var(--color-border)" }} dataKey="label" interval="preserveStartEnd" tick={AXIS_TICK} tickLine={false} />
            <YAxis axisLine={false} tick={AXIS_TICK} tickFormatter={(value: number) => formatIskAbbreviated(String(value))} tickLine={false} width={52} />
            <Tooltip
              content={<BucketTooltip granularity={report.granularity} />}
              cursor={{ fill: "var(--color-panel-strong)", opacity: 0.6 }}
              isAnimationActive={false}
            />
            {categories.map((category) => (
              <Bar dataKey={category} fill={categoryColor(category)} isAnimationActive={false} key={category} maxBarSize={36} stackId="consumption">
                {rows.map((row) => <Cell fillOpacity={row.raw.partial ? 0.45 : 0.85} key={row.raw.start} />)}
              </Bar>
            ))}
          </BarChart>
        </ResponsiveContainer>
      </div>
      {partial ? (
        <p className="mt-1 text-[0.625rem] text-muted">
          Lighter bar: {bucketRange(partial.start, report.granularity)} is still in progress.
        </p>
      ) : null}
    </div>
  );
}

const DONUT = 140;

/** Share of the range's consumption by category. Clicking a slice or legend
 * row filters the whole tab; clicking the selected one clears it. */
export function CategoryDonut({
  categories,
  selected,
  onSelect,
}: {
  categories: InventoryConsumption["categories"];
  selected: MaterialCategory | null;
  onSelect: (category: MaterialCategory | null) => void;
}) {
  if (categories.length === 0) return <p className="py-8 text-center text-[0.6875rem] text-muted">No consumption in this period.</p>;
  const rows = categories.map((row) => ({ ...row, plotted: Number(row.value) }));
  const toggle = (category: MaterialCategory) => onSelect(selected === category ? null : category);
  const opacity = (category: MaterialCategory) => (selected === null || selected === category ? 1 : 0.25);
  return (
    <div className="flex flex-col items-center gap-2">
      <PieChart height={DONUT} width={DONUT}>
        <Pie
          data={rows}
          dataKey="plotted"
          innerRadius={36}
          isAnimationActive={false}
          nameKey="category"
          outerRadius={62}
          paddingAngle={1}
          stroke="none"
        >
          {rows.map((row) => (
            <Cell
              className="cursor-pointer"
              fill={categoryColor(row.category)}
              fillOpacity={opacity(row.category)}
              key={row.category}
              onClick={() => toggle(row.category)}
            />
          ))}
        </Pie>
      </PieChart>
      <ul aria-label="Consumption by category" className="w-full space-y-0.5">
        {rows.map((row) => (
          <li key={row.category}>
            <button
              aria-pressed={selected === row.category}
              className="flex w-full items-center gap-1.5 rounded-[2px] px-1 py-0.5 text-left text-[0.6875rem] hover:bg-panel-strong"
              onClick={() => toggle(row.category)}
              style={{ opacity: opacity(row.category) }}
              title={formatIskSummary(row.value)}
              type="button"
            >
              <span aria-hidden="true" className="h-2 w-2 shrink-0 rounded-[1px]" style={{ background: categoryColor(row.category) }} />
              <span className="flex-1 truncate text-muted">{row.category}</span>
              <span className="font-mono tabular-nums">{row.sharePercent.toFixed(1)}%</span>
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
