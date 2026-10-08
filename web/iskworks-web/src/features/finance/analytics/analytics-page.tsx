import { X } from "lucide-react";
import { useMemo, useState } from "react";
import { useNavigate, useSearchParams } from "react-router";

import { exportFinanceAnalytics, type AnalyticsSection, type FinanceAnalytics } from "../../../api/finance-analytics";
import { formatIskSummary } from "../../../components/money";
import { EmptyState, InlineAlert } from "../../../components/primitives";
import { Private } from "../../../observability/private";
import { AnalyticsBreakdowns } from "./analytics-breakdowns";
import { AnalyticsHeader } from "./analytics-header";
import { AnalyticsSkeleton } from "./analytics-skeleton";
import { formatRangeText } from "./analytics-format";
import {
  parseAnalyticsUrlState,
  resolveDateRange,
  serializeAnalyticsUrlState,
  toAnalyticsQuery,
  transactionsLink,
  type AnalyticsUrlState,
} from "./analytics-url-state";
import { CashFlowChart } from "./cash-flow-chart";
import { ChartCard } from "./chart-card";
import { downloadBlob } from "./download-blob";
import { KpiCards } from "./kpi-cards";
import { useFinanceAnalytics } from "./use-finance-analytics";

export function AnalyticsPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const navigate = useNavigate();
  // Presets are relative to "today"; fix it for the life of the page so the
  // query does not shift under the user at midnight.
  const [now] = useState(() => new Date());
  const state = useMemo(() => parseAnalyticsUrlState(searchParams), [searchParams]);
  const query = useMemo(() => toAnalyticsQuery(state, now), [state, now]);
  const range = useMemo(() => resolveDateRange(state, now), [state, now]);
  const { data, loading, error, retry } = useFinanceAnalytics(query);
  const [message, setMessage] = useState<string | null>(null);

  function update(patch: Partial<AnalyticsUrlState>) {
    setSearchParams(serializeAnalyticsUrlState({ ...state, ...patch }), { replace: true });
  }

  async function exportSection(section: AnalyticsSection) {
    try {
      downloadBlob(await exportFinanceAnalytics(query, section), `isk-works-analytics-${section}.csv`);
    } catch (failure) {
      setMessage(failure instanceof Error ? failure.message : "Export failed.");
    }
  }

  const empty = data !== null && data.kpis.transactionCount === 0;

  return (
    <div className="finance-analytics min-h-0 flex-1 overflow-y-auto text-xs">
      <div className="space-y-3 px-3 py-3 sm:px-6">
        <AnalyticsHeader
          characters={data?.availableCharacters ?? []}
          granularity={query.granularity}
          onChange={update}
          range={range}
          state={state}
        />
        {state.category ? (
          <div className="flex items-center gap-2 text-muted">
            Filter:
            <span className="inline-flex items-center gap-1 rounded-[3px] border border-primary/50 bg-primary/10 px-1.5 py-0.5 text-primary">
              Category: {state.category}
              <button aria-label="Remove category filter" onClick={() => update({ category: null })} type="button">
                <X aria-hidden="true" className="h-3 w-3" />
              </button>
            </span>
          </div>
        ) : null}
        {message ? <InlineAlert title="Something went wrong" tone="warning">{message}</InlineAlert> : null}
        {error && !data ? (
          <InlineAlert title="Analytics could not load">
            {error.message}{" "}
            <button className="underline" onClick={retry} type="button">Try again</button>
          </InlineAlert>
        ) : null}
        {!data && !error ? <AnalyticsSkeleton /> : null}
        {data && empty ? (
          <EmptyState
            action={
              <div className="flex flex-wrap gap-2">
                {state.preset !== "30d" ? (
                  <button className="iw-button-secondary" onClick={() => update({ preset: "30d", dateFrom: null, dateTo: null })} type="button">Expand to 30d</button>
                ) : null}
                {state.characters.length > 0 ? (
                  <button className="iw-button-primary" onClick={() => update({ characters: [] })} type="button">All characters</button>
                ) : null}
              </div>
            }
            title="No transactions in this period"
          >
            No market transactions found between {formatRangeText(range.dateFrom, range.dateTo)}. Try a different date range or check character access.
          </EmptyState>
        ) : null}
        {data && !empty ? (
          <div aria-busy={loading} className={`space-y-3 transition-opacity ${loading ? "opacity-60" : ""}`}>
            <KpiCards kpis={data.kpis} rangeFrom={data.range.dateFrom} />
            <ChartCard
              onExportCsv={() => void exportSection("cashFlow")}
              onViewTransactions={() => navigate(transactionsLink(state, now))}
              subtitle={`${formatRangeText(data.range.dateFrom, data.range.dateTo)} · ${capitalize(data.range.granularity)} granularity`}
              title="Cash Flow"
            >
              <CashFlowChart buckets={data.cashFlow} range={data.range} />
            </ChartCard>
            <AnalyticsBreakdowns
              data={data}
              onCategory={(category) => update({ category })}
              onExport={(section) => void exportSection(section)}
              onView={(extra) => navigate(transactionsLink(state, now, extra))}
              state={state}
            />
            <Footnotes data={data} />
          </div>
        ) : null}
      </div>
    </div>
  );
}

function capitalize(value: string) {
  return value.charAt(0).toUpperCase() + value.slice(1);
}

function Footnotes({ data }: { data: FinanceAnalytics }) {
  const fees = data.kpis.fees;
  const notes: string[] = [
    fees
      ? "Income and expenses count market trades only; taxes & fees come from the wallet journal."
      : "Income and expenses count market trades only; the wallet journal is not included.",
  ];
  if (fees?.availableFrom && fees.availableFrom > data.range.dateFrom) {
    notes.push(`Taxes & fees are available from ${fees.availableFrom}; earlier days in this range are not included.`);
  }
  const excluded = data.excludedIntraAccount;
  if (excluded.transactionCount > 0) {
    notes.push(
      `${excluded.transactionCount.toLocaleString()} trade${excluded.transactionCount === 1 ? "" : "s"} between your own characters (${formatIskSummary(excluded.totalIsk)}) are excluded.`,
    );
  }
  const inventory = data.excludedInventoryBuys;
  if (inventory.transactionCount > 0) {
    notes.push(
      `${inventory.transactionCount.toLocaleString()} purchase${inventory.transactionCount === 1 ? "" : "s"} already recorded into Inventory (${formatIskSummary(inventory.totalIsk)}) are excluded as build inputs.`,
    );
  }
  const earliest = data.earliestObservedAt?.slice(0, 10);
  if (earliest && earliest > data.range.dateFrom) {
    notes.push(`Synced history starts ${earliest}; earlier days in this range have no data.`);
  }
  return (
    <Private as="ul" className="space-y-0.5 text-[0.625rem] text-muted">
      {notes.map((note) => <li key={note}>{note}</li>)}
    </Private>
  );
}
