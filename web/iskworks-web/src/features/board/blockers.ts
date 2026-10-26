import type { TicketBlockerRef } from "../../api/industry";

// A combined ticket carries one copy of each member's prerequisites, so the
// same material can block it once per Epic. Shown as one blocker per
// material: outstanding quantities summed, the first fulfilling ticket any
// copy names kept as the representative.
export function groupBlockersByType(blockers: TicketBlockerRef[]): TicketBlockerRef[] {
  const byType = new Map<number, TicketBlockerRef>();
  for (const blocker of blockers) {
    const existing = byType.get(blocker.typeId);
    if (!existing) {
      byType.set(blocker.typeId, blocker);
      continue;
    }
    byType.set(blocker.typeId, {
      ...existing,
      outstandingQuantity: existing.outstandingQuantity + blocker.outstandingQuantity,
      representativeFulfillingTicketId:
        existing.representativeFulfillingTicketId ?? blocker.representativeFulfillingTicketId,
      representativeFulfillingTicketDisplayId:
        existing.representativeFulfillingTicketDisplayId ?? blocker.representativeFulfillingTicketDisplayId,
      representativeFulfillingTicketStatus:
        existing.representativeFulfillingTicketStatus ?? blocker.representativeFulfillingTicketStatus,
    });
  }
  return [...byType.values()];
}
