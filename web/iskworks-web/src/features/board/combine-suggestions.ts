import type { FacilityProfile, Order, Ticket } from "../../api/industry";
import { sumDecimalStrings } from "../industry/shared/decimal";
import { isCombinableTicket } from "./ticket-selection";

// Groups of Board tickets the user could Combine in one go: unstarted
// production steps of the same item and owner, priced at the same facility.
// The facility isn't a server rule -- combining across facilities works but
// drops the per-job rates -- so a suggestion never mixes them. The server
// re-checks everything when each group is combined.

export interface CombineSuggestion {
  key: string;
  capturedName: string;
  facility: FacilityProfile | null;
  // Ordered by display id.
  tickets: Ticket[];
  quantity: number;
  runs: number;
  epicCount: number;
  estimatedLineTotal: string | null;
  // The combined ticket takes the first member's assignee.
  mixedAssignees: boolean;
}

function epicOf(ticket: Ticket): string | null {
  return ticket.operations?.[0]?.orderId ?? ticket.orderId;
}

function groupKey(ticket: Ticket): string {
  return [ticket.kind, ticket.typeId, ticket.ownerId, ticket.executionSnapshot?.facility?.id ?? ""].join("|");
}

// Whether two tickets fall in the same suggestion group (the selection bar's
// "Select all").
export function isSameCombineGroup(a: Ticket, b: Ticket): boolean {
  return groupKey(a) === groupKey(b);
}

const byDisplayId = (a: Ticket, b: Ticket) => a.displayId.localeCompare(b.displayId, undefined, { numeric: true });

export function combineSuggestions<T extends Ticket>(
  tickets: T[],
  {
    hasJobs,
    ordersById = new Map(),
  }: {
    hasJobs: (ticket: T) => boolean;
    ordersById?: Map<string, Pick<Order, "planningSnapshotVersion">>;
  },
): CombineSuggestion[] {
  const groups = new Map<string, T[]>();
  for (const ticket of tickets) {
    if (!isCombinableTicket(ticket, { hasJobs: hasJobs(ticket) })) continue;
    // A known pre-v3 Epic is refused by the server; an unknown version is
    // left to it.
    const epic = epicOf(ticket);
    const version = epic ? ordersById.get(epic)?.planningSnapshotVersion : undefined;
    if (version !== undefined && version < 3) continue;
    const key = groupKey(ticket);
    groups.set(key, [...(groups.get(key) ?? []), ticket]);
  }

  return [...groups.entries()]
    .filter(([, members]) => members.length > 1)
    .map(([key, members]) => {
      const sorted = [...members].sort(byDisplayId);
      return {
        key,
        capturedName: sorted[0].capturedName,
        facility: sorted[0].executionSnapshot?.facility ?? null,
        tickets: sorted,
        quantity: sorted.reduce((total, ticket) => total + (ticket.quantity ?? 0), 0),
        runs: sorted.reduce((total, ticket) => total + (ticket.executionSnapshot?.runs ?? 0), 0),
        epicCount: new Set(sorted.map(epicOf)).size,
        estimatedLineTotal: sumDecimalStrings(sorted.map((ticket) => ticket.estimatedLineTotal)).value,
        mixedAssignees: new Set(sorted.map((ticket) => ticket.assigneeCharacterId)).size > 1,
      };
    })
    .sort(
      (a, b) =>
        a.capturedName.localeCompare(b.capturedName) ||
        (a.facility?.name ?? "").localeCompare(b.facility?.name ?? ""),
    );
}
