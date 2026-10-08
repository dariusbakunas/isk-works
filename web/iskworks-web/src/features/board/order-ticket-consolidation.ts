import type { AcquisitionRunItem, Ticket } from "../../api/industry";
import { sumDecimalStrings } from "../industry/shared/decimal";

export interface ConsolidatedOrderItem {
  typeId: number;
  capturedName: string;
  neededQuantity: number;
  acquiredQuantity: number;
  lineCost: string | null;
  tickets: Ticket[];
}

// One-row-per-type_id shopping-trip view of an Acquisition Run's tickets,
// summing `ticket.quantity` (see order-ticket.rs: a standalone Ticket's
// `quantity` already *is* the fresh/outstanding amount).
export function consolidateOrderTicketsByType(tickets: Ticket[]): ConsolidatedOrderItem[] {
  const byType = new Map<number, ConsolidatedOrderItem>();
  for (const ticket of tickets) {
    // Every ticket reaching this consolidation is an Acquisition ticket
    // (AcquisitionRun membership), which always has a real typeId/quantity
    // -- the `?? 0` fallback is defensive, never actually hit.
    const typeId = ticket.typeId ?? 0;
    const quantity = ticket.quantity ?? 0;
    const existing = byType.get(typeId);
    if (existing) {
      existing.neededQuantity += quantity;
      existing.acquiredQuantity += ticket.acquiredQuantity ?? 0;
      existing.tickets.push(ticket);
    } else {
      byType.set(typeId, {
        typeId,
        capturedName: ticket.capturedName,
        neededQuantity: quantity,
        acquiredQuantity: ticket.acquiredQuantity ?? 0,
        lineCost: null,
        tickets: [ticket],
      });
    }
  }
  const items = Array.from(byType.values());
  for (const item of items) {
    item.lineCost = sumDecimalStrings(item.tickets.map((ticket) => ticket.estimatedLineTotal)).value;
  }
  return items.sort((a, b) => a.capturedName.localeCompare(b.capturedName));
}

// See withRealAcquiredTotals -- identical reasoning, applied to the
// standalone Run's own uncapped per-type totals.
export function withRealOrderAcquiredTotals(
  items: ConsolidatedOrderItem[],
  runItems: AcquisitionRunItem[],
): ConsolidatedOrderItem[] {
  const byType = new Map(runItems.map((item) => [item.typeId, item.acquiredQuantity]));
  return items.map((item) => {
    const real = byType.get(item.typeId);
    return real === undefined ? item : { ...item, acquiredQuantity: real };
  });
}
