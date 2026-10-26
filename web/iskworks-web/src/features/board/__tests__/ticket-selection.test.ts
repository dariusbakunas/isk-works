import { describe, expect, it } from "vitest";

import { isCombinableTicket, isSelectable, selectionKind } from "../ticket-selection";
import { ticketFixture } from "./fixtures";

function step(overrides: Parameters<typeof ticketFixture>[0] = {}) {
  return ticketFixture({
    id: overrides.id ?? "step-1",
    kind: "reaction",
    typeId: 16658,
    capturedName: "Hexite",
    orderId: "epic-1",
    occurrenceKey: "build:hexite",
    operations: [{ orderId: "epic-1", occurrenceKey: "build:hexite" }],
    recording: {
      state: "notRecorded",
      requestedQuantity: 2,
      recordedQuantity: 0,
      remainingQuantity: 2,
      surplusQuantity: 0,
    },
    ...overrides,
  });
}

describe("isCombinableTicket", () => {
  it("accepts an unstarted, unrecorded Epic step", () => {
    expect(isCombinableTicket(step())).toBe(true);
  });

  it.each([
    ["an acquisition ticket", { kind: "acquisition" as const }],
    ["a started ticket", { status: "inProgress" as const }],
    ["an archived ticket", { archivedAt: "2026-10-26T00:00:00Z" }],
    ["a merged member", { status: "canceled" as const, mergedIntoTicketId: "combined-1" }],
    ["a standalone ticket", { operations: [] }],
    [
      "a combined ticket",
      {
        operations: [
          { orderId: "epic-1", occurrenceKey: "build:hexite" },
          { orderId: "epic-2", occurrenceKey: "build:hexite" },
        ],
      },
    ],
    ["an Epic's root", { operations: [{ orderId: "epic-1", occurrenceKey: "root:epic-1" }] }],
    [
      "a recorded ticket",
      {
        recording: {
          state: "partiallyRecorded" as const,
          requestedQuantity: 2,
          recordedQuantity: 1,
          remainingQuantity: 1,
          surplusQuantity: 0,
        },
      },
    ],
  ])("refuses %s", (_label, overrides) => {
    expect(isCombinableTicket(step(overrides))).toBe(false);
  });

  it("refuses a ticket with matched industry jobs", () => {
    expect(isCombinableTicket(step(), { hasJobs: true })).toBe(false);
  });
});

describe("selectionKind", () => {
  it("is acquisition for a batchable ticket and production for a combinable one", () => {
    expect(selectionKind(ticketFixture())).toBe("acquisition");
    expect(selectionKind(step())).toBe("production");
    expect(selectionKind(step({ status: "complete" }))).toBeNull();
  });
});

describe("isSelectable", () => {
  it("allows any eligible ticket while nothing is selected", () => {
    expect(isSelectable(ticketFixture(), [])).toBe(true);
    expect(isSelectable(step(), [])).toBe(true);
  });

  it("locks to the first pick's kind, and for production to its item", () => {
    const first = step();
    expect(isSelectable(ticketFixture({ id: "buy-1" }), [first])).toBe(false);
    expect(isSelectable(step({ id: "step-2", orderId: "epic-2" }), [first])).toBe(true);
    expect(isSelectable(step({ id: "step-3", typeId: 16659 }), [first])).toBe(false);
    expect(isSelectable(step({ id: "step-4", kind: "manufacturing" }), [first])).toBe(false);
    expect(isSelectable(step({ id: "step-5" }), [ticketFixture()])).toBe(false);
  });

  it("keeps an already selected ticket selectable so it can be cleared", () => {
    const first = step();
    expect(isSelectable(first, [first])).toBe(true);
  });
});
