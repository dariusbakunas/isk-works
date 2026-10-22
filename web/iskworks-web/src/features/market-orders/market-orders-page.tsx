import { AlertTriangle, Info, Lock, RefreshCw, Search, TrendingDown } from "lucide-react";
import { useMemo, useState } from "react";
import { Link, useSearchParams } from "react-router";

import { beginAuthorization } from "../../api/esi";
import type { MarketOrderLocation, MarketOrderRow, MarketOrderSide, TraderCharacter } from "../../api/market-orders";
import { KpiCard } from "../../components/kpi-card";
import { formatIskCompact, formatIskSummary } from "../../components/money";
import { OperationalTable, OperationalTableGroup, OperationalTableRow, type OperationalColumn } from "../../components/operational-table";
import { EmptyState, InlineAlert, PageHeader, Panel } from "../../components/primitives";
import { CharacterName } from "../../observability/private";
import {
  BestCompetitorCell,
  CostBasisCell,
  ExpiresCell,
  GapCell,
  ItemCell,
  MarginCell,
  ModifyCell,
  PriceCell,
  StatusCell,
  SuggestedCell,
  VolumeCell,
} from "./market-order-cells";
import { MarketOrderInspector } from "./market-order-inspector";
import { ago } from "./market-orders-format";
import { useMarketOrders, useNow } from "./use-market-orders";

const COLUMNS: OperationalColumn[] = [
  { key: "item", label: "Item", width: "minmax(12rem, 1fr)", sticky: true },
  { key: "status", label: "Status", width: "5.5rem" },
  { key: "price", label: "Your price", width: "6.5rem", align: "right", numeric: true },
  { key: "best", label: "Best comp.", width: "7.5rem", align: "right", numeric: true },
  { key: "gap", label: "Gap", width: "5.5rem", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "suggested", label: "Suggested", width: "7rem", align: "right", numeric: true },
  { key: "modify", label: "Modify", width: "4.5rem", align: "right", hideBelow: "tablet" },
  { key: "volume", label: "Volume", width: "7rem", hideBelow: "desktop" },
  { key: "cost", label: "Cost basis", width: "6rem", align: "right", numeric: true, hideBelow: "desktop" },
  { key: "margin", label: "Margin", width: "8.5rem", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "expires", label: "Expires", width: "5rem", align: "right", hideBelow: "desktop" },
];

type SideFilter = MarketOrderSide | "all";

function PageSkeleton() {
  const block = "animate-pulse rounded-[2px] bg-panel-strong";
  return (
    <div aria-busy="true" aria-label="Loading market orders" className="space-y-3" role="status">
      <div className="grid grid-cols-2 gap-2 lg:grid-cols-5">
        {Array.from({ length: 5 }, (_, index) => <div className={`${block} h-16`} key={index} />)}
      </div>
      <div className="space-y-1">
        {Array.from({ length: 10 }, (_, index) => <div className={`${block} h-9 w-full`} key={index} />)}
      </div>
    </div>
  );
}

async function reauthorize() {
  const started = await beginAuthorization();
  if (!started.fixtureMode) window.location.assign(started.authorizationUrl);
}

function namesList(characters: TraderCharacter[]) {
  return characters.map((character, index) => (
    <span key={character.connectionId}>
      {index > 0 ? (index === characters.length - 1 ? " and " : ", ") : ""}
      <CharacterName className="font-semibold" name={character.characterName} />
    </span>
  ));
}

function unavailableText(location: MarketOrderLocation): { title: string; detail: string } | null {
  const unavailable = location.unavailable;
  if (!unavailable) return null;
  switch (unavailable.reason) {
    case "unknownLocation":
      return {
        title: "Unknown structure",
        detail: "None of your characters could look this structure up, so its market can't be read. Statuses show as Unknown.",
      };
    case "refreshFailed":
      return { title: "Cannot read this market", detail: `${unavailable.detail}. Statuses show as Unknown until access is granted.` };
    case "noDataYet":
      return { title: "Waiting for market data", detail: "The order book hasn't been fetched yet. It usually arrives within a minute." };
  }
}

function matches(row: MarketOrderRow, search: string): boolean {
  const needle = search.trim().toLowerCase();
  return !needle || (row.typeName ?? "").toLowerCase().includes(needle);
}

// Market Orders: every open buy and sell order of the workspace's
// characters, with whether a competitor beat it, the price to move to, and
// (for sell orders) the margin over the stock's cost basis.
export function MarketOrdersPage() {
  const { data, loading, error, reload, refresh, refreshing } = useMarketOrders();
  const nowMs = useNow();
  const [params, setParams] = useSearchParams();
  const [search, setSearch] = useState("");
  const side = (["buy", "sell"].includes(params.get("side") ?? "") ? params.get("side") : "all") as SideFilter;
  const outbidOnly = params.get("outbid") === "1";
  const locationFilter = params.get("location") ?? "";
  const hiddenCharacters = useMemo(() => new Set(params.getAll("hide")), [params]);
  const selectedOrderId = params.has("order") ? Number(params.get("order")) : null;

  function update(change: (next: URLSearchParams) => void) {
    const next = new URLSearchParams(params);
    change(next);
    setParams(next, { replace: true });
  }

  const groups = useMemo(
    () =>
      (data?.locations ?? [])
        .filter((location) => !locationFilter || String(location.locationId) === locationFilter)
        .map((location) => ({
          ...location,
          orders: location.orders.filter(
            (row) =>
              (side === "all" || row.side === side) &&
              (!outbidOnly || row.standing.status === "outbid") &&
              !hiddenCharacters.has(row.connectionId) &&
              matches(row, search),
          ),
        }))
        .filter((location) => location.orders.length > 0),
    [data, locationFilter, side, outbidOnly, hiddenCharacters, search],
  );
  const selected = useMemo(() => {
    for (const location of data?.locations ?? []) {
      const row = location.orders.find((order) => order.orderId === selectedOrderId);
      if (row) return { row, location };
    }
    return null;
  }, [data, selectedOrderId]);

  const header = (
    <PageHeader eyebrow="Trade" title="Market Orders">
      Your characters' buy and sell orders: which ones a competitor beat, what to reprice to, and what each sale earns over cost.
    </PageHeader>
  );

  if (loading && !data) return <>{header}<PageSkeleton /></>;
  if (error && !data) {
    return (
      <>
        {header}
        <InlineAlert title="Market orders unavailable">
          {error.message} <button className="underline" onClick={reload} type="button">Retry</button>
        </InlineAlert>
      </>
    );
  }
  if (!data) return null;

  const { characters, summary } = data;
  if (characters.length === 0) {
    return (
      <>
        {header}
        <Panel>
          <EmptyState action={<Link className="iw-button-primary" to="/characters">Connect a character</Link>} title="No characters connected">
            Connect an EVE character with the market orders permission to see its orders here.
          </EmptyState>
        </Panel>
      </>
    );
  }

  const missingScope = characters.filter((character) => character.ordersScopeMissing);
  const syncFailed = characters.filter((character) => !character.ordersScopeMissing && character.ordersSyncError);
  const syncedAt = characters
    .map((character) => character.ordersSyncedAt)
    .filter((value): value is string => value !== null)
    .sort()[0] ?? null;
  const totalOrders = data.locations.reduce((count, location) => count + location.orders.length, 0);

  return (
    <>
      {header}
      <div className="space-y-3">
        <div className="flex items-center justify-end gap-2 text-xs text-muted">
          <span>Books as of {ago(summary.oldestBookAt, nowMs)} · Your orders synced {ago(syncedAt, nowMs)}</span>
          <button
            aria-label="Refresh market orders"
            className="iw-button-secondary inline-flex items-center gap-1"
            disabled={refreshing}
            onClick={() => void refresh()}
            type="button"
          >
            <RefreshCw aria-hidden="true" className={`h-3.5 w-3.5 ${refreshing || loading ? "animate-spin" : ""}`} />
            Refresh
          </button>
        </div>

        {missingScope.length > 0 ? (
          <div className="flex flex-col gap-2 rounded-md border border-warning/60 bg-warning/10 px-3 py-2 text-sm sm:flex-row sm:items-center sm:justify-between" role="status">
            <span className="flex items-start gap-2 text-warning">
              <AlertTriangle aria-hidden="true" className="mt-0.5 h-4 w-4 shrink-0" />
              <span>
                <span className="block font-semibold">
                  {missingScope.length} character{missingScope.length === 1 ? " hasn't" : "s haven't"} granted market order access
                </span>
                <span className="text-xs">
                  {namesList(missingScope)} {missingScope.length === 1 ? "is" : "are"} missing the{" "}
                  <code className="break-all">esi-markets.read_character_orders.v1</code> scope. Their orders won't appear until you re-authorize.
                </span>
              </span>
            </span>
            <button className="iw-button-secondary inline-flex shrink-0 items-center gap-1 self-start sm:self-auto" onClick={() => void reauthorize()} type="button">
              <Lock aria-hidden="true" className="h-3.5 w-3.5" /> Refresh permissions
            </button>
          </div>
        ) : null}
        {syncFailed.length > 0 ? (
          <InlineAlert title="Some orders couldn't be synced" tone="warning">
            {namesList(syncFailed)}: {syncFailed[0].ordersSyncError}. ISK Works retries automatically.
          </InlineAlert>
        ) : null}
        {data.stargatesMissing ? (
          <div className="flex items-start gap-2 rounded-md border border-primary/50 bg-primary/10 px-3 py-2 text-sm" role="status">
            <Info aria-hidden="true" className="mt-0.5 h-4 w-4 shrink-0 text-primary" />
            <span>
              <span className="font-semibold">Buy order range checks need stargates.</span>{" "}
              <span className="text-muted">
                Re-run the SDE import (an operator task) to evaluate jump ranges. Until then, buy orders that a ranged order may beat show as Unknown.
              </span>
            </span>
          </div>
        ) : null}

        <div className="grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-[repeat(5,minmax(0,11rem))]">
          <KpiCard detail="reprice in game" label="Outbid" tone={summary.outbid > 0 ? "danger" : "muted"} value={String(summary.outbid)} />
          <KpiCard detail="same price as a competitor" label="Tied" tone={summary.tied > 0 ? "warning" : "muted"} value={String(summary.tied)} />
          <KpiCard detail="competition not visible" label="Unknown" tone="muted" value={String(summary.unknown)} />
          <KpiCard
            detail={formatIskSummary(summary.activeSellValue)}
            label="Active sell"
            title={formatIskSummary(summary.activeSellValue)}
            tone="positive"
            value={formatIskCompact(summary.activeSellValue)}
          />
          <KpiCard
            detail={formatIskSummary(summary.buyEscrow)}
            label="Buy escrow"
            title={formatIskSummary(summary.buyEscrow)}
            tone="warning"
            value={formatIskCompact(summary.buyEscrow)}
          />
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <div aria-label="Side" className="flex" role="group">
            {(["all", "sell", "buy"] as const).map((option) => (
              <button
                aria-pressed={side === option}
                className={`border px-2 py-1 text-xs first:rounded-l-[2px] last:rounded-r-[2px] ${side === option ? "border-primary/60 bg-primary/10 text-foreground" : "border-border text-muted hover:text-foreground"}`}
                key={option}
                onClick={() => update((next) => (option === "all" ? next.delete("side") : next.set("side", option)))}
                type="button"
              >
                {option === "all" ? "All" : option === "sell" ? "Sell" : "Buy"}
              </button>
            ))}
          </div>
          <button
            aria-pressed={outbidOnly}
            className={`inline-flex items-center gap-1 rounded-[2px] border px-2 py-1 text-xs ${outbidOnly ? "border-danger/60 bg-danger/10 text-danger" : "border-border text-muted hover:text-foreground"}`}
            onClick={() => update((next) => (outbidOnly ? next.delete("outbid") : next.set("outbid", "1")))}
            type="button"
          >
            <TrendingDown aria-hidden="true" className="h-3.5 w-3.5" /> Outbid only
          </button>
          <div aria-label="Characters" className="flex flex-wrap gap-1" role="group">
            {characters.map((character) => {
              const shown = !hiddenCharacters.has(character.connectionId);
              return (
                <button
                  aria-pressed={shown}
                  className={`rounded-[2px] border px-2 py-1 text-xs ${shown ? "border-primary/60 bg-primary/10 text-foreground" : "border-border text-muted"}`}
                  key={character.connectionId}
                  onClick={() =>
                    update((next) => {
                      const hidden = new Set(next.getAll("hide"));
                      if (hidden.has(character.connectionId)) hidden.delete(character.connectionId);
                      else hidden.add(character.connectionId);
                      next.delete("hide");
                      for (const id of hidden) next.append("hide", id);
                    })
                  }
                  type="button"
                >
                  <CharacterName name={character.characterName} />
                </button>
              );
            })}
          </div>
          <label>
            <span className="sr-only">Location</span>
            <select
              className="iw-input"
              onChange={(event) => update((next) => (event.target.value ? next.set("location", event.target.value) : next.delete("location")))}
              value={locationFilter}
            >
              <option value="">All locations</option>
              {data.locations.map((location) => (
                <option key={location.locationId} value={location.locationId}>
                  {location.locationName ?? `Location ${location.locationId}`}
                </option>
              ))}
            </select>
          </label>
          <label className="relative">
            <Search aria-hidden="true" className="pointer-events-none absolute left-2 top-1/2 h-4 w-4 -translate-y-1/2 text-muted" />
            <span className="sr-only">Search items</span>
            <input className="iw-input w-52 pl-8" onChange={(event) => setSearch(event.target.value)} placeholder="Search item…" type="search" value={search} />
          </label>
        </div>

        {totalOrders === 0 ? (
          <Panel>
            <EmptyState action={<button className="iw-button-secondary" onClick={reload} type="button">Refresh</button>} title="No active market orders">
              Your characters don't have any buy or sell orders on the market.
            </EmptyState>
          </Panel>
        ) : groups.length === 0 ? (
          <Panel>
            <EmptyState title="No matching orders">No orders match the current filters.</EmptyState>
          </Panel>
        ) : (
          <OperationalTable
            ariaLabel="Market orders"
            columns={COLUMNS}
            onSelectRow={(rowKey) => update((next) => next.set("order", rowKey))}
            selectedRowKey={selectedOrderId === null ? null : String(selectedOrderId)}
          >
            {groups.map((location) => {
              const outbid = location.orders.filter((row) => row.standing.status === "outbid").length;
              const problem = unavailableText(location);
              return (
                <OperationalTableGroup
                  groupKey={String(location.locationId)}
                  itemCount={location.orders.length}
                  key={location.locationId}
                  label={location.locationName ?? `Location ${location.locationId}`}
                  labelCase="normal"
                  summary={
                    <span className="inline-flex items-center gap-2">
                      {outbid > 0 ? <span className="text-danger">{outbid} outbid</span> : null}
                      {problem ? (
                        <span className="text-warning" title={problem.detail}>
                          {problem.title}
                        </span>
                      ) : null}
                    </span>
                  }
                >
                  {location.orders.map((row) => (
                    <OperationalTableRow
                      accent={row.standing.status === "outbid" ? "blocking" : undefined}
                      cells={{
                        item: <ItemCell row={row} />,
                        status: <StatusCell standing={row.standing} />,
                        price: <PriceCell value={row.standing.price} />,
                        best: <BestCompetitorCell standing={row.standing} />,
                        gap: <GapCell standing={row.standing} />,
                        suggested: <SuggestedCell standing={row.standing} />,
                        modify: <ModifyCell nowMs={nowMs} standing={row.standing} />,
                        volume: <VolumeCell row={row} />,
                        cost: <CostBasisCell row={row} />,
                        margin: <MarginCell row={row} />,
                        expires: <ExpiresCell nowMs={nowMs} row={row} />,
                      }}
                      key={row.orderId}
                      rowKey={String(row.orderId)}
                    />
                  ))}
                </OperationalTableGroup>
              );
            })}
          </OperationalTable>
        )}
        {groups.flatMap((location) => {
          const problem = unavailableText(location);
          return problem && location.unavailable?.reason !== "noDataYet"
            ? [
                <InlineAlert key={location.locationId} title={`${problem.title}: ${location.locationName ?? `Location ${location.locationId}`}`} tone="warning">
                  {problem.detail}
                </InlineAlert>,
              ]
            : [];
        })}
      </div>
      {selected ? (
        <MarketOrderInspector
          location={selected.location}
          nowMs={nowMs}
          onClose={() => update((next) => next.delete("order"))}
          row={selected.row}
        />
      ) : null}
    </>
  );
}
