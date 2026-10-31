import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { Check, Clock, Pencil } from "lucide-react";

import type { PlannerResource } from "../../../../api/pi-planner";
import {
  cleanDensityInput,
  densitySegments,
  isStaleReading,
  isValidDensity,
  readingAgeDays,
  resourceCode,
} from "./planner-format";

/** Focuses the density input at this position in the table, across planets. */
export function focusDensity(index: number): void {
  const next = document.querySelector<HTMLInputElement>(`[data-density-index="${index}"]`);
  next?.focus();
  next?.select();
}

function SegmentBar({ pct, blank }: { pct: number | null; blank: boolean }) {
  const filled = densitySegments(pct);
  return (
    <span aria-hidden="true" className="flex h-1 w-full gap-px" data-segments={filled}>
      {Array.from({ length: 10 }, (_, index) => (
        <i
          className={`h-full min-w-0 flex-1 rounded-[1px] border ${
            index < filled
              ? "border-primary bg-primary"
              : blank
                ? "border-dashed border-border bg-transparent"
                : "border-border bg-panel-strong"
          }`}
          key={index}
        />
      ))}
    </span>
  );
}

/**
 * One raw resource's reading on one planet: the scan tooltip's density %,
 * or a head value from a draft program, which overrides it.
 */
export function DensityCell({
  highlighted = false,
  index,
  nowMs,
  onCommit,
  onEditHeadValue,
  resource,
  saved,
}: {
  /** Picked from "Scan these first": ringed until the next pick. */
  highlighted?: boolean;
  /** Position across the whole table, for Enter-to-next. */
  index: number;
  nowMs: number;
  /** A changed, valid density; `null` clears it. */
  onCommit: (densityPct: string | null) => void;
  onEditHeadValue: () => void;
  resource: PlannerResource;
  saved: boolean;
}) {
  const stored = resource.densityPct ?? "";
  const [text, setText] = useState(stored);
  useEffect(() => setText(stored), [stored]);
  // Escape blurs after reverting; that blur must not commit the typed value.
  const reverting = useRef(false);

  const blank = text === "";
  const invalid = !isValidDensity(text);
  const stale = isStaleReading(resource.updatedAt, nowMs);
  const fallback = Number(resource.defaultDensityPct);
  const shownPct = blank ? null : Number(text);
  const code = resourceCode(resource.name);

  const commit = () => {
    if (reverting.current) {
      reverting.current = false;
      return;
    }
    if (invalid || text === stored) return;
    onCommit(text === "" ? null : text);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    // Keep the table row's own Enter / arrow handling out of typing.
    event.stopPropagation();
    if (event.key === "Enter") {
      event.preventDefault();
      commit();
      focusDensity(index + 1);
    } else if (event.key === "Escape") {
      event.preventDefault();
      reverting.current = true;
      setText(stored);
      event.currentTarget.blur();
    } else if (event.key === "Delete") {
      event.preventDefault();
      setText("");
    }
  };

  const title = invalid
    ? "Density is 0–100 %"
    : resource.headValue !== null
      ? `${resource.name} · head value ${resource.headValue} overrides the density`
      : stale && resource.updatedAt
        ? `${resource.name} · scanned ${readingAgeDays(resource.updatedAt, nowMs)} days ago — hotspots move`
        : blank
          ? `${resource.name} · not scanned — using ${fallback} % for this security band`
          : `${resource.name} · Resource Density ${text} %`;

  return (
    <div
      className={`min-w-0 px-1 py-0.5 [container-type:inline-size] ${highlighted ? "bg-primary/15 ring-1 ring-inset ring-primary" : ""}`}
      data-highlighted={highlighted || undefined}
      data-reading={`${resource.typeId}`}
      data-testid="density-cell"
      title={title}
    >
      <div className="flex min-w-0 items-center gap-1 text-[0.65rem] leading-4">
        <span className="truncate text-muted @max-[119px]:hidden">{resource.name}</span>
        <span className="hidden font-mono text-muted @max-[119px]:inline">{code}</span>
        {stale ? (
          <Clock aria-label="Stale reading" className="ml-auto h-3 w-3 shrink-0 text-warning" />
        ) : null}
      </div>
      <div className="mt-0.5 grid grid-cols-[minmax(0,1fr)_3rem] items-center gap-1">
        <SegmentBar blank={blank && resource.headValue === null} pct={shownPct} />
        {resource.headValue !== null ? (
          <button
            aria-label={`${resource.name} head value ${resource.headValue}, edit`}
            className="flex h-5 w-12 items-center justify-end gap-0.5 rounded-sm border border-primary/50 bg-primary/10 px-1 font-mono text-[0.65rem] text-foreground"
            onClick={onEditHeadValue}
            type="button"
          >
            HV {Math.round(Number(resource.headValue))}
            <Pencil aria-hidden="true" className="h-2.5 w-2.5 shrink-0" />
          </button>
        ) : (
          <span className="relative w-12">
            <input
              aria-invalid={invalid || undefined}
              aria-label={`${resource.name} density percent`}
              className={`h-5 w-full rounded-sm border bg-background pl-0.5 pr-3 text-right font-mono text-[0.7rem] text-foreground outline-none placeholder:italic placeholder:text-faint focus:ring-2 focus:ring-primary/60 ${
                blank ? "border-dashed" : ""
              } ${invalid ? "border-destructive text-destructive" : "border-border"}`}
              data-density-index={index}
              inputMode="decimal"
              onBlur={commit}
              onChange={(event) => setText(cleanDensityInput(event.target.value))}
              onFocus={(event) => event.currentTarget.select()}
              onKeyDown={onKeyDown}
              placeholder={`~${fallback}`}
              value={text}
            />
            <span className="pointer-events-none absolute right-0.5 top-[3px] font-mono text-[0.6rem] text-faint">%</span>
            {saved ? (
              <Check aria-label="Saved" className="absolute -right-3.5 top-1 h-3 w-3 text-positive" />
            ) : null}
          </span>
        )}
      </div>
    </div>
  );
}
