import { useState } from "react";
import { useSearchParams } from "react-router";

import type { Ticket } from "../../api/industry";

/**
 * What the Board's right rail shows: an Epic Inspector, a Ticket Inspector,
 * the Ticket editor, or an acquisition run's drawer -- with the rail's
 * one-frame "Back" from a ticket to the Epic it was opened from.
 *
 * `?ticket=<id>` / `?epic=<id>` deep-links are read once at mount to seed
 * which inspector opens (e.g. "Navigate to ticket" from the Inventory
 * Reservations tab, which has no other route to a bare Ticket). They are not
 * kept in sync afterward: every other way of opening one (a card click) is
 * plain state, same as `openRunId`, so the params only seed the initial
 * value and are cleared on close.
 *
 * This is plain state with no history/router involvement: clicking an Epic
 * or Ticket card never navigates (see order-card.tsx /
 * order-ticket-card.tsx), so every Board filter/search/scroll/loaded-data
 * stays exactly as it was.
 */
export function useBoardDrawers(reload: () => Promise<unknown>) {
  const [params, setParams] = useSearchParams();
  const [openRunId, setOpenRunId] = useState<string | null>(null);
  const [openOrderId, setOpenOrderId] = useState<string | null>(() => params.get("epic"));
  const [openTicketId, setOpenTicketId] = useState<string | null>(() => params.get("ticket"));
  // Set only when the currently-open ticket was opened from within the Epic
  // Inspector named here -- the navigation stack's one frame of "back"
  // state. Null when a ticket was opened directly from a Board card, which
  // is what keeps that path's Close going to no inspector at all (rather
  // than resurrecting an Epic Inspector that was never open).
  const [ticketBackToOrderId, setTicketBackToOrderId] = useState<string | null>(null);
  // The canonical Ticket editor -- opening it always replaces whatever else
  // is in the rail (same "one thing in the rail" rule as Epic/Ticket).
  // `editorBackToEpicId` is set only when opened from within an Epic
  // Inspector's own "Create ticket" action, so Cancel/Close returns there
  // (mirrors the Ticket Inspector's own onBack) and a successful create
  // opens the new Ticket with that same Epic as its Back target.
  const [editorOpen, setEditorOpen] = useState(false);
  const [editorInitialEpicId, setEditorInitialEpicId] = useState<string | null>(null);
  const [editorBackToEpicId, setEditorBackToEpicId] = useState<string | null>(null);

  function openEpic(orderId: string) {
    setOpenOrderId(orderId);
    setOpenTicketId(null);
    setTicketBackToOrderId(null);
  }

  function closeEpic() {
    setOpenOrderId(null);
    if (params.has("epic")) {
      const next = new URLSearchParams(params);
      next.delete("epic");
      setParams(next);
    }
  }

  // A ticket opened directly from a Board card (not via an Epic) has no
  // "back" frame -- closing it goes to no inspector at all, so any Epic
  // Inspector that happened to be open underneath is cleared too.
  function openTicketFromCard(ticketId: string) {
    setOpenTicketId(ticketId);
    setOpenOrderId(null);
    setTicketBackToOrderId(null);
  }

  // A ticket opened from within the currently-open Epic Inspector's own
  // Tickets list -- `openOrderId` is left as-is (it's what "Back" returns
  // to); only the Ticket Inspector takes over the rail in the meantime.
  function openTicketFromEpic(ticketId: string) {
    setTicketBackToOrderId(openOrderId);
    setOpenTicketId(ticketId);
  }

  function backFromTicketToEpic() {
    setOpenTicketId(null);
    setTicketBackToOrderId(null);
    if (params.has("ticket")) {
      const next = new URLSearchParams(params);
      next.delete("ticket");
      setParams(next);
    }
  }

  function closeTicket() {
    setOpenTicketId(null);
    setOpenOrderId(null);
    setTicketBackToOrderId(null);
    if (params.has("ticket") || params.has("epic")) {
      const next = new URLSearchParams(params);
      next.delete("ticket");
      next.delete("epic");
      setParams(next);
    }
  }

  // `backToEpicId` is only passed by Epic Inspector's own "Create ticket"
  // action -- everywhere else (Board-level "Create ticket") it's omitted,
  // so Cancel/Close goes to no inspector at all, same as a ticket opened
  // directly from a Board card.
  function openTicketEditor(initialEpicId: string | null = null, backToEpicId: string | null = null) {
    setOpenTicketId(null);
    setOpenOrderId(null);
    setTicketBackToOrderId(null);
    setEditorInitialEpicId(initialEpicId);
    setEditorBackToEpicId(backToEpicId);
    setEditorOpen(true);
  }

  function closeTicketEditor() {
    setEditorOpen(false);
    if (editorBackToEpicId) openEpic(editorBackToEpicId);
  }

  // Reloads first so the newly created ticket is already among the loaded
  // tickets by the time the Ticket Inspector opens for it.
  async function handleTicketCreated(ticket: Ticket) {
    setEditorOpen(false);
    await reload();
    setTicketBackToOrderId(editorBackToEpicId);
    setOpenOrderId(editorBackToEpicId);
    setOpenTicketId(ticket.id);
  }

  return {
    openRunId,
    setOpenRunId,
    openOrderId,
    openTicketId,
    ticketBackToOrderId,
    editorOpen,
    editorInitialEpicId,
    openEpic,
    closeEpic,
    openTicketFromCard,
    openTicketFromEpic,
    backFromTicketToEpic,
    closeTicket,
    openTicketEditor,
    closeTicketEditor,
    handleTicketCreated,
  };
}
