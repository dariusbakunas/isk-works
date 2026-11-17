import { Link } from "react-router";

import type { RunwayEpicReadiness } from "../../api/inventory-analytics";
import type { OrderStatus, OrderSummary } from "../../api/industry/orders";
import { formatIskCompact } from "../../components/money";
import { Badge, ProgressBar, type Tone } from "../../components/primitives";
import { useOverviewData } from "./overview-data";
import { panelState, PanelFrame } from "./panel-frame";

const EPIC_LIMIT = 6;
const OPEN: OrderStatus[] = ["blocked", "ready", "inProgress"];
const STATUS: Partial<Record<OrderStatus, { label: string; tone: Tone }>> = {
  blocked: { label: "Blocked", tone: "danger" },
  ready: { label: "Ready", tone: "positive" },
  inProgress: { label: "In progress", tone: "primary" },
};

/** Open Epics (not complete or canceled), oldest first, capped. */
export function activeEpics(orders: OrderSummary[], limit: number): OrderSummary[] {
  return orders
    .filter((order) => OPEN.includes(order.status))
    .sort((a, b) => Date.parse(a.createdAt) - Date.parse(b.createdAt))
    .slice(0, limit);
}

function Coverage({ readiness }: { readiness: RunwayEpicReadiness | undefined }) {
  if (!readiness || readiness.coveredPercent === null) {
    return <span className="justify-self-end text-[0.6875rem] text-muted" title="Not in Materials runway">—</span>;
  }
  const percent = Math.round(readiness.coveredPercent);
  return (
    <span
      className={`justify-self-end whitespace-nowrap rounded-[2px] border px-1 text-[0.625rem] ${readiness.blocked ? "border-danger/50 text-danger" : "border-border text-muted"}`}
      title="Share of outstanding material value covered by stock"
    >
      {readiness.blocked ? `${percent}% · blocked` : `${percent}% covered`}
    </span>
  );
}

function EpicRow({ order, readiness, mobileHidden }: { order: OrderSummary; readiness: RunwayEpicReadiness | undefined; mobileHidden: boolean }) {
  const status = STATUS[order.status]!;
  const { satisfied, total, readyToStart } = order.rollup;
  return (
    <Link
      className={`${mobileHidden ? "hidden sm:grid" : "grid"} min-h-10 grid-cols-[minmax(0,1fr)_5rem_auto] items-center gap-2 border-t border-border/60 px-3 py-1 first:border-t-0 hover:bg-panel-strong/50 sm:grid-cols-[minmax(0,1fr)_5rem_minmax(5rem,7rem)_6.5rem_auto]`}
      to={`/board?epic=${order.id}`}
    >
      <span className="truncate text-xs text-foreground" title={order.displayName}>{order.displayName}</span>
      <span><Badge square tone={status.tone}>{status.label}</Badge></span>
      <span className="hidden sm:block">
        <span className="block font-mono text-[0.625rem] text-muted">{satisfied} / {total}</span>
        <ProgressBar percent={total > 0 ? (satisfied / total) * 100 : 0} />
      </span>
      <span className="hidden text-[0.6875rem] text-muted sm:block">
        {readyToStart ? `${readyToStart} ready to start` : null}
      </span>
      <Coverage readiness={readiness} />
    </Link>
  );
}

/** Open Epics, oldest first, with progress and material readiness from the
 * Materials runway (spec §3.6, §9.6). */
export function EpicsPanel({ className = "" }: { className?: string }) {
  const { sources } = useOverviewData();
  const orders = sources.orders;
  const runway = sources.runway.data;
  const epics = activeEpics(orders.data ?? [], EPIC_LIMIT);
  const readiness = new Map((runway?.epics ?? []).map((entry) => [entry.orderId, entry]));
  const stamp = runway
    ? `${runway.kpis.epicsBlocked} of ${runway.kpis.epicsTotal} blocked · short ${formatIskCompact(runway.kpis.shortfallValue)}`
    : undefined;

  return (
    <PanelFrame
      className={className}
      link={{ to: "/board" }}
      stamp={stamp}
      state={panelState(orders, () => epics.length === 0, "No active Epics.")}
      title="Epics"
    >
      <div>
        {epics.map((order, index) => (
          <EpicRow key={order.id} mobileHidden={index >= 3} order={order} readiness={readiness.get(order.id)} />
        ))}
      </div>
    </PanelFrame>
  );
}
