import { ChevronDown, Coins, Package } from "lucide-react";
import { useId, useState } from "react";

import { costScope, type CostSection, type FitCostView } from "../../api/fittings";
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

/** A section's sum; a dash when nothing in it has a price. */
function Row({ label, value, priced }: { label: string; value: string; priced: boolean }) {
  return (
    <div className="flex items-baseline justify-between text-xs">
      <span className="text-muted">{label}</span>
      {priced ? <Isk value={value} /> : <span className="font-mono text-muted">—</span>}
    </div>
  );
}

/** The rail's Price section: the saved fit at the workspace market. */
export function PriceSection({ cost, error, stale }: { cost: FitCostView | null; error: string | null; stale: boolean }) {
  const scope = useMarketScopeLabel(cost ? costScope(cost) : null);
  const market = cost?.scope.locationId === null ? scope.regionName : scope.locationName;
  const unpriced = cost ? cost.lines.filter((line) => line.unitPrice === null) : [];
  const has = (section: CostSection) => cost?.lines.some((line) => line.section === section) ?? false;
  const priced = (section: CostSection) =>
    cost?.lines.some((line) => line.section === section && line.unitPrice !== null) ?? false;
  return (
    <Section icon={<Coins aria-hidden="true" className="h-3 w-3" />} title="Price">
      {error ? <p className="text-xs text-danger">{error}</p> : null}
      {!cost && !error ? <p className="text-xs text-muted">Loading prices…</p> : null}
      {cost ? (
        <>
          <Row label="Hull" priced={priced("hull")} value={cost.hull} />
          <Row label="Fittings" priced={priced("fittings")} value={cost.fittings} />
          {has("drones") ? <Row label="Drones" priced={priced("drones")} value={cost.drones} /> : null}
          {has("cargo") ? <Row label="Cargo" priced={priced("cargo")} value={cost.cargo} /> : null}
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
