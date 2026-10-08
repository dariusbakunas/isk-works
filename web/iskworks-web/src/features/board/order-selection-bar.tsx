import type { Ticket } from "../../api/industry";
import { isBatchableOrderTicket } from "./order-ticket-card";

// Standalone-ticket counterpart to SelectionBar -- kept as a fully separate
// component (not a shared/parameterized one) so the two selection systems
// never risk sharing state by accident, matching board-page.tsx's separate
// `selectedOrderTicketIds` set.
export function OrderSelectionBar({
  selectedTickets,
  onClear,
  onCreateRun,
}: {
  selectedTickets: Ticket[];
  onClear: () => void;
  onCreateRun: () => void;
}) {
  const ineligible = selectedTickets.filter((ticket) => !isBatchableOrderTicket(ticket));
  const canCreate = selectedTickets.length > 0 && ineligible.length === 0;

  return (
    <div className="fixed inset-x-0 bottom-0 z-40 flex items-center gap-3 border-t border-border bg-panel-strong px-4 py-2.5 shadow-lg">
      <span className="text-sm font-medium text-foreground">
        {selectedTickets.length} order ticket{selectedTickets.length === 1 ? "" : "s"} selected
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
