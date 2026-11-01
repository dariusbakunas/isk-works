import { Fragment, useState } from "react";
import { Info } from "lucide-react";

import {
  MAX_RADIUS,
  type OutOfReachProduct,
  type PlannerRanking,
  type RankedProduct,
} from "../../../../api/pi-planner";
import { formatIskCompact, formatIskSummary } from "../../../../components/money";
import {
  OperationalTable,
  OperationalTableGroup,
  OperationalTableRow,
  type OperationalColumn,
} from "../../../../components/operational-table";
import { InlineAlert } from "../../../../components/primitives";

const TIERS = [1, 2, 3, 4] as const;

const COLUMNS: OperationalColumn[] = [
  { key: "product", label: "Product", width: "minmax(9rem,1fr)" },
  { key: "isk", label: "ISK/mo", width: "5rem", align: "right", numeric: true },
  { key: "colonies", label: "Colonies", width: "4.5rem", align: "right", numeric: true },
  { key: "effort", label: "Effort", title: "Logins and haul runs per week", width: "7.5rem" },
  { key: "inputs", label: "Inputs", width: "minmax(10rem,1.2fr)" },
  { key: "confidence", label: "Conf.", title: "How much rests on scans rather than defaults", width: "4rem" },
];

/** Actions the page can take for an out-of-reach reason or the empty result. */
export interface RankingActions {
  tryRadius: (radius: number) => void;
  addCharacter: () => void;
  allowBuying: () => void;
  showNotScanned: () => void;
  estimateUnscanned: () => void;
}

function TierChip({ tier }: { tier: number }) {
  return <span className="rounded-sm border border-border px-1 text-[0.65rem] text-muted">P{tier}</span>;
}

/** Four segments: the share of the chain resting on scans and head values. */
export function ConfidenceMeter({ confidence }: { confidence: number }) {
  const filled = Math.max(0, Math.min(4, Math.round(confidence * 4)));
  return (
    <span
      aria-label={`Confidence ${Math.round(confidence * 100)} %`}
      className="inline-flex gap-0.5"
      data-filled={filled}
      title={`${Math.round(confidence * 100)} % of the chain rests on scans or head values`}
    >
      {Array.from({ length: 4 }, (_, index) => (
        <i className={`h-1.5 w-3 ${index < filled ? "bg-positive" : "bg-border"}`} key={index} />
      ))}
    </span>
  );
}

function average(values: string[]): number {
  if (values.length === 0) return 0;
  return values.reduce((sum, value) => sum + Number(value), 0) / values.length;
}

function EffortCell({ product, haulDays }: { product: RankedProduct; haulDays: number }) {
  const logins = average(product.effort.loginsPerWeekByCharacter);
  const constrained = product.haulConstrainedDays;
  const detail = `${Math.round(Number(product.effort.m3PerRun)).toLocaleString("en-US")} m³ per haul run · ${
    product.effort.stopsPerRun
  } stops`;
  return (
    <span
      className="font-mono text-[0.7rem] leading-4"
      title={
        constrained === null
          ? detail
          : `Inputs overflow the skyhook at ${haulDays}-day hauls; this assumes hauling every ${constrained} days. ${detail}`
      }
    >
      <span className="block">{logins.toFixed(1)} logins ·</span>
      <span className={`block ${constrained === null ? "text-muted" : "text-warning"}`}>
        {Number(product.effort.haulRunsPerWeek).toFixed(1)} hauls /wk
      </span>
    </span>
  );
}

function InputsCell({ product }: { product: RankedProduct }) {
  const defaults = product.inputs.filter((input) => input.source === "default").length;
  return (
    <span
      className="block truncate text-xs text-muted"
      title={
        defaults === 0
          ? product.inputs.map((input) => `${input.resourceName} · ${input.planetName}`).join("\n")
          : `${defaults} of ${product.inputs.length} inputs use default densities`
      }
    >
      {product.inputs.map((input, index) => (
        <Fragment key={`${input.resourceTypeId}-${input.planetId}`}>
          {index > 0 ? " · " : null}
          <span
            className={input.source === "default" ? "border-b border-dashed border-muted" : undefined}
            data-unscanned={input.source === "default" || undefined}
          >
            {input.resourceName}
          </span>
        </Fragment>
      ))}
      {product.bought.length > 0 ? (
        <span className="text-faint"> · buys {product.bought.map((entry) => entry.name).join(", ")}</span>
      ) : null}
    </span>
  );
}

function reasonText(product: OutOfReachProduct, radius: number): string {
  const range = `within ${radius} ${radius === 1 ? "jump" : "jumps"}`;
  switch (product.reason.kind) {
    case "missingResource":
      return `Missing: ${product.reason.name} — no planet ${range} yields it`;
    case "notScanned":
      return `Not scanned: ${product.reason.name} — no planet ${range} has a reading`;
    case "noFactoryPlanet":
      return `No planet ${range} can host its factories`;
    case "needsColonies":
      return `Needs ${product.reason.needed} colonies — your characters have ${product.reason.available}`;
    case "overflowsSkyhook":
      return "Its inputs overflow a skyhook even with daily hauls";
    case "noRoom":
      return "A colony can't fit even one extractor head or factory";
    case "noPrice":
      return "No market price yet";
  }
}

function ReasonAction({
  actions,
  product,
  radius,
}: {
  actions: RankingActions;
  product: OutOfReachProduct;
  radius: number;
}) {
  const kind = product.reason.kind;
  if ((kind === "missingResource" || kind === "noFactoryPlanet") && radius < MAX_RADIUS) {
    return (
      <button className="text-xs text-primary hover:underline" onClick={() => actions.tryRadius(radius + 1)} type="button">
        Try {radius + 1} {radius + 1 === 1 ? "jump" : "jumps"}
      </button>
    );
  }
  if (kind === "notScanned") {
    return (
      <button className="text-xs text-primary hover:underline" onClick={actions.showNotScanned} type="button">
        Show not scanned
      </button>
    );
  }
  if (kind === "needsColonies") {
    return (
      <button className="text-xs text-primary hover:underline" onClick={actions.addCharacter} type="button">
        Add a character
      </button>
    );
  }
  return null;
}

/** Every product the planets in range support, best ISK/month first. */
export function RankingPanel({
  actions,
  estimating,
  haulDays,
  radius,
  ranking,
  refreshing,
  scannedPlanets,
}: {
  actions: RankingActions;
  /** Unscanned resources are planned at a default density. */
  estimating: boolean;
  haulDays: number;
  radius: number;
  ranking: PlannerRanking;
  refreshing: boolean;
  scannedPlanets: number;
}) {
  const [tiers, setTiers] = useState<Set<number>>(new Set(TIERS));
  const [outOfReachOpen, setOutOfReachOpen] = useState(false);
  const toggleTier = (tier: number) =>
    setTiers((current) => {
      const next = new Set(current);
      if (next.has(tier)) next.delete(tier);
      else next.add(tier);
      return next;
    });

  const ranked = ranking.ranked.filter((product) => tiers.has(product.tier));
  const outOfReach = ranking.outOfReach.filter((product) => tiers.has(product.tier));
  const range = `${radius} ${radius === 1 ? "jump" : "jumps"}`;

  return (
    <section aria-busy={refreshing || undefined} aria-label="Ranking" className="iw-panel min-w-0">
      <div className="flex flex-wrap items-center gap-2 border-b border-border p-2">
        <div aria-label="Tiers" className="flex gap-1" role="group">
          {TIERS.map((tier) => (
            <button
              aria-pressed={tiers.has(tier)}
              className={`h-7 rounded-sm border px-2 text-xs ${
                tiers.has(tier) ? "border-primary/60 bg-primary/15 text-primary" : "border-border text-muted"
              }`}
              key={tier}
              onClick={() => toggleTier(tier)}
              type="button"
            >
              P{tier}
            </button>
          ))}
        </div>
        <span
          className="ml-auto inline-flex items-center gap-1 rounded-sm border border-border px-1.5 py-0.5 text-[0.65rem] uppercase text-muted"
          title={`Yields are estimated from scan densities. ${
            estimating ? "Blank resources use a low default for the security band." : "Blank resources are left out."
          } Prices are best buy in your market scope.`}
        >
          <Info aria-hidden="true" className="h-3 w-3" />
          Estimates · {ranking.availableColonies} colonies
        </span>
      </div>
      {scannedPlanets === 0 ? (
        <div className="p-2">
          <InlineAlert title="No planets scanned yet" tone="info">
            {estimating
              ? "Every resource uses a low default. Scan a few planets to sharpen this. "
              : "Only scanned resources are planned. Enter the densities you read in game, or estimate unscanned planets with a low default. "}
            <button className="underline" onClick={actions.showNotScanned} type="button">
              Show not scanned
            </button>
            {estimating ? null : (
              <>
                {" · "}
                <button className="underline" onClick={actions.estimateUnscanned} type="button">
                  Estimate unscanned planets
                </button>
              </>
            )}
          </InlineAlert>
        </div>
      ) : null}
      {ranking.ranked.length === 0 && (estimating || scannedPlanets > 0) ? (
        <div className="grid place-items-center p-6 text-center">
          <h2 className="text-sm font-semibold">Nothing can be built within {range} with these characters.</h2>
          <div className="mt-3 flex flex-wrap justify-center gap-2">
            {radius < MAX_RADIUS ? (
              <button className="iw-button-primary" onClick={() => actions.tryRadius(radius + 1)} type="button">
                Try {radius + 1} {radius + 1 === 1 ? "jump" : "jumps"}
              </button>
            ) : null}
            <button className="iw-button-secondary" onClick={actions.allowBuying} type="button">
              Allow buying intermediates
            </button>
          </div>
        </div>
      ) : null}
      <OperationalTable ariaLabel="Products" columns={COLUMNS} onSelectRow={() => {}} selectedRowKey={null}>
        <OperationalTableGroup
          groupKey="ranked"
          itemCount={ranked.length}
          label="In reach"
          emptyMessage={ranking.ranked.length === 0 ? "None" : "No products in the selected tiers"}
        >
          {ranked.map((product) => (
            <OperationalTableRow
              cells={{
                product: (
                  <span className="flex min-w-0 items-center gap-1.5">
                    <span className="truncate text-sm text-foreground">{product.name}</span>
                    <TierChip tier={product.tier} />
                  </span>
                ),
                isk: (
                  <span className="font-mono font-semibold" title={`${formatIskSummary(product.iskPerMonth)} ISK/mo`}>
                    {formatIskCompact(product.iskPerMonth)}
                  </span>
                ),
                colonies: (
                  <span className="font-mono text-xs" title={`${product.chains} ${product.chains === 1 ? "chain" : "chains"}`}>
                    {product.colonies}
                  </span>
                ),
                effort: <EffortCell haulDays={haulDays} product={product} />,
                inputs: <InputsCell product={product} />,
                confidence: <ConfidenceMeter confidence={Number(product.confidence)} />,
              }}
              interactive={false}
              key={product.typeId}
              rowKey={String(product.typeId)}
            />
          ))}
        </OperationalTableGroup>
        <OperationalTableGroup
          expanded={outOfReachOpen}
          groupKey="out-of-reach"
          itemCount={outOfReach.length}
          label="Out of reach"
          onExpandedChange={setOutOfReachOpen}
        >
          {outOfReach.map((product) => (
            <OperationalTableRow
              cells={{
                product: (
                  <span className="flex min-w-0 items-center gap-1.5">
                    <span className="truncate text-sm text-muted">{product.name}</span>
                    <TierChip tier={product.tier} />
                  </span>
                ),
                isk: (
                  <span className="font-mono text-faint" title={reasonText(product, radius)}>
                    —
                  </span>
                ),
                colonies: null,
                effort: null,
                inputs: <span className="block truncate text-xs text-muted">{reasonText(product, radius)}</span>,
                confidence: <ReasonAction actions={actions} product={product} radius={radius} />,
              }}
              interactive={false}
              key={product.typeId}
              rowKey={`out-${product.typeId}`}
            />
          ))}
        </OperationalTableGroup>
      </OperationalTable>
    </section>
  );
}
