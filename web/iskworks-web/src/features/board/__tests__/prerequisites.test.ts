import { describe, expect, it } from "vitest";

import type { TicketBlockerRef, TicketPrerequisite } from "../../../api/industry";
import { groupPrerequisites, multibuyText } from "../prerequisites";

function prerequisite(
  id: string,
  typeId: number,
  requiredQuantity: number,
  freshQuantity: number,
  overrides: Partial<TicketPrerequisite> = {},
): TicketPrerequisite {
  return {
    id,
    ticketId: "combined-1",
    typeId,
    capturedName: typeId === 1 ? "Fullerides" : "Nanotransistors",
    kind: "react",
    sourceBuildId: null,
    requiredQuantity,
    fulfillmentScope: "missing",
    reusedQuantity: requiredQuantity - freshQuantity,
    freshQuantity,
    estimatedUnitCost: null,
    estimatedLineTotal: null,
    reusedLineTotal: null,
    ...overrides,
  };
}

function blocker(prerequisiteId: string): TicketBlockerRef {
  return { prerequisiteId } as TicketBlockerRef;
}

describe("groupPrerequisites", () => {
  it("merges a combined ticket's per-Epic copies of one material", () => {
    const grouped = groupPrerequisites(
      [
        prerequisite("a", 1, 1006, 0, { orderId: "epic-1" }),
        prerequisite("b", 2, 67, 67, { orderId: "epic-1" }),
        prerequisite("c", 1, 624, 0, { orderId: "epic-2" }),
        prerequisite("d", 2, 108, 108, { orderId: "epic-2" }),
      ],
      [],
    );

    expect(grouped.map((row) => [row.typeId, row.requiredQuantity, row.freshQuantity, row.orderIds])).toEqual([
      [1, 1630, 0, ["epic-1", "epic-2"]],
      [2, 175, 175, ["epic-1", "epic-2"]],
    ]);
  });

  it("is met only once every copy is", () => {
    const grouped = groupPrerequisites(
      [prerequisite("a", 2, 67, 67, { orderId: "epic-1" }), prerequisite("b", 2, 108, 108, { orderId: "epic-2" })],
      [blocker("b")],
    );

    expect(grouped).toHaveLength(1);
    expect(grouped[0].met).toBe(false);
  });

  it("keeps copies that source differently apart", () => {
    const grouped = groupPrerequisites(
      [prerequisite("a", 2, 67, 67, { kind: "buy" }), prerequisite("b", 2, 108, 108, { kind: "react" })],
      [],
    );

    expect(grouped.map((row) => row.kind)).toEqual(["buy", "react"]);
  });
});

describe("multibuyText", () => {
  it("lists what is still to buy, one Multibuy line per material", () => {
    const grouped = groupPrerequisites(
      [
        prerequisite("a", 1, 1006, 400, { kind: "buy", orderId: "epic-1" }),
        prerequisite("b", 1, 624, 624, { kind: "buy", orderId: "epic-2" }),
        // Reacted by another ticket, not bought.
        prerequisite("c", 2, 67, 67, { kind: "react" }),
      ],
      // Unmet rows are the ones the server lists as blockers.
      [blocker("a"), blocker("b"), blocker("c")],
    );

    expect(multibuyText(grouped)).toBe("Fullerides 1024");
  });

  it("skips covered and already-bought rows, and buys all of a full-scope row", () => {
    const grouped = groupPrerequisites(
      [
        prerequisite("covered", 1, 1006, 0, { kind: "buy" }),
        prerequisite("bought", 2, 67, 67, { kind: "buy", fulfillmentScope: "full" }),
        prerequisite("full", 2, 108, 0, { kind: "buy", fulfillmentScope: "full", orderId: "epic-2" }),
      ],
      [blocker("full")],
    );

    // "bought" is met; the unmet full-scope row merges with it, so the
    // whole group is unmet and all of its required quantity is to buy.
    expect(multibuyText(grouped)).toBe("Nanotransistors 175");
  });

  it("is empty when nothing is left to buy", () => {
    expect(multibuyText(groupPrerequisites([prerequisite("a", 1, 10, 0, { kind: "buy" })], []))).toBe("");
  });
});
