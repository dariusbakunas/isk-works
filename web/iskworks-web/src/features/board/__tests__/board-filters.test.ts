import { describe, expect, it } from "vitest";

import type { OrderSummary, TicketSummary } from "../../../api/industry";
import {
  boardEpicCardIsVisible,
  EMPTY_BOARD_FILTERS,
  filterBoardTickets,
  hasActiveBoardFilters,
  parseBoardFiltersFromParams,
  withBoardFilterParam,
  withoutBoardFilters,
  type BoardFilterState,
} from "../board-filters";

function ticketFixture(overrides: Partial<TicketSummary> = {}): TicketSummary {
  return {
    id: "ticket-1",
    workspaceId: "workspace-1",
    ownerId: "owner-1",
    displayId: "ISK-1000",
    kind: "generic",
    typeId: null,
    capturedName: "Move blueprints",
    quantity: null,
    orderId: null,
    notes: "",
    assigneeCharacterId: null,
    sourceBuildId: null,
    status: "todo",
    estimatedUnitCost: null,
    estimatedLineTotal: null,
    actualUnitCost: null,
    actualLineTotal: null,
    marketRegionId: null,
    marketLocationId: null,
    priceSourceId: null,
    acquisitionRunId: null,
    acquiredQuantity: null,
    executionSnapshot: null,
    createdAt: "2026-09-05T00:00:00Z",
    updatedAt: "2026-09-05T00:00:00Z",
    archivedAt: null,
    blockedBy: [],
    prerequisites: [],
    recording: null,
    ...overrides,
  };
}

function orderFixture(overrides: Partial<OrderSummary> = {}): OrderSummary {
  return {
    id: "order-1",
    workspaceId: "workspace-1",
    ownerId: "owner-1",
    sourceBuildId: "build-1",
    sourceBuildRevision: 1,
    displayName: "Weekend Ishtar Production",
    runs: 1,
    recipeFingerprint: "fp",
    priceSnapshotId: "snapshot-1",
    estimatedMaterialCost: "1000.0000",
    expectedRevenue: null,
    estimatedMargin: null,
    missingPriceCount: 0,
    createdAt: "2026-08-21T00:00:00Z",
    updatedAt: "2026-08-21T00:00:00Z",
    startedAt: null,
    completedAt: null,
    canceledAt: null,
    archivedAt: null,
    status: "blocked",
    rollup: { satisfied: 0, needsAction: 1, inProgress: 0, total: 1 },
    ...overrides,
  };
}

const context = {
  orderNameById: new Map([
    ["epic-a", "Weekend Production"],
    ["epic-b", "Ion Batch"],
  ]),
  characterNameById: new Map([
    ["char-valka", "Valka"],
    ["char-freya", "Freya"],
  ]),
};

describe("parseBoardFiltersFromParams / withBoardFilterParam / withoutBoardFilters", () => {
  it("defaults every filter to empty when no params are present", () => {
    expect(parseBoardFiltersFromParams(new URLSearchParams())).toEqual(EMPTY_BOARD_FILTERS);
  });

  it("parses each filter from its own distinct param, leaving inspector-selection params untouched", () => {
    const params = new URLSearchParams(
      "epicFilter=epic-a&assigneeFilter=char-valka&typeFilter=manufacturing&q=ion&epic=epic-b&ticket=ticket-9",
    );
    expect(parseBoardFiltersFromParams(params)).toEqual({
      epicFilter: "epic-a",
      assigneeFilter: "char-valka",
      typeFilter: "manufacturing",
      search: "ion",
    });
    // Inspector-selection params are a distinct dimension -- untouched by parsing filters.
    expect(params.get("epic")).toBe("epic-b");
    expect(params.get("ticket")).toBe("ticket-9");
  });

  it("sanitizes an unrecognized typeFilter value to All types rather than passing it through", () => {
    const params = new URLSearchParams("typeFilter=bogus");
    expect(parseBoardFiltersFromParams(params).typeFilter).toBe("");
  });

  it("withBoardFilterParam sets or removes exactly one filter param without touching others", () => {
    let params = new URLSearchParams("epic=epic-b&ticket=ticket-9");
    params = withBoardFilterParam(params, "epicFilter", "epic-a");
    params = withBoardFilterParam(params, "search", "ion");
    expect(params.get("epicFilter")).toBe("epic-a");
    expect(params.get("q")).toBe("ion");
    expect(params.get("epic")).toBe("epic-b");
    expect(params.get("ticket")).toBe("ticket-9");

    params = withBoardFilterParam(params, "epicFilter", "");
    expect(params.has("epicFilter")).toBe(false);
    expect(params.get("q")).toBe("ion");
  });

  it("withoutBoardFilters clears every filter param but leaves inspector-selection params alone", () => {
    const params = new URLSearchParams(
      "epicFilter=epic-a&assigneeFilter=char-valka&typeFilter=manufacturing&q=ion&epic=epic-b&ticket=ticket-9",
    );
    const cleared = withoutBoardFilters(params);
    expect(parseBoardFiltersFromParams(cleared)).toEqual(EMPTY_BOARD_FILTERS);
    expect(cleared.get("epic")).toBe("epic-b");
    expect(cleared.get("ticket")).toBe("ticket-9");
  });
});

describe("hasActiveBoardFilters", () => {
  it("is false for the empty filter state", () => {
    expect(hasActiveBoardFilters(EMPTY_BOARD_FILTERS)).toBe(false);
  });

  it("is false for whitespace-only search", () => {
    expect(hasActiveBoardFilters({ ...EMPTY_BOARD_FILTERS, search: "   " })).toBe(false);
  });

  it.each([
    ["epicFilter", "epic-a"],
    ["assigneeFilter", "unassigned"],
    ["typeFilter", "generic"],
    ["search", "ion"],
  ] as const)("is true when %s is set", (key, value) => {
    expect(hasActiveBoardFilters({ ...EMPTY_BOARD_FILTERS, [key]: value })).toBe(true);
  });
});

describe("filterBoardTickets", () => {
  const tickets: TicketSummary[] = [
    ticketFixture({ id: "t-acq-valka", kind: "acquisition", orderId: "epic-a", assigneeCharacterId: "char-valka", capturedName: "Tritanium" }),
    ticketFixture({ id: "t-man-freya", kind: "manufacturing", orderId: "epic-a", assigneeCharacterId: "char-freya", capturedName: "Ishtar Hull" }),
    ticketFixture({ id: "t-gen-unassigned", kind: "generic", orderId: "epic-a", assigneeCharacterId: null, capturedName: "Move blueprints" }),
    ticketFixture({ id: "t-man-valka-b", kind: "manufacturing", orderId: "epic-b", assigneeCharacterId: "char-valka", capturedName: "Ion battery" }),
    ticketFixture({
      id: "t-gen-standalone",
      kind: "generic",
      orderId: null,
      assigneeCharacterId: "char-valka",
      capturedName: "Standalone task",
      notes: "check ion crystals",
    }),
    ticketFixture({ id: "t-reaction", kind: "reaction", orderId: null, assigneeCharacterId: null, capturedName: "Hull Section", displayId: "ISK-7000" }),
  ];

  it("returns every ticket when no filters are active", () => {
    expect(filterBoardTickets(tickets, EMPTY_BOARD_FILTERS, context)).toHaveLength(tickets.length);
  });

  it("filters by a specific Epic", () => {
    const result = filterBoardTickets(tickets, { ...EMPTY_BOARD_FILTERS, epicFilter: "epic-a" }, context);
    expect(result.map((t) => t.id).sort()).toEqual(["t-acq-valka", "t-gen-unassigned", "t-man-freya"].sort());
  });

  it("filters by No Epic", () => {
    const result = filterBoardTickets(tickets, { ...EMPTY_BOARD_FILTERS, epicFilter: "none" }, context);
    expect(result.map((t) => t.id).sort()).toEqual(["t-gen-standalone", "t-reaction"].sort());
  });

  it("filters by a specific Assignee", () => {
    const result = filterBoardTickets(tickets, { ...EMPTY_BOARD_FILTERS, assigneeFilter: "char-valka" }, context);
    expect(result.map((t) => t.id).sort()).toEqual(["t-acq-valka", "t-gen-standalone", "t-man-valka-b"].sort());
  });

  it("filters by Unassigned", () => {
    const result = filterBoardTickets(tickets, { ...EMPTY_BOARD_FILTERS, assigneeFilter: "unassigned" }, context);
    expect(result.map((t) => t.id).sort()).toEqual(["t-gen-unassigned", "t-reaction"].sort());
  });

  it.each([
    ["generic", ["t-gen-unassigned", "t-gen-standalone"]],
    ["acquisition", ["t-acq-valka"]],
    ["manufacturing", ["t-man-freya", "t-man-valka-b"]],
    ["reaction", ["t-reaction"]],
  ] as const)("filters by Ticket type = %s", (kind, expectedIds) => {
    const result = filterBoardTickets(tickets, { ...EMPTY_BOARD_FILTERS, typeFilter: kind }, context);
    expect(result.map((t) => t.id).sort()).toEqual([...expectedIds].sort());
  });

  it("searches by captured/title name, case-insensitively", () => {
    const result = filterBoardTickets(tickets, { ...EMPTY_BOARD_FILTERS, search: "ISHTAR" }, context);
    expect(result.map((t) => t.id)).toEqual(["t-man-freya"]);
  });

  it("searches by notes", () => {
    const result = filterBoardTickets(tickets, { ...EMPTY_BOARD_FILTERS, search: "crystals" }, context);
    expect(result.map((t) => t.id)).toEqual(["t-gen-standalone"]);
  });

  it("searches by display id", () => {
    const result = filterBoardTickets(tickets, { ...EMPTY_BOARD_FILTERS, search: "isk-7000" }, context);
    expect(result.map((t) => t.id)).toEqual(["t-reaction"]);
  });

  it("searches by the assignee's name", () => {
    const result = filterBoardTickets(tickets, { ...EMPTY_BOARD_FILTERS, search: "freya" }, context);
    expect(result.map((t) => t.id)).toEqual(["t-man-freya"]);
  });

  it("searches by the containing Epic's title", () => {
    const result = filterBoardTickets(tickets, { ...EMPTY_BOARD_FILTERS, search: "weekend production" }, context);
    expect(result.map((t) => t.id).sort()).toEqual(["t-acq-valka", "t-gen-unassigned", "t-man-freya"].sort());
  });

  it("trims surrounding whitespace from search", () => {
    const result = filterBoardTickets(tickets, { ...EMPTY_BOARD_FILTERS, search: "  ishtar  " }, context);
    expect(result.map((t) => t.id)).toEqual(["t-man-freya"]);
  });

  it("combines every active filter with AND semantics", () => {
    const filters: BoardFilterState = {
      epicFilter: "epic-a",
      assigneeFilter: "char-valka",
      typeFilter: "acquisition",
      search: "trit",
    };
    expect(filterBoardTickets(tickets, filters, context).map((t) => t.id)).toEqual(["t-acq-valka"]);

    // Tightening one more dimension (Type -> manufacturing, which no
    // Epic-A/Valka ticket satisfies) collapses the result to nothing.
    expect(
      filterBoardTickets(tickets, { ...filters, typeFilter: "manufacturing", search: "" }, context),
    ).toHaveLength(0);
  });

  it("a stale/unknown Epic id yields zero matches, not an error", () => {
    expect(filterBoardTickets(tickets, { ...EMPTY_BOARD_FILTERS, epicFilter: "epic-deleted" }, context)).toHaveLength(0);
  });

  it("a stale/unknown Assignee id yields zero matches, not an error", () => {
    expect(
      filterBoardTickets(tickets, { ...EMPTY_BOARD_FILTERS, assigneeFilter: "char-deleted" }, context),
    ).toHaveLength(0);
  });
});

describe("boardEpicCardIsVisible", () => {
  const epicA = orderFixture({ id: "epic-a", displayName: "Weekend Production" });
  const epicB = orderFixture({ id: "epic-b", displayName: "Ion Batch" });
  const tickets: TicketSummary[] = [
    ticketFixture({ id: "t1", orderId: "epic-a", assigneeCharacterId: "char-valka", kind: "acquisition" }),
    ticketFixture({ id: "t2", orderId: "epic-a", assigneeCharacterId: "char-freya", kind: "manufacturing" }),
    ticketFixture({ id: "t3", orderId: "epic-b", assigneeCharacterId: "char-valka", kind: "manufacturing" }),
  ];

  it("is unconditionally visible when no filters are active, even with zero child Tickets", () => {
    const emptyEpic = orderFixture({ id: "epic-empty", displayName: "Brand New Epic" });
    expect(boardEpicCardIsVisible(emptyEpic, [], EMPTY_BOARD_FILTERS, context)).toBe(true);
  });

  it("No Epic filter hides every Epic card", () => {
    expect(boardEpicCardIsVisible(epicA, tickets, { ...EMPTY_BOARD_FILTERS, epicFilter: "none" }, context)).toBe(
      false,
    );
  });

  it("an Epic-specific filter hides every other Epic", () => {
    expect(boardEpicCardIsVisible(epicB, tickets, { ...EMPTY_BOARD_FILTERS, epicFilter: "epic-a" }, context)).toBe(
      false,
    );
    expect(boardEpicCardIsVisible(epicA, tickets, { ...EMPTY_BOARD_FILTERS, epicFilter: "epic-a" }, context)).toBe(
      true,
    );
  });

  it("under Assignee filtering, an Epic is visible only if it has a matching child Ticket", () => {
    // Both Epics have a Valka ticket.
    expect(
      boardEpicCardIsVisible(epicA, tickets, { ...EMPTY_BOARD_FILTERS, assigneeFilter: "char-valka" }, context),
    ).toBe(true);
    expect(
      boardEpicCardIsVisible(epicB, tickets, { ...EMPTY_BOARD_FILTERS, assigneeFilter: "char-valka" }, context),
    ).toBe(true);

    // No Epic has a Freya ticket under epic-b.
    expect(
      boardEpicCardIsVisible(epicB, tickets, { ...EMPTY_BOARD_FILTERS, assigneeFilter: "char-freya" }, context),
    ).toBe(false);
  });

  it("an Epic explicitly selected but with zero Tickets matching the other filters is hidden -- no unrelated/empty Epic cards floating", () => {
    const filters: BoardFilterState = { ...EMPTY_BOARD_FILTERS, epicFilter: "epic-b", assigneeFilter: "char-freya" };
    expect(boardEpicCardIsVisible(epicB, tickets, filters, context)).toBe(false);
  });
});
