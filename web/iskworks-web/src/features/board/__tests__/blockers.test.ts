import { describe, expect, it } from "vitest";

import type { TicketBlockerRef } from "../../../api/industry";
import { groupBlockersByType } from "../blockers";

function blocker(prerequisiteId: string, typeId: number, outstandingQuantity: number, fulfilling: string | null = null) {
  return {
    prerequisiteId,
    kind: "buy",
    typeId,
    capturedName: typeId === 1 ? "Fernite Carbide" : "Nanotransistors",
    outstandingQuantity,
    representativeFulfillingTicketId: fulfilling,
    representativeFulfillingTicketDisplayId: fulfilling,
    representativeFulfillingTicketStatus: fulfilling ? "todo" : null,
  } as TicketBlockerRef;
}

describe("groupBlockersByType", () => {
  it("merges a combined ticket's per-Epic copies of one material", () => {
    const grouped = groupBlockersByType([blocker("a", 1, 35_917), blocker("b", 2, 10), blocker("c", 1, 70_106, "ISK-9")]);

    expect(grouped.map((entry) => [entry.typeId, entry.outstandingQuantity])).toEqual([
      [1, 106_023],
      [2, 10],
    ]);
    // A fulfilling ticket from any copy is kept for display.
    expect(grouped[0].representativeFulfillingTicketDisplayId).toBe("ISK-9");
  });

  it("leaves distinct materials alone", () => {
    const blockers = [blocker("a", 1, 5), blocker("b", 2, 6)];
    expect(groupBlockersByType(blockers)).toEqual(blockers);
  });
});
