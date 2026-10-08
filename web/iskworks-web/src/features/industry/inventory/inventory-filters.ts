import type { InventoryItem, InventoryListScope } from "../../../api/inventory";

/**
 * There is no "all" scope -- with `untrackedObserved` split off into
 * its own fetch (see `filterScope` below), the tracked-scope response
 * from the API *is* already "all" the accounting-relevant rows, so a
 * separate no-op "all" tab would just be a second name for the same
 * thing. "untrackedObserved" is its own view, not a sub-filter of
 * tracked: selecting it fetches a completely different (ESI-driven, no
 * `inventory_balances` row) result set rather than filtering the
 * already-loaded tracked list client-side.
 */
export type InventoryFilterId = "tracked" | "needsAttention" | "negativeAvailable" | "withReservations" | "untrackedObserved";

export const INVENTORY_FILTERS: { id: InventoryFilterId; label: string }[] = [
  { id: "tracked", label: "Tracked" },
  { id: "needsAttention", label: "Needs attention" },
  { id: "negativeAvailable", label: "Negative available" },
  { id: "withReservations", label: "With reservations" },
  { id: "untrackedObserved", label: "Untracked observed" },
];

/** Which `GET /api/inventory` scope a filter tab needs. */
export function filterScope(filter: InventoryFilterId): InventoryListScope {
  return filter === "untrackedObserved" ? "untracked" : "tracked";
}

/**
 * Single source of truth for "does this item need attention" -- nothing
 * else should re-derive this rule. A shortfall (available < 0) always
 * qualifies; a meaningful ESI discrepancy also qualifies, but only when an
 * observation actually exists (`esiObservedQuantity == null` means "no
 * ESI data", never treated as a mismatch against 0).
 */
export function needsAttention(item: InventoryItem): boolean {
  return item.availableQuantity < 0 || hasEsiDiscrepancy(item);
}

export function hasEsiDiscrepancy(item: InventoryItem): boolean {
  return item.reconciliationDifference != null && item.reconciliationDifference !== 0;
}

export function matchesInventoryFilter(item: InventoryItem, filter: InventoryFilterId): boolean {
  switch (filter) {
    case "tracked":
    // The fetched list is already scoped server-side for this tab -- no
    // further row-level filtering applies.
    case "untrackedObserved":
      return true;
    case "needsAttention":
      return needsAttention(item);
    case "negativeAvailable":
      return item.availableQuantity < 0;
    case "withReservations":
      return item.reservedQuantity > 0;
  }
}

export function matchesInventorySearch(item: InventoryItem, query: string): boolean {
  const trimmed = query.trim().toLowerCase();
  if (!trimmed) return true;
  return item.balance.typeName.toLowerCase().includes(trimmed);
}
