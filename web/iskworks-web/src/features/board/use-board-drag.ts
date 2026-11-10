import { useState } from "react";

import { updateTicketStatus, type TicketStatus, type TicketSummary } from "../../api/industry";
import { apiMessage } from "../../api/api-error";
import { canDropTicket, isTicketDraggable } from "./ticket-drag";

export interface BoardDragOptions {
  /** The loaded tickets, or `null` while the Board isn't ready. */
  tickets: TicketSummary[] | null;
  patchTicketStatus: (ticketId: string, status: TicketStatus) => void;
  reload: () => Promise<unknown>;
}

/**
 * Dragging a ticket card between lanes. Which lanes accept the dragged
 * ticket, which one it hovers, and the drop itself.
 */
export function useBoardDrag({ tickets, patchTicketStatus, reload }: BoardDragOptions) {
  const [draggingTicketId, setDraggingTicketId] = useState<string | null>(null);
  const [dragOverLane, setDragOverLane] = useState<TicketStatus | null>(null);
  const [dragError, setDragError] = useState<string | null>(null);

  const draggingTicket = draggingTicketId ? (tickets?.find((t) => t.id === draggingTicketId) ?? null) : null;
  const draggingTicketDraggable = draggingTicket !== null && isTicketDraggable(draggingTicket);

  function startDrag(ticketId: string) {
    setDraggingTicketId(ticketId);
  }

  function endDrag() {
    setDraggingTicketId(null);
    setDragOverLane(null);
  }

  // Moving a card between lanes is a bare workflow-status write and nothing
  // else (see ticket-drag.ts): it never calls /start, /complete, or
  // /cancel, so it has no inventory, cascade, or Order/Run side effect, and
  // it works in both directions. Optimistically repaints the card, fires
  // `PATCH /api/tickets/:id`, then resyncs (ACQ-group re-derivation etc. is
  // still cheaper to reload than to predict). On failure the optimistic
  // patch is rolled back and `dragError` carries the message.
  async function dropTicket(ticketId: string, targetStatus: TicketStatus) {
    const ticket = tickets?.find((t) => t.id === ticketId);
    if (!ticket || !isTicketDraggable(ticket)) return;
    if (!canDropTicket(ticket.status, targetStatus)) return;

    const originalStatus = ticket.status;
    setDragError(null);
    patchTicketStatus(ticketId, targetStatus);
    try {
      await updateTicketStatus(ticketId, targetStatus);
      void reload();
    } catch (requestError) {
      patchTicketStatus(ticketId, originalStatus);
      setDragError(apiMessage(requestError));
    }
  }

  /** How a lane looks during a drag, and its drag-and-drop handlers. */
  function lane(status: TicketStatus) {
    const eligible =
      draggingTicket !== null && draggingTicketDraggable && canDropTicket(draggingTicket.status, status);
    return {
      eligible,
      hovered: eligible && dragOverLane === status,
      onDragLeave: () => setDragOverLane((current) => (current === status ? null : current)),
      onDragOver: (event: { preventDefault: () => void }) => {
        if (!eligible) return;
        event.preventDefault();
        if (dragOverLane !== status) setDragOverLane(status);
      },
      onDrop: (event: { preventDefault: () => void }) => {
        event.preventDefault();
        setDragOverLane(null);
        if (eligible && draggingTicket) {
          void dropTicket(draggingTicket.id, status);
        }
      },
    };
  }

  return { dragError, startDrag, endDrag, lane };
}
