import { useRef, useState } from "react";
import { createPortal } from "react-dom";

import type { WorksheetBlueprintCoverage, WorksheetSourcing } from "../../../../api/industry";

/** How the badge reads next to the row's sourcing word. */
export type BlueprintBadgeTone = "blocker" | "short" | "opportunity" | "calm" | "neutral";

const toneClass: Record<BlueprintBadgeTone, string> = {
  blocker: "border-destructive/60 bg-destructive/15 text-destructive",
  short: "border-warning/50 bg-warning/10 text-warning",
  opportunity: "border-primary/40 bg-primary/10 text-primary",
  calm: "border-border text-muted/70",
  neutral: "border-border bg-panel-strong text-muted",
};

export function blueprintBadgeLabel(coverage: WorksheetBlueprintCoverage): string {
  const { holdings, state } = coverage;
  if (state === "notOwned") return "No BP";
  if (state === "unknownKind") return "BP ?";
  if (state === "insufficientRuns") return `BPC · ${holdings.copyRuns}r < ${coverage.producer?.requiredRuns ?? "?"}`;
  const best = holdings.best;
  if (!best) return "BP";
  const efficiency = `${best.materialEfficiency}/${best.timeEfficiency}`;
  return best.kind === "original" ? `BPO ${efficiency}` : `BPC ${efficiency} · ${holdings.copyRuns}r`;
}

/** `inventory` is a Buy row fully covered from stock: nothing is bought, so
 * an owned blueprint isn't an opportunity there. */
export function blueprintBadgeTone(coverage: WorksheetBlueprintCoverage, sourcing: WorksheetSourcing, inventory = false): BlueprintBadgeTone {
  const { state } = coverage;
  if (state === "insufficientRuns") return "short";
  if (sourcing === "manufacturing") return state === "notOwned" ? "blocker" : "neutral";
  if (state === "notOwned") return "calm";
  if (state === "owned" && !inventory) return "opportunity";
  return "neutral";
}

/** You own a usable blueprint, but the producer plans on manual ME/TE. */
export function usesManualDespiteOwned(coverage: WorksheetBlueprintCoverage, sourcing: WorksheetSourcing): boolean {
  return sourcing === "manufacturing" && coverage.state === "owned" && coverage.producer?.source === "manual";
}

export function BlueprintCoverageBadge({ coverage, inventory = false, sourcing }: { coverage: WorksheetBlueprintCoverage; inventory?: boolean; sourcing: WorksheetSourcing }) {
  // Table cells clip overflow, so the tooltip is portalled and fixed under
  // the badge (same as the Planetary storage bars).
  const anchorRef = useRef<HTMLSpanElement>(null);
  const [anchor, setAnchor] = useState<DOMRect | null>(null);
  const show = () => setAnchor(anchorRef.current?.getBoundingClientRect() ?? null);
  const hide = () => setAnchor(null);
  const tone = blueprintBadgeTone(coverage, sourcing, inventory);
  const manual = usesManualDespiteOwned(coverage, sourcing);
  const label = blueprintBadgeLabel(coverage);
  return (
    <span
      aria-label={`${coverage.blueprintName}: ${label}${manual ? ", producer uses manual ME/TE" : ""}`}
      className={`inline-flex h-[18px] shrink-0 items-center gap-1 whitespace-nowrap border px-1.5 font-mono text-[0.65rem] outline-none focus-visible:ring-1 focus-visible:ring-primary ${toneClass[tone]} ${coverage.state === "unknownKind" ? "underline decoration-dotted underline-offset-2" : ""}`}
      data-blueprint-tone={tone}
      onBlur={hide}
      onFocus={show}
      onMouseEnter={show}
      onMouseLeave={hide}
      ref={anchorRef}
      role="group"
      tabIndex={0}
    >
      {label}
      {manual ? <span className="border-l border-current pl-1 font-sans text-[0.6rem] opacity-70">manual</span> : null}
      {anchor ? createPortal(
        <BlueprintCoverageTooltip
          coverage={coverage}
          left={Math.max(8, Math.min(anchor.left, window.innerWidth - 300))}
          sourcing={sourcing}
          top={anchor.bottom + 4}
        />,
        document.body,
      ) : null}
    </span>
  );
}

function BlueprintCoverageTooltip({ coverage, left, sourcing, top }: { coverage: WorksheetBlueprintCoverage; left: number; sourcing: WorksheetSourcing; top: number }) {
  const { holdings, producer } = coverage;
  const best = holdings.best;
  return (
    <div className="iw-panel-strong pointer-events-none fixed z-50 w-[18rem] border p-2.5 text-xs shadow-2xl" role="tooltip" style={{ left, top }}>
      <div className="border-b border-border pb-1.5">
        <p className="font-semibold text-foreground">{coverage.blueprintName}</p>
        <p className="text-muted">{ownedSummary(coverage)}</p>
      </div>
      <dl className="space-y-1.5 pt-1.5">
        {best ? <div>
          <dt className="text-muted">Best blueprint</dt>
          <dd>{bestLine(coverage)}</dd>
        </div> : null}
        {sourcing === "manufacturing" && producer ? <div>
          <dt className="text-muted">Planned job</dt>
          <dd>
            Job needs {producer.requiredRuns} {producer.requiredRuns === 1 ? "run" : "runs"} ·{" "}
            {producer.source === "manual"
              ? `uses manual ME ${producer.materialEfficiency ?? 0} / TE ${producer.timeEfficiency ?? 0}`
              : `uses owned blueprint ME ${producer.materialEfficiency ?? 0} / TE ${producer.timeEfficiency ?? 0}`}
          </dd>
        </div> : null}
        {holdings.observedAt ? <p className="pt-0.5 text-muted">As of {syncedAgo(holdings.observedAt)}</p> : null}
      </dl>
    </div>
  );
}

function ownedSummary({ holdings }: WorksheetBlueprintCoverage): string {
  const parts = [
    holdings.originals ? plural(holdings.originals, "original") : null,
    holdings.copies ? `${plural(holdings.copies, "copy", "copies")} · ${plural(holdings.copyRuns, "run")}` : null,
    holdings.unknown ? `${holdings.unknown} of unknown kind` : null,
  ].filter(Boolean);
  return parts.length ? parts.join(" · ") : "No blueprints owned";
}

function bestLine({ holdings }: WorksheetBlueprintCoverage): string {
  const best = holdings.best;
  if (!best) return "";
  const kind = best.kind === "original" ? "BPO" : best.kind === "copy" ? "BPC" : "Unknown kind";
  return [
    kind,
    `ME ${best.materialEfficiency}`,
    `TE ${best.timeEfficiency}`,
    best.kind === "copy" && best.licensedRuns != null ? plural(best.licensedRuns, "run") : null,
    best.locationName,
  ].filter(Boolean).join(" · ");
}

function plural(count: number, one: string, many = `${one}s`): string {
  return `${count} ${count === 1 ? one : many}`;
}

// Sync freshness, finer than the library's day-level formatRelativeAge.
export function syncedAgo(iso: string): string {
  const minutes = Math.floor((Date.now() - new Date(iso).getTime()) / 60_000);
  if (Number.isNaN(minutes)) return "unknown";
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  return `${Math.floor(hours / 24)} d ago`;
}
