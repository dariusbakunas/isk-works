import { useMemo } from "react";

import type { AnalyticsSection, FinanceAnalytics } from "../../../api/finance-analytics";
import type { FinanceDirection } from "../../../api/finance";
import { assignCategoryColors, categoryOrder, numeric } from "./analytics-format";
import type { AnalyticsUrlState } from "./analytics-url-state";
import { CharacterBars } from "./character-bars";
import { ChartCard } from "./chart-card";
import { DailyNetHeatmap } from "./daily-net-heatmap";
import { DonutSection } from "./category-donuts";
import { InsightsRow } from "./insights-row";
import { LocationsTable } from "./locations-table";
import { SankeyDiagram } from "./sankey-diagram";
import { TopTable } from "./top-items-table";

export interface TransactionsLinkExtra {
  direction?: FinanceDirection;
  category?: string | null;
  locationId?: number;
  typeId?: number;
  itemLabel?: string;
  locationLabel?: string;
  range?: { dateFrom: string; dateTo: string };
}

/** Category donuts, by character, by location and the two top-item tables. */
export function AnalyticsBreakdowns({
  data,
  state,
  onCategory,
  onExport,
  onView,
}: {
  data: FinanceAnalytics;
  state: AnalyticsUrlState;
  onCategory: (category: string | null) => void;
  onExport: (section: AnalyticsSection) => void;
  onView: (extra: TransactionsLinkExtra) => void;
}) {
  const colors = useMemo(
    () => assignCategoryColors(categoryOrder(data.spendingByCategory, data.incomeByCategory)),
    [data.spendingByCategory, data.incomeByCategory],
  );
  const roster = data.availableCharacters.map((character) => character.connectionId);
  const sum = (rows: Array<{ total: string }>) => String(rows.reduce((total, row) => total + numeric(row.total), 0));
  const heatmapRange = data.heatmap.length > 0
    ? { dateFrom: data.heatmap[0].date, dateTo: data.heatmap[data.heatmap.length - 1].date }
    : undefined;
  const filterNote = state.category ? ` · ${state.category} highlighted` : "";

  return (
    <>
      <div className="grid gap-3 lg:grid-cols-2">
        {/* The donuts always show every category, so their links drop the page's category filter. */}
        <ChartCard
          onExportCsv={() => onExport("income")}
          onViewTransactions={() => onView({ direction: "income", category: null })}
          subtitle={`Click a category to filter the page${filterNote}`}
          title="Income by Category"
        >
          <DonutSection colors={colors} onSelect={onCategory} rows={data.incomeByCategory} selected={state.category} side="income" total={sum(data.incomeByCategory)} />
        </ChartCard>
        <ChartCard
          onExportCsv={() => onExport("spending")}
          onViewTransactions={() => onView({ direction: "expense", category: null })}
          subtitle={`Click a category to filter the page${filterNote}`}
          title="Spending by Category"
        >
          <DonutSection colors={colors} onSelect={onCategory} rows={data.spendingByCategory} selected={state.category} side="spending" total={sum(data.spendingByCategory)} />
        </ChartCard>
      </div>
      <ChartCard
        onExportCsv={() => onExport("flow")}
        onViewTransactions={() => onView({})}
        subtitle="Income sources → Wallet → Spending and net saved"
        title="Money Flow"
      >
        <SankeyDiagram colors={colors} income={data.incomeByCategory} onSelect={onCategory} selected={state.category} spending={data.spendingByCategory} />
      </ChartCard>
      <div className="grid gap-3 2xl:grid-cols-2">
        <ChartCard onExportCsv={() => onExport("characters")} onViewTransactions={() => onView({})} title="By Character">
          <CharacterBars roster={roster} rows={data.byCharacter} />
        </ChartCard>
        <ChartCard onExportCsv={() => onExport("locations")} onViewTransactions={() => onView({})} subtitle="Top locations by traded volume" title="By Location">
          <LocationsTable
            onOpen={(row) => onView({ locationId: row.locationId, locationLabel: row.locationName })}
            rows={data.byLocation}
          />
        </ChartCard>
      </div>
      <div className="grid gap-3 2xl:grid-cols-2">
        <ChartCard onExportCsv={() => onExport("topEarners")} onViewTransactions={() => onView({ direction: "income" })} subtitle="Largest sales by value" title="Top Earners">
          <TopTable colors={colors} kind="income" onOpen={(row) => onView({ direction: "income", typeId: row.typeId, itemLabel: row.typeName })} rows={data.topEarners} />
        </ChartCard>
        <ChartCard onExportCsv={() => onExport("topExpenses")} onViewTransactions={() => onView({ direction: "expense" })} subtitle="Largest purchases by value" title="Top Expenses">
          <TopTable colors={colors} kind="expense" onOpen={(row) => onView({ direction: "expense", typeId: row.typeId, itemLabel: row.typeName })} rows={data.topExpenses} />
        </ChartCard>
      </div>
      <ChartCard
        onExportCsv={() => onExport("heatmap")}
        onViewTransactions={() => onView({ range: heatmapRange })}
        subtitle="Last 91 days · hover for exact value"
        title="Daily Net"
      >
        <DailyNetHeatmap days={data.heatmap} />
      </ChartCard>
      <InsightsRow insights={data.insights} />
    </>
  );
}
