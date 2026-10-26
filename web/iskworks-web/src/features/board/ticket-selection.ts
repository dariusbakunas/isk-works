import type { Ticket, TicketSummary } from "../../api/industry";

// Board selection mode serves two bulk actions: an Acquisition Run (To Do,
// unbatched acquisition tickets) and Combine (unstarted production steps
// of the same item). The first ticket picked fixes which one -- and, for
// Combine, the item -- so the selection always fits one action. The server
// re-checks everything.

export type SelectionKind = "acquisition" | "production";

// Mirrors the backend's `ROOT_OCCURRENCE_PREFIX`: an Epic's own product.
const ROOT_OCCURRENCE_PREFIX = "root:";

export function isBatchableOrderTicket(ticket: Ticket): boolean {
  return ticket.kind === "acquisition" && ticket.status === "todo" && ticket.acquisitionRunId === null;
}

// What the client can see of the backend's combine rules: a To Do,
// unarchived manufacturing/reaction ticket running exactly one non-root
// Epic step, with nothing recorded and no matched industry jobs.
export function isCombinableTicket(ticket: Ticket | TicketSummary, options: { hasJobs?: boolean } = {}): boolean {
  if (ticket.kind !== "manufacturing" && ticket.kind !== "reaction") return false;
  if (ticket.status !== "todo" || ticket.archivedAt !== null || ticket.mergedIntoTicketId) return false;
  const operations = ticket.operations ?? [];
  if (operations.length !== 1 || operations[0].occurrenceKey.startsWith(ROOT_OCCURRENCE_PREFIX)) return false;
  if ("recording" in ticket && (ticket.recording?.recordedQuantity ?? 0) > 0) return false;
  return !options.hasJobs;
}

export function selectionKind(
  ticket: Ticket | TicketSummary,
  options: { hasJobs?: boolean } = {},
): SelectionKind | null {
  if (isBatchableOrderTicket(ticket)) return "acquisition";
  if (isCombinableTicket(ticket, options)) return "production";
  return null;
}

// Whether `ticket`'s checkbox is enabled given what is already selected.
// A selected ticket always stays enabled, so it can be unchecked.
export function isSelectable(
  ticket: Ticket | TicketSummary,
  selected: Ticket[],
  options: { hasJobs?: boolean } = {},
): boolean {
  if (selected.some((other) => other.id === ticket.id)) return true;
  const kind = selectionKind(ticket, options);
  if (kind === null) return false;
  const first = selected[0];
  if (!first) return true;
  if (selectionKind(first) !== kind) return false;
  return kind === "acquisition" || (first.kind === ticket.kind && first.typeId === ticket.typeId);
}
