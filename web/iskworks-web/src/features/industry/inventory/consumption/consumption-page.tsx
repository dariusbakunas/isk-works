import { X } from "lucide-react";
import { useSearchParams } from "react-router";

import { MATERIAL_CATEGORIES, type ConsumptionGranularity } from "../../../../api/inventory-analytics";
import { formatIskAbbreviated, formatIskSummary } from "../../../../components/money";
import { EmptyState, InlineAlert, PageHeader, Panel } from "../../../../components/primitives";
import { KpiCard } from "../../../../components/kpi-card";
import { InventorySectionTabs } from "../inventory-section-tabs";
import { categoryFromParam } from "../runway/runway-table";
import { CategoryDonut, ConsumptionChart } from "./consumption-charts";
import {
  BURN_RATE_HISTORY_WEEKS,
  bucketRange,
  CONSUMPTION_RANGES,
  historyWeeks,
  longDate,
  rangeFrom,
  utcToday,
  type ConsumptionRange,
} from "./consumption-format";
import { ConsumedMaterialsTable } from "./consumed-materials-table";
import { useInventoryConsumption } from "./use-inventory-consumption";

/** Below this much history the page says how short it is. */
const SHORT_HISTORY_WEEKS = 8;

function rangeParam(value: string | null): ConsumptionRange {
  return CONSUMPTION_RANGES.find((option) => option.id === value)?.id ?? "all";
}

function Toggle<T extends string>({
  label,
  options,
  value,
  onChange,
}: {
  label: string;
  options: Array<{ id: T; label: string }>;
  value: T;
  onChange: (value: T) => void;
}) {
  return (
    <div aria-label={label} className="flex items-center gap-1" role="group">
      <span className="mr-1 text-[0.625rem] font-semibold uppercase tracking-wider text-muted">{label}</span>
      {options.map((option) => (
        <button
          aria-pressed={value === option.id}
          className={`rounded-[2px] border px-2 py-0.5 text-xs ${
            value === option.id ? "border-primary/60 bg-primary/10 text-primary" : "border-border text-muted hover:text-foreground"
          }`}
          key={option.id}
          onClick={() => onChange(option.id)}
          type="button"
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

function ConsumptionSkeleton() {
  const block = "animate-pulse rounded-[2px] bg-panel-strong";
  return (
    <div aria-busy="true" aria-label="Loading consumption" className="space-y-3" role="status">
      <div className="grid grid-cols-2 gap-2 lg:grid-cols-4">
        {Array.from({ length: 4 }, (_, index) => <div className={`${block} h-16`} key={index} />)}
      </div>
      <div className={`${block} h-64 w-full`} />
      <div className="space-y-1">
        {Array.from({ length: 8 }, (_, index) => <div className={`${block} h-6 w-full`} key={index} />)}
      </div>
    </div>
  );
}

// Inventory › Consumption: what recorded production has used, over time.
// Filters live in the URL (?range=, ?granularity=, ?category=).
export function ConsumptionPage() {
  const [params, setParams] = useSearchParams();
  const range = rangeParam(params.get("range"));
  const granularity: ConsumptionGranularity = params.get("granularity") === "day" ? "day" : "week";
  const category = categoryFromParam(params.get("category"));
  const to = utcToday();
  const { data, loading, error, retry } = useInventoryConsumption({
    from: rangeFrom(range, to),
    to,
    granularity,
    category,
  });

  function setParam(key: string, value: string | null) {
    const next = new URLSearchParams(params);
    if (value) next.set(key, value);
    else next.delete(key);
    setParams(next, { replace: true });
  }

  const weeks = data?.earliestObservedAt ? historyWeeks(data.earliestObservedAt, data.to) : null;
  const busiest = data?.kpis.busiestBucket ?? null;

  return (
    <>
      <PageHeader eyebrow="Accounting inventory" title="Inventory">
        What recorded production has consumed, by category and material, and how long your stock lasts at that pace.
      </PageHeader>
      <InventorySectionTabs />
      <div className="mb-3 flex flex-wrap items-end justify-between gap-3">
        <div className="flex flex-wrap items-center gap-4">
          <Toggle
            label="Range"
            onChange={(value) => setParam("range", value === "all" ? null : value)}
            options={CONSUMPTION_RANGES}
            value={range}
          />
          <Toggle
            label="Granularity"
            onChange={(value) => setParam("granularity", value === "week" ? null : value)}
            options={[{ id: "week", label: "Week" }, { id: "day", label: "Day" }]}
            value={granularity}
          />
        </div>
        <label className="min-w-48 text-sm font-semibold">
          Category
          <select
            className="iw-input mt-1"
            onChange={(event) => setParam("category", event.target.value || null)}
            value={category ?? ""}
          >
            <option value="">All categories</option>
            {MATERIAL_CATEGORIES.map((option) => <option key={option} value={option}>{option}</option>)}
          </select>
        </label>
      </div>
      {error ? (
        <div className="mb-3">
          <InlineAlert title="Consumption unavailable">
            {error.message}{" "}
            <button className="underline" onClick={retry} type="button">Retry</button>
          </InlineAlert>
        </div>
      ) : null}
      {loading && !data ? <ConsumptionSkeleton /> : null}
      {data && data.earliestObservedAt === null ? (
        <Panel>
          <EmptyState title="No consumption recorded yet">
            Recording production on a ticket consumes its inputs from inventory; that history shows up here.
          </EmptyState>
        </Panel>
      ) : null}
      {data && data.earliestObservedAt !== null ? (
        <div aria-busy={loading} className={`space-y-3 ${loading ? "opacity-60" : ""}`}>
          {weeks !== null && weeks < SHORT_HISTORY_WEEKS ? (
            <InlineAlert title={`Consumption history starts ${longDate(data.earliestObservedAt)} (${weeks} week${weeks === 1 ? "" : "s"})`} tone="warning">
              Burn rate and days of cover appear once a material has {BURN_RATE_HISTORY_WEEKS} weeks of history.
            </InlineAlert>
          ) : null}
          {category ? (
            <div className="flex items-center gap-2 text-xs">
              <span className="inline-flex items-center gap-1 rounded-[2px] border border-primary/60 bg-primary/10 px-1.5 py-0.5 text-primary">
                {category}
                <button aria-label={`Clear ${category} filter`} onClick={() => setParam("category", null)} type="button">
                  <X aria-hidden="true" className="h-3 w-3" />
                </button>
              </span>
            </div>
          ) : null}
          <div className="grid grid-cols-2 gap-2 lg:grid-cols-4">
            <KpiCard
              detail={`${longDate(data.from)} – ${longDate(data.to)}`}
              label="Consumed this period"
              title={formatIskSummary(data.kpis.consumedValue)}
              tone="primary"
              value={formatIskAbbreviated(data.kpis.consumedValue, { currency: true })}
            />
            <KpiCard detail="distinct materials" label="Materials used" tone="primary" value={String(data.kpis.materialsUsed)} />
            <KpiCard
              detail={busiest ? bucketRange(busiest.start, data.granularity) : "nothing consumed"}
              label={`Busiest ${data.granularity}`}
              title={busiest ? formatIskSummary(busiest.value) : undefined}
              tone="primary"
              value={busiest ? formatIskAbbreviated(busiest.value, { currency: true }) : "—"}
            />
            <KpiCard detail="by ISK value" label="Most-used category" tone="primary" value={data.kpis.topCategory ?? "—"} />
          </div>
          <div className="grid gap-3 lg:grid-cols-[1fr_15rem]">
            <section aria-label="Consumption over time" className="iw-panel min-w-0 p-3">
              <h2 className="mb-2 text-xs font-semibold">Consumption by {data.granularity}</h2>
              <ConsumptionChart report={data} />
            </section>
            <section aria-label="By category" className="iw-panel min-w-0 p-3">
              <h2 className="mb-2 text-xs font-semibold">By category</h2>
              <CategoryDonut
                categories={data.categories}
                onSelect={(next) => setParam("category", next)}
                selected={category}
              />
            </section>
          </div>
          <section aria-label="Materials">
            <h2 className="iw-eyebrow mb-1.5">Materials by consumption</h2>
            {data.materials.length > 0 ? (
              <ConsumedMaterialsTable materials={data.materials} />
            ) : (
              <Panel>
                <EmptyState title="Nothing consumed in this period">
                  Widen the range{category ? " or clear the category filter" : ""} to see earlier consumption.
                </EmptyState>
              </Panel>
            )}
          </section>
        </div>
      ) : null}
    </>
  );
}
