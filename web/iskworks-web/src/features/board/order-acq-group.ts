import type { TicketSummary } from "../../api/industry";
import {
  acquisitionPricingIdentity,
  acquisitionPricingKey,
  isBatchablePricingIdentity,
} from "./acquisition-pricing";

export interface OrderAcquisitionGroup {
  /** Stable grouping identity, mirroring the backend `BatchKey`
   * (`scope:<region>:<location>` | `list:<priceSourceId>` | `none`). Two
   * tickets share a group iff the server would let them share one
   * Acquisition Run. */
  key: string;
  /** Populated only for a market-scope group. */
  marketRegionId: number | null;
  marketLocationId: number | null;
  /** Populated only for a manual-price-list group. */
  priceSourceId: string | null;
  /** `false` for the `none` group -- tickets with no market scope and no
   * price list cannot be batched into a Run at all. */
  batchable: boolean;
  tickets: TicketSummary[];
}

// A ticket belongs to an ACQ GROUP only while it hasn't joined an
// Acquisition Run -- the moment it's batched it's represented by
// OrderAcquisitionRunCard instead, so group membership and Run membership
// never overlap. Grouping is by the real batching-compatibility key (see
// `acquisitionPricingIdentity`): a resolved market scope
// (`marketRegionId`/`marketLocationId`) takes precedence, falling back to
// `priceSourceId` only for a manually-priced ticket. Two tickets from
// different Epics with the same scope belong in the same group -- that's
// the point, not an edge case to guard against. `priceSourceId` alone is
// NOT the key: market-priced generated tickets legitimately have a null
// `priceSourceId` and must group by scope, not collapse into one bucket.
export function isGroupableOrderAcquisitionTicket(ticket: TicketSummary): boolean {
  return ticket.kind === "acquisition" && ticket.acquisitionRunId === null;
}

export function deriveOrderAcquisitionGroups(laneTickets: TicketSummary[]): OrderAcquisitionGroup[] {
  const groups: OrderAcquisitionGroup[] = [];
  const byKey = new Map<string, OrderAcquisitionGroup>();
  for (const ticket of laneTickets) {
    if (!isGroupableOrderAcquisitionTicket(ticket)) continue;
    const identity = acquisitionPricingIdentity(ticket);
    const key = acquisitionPricingKey(identity);
    let group = byKey.get(key);
    if (!group) {
      group = {
        key,
        marketRegionId: identity.kind === "scope" ? identity.marketRegionId : null,
        marketLocationId: identity.kind === "scope" ? identity.marketLocationId : null,
        priceSourceId: identity.kind === "list" ? identity.priceSourceId : null,
        batchable: isBatchablePricingIdentity(identity),
        tickets: [],
      };
      byKey.set(key, group);
      groups.push(group);
    }
    group.tickets.push(ticket);
  }
  return groups;
}
