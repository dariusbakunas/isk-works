import type { RequirementKind, TicketBlockerRef, TicketPrerequisite } from "../../api/industry";

export interface GroupedPrerequisite {
  key: string;
  typeId: number;
  capturedName: string;
  kind: RequirementKind;
  fulfillmentScope: "missing" | "full";
  requiredQuantity: number;
  freshQuantity: number;
  /** No copy is still listed in `blockedBy`. */
  met: boolean;
  /** The Epics the copies serve, in first-seen order (`null` = the ticket's own). */
  orderIds: (string | null)[];
}

// A combined ticket carries one copy of each member's prerequisites, so the
// same material is listed once per Epic. Shown as one row per material and
// sourcing: quantities summed, met only once every copy is. Copies that
// source differently (one Epic buys, another builds) stay separate rows, so
// the SOURCE label never blends two kinds.
export function groupPrerequisites(
  prerequisites: TicketPrerequisite[],
  blockedBy: TicketBlockerRef[],
): GroupedPrerequisite[] {
  const blocked = new Set(blockedBy.map((blocker) => blocker.prerequisiteId));
  const groups = new Map<string, GroupedPrerequisite>();
  for (const prerequisite of prerequisites) {
    const key = `${prerequisite.typeId}:${prerequisite.kind}:${prerequisite.fulfillmentScope}`;
    const met = !blocked.has(prerequisite.id);
    const orderId = prerequisite.orderId ?? null;
    const existing = groups.get(key);
    if (!existing) {
      groups.set(key, {
        key,
        typeId: prerequisite.typeId,
        capturedName: prerequisite.capturedName,
        kind: prerequisite.kind,
        fulfillmentScope: prerequisite.fulfillmentScope,
        requiredQuantity: prerequisite.requiredQuantity,
        freshQuantity: prerequisite.freshQuantity,
        met,
        orderIds: [orderId],
      });
      continue;
    }
    existing.requiredQuantity += prerequisite.requiredQuantity;
    existing.freshQuantity += prerequisite.freshQuantity;
    existing.met &&= met;
    if (!existing.orderIds.includes(orderId)) existing.orderIds.push(orderId);
  }
  return [...groups.values()];
}

// What's still to buy, as EVE's Multibuy window pastes it: one
// `<name> <quantity>` line per material. Only unmet Buy rows count -- Build and
// React rows are made by other tickets, and a met row was already bought.
// The remaining amount mirrors `prerequisiteSourcePresentation`: a
// `full`-scoped row was frozen with no reuse, so all of it is to buy.
export function multibuyText(prerequisites: GroupedPrerequisite[]): string {
  const byType = new Map<number, { name: string; quantity: number }>();
  for (const prerequisite of prerequisites) {
    if (prerequisite.kind !== "buy" || prerequisite.met) continue;
    const remaining =
      prerequisite.fulfillmentScope === "full" ? prerequisite.requiredQuantity : prerequisite.freshQuantity;
    if (remaining <= 0) continue;
    const existing = byType.get(prerequisite.typeId);
    if (existing) existing.quantity += remaining;
    else byType.set(prerequisite.typeId, { name: prerequisite.capturedName, quantity: remaining });
  }
  return [...byType.values()].map(({ name, quantity }) => `${name} ${quantity}`).join("\n");
}
