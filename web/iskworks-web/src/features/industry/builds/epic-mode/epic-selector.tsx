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
}: {
  epics: OrderSummary[];
  selectedEpicId: string | null;
  onSelect: (epicId: string | null) => void;
  disabled?: boolean;
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
