import { useState } from "react";

import type { Ticket } from "../../api/industry";

/**
 * Selection mode: picking tickets to combine or to put on an acquisition
 * run. `selectedTickets` resolves the picked ids against the loaded
 * tickets, so a ticket that disappears on reload drops out of it.
 */
export function useBoardSelection(tickets: Ticket[]) {
  const [selectionMode, setSelectionMode] = useState(false);
  const [selectedTicketIds, setSelectedTicketIds] = useState<Set<string>>(new Set());

  function startSelection() {
    setSelectionMode(true);
  }

  function exitSelectionMode() {
    setSelectionMode(false);
    setSelectedTicketIds(new Set());
  }

  function clearSelection() {
    setSelectedTicketIds(new Set());
  }

  function toggleSelected(ticketId: string) {
    setSelectedTicketIds((current) => {
      const next = new Set(current);
      if (next.has(ticketId)) {
        next.delete(ticketId);
      } else {
        next.add(ticketId);
      }
      return next;
    });
  }

  // Group-level "select all" / header checkbox: already-all-selected clears
  // the set, otherwise selects every id -- shared by an ACQ GROUP's header
  // checkbox and its expanded "Select N actionable" button.
  function selectMany(ticketIds: string[]) {
    setSelectedTicketIds((current) => {
      const next = new Set(current);
      const allSelected = ticketIds.every((id) => next.has(id));
      for (const id of ticketIds) {
        if (allSelected) next.delete(id);
        else next.add(id);
      }
      return next;
    });
  }

  /** Adds these tickets to the selection (a combine suggestion's group). */
  function addToSelection(ticketIds: string[]) {
    setSelectedTicketIds(new Set([...selectedTicketIds, ...ticketIds]));
  }

  const selectedTickets = tickets.filter((ticket) => selectedTicketIds.has(ticket.id));

  return {
    selectionMode,
    selectedTicketIds,
    selectedTickets,
    startSelection,
    exitSelectionMode,
    clearSelection,
    toggleSelected,
    selectMany,
    addToSelection,
  };
}
