import type { Ticket } from "../../api/industry";
import { isBatchableOrderTicket, isCombinableTicket, selectionKind } from "./ticket-selection";

// Standalone-ticket counterpart to SelectionBar -- kept as a fully separate
// component (not a shared/parameterized one) so the two selection systems
// never risk sharing state by accident, matching board-page.tsx's separate
// `selectedOrderTicketIds` set. The first selected ticket decides the action:
// an Acquisition Run for acquisition tickets, Combine for production steps.
export function OrderSelectionBar({
  selectedTickets,
  onClear,
  onCreateRun,
  onCombine,
  onSelectGroup,
  groupSize = 0,
}: {
  selectedTickets: Ticket[];
  onClear: () => void;
  onCreateRun: () => void;
  onCombine: () => void;
  // Set while the first selected step's combine suggestion group (same
  // item, owner and facility) has unselected tickets: "Select all" adds them.
  onSelectGroup?: () => void;
  groupSize?: number;
}) {
  const combining = selectedTickets.length > 0 && selectionKind(selectedTickets[0]) === "production";
  const count = selectedTickets.length;

  if (combining) {
    const ineligible = selectedTickets.filter((ticket) => !isCombinableTicket(ticket));
    const canCombine = count >= 2 && ineligible.length === 0;
    return (
      <div className="fixed inset-x-0 bottom-0 z-40 flex items-center gap-3 border-t border-border bg-panel-strong px-4 py-2.5 shadow-lg">
        <span className="text-sm font-medium text-foreground">
          {count} {selectedTickets[0].capturedName} step{count === 1 ? "" : "s"} selected
        </span>
        <span className="iw-muted text-xs">
          {ineligible.length > 0
            ? `Remove ${ineligible.length} ticket${ineligible.length === 1 ? "" : "s"} that can no longer be combined.`
            : count < 2
              ? "Select another unstarted step of the same item to combine."
              : "Unstarted steps of the same item -- one ticket will do their work together."}
        </span>
        <div className="ml-auto flex items-center gap-2">
          {onSelectGroup ? (
            <button className="iw-button-secondary" onClick={onSelectGroup} type="button">
              Select all {groupSize}
            </button>
          ) : null}
          <button className="iw-button-secondary" onClick={onClear} type="button">
            Clear
          </button>
          <button className="iw-button-primary" disabled={!canCombine} onClick={onCombine} type="button">
            Combine ({count})
          </button>
        </div>
      </div>
    );
  }

  const ineligible = selectedTickets.filter((ticket) => !isBatchableOrderTicket(ticket));
  const canCreate = count > 0 && ineligible.length === 0;

  return (
    <div className="fixed inset-x-0 bottom-0 z-40 flex items-center gap-3 border-t border-border bg-panel-strong px-4 py-2.5 shadow-lg">
      <span className="text-sm font-medium text-foreground">
        {count} order ticket{count === 1 ? "" : "s"} selected
      </span>
      <span className="iw-muted text-xs">
        {ineligible.length > 0
          ? `Remove ${ineligible.length} non-Acquisition, already-started, or already-batched ticket${ineligible.length === 1 ? "" : "s"} to continue.`
          : "To Do Acquisition tickets only -- shared acquisition location is checked when the Run is created."}
      </span>
      <div className="ml-auto flex items-center gap-2">
        <button className="iw-button-secondary" onClick={onClear} type="button">
          Clear
        </button>
        <button className="iw-button-primary" disabled={!canCreate} onClick={onCreateRun} type="button">
          Create Acquisition Run
        </button>
      </div>
    </div>
  );
}
