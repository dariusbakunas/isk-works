import { CheckCircle2 } from "lucide-react";
import { Link } from "react-router";

import type { MarketOrderRow, MarketOrdersOverview } from "../../api/market-orders";
import { eveTypeImageUrl } from "../../components/eve-image-url";
import { IskCompact } from "../../components/money";
import { CharacterName } from "../../observability/private";
import { ago } from "../jobs/jobs-format";
import { useNow } from "../jobs/use-jobs";
import { percentLabel } from "../market-orders/market-orders-format";
import { useOverviewData } from "./overview-data";
import { panelState, PanelFrame } from "./panel-frame";
import { Stat } from "./stat";

const OUTBID_LIMIT = 4;
const OUTBID_PAGE = "/market-orders?outbid=1";

/** Outbid orders across every location, outbid longest first (an unknown
 * outbid time last), capped at `limit`. */
export function outbidRows(market: MarketOrdersOverview, limit: number): MarketOrderRow[] {
  const since = (order: MarketOrderRow) => (order.standing.outbidSince ? Date.parse(order.standing.outbidSince) : Number.POSITIVE_INFINITY);
  return market.locations
    .flatMap((location) => location.orders)
    .filter((order) => order.standing.status === "outbid")
    .sort((a, b) => since(a) - since(b))
    .slice(0, limit);
}

const STANDING_SEGMENTS = [
  { key: "best", label: "best", className: "bg-positive" },
  { key: "tied", label: "tied", className: "bg-primary" },
  { key: "outbid", label: "outbid", className: "bg-warning" },
  { key: "unknown", label: "unknown", className: "bg-muted" },
  {
    key: "likelyFilled",
    label: "likely filled",
    className: "bg-[repeating-linear-gradient(135deg,var(--color-border)_0,var(--color-border)_2px,transparent_2px,transparent_4px)]",
  },
] as const;

function StandingBar({ summary }: { summary: MarketOrdersOverview["summary"] }) {
  const total = STANDING_SEGMENTS.reduce((sum, segment) => sum + summary[segment.key], 0);
  const description = STANDING_SEGMENTS.map((segment) => `${summary[segment.key]} ${segment.label}`).join(", ");
  return (
    <div className="px-3 py-2.5">
      <div aria-label={`Order standing: ${description}`} className="flex h-2 overflow-hidden rounded-[1px] bg-border" role="img">
        {total > 0
          ? STANDING_SEGMENTS.map((segment) =>
              summary[segment.key] > 0 ? (
                <i key={segment.key} className={segment.className} style={{ flex: summary[segment.key] }} />
              ) : null,
            )
          : null}
      </div>
      <div aria-hidden="true" className="mt-1 flex flex-wrap gap-x-2.5 text-[0.625rem] text-muted">
        {STANDING_SEGMENTS.map((segment) => (
          <span key={segment.key} className={segment.key === "outbid" && summary.outbid > 0 ? "text-warning" : undefined}>
            <span className="capitalize">{segment.label}</span> {summary[segment.key]}
          </span>
        ))}
      </div>
    </div>
  );
}

function OutbidRow({ order, nowMs }: { order: MarketOrderRow; nowMs: number }) {
  const percent = order.standing.gapPercent ? percentLabel(order.standing.gapPercent.replace("-", "")) : null;
  const since = order.standing.outbidSince ? ago(order.standing.outbidSince, nowMs) : null;
  return (
    <Link
      className="grid min-h-9 grid-cols-[1.5rem_minmax(0,1fr)_2.5rem_auto] items-center gap-2 border-t border-border/60 px-3 py-1 hover:bg-panel-strong/50 sm:grid-cols-[1.5rem_minmax(0,1fr)_2.5rem_minmax(0,0.8fr)_auto]"
      to={OUTBID_PAGE}
    >
      <img alt="" className="h-6 w-6 rounded-[2px] border border-border" loading="lazy" src={eveTypeImageUrl(order.typeId, "icon", 32)} />
      <span className="truncate text-xs text-foreground">{order.typeName ?? `Type ${order.typeId}`}</span>
      <span
        className={`w-fit rounded-[2px] border px-1 text-[0.625rem] ${order.side === "sell" ? "border-danger/50 text-danger" : "border-primary/50 text-primary"}`}
      >
        {order.side === "sell" ? "Sell" : "Buy"}
      </span>
      <span className="hidden truncate text-[0.6875rem] text-muted sm:block">
        <CharacterName name={order.characterName} />
      </span>
      <span className="justify-self-end font-mono text-[0.6875rem] text-warning" title="How far the best competitor beats this order, and since when">
        {[percent, since].filter(Boolean).join(" · ")}
      </span>
    </Link>
  );
}

/** Active orders at a glance: sell value, escrow, how they stand, and the
 * orders outbid longest (spec §3.4, §9.4). */
export function MarketPanel({ className = "" }: { className?: string }) {
  const { sources } = useOverviewData();
  const nowMs = useNow();
  const market = sources.market;
  const data = market.data;
  const isEmpty = (overview: MarketOrdersOverview) => overview.locations.every((location) => location.orders.length === 0);
  const outbid = data ? outbidRows(data, OUTBID_LIMIT) : [];

  return (
    <PanelFrame
      className={className}
      link={{ to: "/market-orders" }}
      stamp={data ? `synced ${ago(data.generatedAt, nowMs)}` : undefined}
      state={panelState(market, isEmpty, "No active market orders.")}
      title="Market"
    >
      {data ? (
        <>
          <div aria-label="Market totals" className="grid grid-cols-2 border-b border-border" role="group">
            <Stat label="Active sells"><IskCompact value={data.summary.activeSellValue} /></Stat>
            <Stat label="Buy escrow"><IskCompact value={data.summary.buyEscrow} /></Stat>
          </div>
          <StandingBar summary={data.summary} />
          <div className="flex h-6 items-center bg-panel-strong/60 px-3 text-[0.625rem] font-semibold uppercase tracking-wider text-muted">
            Outbid now
          </div>
          {outbid.length === 0 ? (
            <p className="flex items-center justify-center gap-1.5 px-3 py-4 text-xs text-muted">
              <CheckCircle2 aria-hidden="true" className="h-3.5 w-3.5 text-positive" />
              No orders outbid.
            </p>
          ) : (
            outbid.map((order, index) => (
              <div key={order.orderId} className={index >= 3 ? "hidden sm:block" : undefined}>
                <OutbidRow nowMs={nowMs} order={order} />
              </div>
            ))
          )}
          {data.summary.oldestBookAt ? (
            <p className="mt-auto border-t border-border px-3 py-1.5 text-right text-[0.6875rem] text-muted">
              Books {ago(data.summary.oldestBookAt, nowMs).replace(" ago", " old")}
            </p>
          ) : null}
        </>
      ) : null}
    </PanelFrame>
  );
}
