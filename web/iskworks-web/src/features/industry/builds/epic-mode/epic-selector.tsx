import { Lock } from "lucide-react";

import type { OrderSummary } from "../../../../api/industry/orders";

export function epicOptionLabel(epic: OrderSummary): string {
  const created = new Date(epic.createdAt).toLocaleDateString(undefined, { month: "short", day: "numeric" });
  return `${epic.displayName} · ${epic.runs.toLocaleString()} run${epic.runs === 1 ? "" : "s"} · ${created}`;
}

/** Which stock the Plan counts: free stock (No Epic) or one Epic's view. */
export function EpicSelector({
  epics,
  selectedEpicId,
  onSelect,
  disabled = false,
  buildRevision = null,
}: {
  epics: OrderSummary[];
  selectedEpicId: string | null;
  onSelect: (epicId: string | null) => void;
  disabled?: boolean;
  /** The Build's current revision, to flag edits made after the freeze. */
  buildRevision?: number | null;
}) {
  const selected = epics.find((epic) => epic.id === selectedEpicId) ?? null;
  const changedSinceFreeze =
    selected !== null && buildRevision !== null && selected.sourceBuildRevision !== buildRevision;
  return (
    <div>
      <EpicSelect disabled={disabled} epics={epics} onSelect={onSelect} selectedEpicId={selectedEpicId} />
      {selected ? (
        // A quiet, page-level reminder: every tab is read-only while an
        // Epic is shown.
        <p className="mt-1 flex items-center gap-1 text-[11px] text-muted" role="status">
          <Lock aria-hidden="true" className="h-3 w-3 shrink-0" />
          Read-only · choose No Epic to edit
        </p>
      ) : null}
      {changedSinceFreeze ? (
        <p
          className="mt-0.5 text-[11px] text-warning"
          title="This view shows the plan as it was when the Epic was created, not the Build's current recipe, sourcing or runs."
        >
          Build changed since this Epic
        </p>
      ) : null}
    </div>
  );
}

function EpicSelect({
  epics,
  selectedEpicId,
  onSelect,
  disabled,
}: {
  epics: OrderSummary[];
  selectedEpicId: string | null;
  onSelect: (epicId: string | null) => void;
  disabled: boolean;
}) {
  return (
    <label className="block">
      <span className="mb-1 block text-balance text-sm font-semibold">Epic</span>
      <select
        className="iw-input w-full"
        disabled={disabled}
        onChange={(event) => onSelect(event.target.value === "" ? null : event.target.value)}
        value={selectedEpicId ?? ""}
      >
        <option value="">No Epic (free stock)</option>
        {epics.map((epic) => (
          <option key={epic.id} value={epic.id}>{epicOptionLabel(epic)}</option>
        ))}
      </select>
    </label>
  );
}
