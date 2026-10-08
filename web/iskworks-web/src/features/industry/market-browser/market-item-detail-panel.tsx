import { useCallback, useEffect, useState, type ReactNode } from "react";

import {
  getMarketItemOrders,
  getMarketItemsFreshness,
  requestMarketData,
  type MarketCategoryNode,
  type MarketItemOrders,
  type MarketOrderRow,
} from "../../../api/industry";
import { formatIskSummary } from "../../../components/money";
import { EveTypeImage } from "../../../components/eve-type-image";
import { InlineAlert, Panel, SectionHead, StatusDot } from "../../../components/primitives";
import {
  OperationalTable,
  OperationalTableRow,
  type OperationalColumn,
} from "../../../components/operational-table";
import { apiMessage, formatAge, formatExpiresIn, MoneyCell } from "./shared";

// Column order/set matches the EVE-style reference this was checked
// against exactly -- Range and Min Volume are buy-order-only concepts (a
// sell order has neither), so sellers and buyers deliberately get their
// own column set rather than one generic shared layout with irrelevant
// columns on the wrong side.
const SELLER_COLUMNS: OperationalColumn[] = [
  { key: "quantity", label: "Quantity", width: "90px", align: "right", numeric: true },
  { key: "price", label: "Price", width: "130px", align: "right", numeric: true },
  { key: "location", label: "Location", width: "minmax(160px,1fr)" },
  { key: "expires", label: "Expires in", width: "110px", align: "right" },
];

const BUYER_COLUMNS: OperationalColumn[] = [
  { key: "quantity", label: "Quantity", width: "90px", align: "right", numeric: true },
  { key: "price", label: "Price", width: "130px", align: "right", numeric: true },
  { key: "range", label: "Range", width: "90px" },
  { key: "location", label: "Location", width: "minmax(160px,1fr)" },
  { key: "minVolume", label: "Min Volume", width: "100px", align: "right", numeric: true },
  { key: "expires", label: "Expires in", width: "110px", align: "right" },
];

// Beyond one worker refresh cycle's worth of slack (the default refresh
// interval is 5 minutes) -- past this, or when nothing has ever been
// observed at all, selecting an item triggers the synchronous exact-item
// refresh instead of just showing whatever's cached.
const STALE_AFTER_MS = 15 * 60 * 1000;

interface Shown {
  data: MarketItemOrders;
  // From the freshness check (market_source_coverage's own completed-batch
  // timestamp), never from `data.summary.observedAt` -- that field is
  // derived purely from order rows and is `null` both when nothing has
  // ever been fetched *and* when a fetch completed but genuinely found
  // zero orders, so it can't distinguish "missing" from "confirmed empty"
  // the way this freshness check can.
  observedAt: string | null;
}

type PanelState =
  | { status: "loading" }
  | { status: "refreshing"; cached: Shown | null }
  | { status: "ready"; shown: Shown }
  | { status: "error"; message: string; cached: Shown | null };

function isStale(observedAt: string | null): boolean {
  return observedAt === null || Date.now() - new Date(observedAt).getTime() > STALE_AFTER_MS;
}

export function MarketItemDetailPanel({
  typeId,
  typeName,
  regionId,
  locationId,
  categories = [],
}: {
  typeId: number;
  typeName: string;
  regionId: number;
  locationId?: number;
  categories?: MarketCategoryNode[];
}) {
  const [state, setState] = useState<PanelState>({ status: "loading" });

  const load = useCallback(async () => {
    setState({ status: "loading" });
    const scope = { regionId, locationId };

    let mostRecentUpdatedAt: string | null;
    try {
      mostRecentUpdatedAt = (await getMarketItemsFreshness([typeId], scope)).mostRecentUpdatedAt;
    } catch {
      // The freshness check itself failing is treated conservatively as
      // "missing" -- still attempt a refresh rather than silently
      // rendering nothing.
      mostRecentUpdatedAt = null;
    }

    if (!isStale(mostRecentUpdatedAt)) {
      // Fresh -- render whatever's there as-is, including a confirmed-zero
      // order book, with no refresh triggered.
      try {
        const data = await getMarketItemOrders(typeId, regionId, locationId);
        setState({ status: "ready", shown: { data, observedAt: mostRecentUpdatedAt } });
      } catch (error) {
        setState({ status: "error", message: apiMessage(error), cached: null });
      }
      return;
    }

    // Missing or stale: if there's existing (stale) data, load it first so
    // something useful is visible while the synchronous refresh runs.
    let cached: Shown | null = null;
    if (mostRecentUpdatedAt !== null) {
      try {
        const data = await getMarketItemOrders(typeId, regionId, locationId);
        cached = { data, observedAt: mostRecentUpdatedAt };
      } catch {
        cached = null;
      }
    }
    setState({ status: "refreshing", cached });

    try {
      await requestMarketData(typeId, scope);
      const data = await getMarketItemOrders(typeId, regionId, locationId);
      setState({ status: "ready", shown: { data, observedAt: new Date().toISOString() } });
    } catch (error) {
      setState({ status: "error", message: apiMessage(error), cached });
    }
  }, [typeId, regionId, locationId]);

  useEffect(() => {
    void load();
  }, [load]);

  const shown: Shown | null =
    state.status === "ready" ? state.shown : state.status === "refreshing" || state.status === "error" ? state.cached : null;
  const isRefreshing = state.status === "refreshing";
  const errorMessage = state.status === "error" ? state.message : null;
  const breadcrumb = shown ? categoryBreadcrumb(categories, shown.data.marketGroupId) : null;
  const stale = shown ? isStale(shown.observedAt) : false;

  return (
    <Panel className="sticky top-3 flex min-w-0 max-h-[calc(100vh-8rem)] flex-col gap-3 overflow-hidden">
      {/* Identity/summary stay pinned above the two scrollable order-book
          halves below -- never part of their scroll area, so the item's
          icon/name and Best Sell/Buy/Spread/Sell Vol are always visible
          regardless of how far either side is scrolled. */}
      <div className="flex shrink-0 flex-col gap-3">
        <div className="flex items-start justify-between gap-2">
          <div className="flex min-w-0 items-center gap-2">
            <EveTypeImage size={32} typeId={typeId} typeName={typeName} />
            <div className="min-w-0">
              <strong className="block truncate">{typeName}</strong>
              {breadcrumb ? <span className="iw-muted block truncate text-xs">{breadcrumb}</span> : null}
            </div>
          </div>
          {shown ? (
            <span
              className={`flex shrink-0 items-center gap-1 text-xs ${
                isRefreshing ? "text-muted" : stale ? "text-warning" : "text-positive"
              }`}
            >
              <StatusDot tone={isRefreshing ? "muted" : stale ? "warning" : "positive"} />
              {isRefreshing ? "Refreshing · " : stale ? "Stale · " : "Updated "}
              {formatAge(shown.observedAt)}
            </span>
          ) : null}
        </div>

        {state.status === "loading" ? <p className="iw-muted">Loading market data...</p> : null}
        {isRefreshing && !shown ? <p className="iw-muted">Fetching prices...</p> : null}

        {errorMessage ? (
          <InlineAlert title={shown ? "Couldn't refresh prices" : "Market data unavailable"} tone="error">
            {errorMessage}
            {shown ? " Showing the last known data below." : ""}
            <button className="ml-2 underline" onClick={() => void load()} type="button">
              Try again
            </button>
          </InlineAlert>
        ) : null}

        {isRefreshing && shown ? (
          <InlineAlert title="Refreshing prices..." tone="info">
            Fetching the latest orders. Showing the last known data below.
          </InlineAlert>
        ) : null}

        {shown ? (
          <>
            <dl className="grid grid-cols-2 gap-2 sm:grid-cols-4">
              <SummaryMetric label="Best Sell" value={formatSummaryMoney(shown.data.summary.bestSell)} />
              <SummaryMetric label="Best Buy" value={formatSummaryMoney(shown.data.summary.bestBuy)} />
              <SummaryMetric label="Spread" value={formatSummaryMoney(shown.data.summary.spread)} />
              <SummaryMetric
                label="Sell Vol"
                value={<span className="font-mono text-sm">{formatQuantityCompact(shown.data.summary.sellVolume)}</span>}
              />
            </dl>

            {locationId ? <p className="iw-muted -mb-1 text-xs">This location only</p> : null}
          </>
        ) : null}
      </div>

      {shown ? (
        <div className="grid min-h-0 flex-1 grid-rows-2 gap-3">
          {/* CSS Grid, not flexbox, for the 50/50 split: a flex `flex-1` +
              `min-h-0` pair on these two siblings measurably failed to
              divide the remaining space evenly in practice (each ended up
              sized roughly proportional to its own row count instead) --
              `minmax(0,1fr)` (what `grid-rows-2` expands to) is the
              standard, reliable way to force equal tracks regardless of
              content, so each half's own heading stays pinned above its
              own independently scrolling order list without fighting the
              other for space. */}
          <div className="flex min-h-0 min-w-0 flex-col overflow-hidden">
            <SectionHead>Sellers</SectionHead>
            <div className="min-h-0 flex-1 overflow-hidden">
              <OrderBookTable
                ariaLabel="Sell orders"
                columns={SELLER_COLUMNS}
                emptyText="No sell orders at this market scope."
                orders={shown.data.sellOrders}
              />
            </div>
          </div>

          <div className="flex min-h-0 min-w-0 flex-col overflow-hidden">
            <SectionHead>Buyers</SectionHead>
            <div className="min-h-0 flex-1 overflow-hidden">
              <OrderBookTable
                ariaLabel="Buy orders"
                columns={BUYER_COLUMNS}
                emptyText="No buy orders at this market scope."
                orders={shown.data.buyOrders}
              />
            </div>
          </div>
        </div>
      ) : null}
    </Panel>
  );
}

// The mock's fISK-style abbreviation applied to a raw unit count rather
// than an ISK amount -- Sell Vol isn't money, so `formatIskCompact` (which
// expects a decimal `Money` string) doesn't apply here.
function formatQuantityCompact(value: number): string {
  const abs = Math.abs(value);
  if (abs >= 1e9) return `${(value / 1e9).toFixed(2)}b`;
  if (abs >= 1e6) return `${(value / 1e6).toFixed(2)}m`;
  if (abs >= 1e3) return `${(value / 1e3).toFixed(1)}k`;
  return value.toFixed(0);
}

function formatSummaryMoney(value: string | null): ReactNode {
  if (value === null) return <span className="text-muted">—</span>;
  return <span className="font-mono text-sm">{formatIskSummary(value)}</span>;
}

// Root category name + the item's own (leaf) market group name, e.g.
// "Materials · Minerals" -- resolved from the category tree the page
// already has loaded rather than a dedicated lookup endpoint.
function categoryBreadcrumb(categories: MarketCategoryNode[], marketGroupId: number | null): string | null {
  if (marketGroupId === null) return null;
  const path = findCategoryPath(categories, marketGroupId);
  if (!path) return null;
  return path.length === 1 ? path[0].name : `${path[0].name} · ${path[path.length - 1].name}`;
}

function findCategoryPath(nodes: MarketCategoryNode[], marketGroupId: number): MarketCategoryNode[] | null {
  for (const node of nodes) {
    if (node.marketGroupId === marketGroupId) return [node];
    if (node.children.length > 0) {
      const childPath = findCategoryPath(node.children, marketGroupId);
      if (childPath) return [node, ...childPath];
    }
  }
  return null;
}

function SummaryMetric({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="min-w-0">
      <dt className="iw-eyebrow">{label}</dt>
      <dd className="mt-0.5">{value}</dd>
    </div>
  );
}

// Sellers and buyers are always shown at once now (no sell/buy tab
// interaction), each with its own column set and its own independent empty
// state -- Range and Min Volume are buy-order-only concepts, so a sell
// order simply never renders those cells rather than sharing one column
// layout with irrelevant columns. Orders are rendered in exactly the order
// the API returns them (`applicable_orders_sorted`: sells cheapest-first,
// buys highest-first) -- never re-sorted here.
function OrderBookTable({
  orders,
  columns,
  ariaLabel,
  emptyText,
}: {
  orders: MarketOrderRow[];
  columns: OperationalColumn[];
  ariaLabel: string;
  emptyText: string;
}) {
  if (orders.length === 0) {
    return <p className="iw-muted">{emptyText}</p>;
  }
  return (
    <OperationalTable
      ariaLabel={ariaLabel}
      columns={columns}
      onSelectRow={() => {}}
      scrollClassName="h-full overflow-y-auto"
      selectedRowKey={null}
    >
      <tbody>
        {orders.map((order, index) => (
          <OperationalTableRow
            cells={{
              quantity: <span className="font-mono tabular-nums">{order.quantity.toLocaleString()}</span>,
              price: <MoneyCell value={order.price} />,
              range: <span className="text-muted">{order.orderRange}</span>,
              location: <span className="truncate text-muted">{order.locationName}</span>,
              minVolume: <span className="font-mono tabular-nums text-muted">{order.minQuantity.toLocaleString()}</span>,
              expires: <span className="text-muted">{formatExpiresIn(order.expiresAt)}</span>,
            }}
            key={`${order.locationId}-${index}`}
            rowKey={`${order.locationId}-${index}`}
          />
        ))}
      </tbody>
    </OperationalTable>
  );
}
