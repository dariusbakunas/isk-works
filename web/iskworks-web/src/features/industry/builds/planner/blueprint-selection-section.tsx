import { ChevronDown, X } from "lucide-react";
import { useEffect, useId, useRef, useState, type KeyboardEvent, type ReactNode } from "react";

import type { BlueprintObservation } from "../../../../api/industry";
import { EveTypeImage } from "../../../../components/eve-type-image";
import { EmptyState, Panel } from "../../../../components/primitives";
import { Field } from "../../shared/field";
import { formatDate, splitCamel } from "../../shared/formatting";
import { jobCount } from "./job-split";

export interface BlueprintSelectionFieldsProps {
  mode: "manual" | "observedAsset";
  kind: "original" | "copy";
  me: string;
  te: string;
  licensedRuns: string;
  notes: string;
  observations: BlueprintObservation[];
  selectedObservationId: string;
  requiredRuns: number;
  onMode: (value: "manual" | "observedAsset") => void;
  onKind: (value: "original" | "copy") => void;
  onMe: (value: string) => void;
  onTe: (value: string) => void;
  onLicensedRuns: (value: string) => void;
  onNotes: (value: string) => void;
  onObservation: (value: string) => void;
  /** Fired when an edited manual field loses focus -- lets a host that
   * persists on a drop-prone path (the inspector's Build-ID PATCH) commit
   * once per edit instead of per keystroke. The dialog omits it. */
  onCommit?: () => void;
}

/**
 * The blueprint source control -- segmented "Enter manually" / "Use available
 * blueprint" toggle, the manual ME/TE/kind/runs grid, and the observed-asset
 * picker. Shared verbatim by the standalone `BlueprintSelectionSection`
 * (Choose Blueprint dialog) and the canonical inspector's Blueprint section
 * so both offer the same selection UX.
 */
export function BlueprintSelectionFields({
  mode, kind, me, te, licensedRuns, notes, observations, selectedObservationId,
  requiredRuns, onMode, onKind, onMe, onTe, onLicensedRuns, onNotes, onObservation,
  onCommit,
}: BlueprintSelectionFieldsProps) {
  return (
    <>
      <div className="mt-3 inline-flex rounded-md border border-border p-1" role="group" aria-label="Blueprint source">
        <button aria-pressed={mode === "manual"} className={mode === "manual" ? "iw-button-secondary bg-elevated text-foreground" : "iw-button-secondary border-transparent"} onClick={() => onMode("manual")} type="button">Enter manually</button>
        <button aria-pressed={mode === "observedAsset"} className={mode === "observedAsset" ? "iw-button-secondary bg-elevated text-foreground" : "iw-button-secondary border-transparent"} onClick={() => onMode("observedAsset")} type="button">Use available blueprint</button>
      </div>
      {mode === "manual" ? (
        <>
          <p className="iw-muted mt-3">Use manual assumptions for hypothetical planning or when ESI does not expose the blueprint.</p>
          {/* Two columns at every width: the inspector rail is narrow even on
              wide screens, and bottom-aligning keeps the inputs on one line
              when a label wraps. */}
          <div className="mt-3 grid grid-cols-2 items-end gap-3">
            <Field label="Material Efficiency (0-10)" value={me} onChange={onMe} onBlur={onCommit} inputMode="numeric" />
            <Field label="Time Efficiency (0-20)" value={te} onChange={onTe} onBlur={onCommit} inputMode="numeric" />
            <label className="block">
              <span className="mb-1 block text-sm font-semibold">Blueprint kind</span>
              <select className="iw-input" value={kind} onChange={(event) => onKind(event.target.value as "original" | "copy")}>
                <option value="original">Original</option><option value="copy">Copy</option>
              </select>
            </label>
            {kind === "copy" ? <Field label="Licensed runs" value={licensedRuns} onChange={onLicensedRuns} onBlur={onCommit} inputMode="numeric" /> : null}
          </div>
          {kind === "copy" ? <ManualJobSplitHint licensedRuns={licensedRuns} requiredRuns={requiredRuns} /> : null}
          <Field className="mt-3" label="Blueprint assumption notes" value={notes} onChange={onNotes} onBlur={onCommit} multiline />
        </>
      ) : observations.length === 0 ? (
        <EmptyState title="No available blueprints">Blueprints found during your latest ESI synchronization.</EmptyState>
      ) : (
        <div className="mt-3">
          <p className="iw-muted mb-1.5 text-xs">Blueprints found during your latest ESI synchronization.</p>
          <BlueprintObservationPicker
            observations={observations}
            onSelect={onObservation}
            requiredRuns={requiredRuns}
            selectedId={selectedObservationId}
          />
        </div>
      )}
      <p className="iw-muted mt-3 text-xs">Selection records immutable planning provenance. It does not reserve, lock, consume, or decrement a blueprint.</p>
    </>
  );
}

function plural(count: number, noun: string, nouns = `${noun}s`) {
  return `${count} ${count === 1 ? noun : nouns}`;
}

function ManualJobSplitHint({ licensedRuns, requiredRuns }: { licensedRuns: string; requiredRuns: number }) {
  const runs = Number(licensedRuns);
  if (!Number.isInteger(runs) || runs <= 0 || requiredRuns <= runs) return null;
  return (
    <p className="iw-muted mt-2 text-xs">
      {`${plural(requiredRuns, "run")} plan as ${plural(jobCount(requiredRuns, runs), "job")} of up to ${plural(runs, "run")}.`}
    </p>
  );
}

/** Copies another copy can stand in for: same kind and ME/TE, and at least
 * as many licensed runs. The list is already one owner's one blueprint type. */
function identicalCopies(observation: BlueprintObservation, observations: BlueprintObservation[]) {
  return observations.filter(
    (other) =>
      other.kind === "copy" &&
      other.materialEfficiency === observation.materialEfficiency &&
      other.timeEfficiency === observation.timeEfficiency &&
      other.licensedRuns !== null &&
      observation.licensedRuns !== null &&
      other.licensedRuns >= observation.licensedRuns,
  ).length;
}

function CopyRunsSummary({
  observation,
  observations,
  requiredRuns,
}: {
  observation: BlueprintObservation;
  observations: BlueprintObservation[];
  requiredRuns: number;
}) {
  const licensed = observation.licensedRuns;
  if (licensed === null || licensed <= 0) {
    return <span className="block truncate text-muted">Unknown licensed runs · planned as 1 job</span>;
  }
  if (requiredRuns <= licensed) {
    return <span className="block truncate text-muted">{`${licensed} licensed runs · ${requiredRuns} required · 1 job`}</span>;
  }
  const jobs = jobCount(requiredRuns, licensed);
  const identical = identicalCopies(observation, observations);
  const text = `${plural(licensed, "run")} per copy · ${plural(jobs, "job")} · ${plural(identical, "identical copy", "identical copies")}`;
  return (
    <span
      className={`block truncate ${identical >= jobs ? "text-muted" : "text-warning"}`}
      title={identical >= jobs ? text : `${text} -- ${jobs} needed to run every job`}
    >
      {text}
    </span>
  );
}

function ObservationSummary({
  observation,
  observations,
  requiredRuns,
}: {
  observation: BlueprintObservation;
  observations: BlueprintObservation[];
  requiredRuns: number;
}) {
  const where = `${observation.ownerName} · ${observation.locationName ?? `Location ${observation.locationId}`} · synced ${formatDate(observation.observedAt)}`;
  return (
    <span className="flex min-w-0 flex-1 items-start gap-2 text-left">
      <EveTypeImage
        size={32}
        typeId={observation.blueprintTypeId}
        typeName={observation.blueprintName}
        variation={observation.kind === "copy" ? "bpc" : "bp"}
      />
      <span className="min-w-0 flex-1 text-xs leading-snug">
        <strong className="block truncate text-foreground" title={`${observation.blueprintName} ${splitCamel(observation.kind)}`}>
          {observation.blueprintName} {splitCamel(observation.kind)}
        </strong>
        {observation.kind === "copy" ? (
          <>
            <span className="block">ME {observation.materialEfficiency} · TE {observation.timeEfficiency}</span>
            <CopyRunsSummary observation={observation} observations={observations} requiredRuns={requiredRuns} />
          </>
        ) : (
          <span className="block">
            <span>ME {observation.materialEfficiency} · TE {observation.timeEfficiency}</span>
            <span className="text-muted"> · Unlimited runs</span>
          </span>
        )}
        <span className="block truncate text-muted" title={where}>{where}</span>
      </span>
    </span>
  );
}

/**
 * Select-only combobox over the observed blueprints: one compact row when
 * closed, a scrollable list when open, so ten copies don't push the rest of
 * the inspector off screen. A copy with fewer licensed runs than required is
 * still a valid choice: the planner splits the runs into one job per copy.
 */
function BlueprintObservationPicker({
  observations,
  onSelect,
  requiredRuns,
  selectedId,
}: {
  observations: BlueprintObservation[];
  onSelect: (id: string) => void;
  requiredRuns: number;
  selectedId: string;
}) {
  const listId = useId();
  const [open, setOpen] = useState(false);
  const [activeIndex, setActiveIndex] = useState(-1);
  const rootRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const selected = observations.find((observation) => observation.id === selectedId) ?? null;

  useEffect(() => {
    if (!open) return;
    function handlePointerDown(event: PointerEvent) {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    }
    document.addEventListener("pointerdown", handlePointerDown);
    return () => document.removeEventListener("pointerdown", handlePointerDown);
  }, [open]);

  useEffect(() => {
    if (!open || activeIndex < 0) return;
    listRef.current?.querySelector(`[data-index="${activeIndex}"]`)?.scrollIntoView?.({ block: "nearest" });
  }, [open, activeIndex]);

  function step(from: number, direction: 1 | -1) {
    return Math.min(Math.max(from + direction, 0), observations.length - 1);
  }

  function openList() {
    const current = observations.findIndex((observation) => observation.id === selectedId);
    setActiveIndex(current >= 0 ? current : step(-1, 1));
    setOpen(true);
  }

  function choose(index: number) {
    onSelect(observations[index].id);
    setOpen(false);
  }

  function handleKeyDown(event: KeyboardEvent<HTMLButtonElement>) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!open) openList();
      else setActiveIndex((index) => step(index, event.key === "ArrowDown" ? 1 : -1));
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      if (!open) openList();
      else if (activeIndex >= 0) choose(activeIndex);
    } else if (event.key === "Escape" && open) {
      event.preventDefault();
      setOpen(false);
    } else if (event.key === "Tab") {
      setOpen(false);
    }
  }

  return (
    // Inline-size containment: the truncated rows must take the width they're
    // given, not report their full text width up the tree -- otherwise a
    // narrow inspector rail grows to fit and scrolls sideways.
    <div className="relative w-full [contain:inline-size]" ref={rootRef}>
      <button
        aria-activedescendant={open && activeIndex >= 0 ? `${listId}-${activeIndex}` : undefined}
        aria-controls={listId}
        aria-expanded={open}
        aria-haspopup="listbox"
        aria-label="Available blueprint"
        className="iw-input flex w-full items-center gap-2 py-1.5 text-left"
        onClick={() => (open ? setOpen(false) : openList())}
        onKeyDown={handleKeyDown}
        role="combobox"
        type="button"
      >
        {selected ? (
          <ObservationSummary observation={selected} observations={observations} requiredRuns={requiredRuns} />
        ) : (
          <span className="flex-1 text-xs text-muted">Choose a blueprint ({observations.length} available)</span>
        )}
        <ChevronDown aria-hidden="true" className="h-4 w-4 shrink-0 text-muted" />
      </button>
      {open ? (
        <div
          className="iw-panel-strong absolute inset-x-0 z-40 mt-1 max-h-72 overflow-y-auto border p-1 shadow-2xl"
          id={listId}
          // Keep focus on the combobox so the keyboard keeps working after a
          // pointer interaction with the list.
          onMouseDown={(event) => event.preventDefault()}
          ref={listRef}
          role="listbox"
        >
          {observations.map((observation, index) => (
            <div
              aria-selected={observation.id === selectedId}
              className={`flex cursor-pointer items-center rounded-md px-2 py-1.5 ${index === activeIndex ? "bg-panel" : ""} ${observation.id === selectedId ? "ring-1 ring-primary/60" : ""}`}
              data-index={index}
              id={`${listId}-${index}`}
              key={observation.id}
              onClick={() => choose(index)}
              onMouseMove={() => setActiveIndex(index)}
              role="option"
            >
              <ObservationSummary observation={observation} observations={observations} requiredRuns={requiredRuns} />
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}

/** The Choose Blueprint dialog's panel -- a titled `Panel` around the shared
 * `BlueprintSelectionFields`. */
export function BlueprintSelectionSection(props: BlueprintSelectionFieldsProps) {
  return (
    <Panel>
      <p className="iw-eyebrow">Planning assumptions</p>
      <h2 className="text-base font-semibold">Blueprint</h2>
      <BlueprintSelectionFields {...props} />
    </Panel>
  );
}

export function BlueprintPlanningDialog({
  children,
  open,
  title,
  onClose,
}: {
  children: ReactNode;
  open: boolean;
  title: string;
  onClose: () => void;
}) {
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-[70] grid place-items-center bg-black/70 p-4" role="presentation">
      <section aria-label={title} aria-modal="true" className="iw-dialog max-h-[calc(100vh-2rem)] w-[min(760px,100%)] overflow-y-auto p-4" role="dialog">
        <div className="mb-3 flex items-center justify-between gap-3">
          <div>
            <p className="iw-eyebrow">Planning assumptions</p>
            <h2 className="text-base font-semibold">{title}</h2>
          </div>
          <button aria-label="Close blueprint editor" className="iw-icon-button" onClick={onClose} title="Close" type="button">
            <X aria-hidden="true" className="h-4 w-4" />
          </button>
        </div>
        {children}
        <div className="mt-3 flex justify-end border-t border-border pt-3">
          <button className="iw-button-primary" onClick={onClose} type="button">Done</button>
        </div>
      </section>
    </div>
  );
}
