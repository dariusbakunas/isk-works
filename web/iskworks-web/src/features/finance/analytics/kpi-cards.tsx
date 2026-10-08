import { ArrowDown, ArrowUp, Minus } from "lucide-react";

import type { AnalyticsDelta, AnalyticsKpis, FeesKpi } from "../../../api/finance-analytics";
import { formatIskAbbreviated, formatIskSummary } from "../../../components/money";
import { Private } from "../../../observability/private";
import { deltaPresentation, numeric, shortDate, type DeltaPresentation, type DeltaTone } from "./analytics-format";
import { Sparkline } from "./sparkline";

const TONE_CLASS: Record<DeltaTone, string> = {
  good: "text-income",
  bad: "text-expense",
  neutral: "text-muted",
};

function DeltaLine({ presentation }: { presentation: DeltaPresentation | null }) {
  if (!presentation) return <div className="h-4 text-[0.625rem] text-muted"> </div>;
  const Arrow = presentation.arrow === "up" ? ArrowUp : presentation.arrow === "down" ? ArrowDown : Minus;
  return (
    <div className={`flex h-4 items-center gap-1 whitespace-nowrap text-[0.625rem] ${TONE_CLASS[presentation.tone]}`}>
      <Arrow aria-hidden="true" className="h-3 w-3" />
      <span className="font-mono tabular-nums">{presentation.text}</span>
      <span className="text-muted">vs prev</span>
    </div>
  );
}

function KpiCard({
  label,
  value,
  title,
  valueClass,
  delta,
  sparkline,
  sparklineColor,
  detail,
}: {
  label: string;
  value: string;
  title?: string;
  valueClass: string;
  delta: DeltaPresentation | null;
  sparkline: number[];
  sparklineColor: string;
  detail?: string;
}) {
  return (
    <div className="iw-panel min-w-0 px-3 py-2" data-testid="kpi-card">
      <div className="text-[0.625rem] font-semibold uppercase tracking-wider text-muted">{label}</div>
      <Private className={`mt-0.5 block whitespace-nowrap font-mono text-lg font-bold tabular-nums `} title={title}>
        {value}
      </Private>
      <div className="mt-1 flex items-end justify-between gap-2">
        {detail ? <div className="h-4 text-[0.625rem] text-muted">{detail}</div> : <DeltaLine presentation={delta} />}
        <Sparkline className="hidden sm:block" color={sparklineColor} height={20} values={sparkline} width={56} />
      </div>
    </div>
  );
}

function moneyDelta(delta: AnalyticsDelta | null, inverse = false) {
  return deltaPresentation(delta, { inverse });
}

/** Percentage-point change of the margin: a relative change of a percentage misleads. */
function marginDelta(current: number | null, previous: number | null): DeltaPresentation | null {
  if (current === null || previous === null) return null;
  const points = Math.round((current - previous) * 10) / 10;
  if (points === 0) return { text: "0.0 pp", arrow: "flat", tone: "neutral" };
  return {
    text: `${points > 0 ? "+" : ""}${points.toFixed(1)} pp`,
    arrow: points > 0 ? "up" : "down",
    tone: points > 0 ? "good" : "bad",
  };
}

function feeTooltip(fees: FeesKpi): string {
  const line = (label: string, value: string) => `${label} ${formatIskSummary(value)}`;
  return [
    line("Total", fees.value),
    line("Broker fees", fees.brokersFee),
    line("Sales tax", fees.transactionTax),
    line("Structure market fees", fees.marketProviderTax),
  ].join(" · ");
}

export function KpiCards({ kpis, rangeFrom }: { kpis: AnalyticsKpis; rangeFrom: string }) {
  const money = (value: string) => formatIskAbbreviated(value, { currency: true });
  const netNegative = numeric(kpis.net.value) < 0;
  const wallet = kpis.walletBalance;
  const margin = kpis.margin;
  const fees = kpis.fees;
  return (
    <div className={`grid grid-cols-2 gap-2 `} data-testid="kpi-cards">
      <KpiCard
        delta={moneyDelta(kpis.income.delta)}
        label="Income"
        sparkline={kpis.income.sparkline.map(numeric)}
        sparklineColor="var(--color-income)"
        title={formatIskSummary(kpis.income.value)}
        value={money(kpis.income.value)}
        valueClass="text-income"
      />
      <KpiCard
        delta={moneyDelta(kpis.expenses.delta, true)}
        label="Expenses"
        sparkline={kpis.expenses.sparkline.map(numeric)}
        sparklineColor="var(--color-expense)"
        title={formatIskSummary(kpis.expenses.value)}
        value={money(kpis.expenses.value)}
        valueClass="text-expense"
      />
      <KpiCard
        delta={moneyDelta(kpis.net.delta)}
        label="Net ISK"
        sparkline={kpis.net.sparkline.map(numeric)}
        sparklineColor="var(--color-net)"
        title={formatIskSummary(kpis.net.value, { signDisplay: "always" })}
        value={formatIskAbbreviated(kpis.net.value, { currency: true, signDisplay: "always" })}
        valueClass={netNegative ? "text-expense" : "text-net"}
      />
      <KpiCard
        delta={marginDelta(margin.percent, margin.previousPercent)}
        label="Profit margin"
        sparkline={margin.sparkline}
        sparklineColor="var(--color-net)"
        value={margin.percent === null ? "—" : `${margin.percent.toFixed(1)}%`}
        valueClass="text-foreground"
      />
      {fees ? (
        <KpiCard
          delta={moneyDelta(fees.delta, true)}
          detail={fees.delta === null && fees.availableFrom && fees.availableFrom > rangeFrom ? `Journal from ${shortDate(fees.availableFrom)}` : undefined}
          label="Taxes & fees"
          sparkline={fees.sparkline.map(numeric)}
          sparklineColor="var(--color-muted)"
          title={feeTooltip(fees)}
          value={money(fees.value)}
          valueClass="text-foreground"
        />
      ) : null}
      <KpiCard
        delta={moneyDelta(wallet.delta)}
        detail={wallet.value === null ? "No wallet snapshot yet" : wallet.delta === null ? "No earlier balance" : undefined}
        label="Wallet balance"
        sparkline={wallet.sparkline.map(numeric)}
        sparklineColor="var(--color-net)"
        title={wallet.value === null ? undefined : formatIskSummary(wallet.value)}
        value={wallet.value === null ? "—" : money(wallet.value)}
        valueClass="text-foreground"
      />
    </div>
  );
}
