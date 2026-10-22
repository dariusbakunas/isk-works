import { Link } from "react-router";
import type { ReactNode } from "react";

import type { DepthOrder, MarketOrderLocation, MarketOrderRow } from "../../api/market-orders";
import { formatIskSummary } from "../../components/money";
import { PlannerInspectorShell } from "../../components/planner-inspector-shell";
import { Badge, SectionHead } from "../../components/primitives";
import { CharacterName } from "../../observability/private";
import { CopyPriceButton, SideChip } from "./market-order-cells";
import {
  ago,
  countdown,
  expiresIn,
  formatPrice,
  isNegative,
  percentLabel,
  rangeLabel,
  statusLabel,
  statusTone,
} from "./market-orders-format";

function Line({ label, children, note }: { label: string; children: ReactNode; note?: string }) {
  return (
    <div className="flex items-start justify-between gap-3 py-1 text-xs">
      <span className="flex flex-col">
        <span>{label}</span>
        {note ? <span className="text-[0.625rem] text-muted">{note}</span> : null}
      </span>
      <span className="text-right font-mono">{children}</span>
    </div>
  );
}

function DepthRow({ order, showLocation, competitorAhead }: { order: DepthOrder; showLocation: boolean; competitorAhead: boolean }) {
  const outOfRange = order.tradesHere === false;
  const tone = order.isOwn
    ? "border border-primary/60 bg-primary/10"
    : competitorAhead
      ? "bg-danger/10 text-danger"
      : "";
  return (
    <li
      className={`grid grid-cols-[6.5rem_4rem_1fr] items-center gap-2 rounded-[2px] px-1.5 py-0.5 font-mono text-[0.6875rem] ${tone} ${outOfRange ? "opacity-50" : ""}`}
      data-testid="depth-row"
    >
      <span title={formatIskSummary(order.price)}>{formatPrice(order.price)}</span>
      <span className="text-right">{order.volumeRemain.toLocaleString("en-US")}</span>
      <span className="flex min-w-0 items-center justify-end gap-1 font-sans text-[0.625rem] text-muted">
        {showLocation ? <span className="truncate">{order.locationName ?? `Location ${order.locationId}`}</span> : null}
        {showLocation ? <span className="shrink-0 whitespace-nowrap">· {rangeLabel(order.range)}</span> : null}
        {order.isOwn ? <Badge square tone="primary">YOU</Badge> : null}
        {order.isAlt ? <Badge square tone="batch">ALT</Badge> : null}
        {outOfRange ? <span className="rounded-[2px] border border-border px-1">out of range</span> : null}
      </span>
    </li>
  );
}

export function MarketOrderInspector({
  row,
  location,
  nowMs,
  onClose,
}: {
  row: MarketOrderRow;
  location: MarketOrderLocation;
  nowMs: number;
  onClose: () => void;
}) {
  const { standing } = row;
  const name = row.typeName ?? `Type ${row.typeId}`;
  const modifyIn = countdown(standing.modifiableAt, nowMs);
  const ownIndex = row.depth.sameSide.findIndex((order) => order.isOwn);
  const isBuy = row.side === "buy";
  return (
    <PlannerInspectorShell
      closeLabel="Close order inspector"
      dismissLabel="Dismiss order inspector"
      eyebrow={location.locationName ?? `Location ${location.locationId}`}
      onClose={onClose}
      open
      returnFocusRowKey={String(row.orderId)}
      title={name}
      width="wide"
    >
      <div className="space-y-4 px-3 pb-4">
        <div className="flex flex-wrap items-center gap-2 text-xs">
          <Badge square tone={statusTone(standing.status)}>{statusLabel(standing.status).toUpperCase()}</Badge>
          <SideChip side={row.side} />
          {standing.outbidSince ? <span className="text-muted">Outbid since {ago(standing.outbidSince, nowMs)}</span> : null}
          <CharacterName className="text-muted" name={row.characterName} />
        </div>

        <section>
          <SectionHead>Order book depth</SectionHead>
          {row.depth.sameSide.length === 0 ? (
            <p className="text-xs text-muted">No book has been read for this order yet.</p>
          ) : (
            <>
              <div className="mb-1 text-[0.625rem] uppercase tracking-wider text-muted">{isBuy ? "Buy orders" : "Sell orders"}</div>
              <ol aria-label={isBuy ? "Buy orders" : "Sell orders"} className="space-y-px">
                {row.depth.sameSide.map((order, index) => (
                  <DepthRow
                    competitorAhead={index < ownIndex && !order.isAlt && order.tradesHere !== false}
                    key={order.orderId}
                    order={order}
                    showLocation={isBuy}
                  />
                ))}
              </ol>
              {isBuy && row.depth.sameSide.some((order) => order.tradesHere === false) ? (
                <p className="mt-1 text-[0.625rem] text-muted">Greyed rows can't reach this location.</p>
              ) : null}
            </>
          )}
          {row.depth.oppositeSide.length > 0 ? (
            <>
              <div className="mb-1 mt-2 text-[0.625rem] uppercase tracking-wider text-muted">{isBuy ? "Sell orders here" : "Buy orders reaching here"}</div>
              <ol aria-label={isBuy ? "Sell orders here" : "Buy orders reaching here"} className="space-y-px">
                {row.depth.oppositeSide.map((order) => (
                  <DepthRow competitorAhead={false} key={order.orderId} order={order} showLocation={!isBuy} />
                ))}
              </ol>
            </>
          ) : null}
        </section>

        <section>
          <SectionHead>Pricing</SectionHead>
          <Line label="Your price">{formatIskSummary(standing.price)}</Line>
          <Line
            label="Best competitor"
            note={standing.bestCompetitor ? `${standing.competitorsAhead} ahead · ${standing.volumeAhead.toLocaleString("en-US")} units` : undefined}
          >
            {standing.bestCompetitor ? (
              <span className={standing.status === "outbid" ? "text-danger" : ""}>{formatIskSummary(standing.bestCompetitor.price)}</span>
            ) : (
              "—"
            )}
          </Line>
          <Line label="Suggested" note="Broker fees not included">
            {standing.suggestedPrice ? (
              <span className="inline-flex items-center gap-1 text-primary">
                {formatIskSummary(standing.suggestedPrice)}
                <CopyPriceButton price={standing.suggestedPrice} />
              </span>
            ) : (
              "—"
            )}
          </Line>
          <Line label="Can modify">
            {modifyIn ? <span className="text-warning">{modifyIn}</span> : <span className="text-positive">now</span>}
          </Line>
        </section>

        {row.side === "sell" ? (
          <section>
            <SectionHead>Cost basis</SectionHead>
            {row.unitCost === null ? (
              <p className="text-xs text-muted">
                No cost basis for {name}. Record it in <Link className="underline" to="/inventory">Inventory</Link> to see margins.
              </p>
            ) : (
              <>
                <Line label="Unit cost (workspace avg)" note={`${row.stockQuantity.toLocaleString("en-US")} in stock`}>
                  {formatIskSummary(row.unitCost)}
                </Line>
                <Line
                  label="Sales tax"
                  note={row.salesTaxEstimated ? "Base rate: Accounting level not synced" : undefined}
                >
                  {percentLabel(String(Number(row.salesTaxRate) * 100))}
                </Line>
                {row.margin ? (
                  <Line label="Margin now">
                    <span className={isNegative(row.margin.marginUnit) ? "text-danger" : "text-positive"}>
                      {percentLabel(row.margin.marginPercent, true)}
                    </span>
                  </Line>
                ) : null}
                {row.marginAtSuggested ? (
                  <Line label="Margin at suggested">
                    <span className={isNegative(row.marginAtSuggested.marginUnit) ? "text-danger" : "text-positive"}>
                      {percentLabel(row.marginAtSuggested.marginPercent, true)}
                    </span>
                  </Line>
                ) : null}
                {row.margin ? (
                  <Line label="Remaining profit (all units)">
                    <span className={isNegative(row.margin.remainingProfit) ? "text-danger" : ""}>
                      {formatIskSummary(row.margin.remainingProfit)}
                    </span>
                  </Line>
                ) : null}
                <Link className="text-xs text-primary hover:underline" to={`/inventory?item=${row.typeId}`}>
                  View inventory history
                </Link>
              </>
            )}
          </section>
        ) : null}

        <section>
          <SectionHead>Data freshness</SectionHead>
          <Line label={location.kind === "structure" ? "Structure book" : "Region book"}>
            {location.bookObservedAt ? ago(location.bookObservedAt, nowMs) : "not read yet"}
          </Line>
          <Line
            label="Price from"
            note={
              standing.freshSource === "book"
                ? "The order's own copy in the book, fresher than the 20-minute order sync"
                : "Not in the book yet, so from the 20-minute order sync"
            }
          >
            {standing.freshSource === "book" ? "book" : "order sync"}
          </Line>
        </section>

        <section>
          <SectionHead>Order</SectionHead>
          <Line label="Issued">{new Date(standing.issuedAt).toISOString().slice(0, 16).replace("T", " ")} EVE</Line>
          <Line label="Expires">{expiresIn(row.expiresAt, nowMs)}</Line>
          <Line label="Volume">{standing.volumeRemain.toLocaleString("en-US")} / {row.volumeTotal.toLocaleString("en-US")}</Line>
          {isBuy ? <Line label="Range">{rangeLabel(row.range)}</Line> : null}
          {row.minVolume > 1 ? <Line label="Minimum volume">{row.minVolume.toLocaleString("en-US")}</Line> : null}
          {row.escrow ? <Line label="Escrow">{formatIskSummary(row.escrow)}</Line> : null}
        </section>
      </div>
    </PlannerInspectorShell>
  );
}
