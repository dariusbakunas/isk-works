import { useRef, useState } from "react";
import { createPortal } from "react-dom";
import { AlertTriangle, CheckSquare, Clock, Square } from "lucide-react";

import type { Planet, PlanetExport, PlanetStorage } from "../../../api/planetary";
import { formatIskCompact, formatIskSummary } from "../../../components/money";
import {
  countdown,
  formatHours,
  formatQuantity,
  importTone,
  PLANET_TYPES,
  securityTone,
  storageTone,
  toneText,
  type Tone,
} from "./planetary-format";

// One sub-line per import/export so the Exports, Excl and ISK/mo columns
// line up row-for-row (fixed sub-line height).
const SUB_LINE = "flex h-5 items-center gap-1.5";

const barFill: Record<Tone, string> = {
  ok: "bg-primary",
  warn: "bg-warning",
  danger: "bg-destructive",
};

function PlanetTypeChip({ type }: { type: string }) {
  const meta = PLANET_TYPES[type] ?? {
    short: type.slice(0, 1).toUpperCase() || "?",
    label: type,
    color: "#8a97ab",
    background: "#131820",
  };
  return (
    <span
      aria-label={meta.label}
      className="inline-flex h-[17px] w-[17px] shrink-0 items-center justify-center rounded-sm border text-[0.55rem] font-bold"
      style={{ background: meta.background, borderColor: `${meta.color}44`, color: meta.color }}
      title={meta.label}
    >
      {meta.short}
    </span>
  );
}

export function PlanetNameCell({ planet, stale }: { planet: Planet; stale: boolean }) {
  const security = planet.security === null ? null : Number(planet.security);
  const securityClass = security === null
    ? "text-muted"
    : { high: "text-positive", low: "text-warning", null: "text-destructive" }[securityTone(security)];
  return (
    <div className="flex min-w-0 flex-col gap-1">
      <div className="flex min-w-0 items-center gap-1.5">
        <PlanetTypeChip type={planet.planetType} />
        <span className="truncate font-semibold text-foreground">{planet.name}</span>
      </div>
      <div className="flex items-center gap-1 text-[0.65rem]">
        <span className="text-muted">{planet.solarSystemName ?? `System ${planet.solarSystemId}`}</span>
        {security !== null ? <span className={`font-mono tabular-nums ${securityClass}`}>{security.toFixed(2)}</span> : null}
        {stale ? (
          <span className="rounded-sm border border-warning/40 bg-warning/10 px-1 text-warning">stale</span>
        ) : null}
      </div>
    </div>
  );
}

export function CcBadge({ level }: { level: number }) {
  const tone = level >= 5 ? "text-primary" : level >= 4 ? "text-positive" : level >= 3 ? "text-warning" : "text-muted";
  return <span className={`font-mono text-xs font-bold tabular-nums ${tone}`}>CC{level}</span>;
}

function CountdownChip({ at, nowMs }: { at: string; nowMs: number }) {
  const value = countdown(at, nowMs);
  const Icon = value.expired ? AlertTriangle : Clock;
  return (
    <span className={`inline-flex items-center gap-1 font-mono text-xs tabular-nums ${toneText[value.tone]}`} title={value.absolute}>
      <Icon aria-hidden="true" className="h-3 w-3" />
      <span className={value.expired ? "font-bold" : undefined}>{value.label}</span>
    </span>
  );
}

export function ExtractionCell({ planet, nowMs }: { planet: Planet; nowMs: number }) {
  if (planet.extractors.length === 0) {
    return <span className="text-xs italic text-muted">Factory only</span>;
  }
  return (
    <div className="flex flex-col gap-1">
      {planet.extractors.map((extractor) => (
        <div className="flex flex-col" key={extractor.pinId}>
          {extractor.expiresAt ? <CountdownChip at={extractor.expiresAt} nowMs={nowMs} /> : null}
          <span className="text-[0.65rem] text-muted">{extractor.productName}</span>
        </div>
      ))}
    </div>
  );
}

export function ProductionCell({ planet }: { planet: Planet }) {
  if (planet.production.length === 0) return <span className="text-muted">—</span>;
  return (
    <div className="flex flex-col gap-1">
      {planet.production.map((production) => (
        <span
          className="text-xs"
          key={`${production.schematicId}:${production.outputTypeId}`}
          title={`${production.factoryCount} ${production.factoryCount === 1 ? "factory" : "factories"}`}
        >
          {production.name}
        </span>
      ))}
    </div>
  );
}

export function ImportsCell({ planet }: { planet: Planet }) {
  if (planet.imports.length === 0) return <div className={SUB_LINE}><span className="text-muted">—</span></div>;
  return (
    <div className="flex w-full flex-col">
      {planet.imports.map((item) => {
        const lasts = Number(item.lastsHours);
        const tone = importTone(lasts);
        return (
          <div className={SUB_LINE} key={item.typeId}>
            <span className="min-w-0 flex-1 truncate text-xs">{item.name}</span>
            <span className="shrink-0 font-mono text-[0.65rem] tabular-nums text-muted">
              {formatQuantity(Number(item.qtyPerHour))}/h
            </span>
            <span
              className={`w-11 shrink-0 text-right font-mono text-[0.65rem] tabular-nums ${toneText[tone]} ${tone === "ok" ? "" : "font-bold"}`}
              title={`Lasts ${lasts.toFixed(1)}h`}
            >
              {formatHours(lasts)}
            </span>
          </div>
        );
      })}
    </div>
  );
}

export function ExportsCell({ planet }: { planet: Planet }) {
  if (planet.exports.length === 0) return <div className={SUB_LINE}><span className="text-muted">—</span></div>;
  return (
    <div className="flex w-full flex-col">
      {planet.exports.map((item) => (
        <div className={SUB_LINE} key={item.typeId}>
          <span className="min-w-0 flex-1 truncate text-xs">{item.name}</span>
          <span className="shrink-0 font-mono text-[0.65rem] tabular-nums text-muted">
            {formatQuantity(Number(item.unitsPerHour))}/h
          </span>
        </div>
      ))}
    </div>
  );
}

export function ExcludedCell({
  planet,
  onToggle,
}: {
  planet: Planet;
  onToggle: (item: PlanetExport) => void;
}) {
  return (
    <div className="flex flex-col items-center">
      {planet.exports.map((item) => (
        <div className={SUB_LINE} key={item.typeId}>
          <button
            aria-label={`${item.excluded ? "Include" : "Exclude"} ${item.name} on ${planet.name} ${item.excluded ? "in" : "from"} totals`}
            aria-pressed={item.excluded}
            className={item.excluded ? "text-muted hover:text-foreground" : "text-primary hover:text-foreground"}
            onClick={(event) => {
              event.stopPropagation();
              onToggle(item);
            }}
            title={item.excluded ? "Excluded from totals" : "Included in totals"}
            type="button"
          >
            {item.excluded
              ? <Square aria-hidden="true" className="h-3.5 w-3.5" />
              : <CheckSquare aria-hidden="true" className="h-3.5 w-3.5" />}
          </button>
        </div>
      ))}
    </div>
  );
}

export function IskCell({ planet }: { planet: Planet }) {
  return (
    <div className="flex w-full flex-col items-end">
      {planet.exports.map((item) => (
        <div className={SUB_LINE} key={item.typeId}>
          {item.iskPerMonth === null ? (
            <span className="text-muted" title="No buy orders in the valuation market">—</span>
          ) : (
            <span
              className={`font-mono text-xs tabular-nums ${item.excluded ? "text-muted line-through" : "text-positive"}`}
              title={`${formatIskSummary(item.iskPerMonth)} / month${item.excluded ? " (excluded)" : ""}`}
            >
              {formatIskCompact(item.iskPerMonth)}
            </span>
          )}
        </div>
      ))}
    </div>
  );
}

const storageLabel: Record<PlanetStorage["kind"], string> = {
  L: "Launchpad",
  S: "Storage",
  C: "Command center",
};

function StorageBar({ storage }: { storage: PlanetStorage }) {
  // Table cells clip overflow, so the contents popover is portalled and
  // fixed-positioned under the bar.
  const anchorRef = useRef<HTMLDivElement>(null);
  const [anchor, setAnchor] = useState<DOMRect | null>(null);
  const open = anchor !== null;
  const show = () => setAnchor(anchorRef.current?.getBoundingClientRect() ?? null);
  const hide = () => setAnchor(null);
  const percent = Number(storage.fillPercent);
  const tone = storageTone(percent);
  return (
    <div
      aria-label={`${storageLabel[storage.kind]} ${percent.toFixed(1)}% full`}
      className="relative flex w-full items-center gap-1.5 outline-none focus-visible:ring-1 focus-visible:ring-primary"
      onBlur={hide}
      onFocus={show}
      onMouseEnter={show}
      onMouseLeave={hide}
      ref={anchorRef}
      role="group"
      tabIndex={0}
    >
      <span className="w-2.5 shrink-0 text-[0.6rem] font-bold text-muted">{storage.kind}</span>
      <span className="relative h-1.5 min-w-10 flex-1 overflow-hidden rounded-[1px] bg-panel-strong">
        <span
          className={`absolute inset-y-0 left-0 opacity-80 ${barFill[tone]}`}
          style={{ width: `${Math.min(100, Math.max(0, percent))}%` }}
        />
      </span>
      <span className={`w-10 shrink-0 text-right font-mono text-[0.65rem] tabular-nums ${tone === "ok" ? "text-muted" : toneText[tone]}`}>
        {percent.toFixed(1)}%
      </span>
      <span className="w-12 shrink-0 text-right font-mono text-[0.65rem] tabular-nums text-muted" title={formatIskSummary(storage.value)}>
        {formatIskCompact(storage.value)}
      </span>
      {open ? createPortal(
        <div
          className="iw-panel-strong pointer-events-none fixed z-50 min-w-60 border p-2 shadow-2xl"
          role="tooltip"
          style={{ left: Math.max(8, Math.min(anchor.left, window.innerWidth - 300)), top: anchor.bottom + 4 }}
        >
          <div className="mb-1.5 flex justify-between border-b border-border pb-1 text-[0.65rem] font-bold">
            <span>{storageLabel[storage.kind]} contents</span>
            <span className={`font-mono tabular-nums ${tone === "ok" ? "text-primary" : toneText[tone]}`}>
              {Number(storage.usedM3).toLocaleString("en-US", { maximumFractionDigits: 0 })} /{" "}
              {Number(storage.capacityM3).toLocaleString("en-US")} m³
            </span>
          </div>
          {storage.contents.length === 0 ? (
            <p className="text-[0.65rem] text-muted">Empty</p>
          ) : (
            <div className="grid grid-cols-[1fr_64px_48px_56px] gap-x-1.5 gap-y-0.5 text-[0.65rem]">
              <span className="text-muted">Item</span>
              <span className="text-right text-muted">Qty</span>
              <span className="text-right text-muted">m³</span>
              <span className="text-right text-muted">Value</span>
              {storage.contents.map((item) => (
                <div className="contents" key={item.typeId}>
                  <span className="truncate">{item.name}</span>
                  <span className="text-right font-mono tabular-nums">{item.quantity.toLocaleString("en-US")}</span>
                  <span className="text-right font-mono tabular-nums text-muted">
                    {Number(item.volumeM3).toLocaleString("en-US", { maximumFractionDigits: 0 })}
                  </span>
                  <span className="text-right font-mono tabular-nums">
                    {item.value === null ? "—" : formatIskCompact(item.value)}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>,
        document.body,
      ) : null}
    </div>
  );
}

export function StorageCell({ planet }: { planet: Planet }) {
  if (planet.storage.length === 0) return <span className="text-muted">—</span>;
  return (
    <div className="flex w-full flex-col gap-1">
      {planet.storage.map((storage) => <StorageBar key={storage.pinId} storage={storage} />)}
    </div>
  );
}
