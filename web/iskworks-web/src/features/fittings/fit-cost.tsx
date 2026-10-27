import { ChevronDown, Coins, Package } from "lucide-react";
import { useId, useState } from "react";

import { costScope, type FitAssetsView, type FitCostView } from "../../api/fittings";
import { formatIskCompact, formatIskSummary } from "../../components/money";
import { useMarketScopeLabel } from "../../hooks/use-market-scope-label";
import { PRIVATE_ATTR } from "../../observability/private";
import { Section } from "./stats-rail";

/** Lowest sell per unit, by type, for the module table. */
export function unitPrices(cost: FitCostView | null): Map<number, string> | null {
  if (!cost) return null;
  return new Map(cost.lines.flatMap((line) => (line.unitPrice === null ? [] : [[line.typeId, line.unitPrice] as const])));
}

function Isk({ value, className = "" }: { value: string; className?: string }) {
  return (
    <span className={`font-mono ${className}`} title={formatIskSummary(value)}>
      {formatIskCompact(value)}
    </span>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline justify-between text-xs">
      <span className="text-muted">{label}</span>
      <Isk value={value} />
    </div>
  );
}

/** The rail's Price section: the saved fit at the workspace market. */
export function PriceSection({ cost, error, stale }: { cost: FitCostView | null; error: string | null; stale: boolean }) {
  const scope = useMarketScopeLabel(cost ? costScope(cost) : null);
  const market = cost?.scope.locationId === null ? scope.regionName : scope.locationName;
  const unpriced = cost ? cost.lines.filter((line) => line.unitPrice === null) : [];
  const has = (section: string) => cost?.lines.some((line) => line.section === section) ?? false;
  return (
    <Section icon={<Coins aria-hidden="true" className="h-3 w-3" />} title="Price">
      {error ? <p className="text-xs text-danger">{error}</p> : null}
      {!cost && !error ? <p className="text-xs text-muted">Loading prices…</p> : null}
      {cost ? (
        <>
          <Row label="Hull" value={cost.hull} />
          <Row label="Fittings" value={cost.fittings} />
          {has("drones") ? <Row label="Drones" value={cost.drones} /> : null}
          {has("cargo") ? <Row label="Cargo" value={cost.cargo} /> : null}
          <div className="flex items-baseline justify-between pt-1">
            <span className="text-xs text-muted">Total</span>
            <Isk className="text-lg font-semibold" value={cost.total} />
          </div>
          <p className="text-[0.625rem] text-muted">
            Lowest sell at <span data-testid="price-market">{market}</span>
            {stale ? " · saved fit" : ""}
          </p>
          {unpriced.length ? (
            <p
              className="text-[0.625rem] text-warning"
              title={unpriced.map((line) => line.name ?? `Type ${line.typeId}`).join(", ")}
            >
              {unpriced.length === 1 ? "1 item unpriced" : `${unpriced.length} items unpriced`}, not in the total
            </p>
          ) : null}
        </>
      ) : null}
    </Section>
  );
}

/** Where the workspace's characters already hold the fit, best covered
 * first, each expandable to what's short there. */
export function AssetsPanel({ assets, error }: { assets: FitAssetsView | null; error: string | null }) {
  const headingId = useId();
  const [expanded, setExpanded] = useState<string | null>(null);
  return (
    <section aria-labelledby={headingId} className="iw-panel">
      <div className="flex items-center gap-2 border-b border-border px-3 py-2">
        <Package aria-hidden="true" className="h-3.5 w-3.5 text-primary" />
        <h2 className="text-xs font-semibold uppercase tracking-wide" id={headingId}>Assets</h2>
        <span className="ml-auto hidden text-[0.625rem] text-muted sm:inline">What your characters already own, by location</span>
      </div>
      {error ? <p className="px-3 py-2 text-xs text-danger">{error}</p> : null}
      {!assets && !error ? <p className="px-3 py-2 text-xs text-muted">Loading assets…</p> : null}
      {assets && !assets.locations.length ? (
        <p className="px-3 py-2 text-xs text-muted">None of your characters hold any of this fit.</p>
      ) : null}
      {assets?.locations.map((location) => {
        const key = `${location.connectionId}:${location.locationId}`;
        const complete = location.short.length === 0;
        return (
          <div key={key}>
            <div className="flex items-stretch border-b border-border">
              <div className="flex min-h-9 flex-1 items-center gap-2 px-3 py-1 text-xs">
                <span className="min-w-0 flex-1 truncate">
                  <span {...PRIVATE_ATTR}>{location.characterName}</span>
                  <span className="text-muted"> · {location.locationName}</span>
                </span>
                <span className={`font-mono ${complete ? "text-positive" : "text-muted"}`}>
                  {location.covered} / {location.needed} items
                </span>
              </div>
              {complete ? (
                <span className="w-[30px]" />
              ) : (
                <button
                  aria-expanded={expanded === key}
                  aria-label={`What's short at ${location.locationName} for ${location.characterName}`}
                  className="px-2 text-muted hover:text-foreground"
                  onClick={() => setExpanded((current) => (current === key ? null : key))}
                  type="button"
                >
                  <ChevronDown aria-hidden="true" className={`h-3.5 w-3.5 transition ${expanded === key ? "rotate-180" : ""}`} />
                </button>
              )}
            </div>
            {expanded === key ? (
              <ul
                aria-label={`Short at ${location.locationName}`}
                className="space-y-1 border-b border-border bg-background/40 px-3 py-2 text-xs md:pl-6"
              >
                {location.short.map((shortfall) => (
                  <li className="flex items-center justify-between gap-2" key={shortfall.typeId}>
                    <span>{shortfall.name ?? `Type ${shortfall.typeId}`}</span>
                    <span className="font-mono text-muted">
                      {shortfall.have.toLocaleString("en-US")} / {shortfall.needed.toLocaleString("en-US")}
                    </span>
                  </li>
                ))}
              </ul>
            ) : null}
          </div>
        );
      })}
    </section>
  );
}
