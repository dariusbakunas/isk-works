import {
  Bar,
  CartesianGrid,
  ComposedChart,
  Line,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

import type { AnalyticsRange, CashFlowBucket } from "../../../api/finance-analytics";
import { formatIskAbbreviated, formatIskSummary } from "../../../components/money";
import { Private } from "../../../observability/private";
import { bucketLabel, bucketRange, numeric } from "./analytics-format";

interface FlowRow {
  label: string;
  range: string;
  income: number;
  /** Plotted negative so expense bars hang below the zero line. */
  expenses: number;
  net: number;
  cumulative: number;
  raw: CashFlowBucket;
}

const AXIS_TICK = { fill: "var(--color-muted)", fontSize: 10, fontFamily: "var(--font-mono)" };

function toRows(buckets: CashFlowBucket[], range: AnalyticsRange): FlowRow[] {
  return buckets.map((bucket) => ({
    label: bucketLabel(bucket.start, range.granularity),
    range: bucketRange(bucket.start, range.granularity, range.dateFrom, range.dateTo),
    income: numeric(bucket.income),
    expenses: -numeric(bucket.expenses),
    net: numeric(bucket.net),
    cumulative: numeric(bucket.cumulativeNet),
    raw: bucket,
  }));
}

function TooltipRow({ label, value, color, strong = false }: { label: string; value: string; color: string; strong?: boolean }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <span className="flex items-center gap-1.5 text-muted">
        <span aria-hidden="true" className="h-2 w-2 rounded-[1px]" style={{ background: color }} />
        {label}
      </span>
      <span className={`font-mono tabular-nums ${strong ? "font-bold" : ""}`}>{value}</span>
    </div>
  );
}

function FlowTooltip({ active, payload }: { active?: boolean; payload?: Array<{ payload: FlowRow }> }) {
  const row = active ? payload?.[0]?.payload : undefined;
  if (!row) return null;
  const { raw } = row;
  return (
    <Private as="div" className="iw-panel-strong min-w-44 border px-2.5 py-2 text-[0.6875rem] shadow-2xl">
      <div className="mb-1 font-semibold uppercase tracking-wide text-foreground">{row.range}</div>
      <TooltipRow color="var(--color-income)" label="Income" value={formatIskSummary(raw.income)} />
      <TooltipRow color="var(--color-expense)" label="Expenses" value={formatIskSummary(raw.expenses)} />
      <div className="my-1 border-t border-border" />
      <TooltipRow color="var(--color-net)" label="Net" strong value={formatIskSummary(raw.net, { signDisplay: "always" })} />
      <TooltipRow color="var(--color-net)" label="Cumulative net" value={formatIskSummary(raw.cumulativeNet, { signDisplay: "always" })} />
    </Private>
  );
}

function Legend() {
  return (
    <div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-[0.625rem] text-muted">
      <span className="flex items-center gap-1.5"><span aria-hidden="true" className="h-2 w-2 rounded-[1px] bg-income" />Income</span>
      <span className="flex items-center gap-1.5"><span aria-hidden="true" className="h-2 w-2 rounded-[1px] bg-expense" />Expenses</span>
      <span className="flex items-center gap-1.5"><span aria-hidden="true" className="h-0.5 w-4 bg-net" />Net ISK</span>
      <span className="flex items-center gap-1.5"><span aria-hidden="true" className="w-4 border-t border-dashed border-net" />Cumulative net</span>
    </div>
  );
}

export function CashFlowChart({ buckets, range }: { buckets: CashFlowBucket[]; range: AnalyticsRange }) {
  const rows = toRows(buckets, range);
  const totalIncome = buckets.reduce((sum, bucket) => sum + numeric(bucket.income), 0);
  const totalExpenses = buckets.reduce((sum, bucket) => sum + numeric(bucket.expenses), 0);
  const summary = `Cash flow by ${range.granularity}: ${rows.length} periods, income ${formatIskAbbreviated(String(totalIncome), { currency: true })}, expenses ${formatIskAbbreviated(String(totalExpenses), { currency: true })}.`;
  return (
    <div>
      <div aria-label={summary} className="h-52 w-full" role="img">
        <ResponsiveContainer height="100%" width="100%">
          <ComposedChart data={rows} stackOffset="sign" margin={{ top: 8, right: 12, bottom: 0, left: 0 }}>
            <CartesianGrid stroke="var(--color-border)" strokeDasharray="3 3" vertical={false} />
            <XAxis axisLine={{ stroke: "var(--color-border)" }} dataKey="label" interval="preserveStartEnd" tick={AXIS_TICK} tickLine={false} />
            <YAxis
              axisLine={false}
              tick={AXIS_TICK}
              tickFormatter={(value: number) => formatIskAbbreviated(String(Math.abs(value)))}
              tickLine={false}
              width={52}
            />
            <ReferenceLine stroke="var(--color-border)" y={0} />
            <Tooltip content={<FlowTooltip />} cursor={{ fill: "var(--color-panel-strong)", opacity: 0.6 }} isAnimationActive={false} />
            <Bar dataKey="income" fill="var(--color-income)" fillOpacity={0.82} isAnimationActive={false} maxBarSize={30} name="Income" radius={[2, 2, 0, 0]} stackId="flow" />
            <Bar dataKey="expenses" fill="var(--color-expense)" fillOpacity={0.82} isAnimationActive={false} maxBarSize={30} name="Expenses" radius={[0, 0, 2, 2]} stackId="flow" />
            <Line dataKey="net" dot={{ r: 3, fill: "var(--color-net)", stroke: "var(--color-net)" }} isAnimationActive={false} name="Net ISK" stroke="var(--color-net)" strokeWidth={2} type="monotone" />
            <Line dataKey="cumulative" dot={false} isAnimationActive={false} name="Cumulative net" stroke="var(--color-net)" strokeDasharray="4 3" strokeOpacity={0.6} strokeWidth={1} type="monotone" />
          </ComposedChart>
        </ResponsiveContainer>
      </div>
      <Legend />
    </div>
  );
}
