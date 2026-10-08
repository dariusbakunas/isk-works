import type { TicketStatus, TicketSummary } from "../../api/industry";

// A Board lane is workflow metadata, not a state machine: moving a card is a
// bare `PATCH /api/tickets/:id { status }` with ZERO domain side effects (no
// inventory, no dependent-ticket cascade, no Order/Run mutation) and it is
// fully reversible -- Complete -> In Progress -> To Do all work. A ticket
// with unmet dependencies is dragged like any other; its blocker indicator
// is purely informational. `canceled` tickets never render on the Board, so
// the Board can't drag to/from Canceled (that transition goes through the
// generic PATCH elsewhere).
const DRAG_SOURCE_STATUSES: readonly TicketStatus[] = ["todo", "inProgress", "complete"];
const DROP_TARGET_STATUSES: readonly TicketStatus[] = ["todo", "inProgress", "complete"];

export const TICKET_DRAG_MIME = "application/x-iskworks-ticket-id";

// A ticket batched into an Acquisition Run still advances only through the
// Run's own start/complete endpoints (Runs are out of scope for this phase),
// so those cards stay undraggable. Every other non-blocked, non-canceled
// ticket can be dragged.
export function isTicketDraggable(ticket: TicketSummary): boolean {
  return ticket.acquisitionRunId === null && DRAG_SOURCE_STATUSES.includes(ticket.status);
}

// Whether a card currently in `sourceStatus` may be dropped into the lane
// for `targetStatus`. Direction-agnostic: forward and backward moves are
// equally valid. Only used for drop-target highlighting and as a guard
// before firing the PATCH; the transition itself carries no meaning beyond
// "set ticket.status".
export function canDropTicket(sourceStatus: TicketStatus, targetStatus: TicketStatus): boolean {
  return (
    DRAG_SOURCE_STATUSES.includes(sourceStatus) &&
    DROP_TARGET_STATUSES.includes(targetStatus) &&
    sourceStatus !== targetStatus
  );
}
