import { FileStack } from "lucide-react";
import type { ReactNode } from "react";
import { Link } from "react-router";

import type { BuildWorksheetRow, WorksheetBlueprintCoverage } from "../../../../api/industry";
import { CopyValueButton } from "../../../../components/copy-value-button";
import { PlannerInspectorShell } from "../../../../components/planner-inspector-shell";
import { blueprintBadgeLabel, syncedAgo, usesManualDespiteOwned } from "./blueprint-coverage-badge";

type CoveredRow = BuildWorksheetRow & { blueprint: WorksheetBlueprintCoverage };

export interface BlueprintPanelSections {
  /** Build rows with no blueprint, or too few runs for the job. */
  missing: CoveredRow[];
  /** Every blueprint owned (or of unknown kind), whether its row is built or bought. */
  owned: CoveredRow[];
  /** Newest observation across all holdings: the sync these are as of. */
  observedAt: string | null;
}

/** Splits the worksheet's rows into blueprints missing for planned builds and
 * blueprints owned, one entry per blueprint type. */
export function blueprintPanelSections(rows: BuildWorksheetRow[]): BlueprintPanelSections {
  const sections: BlueprintPanelSections = { missing: [], owned: [], observedAt: null };
  const seen = { missing: new Set<number>(), owned: new Set<number>() };
  const add = (key: keyof typeof seen, row: CoveredRow) => {
    if (seen[key].has(row.blueprint.blueprintTypeId)) return;
    seen[key].add(row.blueprint.blueprintTypeId);
    sections[key].push(row);
  };
  for (const row of rows) {
    const coverage = row.blueprint;
    if (!coverage) continue;
    const observedAt = coverage.holdings.observedAt;
    if (observedAt && (!sections.observedAt || observedAt > sections.observedAt)) sections.observedAt = observedAt;
    const entry = { ...row, blueprint: coverage };
    if (row.sourcing === "manufacturing" && (coverage.state === "notOwned" || coverage.state === "insufficientRuns")) {
      add("missing", entry);
    } else if (coverage.state === "owned" || coverage.state === "unknownKind") {
      add("owned", entry);
    }
  }
  // A blueprint missing on one row and owned on another reads as missing.
  sections.owned = sections.owned.filter((row) => !seen.missing.has(row.blueprint.blueprintTypeId));
  return sections;
}

export function BlueprintCoveragePanel({ onClose, onOpenProducer, open, rows }: {
  onClose: () => void;
  /** Opens a Build row's producer in the row inspector. */
  onOpenProducer: (row: BuildWorksheetRow) => void;
  open: boolean;
  rows: BuildWorksheetRow[];
}) {
  const sections = blueprintPanelSections(rows);
  const anyBlueprint = rows.some((row) => row.blueprint);
  return (
    <PlannerInspectorShell closeLabel="Close blueprints" dismissLabel="Dismiss blueprints" eyebrow="Blueprints" onClose={onClose} open={open} returnFocusSelector="[data-blueprint-chip]" title="Blueprint coverage">
      {sections.observedAt ? <p className="px-3 pb-2 text-xs text-muted">As of {syncedAgo(sections.observedAt)}</p> : null}
      {!anyBlueprint ? <PanelMessage icon={<FileStack aria-hidden="true" className="h-4 w-4 text-muted" />}>No components here can be built from a blueprint.</PanelMessage> : <>
        {sections.missing.length ? <Section
          action={<CopyValueButton label="Copy missing blueprint names" value={sections.missing.map((row) => row.blueprint.blueprintName).join("\n")}><span className="text-xs">Copy names</span></CopyValueButton>}
          count={sections.missing.length}
          title="Missing"
        >
          {sections.missing.map((row) => <Entry detail={missingDetail(row.blueprint)} key={row.blueprint.blueprintTypeId} onOpenProducer={() => onOpenProducer(row)} row={row} tone={row.blueprint.state === "notOwned" ? "destructive" : "warning"} />)}
        </Section> : null}
        <Section count={sections.owned.length} title="Owned">
          {sections.owned.length
            ? sections.owned.map((row) => <Entry detail={ownedDetail(row)} key={row.blueprint.blueprintTypeId} onOpenProducer={() => onOpenProducer(row)} row={row} />)
            : <PanelMessage icon={<FileStack aria-hidden="true" className="h-4 w-4 text-muted" />}>You own none of these blueprints.</PanelMessage>}
        </Section>
      </>}
    </PlannerInspectorShell>
  );
}

function missingDetail(coverage: WorksheetBlueprintCoverage): string {
  const runs = coverage.producer?.requiredRuns;
  const needs = runs == null ? null : `needs ${runs} ${runs === 1 ? "run" : "runs"}`;
  const owned = coverage.state === "insufficientRuns" ? `owned: BPC · ${coverage.holdings.copyRuns} ${coverage.holdings.copyRuns === 1 ? "run" : "runs"}` : "none";
  return [needs, owned].filter(Boolean).join(" · ");
}

function ownedDetail(row: CoveredRow): string {
  const label = blueprintBadgeLabel(row.blueprint);
  return usesManualDespiteOwned(row.blueprint, row.sourcing) ? `${label} · producer uses manual ME/TE` : label;
}

function Section({ action, children, count, title }: { action?: ReactNode; children: ReactNode; count: number; title: string }) {
  return (
    <section aria-label={title} className="border-t border-border">
      <header className="flex h-8 items-center gap-1 bg-panel-strong px-3">
        <span className="text-[10px] font-semibold uppercase tracking-wide text-muted">{title} ({count})</span>
        <span className="ml-auto">{action}</span>
      </header>
      {children}
    </section>
  );
}

const entryTone = { destructive: "text-destructive", warning: "text-warning", muted: "text-muted" } as const;

function Entry({ detail, onOpenProducer, row, tone = "muted" }: { detail: string; onOpenProducer?: () => void; row: CoveredRow; tone?: keyof typeof entryTone }) {
  const name = row.blueprint.blueprintName;
  return (
    <div className="group border-b border-border/60 px-3 py-2 last:border-b-0 hover:bg-panel-strong">
      <p className="truncate text-xs font-semibold text-foreground" title={name}>{name}</p>
      <div className="mt-0.5 flex items-baseline gap-3 text-xs">
        <p className={`min-w-0 flex-1 ${entryTone[tone]}`}>{detail}</p>
        <span className="flex shrink-0 gap-3 opacity-0 group-focus-within:opacity-100 group-hover:opacity-100">
          <Link className="text-primary hover:underline" to={`/builds/blueprints?search=${encodeURIComponent(name)}`}>Library</Link>
          {onOpenProducer && row.producerBuildId ? <button className="text-primary hover:underline" onClick={onOpenProducer} type="button">Producer</button> : null}
        </span>
      </div>
    </div>
  );
}

function PanelMessage({ children, icon }: { children: ReactNode; icon: ReactNode }) {
  return <p className="flex items-center gap-2 px-3 py-4 text-xs text-muted">{icon}{children}</p>;
}
