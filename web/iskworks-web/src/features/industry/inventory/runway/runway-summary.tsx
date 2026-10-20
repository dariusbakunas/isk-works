import { AlertTriangle, CheckCircle2, ExternalLink } from "lucide-react";
import { Link } from "react-router";

import type { RunwayEpicReadiness, RunwayKpis } from "../../../../api/inventory-analytics";
import { formatIskAbbreviated, formatIskSummary } from "../../../../components/money";
import { ProgressBar } from "../../../../components/primitives";
import { Private } from "../../../../observability/private";
import { InventoryKpiCard as KpiCard } from "../inventory-kpi-card";
import { coverageTone, formatPercent } from "./runway-format";

export function RunwayKpiStrip({ kpis }: { kpis: RunwayKpis }) {
  const short = kpis.materialsShort > 0;
  return (
    <div className="grid grid-cols-2 gap-2 lg:grid-cols-4">
      <KpiCard
        detail={`across ${kpis.materialsShort} material${kpis.materialsShort === 1 ? "" : "s"}`}
        label="Shortfall value"
        title={formatIskSummary(kpis.shortfallValue)}
        tone={short ? "danger" : "positive"}
        value={formatIskAbbreviated(kpis.shortfallValue, { currency: true })}
      />
      <KpiCard
        detail={short ? "need buying" : "all covered by stock"}
        label="Materials short"
        tone={short ? "warning" : "positive"}
        value={`${kpis.materialsShort} of ${kpis.materialsTotal}`}
      />
      <KpiCard
        detail="any input short"
        label="Epics blocked"
        tone={kpis.epicsBlocked > 0 ? "danger" : "positive"}
        value={`${kpis.epicsBlocked} of ${kpis.epicsTotal}`}
      />
      <KpiCard
        detail="by ISK value"
        label="Demand covered by stock"
        tone={coverageTone(kpis.demandCoveredPercent)}
        value={formatPercent(kpis.demandCoveredPercent)}
      />
    </div>
  );
}

/** One card per open Epic, oldest first. Clicking a card filters the table to
 * that Epic's materials; the link opens the Epic on the Board. */
export function EpicReadinessStrip({
  epics,
  selectedEpicId,
  onSelectEpic,
}: {
  epics: RunwayEpicReadiness[];
  selectedEpicId: string | null;
  onSelectEpic: (orderId: string | null) => void;
}) {
  return (
    <section aria-label="Epic readiness">
      <h2 className="iw-eyebrow mb-1.5">Epic readiness</h2>
      <div className="flex gap-2 overflow-x-auto pb-1">
        {epics.map((epic) => {
          const selected = epic.orderId === selectedEpicId;
          const tone = epic.blocked ? coverageTone(epic.coveredPercent) : "positive";
          return (
            <div
              className={`iw-panel flex w-48 shrink-0 flex-col gap-1 px-2.5 py-2 ${selected ? "border-primary" : ""}`}
              key={epic.orderId}
            >
              <div className="flex items-center gap-1.5">
                {epic.blocked
                  ? <AlertTriangle aria-label="Blocked" className="h-3 w-3 shrink-0 text-danger" />
                  : <CheckCircle2 aria-label="Covered" className="h-3 w-3 shrink-0 text-positive" />}
                <button
                  aria-pressed={selected}
                  className="min-w-0 flex-1 truncate text-left text-xs font-semibold hover:text-primary"
                  onClick={() => onSelectEpic(selected ? null : epic.orderId)}
                  title={selected ? "Show all materials" : "Show this Epic's materials"}
                  type="button"
                >
                  <Private>{epic.displayName}</Private>
                </button>
                <Link aria-label="Open Epic on the Board" className="text-muted hover:text-primary" to={`/board?epic=${epic.orderId}`}>
                  <ExternalLink aria-hidden="true" className="h-3 w-3" />
                </Link>
              </div>
              <div className="flex items-center gap-2">
                <div className="flex-1"><ProgressBar percent={epic.coveredPercent ?? 0} tone={tone} /></div>
                <span className="font-mono text-[0.625rem] tabular-nums text-muted">{formatPercent(epic.coveredPercent)}</span>
              </div>
              <div
                className={`font-mono text-[0.625rem] tabular-nums ${epic.blocked ? "text-danger" : "text-muted"}`}
                title={formatIskSummary(epic.shortValue)}
              >
                {epic.blocked ? `−${formatIskAbbreviated(epic.shortValue)} short` : "inputs covered"}
              </div>
            </div>
          );
        })}
      </div>
    </section>
  );
}
