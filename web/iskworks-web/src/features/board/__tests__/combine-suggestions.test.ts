import { describe, expect, it } from "vitest";

import type { TicketSummary } from "../../../api/industry";
import { combineSuggestions, isSameCombineGroup } from "../combine-suggestions";
import { ticketFixture } from "./fixtures";

function step(
  id: string,
  overrides: Partial<TicketSummary> & { epic?: string; facilityId?: string | null; runs?: number } = {},
) {
  const { epic = `epic-${id}`, facilityId = "facility-1", runs = 1, ...rest } = overrides;
  return ticketFixture({
    id,
    displayId: `ISK-${id}`,
    kind: "reaction",
    typeId: 16658,
    capturedName: "Hexite",
    quantity: 200,
    orderId: epic,
    estimatedLineTotal: "100.0000",
    executionSnapshot: {
      runs,
      blueprint: null,
      facility: facilityId === null ? null : ({ id: facilityId, name: `Facility ${facilityId}` } as never),
      durationSeconds: null,
      installationCost: null,
      materialValue: null,
    },
    operations: [{ orderId: epic, occurrenceKey: "build:hexite" }],
    ...rest,
  });
}

const noJobs = () => false;

describe("combineSuggestions", () => {
  it("groups unstarted steps of the same item, owner and facility", () => {
    const [group] = combineSuggestions([step("3"), step("1", { runs: 2 }), step("2")], { hasJobs: noJobs });
    expect(group.tickets.map((ticket) => ticket.id)).toEqual(["1", "2", "3"]);
    expect(group).toMatchObject({
      capturedName: "Hexite",
      facility: { id: "facility-1" },
      quantity: 600,
      runs: 4,
      epicCount: 3,
      estimatedLineTotal: "300.0000",
      mixedAssignees: false,
    });
  });

  it("keeps different items, owners and facilities apart", () => {
    const groups = combineSuggestions(
      [
        step("1"),
        step("2"),
        step("3", { typeId: 1, capturedName: "Crystallite Alloy" }),
        step("4", { typeId: 1, capturedName: "Crystallite Alloy" }),
        step("5", { ownerId: "owner-2" }),
        step("6", { facilityId: "facility-2" }),
        step("7", { kind: "manufacturing" }),
      ],
      { hasJobs: noJobs },
    );

    expect(groups.map((group) => group.tickets.map((ticket) => ticket.id))).toEqual([
      ["3", "4"],
      ["1", "2"],
    ]);
  });

  it("orders display ids numerically", () => {
    const [group] = combineSuggestions([step("1000"), step("999")], { hasJobs: noJobs });
    expect(group.tickets.map((ticket) => ticket.id)).toEqual(["999", "1000"]);
  });

  it("skips tickets Combine would refuse", () => {
    const groups = combineSuggestions(
      [
        step("1"),
        step("2", { status: "inProgress" }),
        step("3", { operations: [{ orderId: "epic-3", occurrenceKey: "root:epic-3" }] }),
        step("4"),
        step("5", { orderId: "legacy", operations: [{ orderId: "legacy", occurrenceKey: "build:hexite" }] }),
      ],
      {
        hasJobs: (ticket) => ticket.id === "4",
        ordersById: new Map([["legacy", { planningSnapshotVersion: 2 }]]),
      },
    );

    expect(groups).toHaveLength(0);
  });

  it("flags differing assignees", () => {
    const [group] = combineSuggestions([step("1", { assigneeCharacterId: "a" }), step("2")], { hasJobs: noJobs });
    expect(group.mixedAssignees).toBe(true);
  });
});

describe("isSameCombineGroup", () => {
  it("matches item, owner and facility", () => {
    expect(isSameCombineGroup(step("1"), step("2"))).toBe(true);
    expect(isSameCombineGroup(step("1"), step("2", { facilityId: "facility-2" }))).toBe(false);
    expect(isSameCombineGroup(step("1"), step("2", { ownerId: "owner-2" }))).toBe(false);
  });
});
