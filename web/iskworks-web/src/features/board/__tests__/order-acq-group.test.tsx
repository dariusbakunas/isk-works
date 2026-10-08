import { describe, expect, it } from "vitest";

import type { TicketSummary } from "../../../api/industry";
import { deriveOrderAcquisitionGroups, isGroupableOrderAcquisitionTicket } from "../order-acq-group";

function ticketFixture(overrides: Partial<TicketSummary> = {}): TicketSummary {
  return {
    id: "ticket-1",
    workspaceId: "workspace-1",
    ownerId: "owner-1",
    displayId: "ISK-2000",
    kind: "acquisition",
    typeId: 34,
    capturedName: "Tritanium",
    quantity: 100,
    orderId: null,
    notes: "",
    assigneeCharacterId: null,
    sourceBuildId: null,
    status: "todo",
    estimatedUnitCost: "5.0000",
    estimatedLineTotal: "500.0000",
    actualUnitCost: null,
    actualLineTotal: null,
    // Default fixture: priced from a market scope (The Forge / Jita 4-4),
    // which is what a Build-generated acquisition ticket actually carries.
    marketRegionId: 10_000_002,
    marketLocationId: 60_003_760,
    priceSourceId: null,
    acquisitionRunId: null,
    acquiredQuantity: null,
    executionSnapshot: null,
    createdAt: "2026-08-22T00:00:00Z",
    updatedAt: "2026-08-22T00:00:00Z",
    archivedAt: null,
    blockedBy: [],
    prerequisites: [],
    recording: null,
    ...overrides,
  };
}

describe("isGroupableOrderAcquisitionTicket", () => {
  it("is true only for an unbatched Acquisition ticket", () => {
    expect(isGroupableOrderAcquisitionTicket(ticketFixture())).toBe(true);
    expect(isGroupableOrderAcquisitionTicket(ticketFixture({ kind: "manufacturing" }))).toBe(false);
    expect(isGroupableOrderAcquisitionTicket(ticketFixture({ acquisitionRunId: "run-1" }))).toBe(false);
  });
});

describe("deriveOrderAcquisitionGroups — market scope identity (mirrors backend BatchKey)", () => {
  it("groups tickets that share a region + location, even across different Epics", () => {
    const tickets = [
      ticketFixture({ id: "epic-a-tritanium", orderId: "order-a" }),
      ticketFixture({ id: "epic-b-pyerite", orderId: "order-b", typeId: 35, capturedName: "Pyerite" }),
    ];

    const groups = deriveOrderAcquisitionGroups(tickets);

    expect(groups).toHaveLength(1);
    expect(groups[0].key).toBe("scope:10000002:60003760");
    expect(groups[0].marketRegionId).toBe(10_000_002);
    expect(groups[0].marketLocationId).toBe(60_003_760);
    expect(groups[0].priceSourceId).toBeNull();
    expect(groups[0].batchable).toBe(true);
    expect(groups[0].tickets.map((ticket) => ticket.id)).toEqual(["epic-a-tritanium", "epic-b-pyerite"]);
  });

  it("splits tickets priced in different regions into separate groups", () => {
    const tickets = [
      ticketFixture({ id: "forge", marketRegionId: 10_000_002, marketLocationId: 60_003_760 }),
      ticketFixture({ id: "domain", marketRegionId: 10_000_043, marketLocationId: 60_008_494 }),
    ];

    const groups = deriveOrderAcquisitionGroups(tickets);

    expect(groups).toHaveLength(2);
    expect(groups.map((group) => group.key)).toEqual([
      "scope:10000002:60003760",
      "scope:10000043:60008494",
    ]);
  });

  it("splits tickets in the same region but different market locations into separate groups", () => {
    const tickets = [
      ticketFixture({ id: "jita", marketRegionId: 10_000_002, marketLocationId: 60_003_760 }),
      ticketFixture({ id: "perimeter", marketRegionId: 10_000_002, marketLocationId: 60_011_866 }),
    ];

    const groups = deriveOrderAcquisitionGroups(tickets);

    expect(groups).toHaveLength(2);
    expect(groups.map((group) => group.key)).toEqual([
      "scope:10000002:60003760",
      "scope:10000002:60011866",
    ]);
  });

  it("keeps a region-wide scope (no location) distinct from a station scope in that region", () => {
    const tickets = [
      ticketFixture({ id: "region-wide", marketRegionId: 10_000_002, marketLocationId: null }),
      ticketFixture({ id: "station", marketRegionId: 10_000_002, marketLocationId: 60_003_760 }),
    ];

    const groups = deriveOrderAcquisitionGroups(tickets);

    expect(groups.map((group) => group.key)).toEqual(["scope:10000002:", "scope:10000002:60003760"]);
  });
});

describe("deriveOrderAcquisitionGroups — manual price list identity", () => {
  it("groups manually-priced tickets by priceSourceId", () => {
    const tickets = [
      ticketFixture({ id: "t1", marketRegionId: null, marketLocationId: null, priceSourceId: "source-2" }),
      ticketFixture({ id: "t2", marketRegionId: null, marketLocationId: null, priceSourceId: "source-1" }),
      ticketFixture({ id: "t3", marketRegionId: null, marketLocationId: null, priceSourceId: "source-2" }),
    ];

    const groups = deriveOrderAcquisitionGroups(tickets);

    expect(groups.map((group) => group.key)).toEqual(["list:source-2", "list:source-1"]);
    expect(groups[0].tickets.map((ticket) => ticket.id)).toEqual(["t1", "t3"]);
    expect(groups[0].priceSourceId).toBe("source-2");
    expect(groups[0].marketRegionId).toBeNull();
    expect(groups[0].batchable).toBe(true);
  });

  it("never merges a market-scope ticket with a manually-priced one", () => {
    const tickets = [
      ticketFixture({ id: "scoped", marketRegionId: 10_000_002, marketLocationId: 60_003_760, priceSourceId: null }),
      ticketFixture({ id: "manual", marketRegionId: null, marketLocationId: null, priceSourceId: "source-1" }),
    ];

    const groups = deriveOrderAcquisitionGroups(tickets);

    expect(groups).toHaveLength(2);
    expect(groups.map((group) => group.key)).toEqual(["scope:10000002:60003760", "list:source-1"]);
  });

  it("market scope wins even when a ticket somehow carries both a scope and a priceSourceId", () => {
    const tickets = [
      ticketFixture({ id: "both", marketRegionId: 10_000_002, marketLocationId: 60_003_760, priceSourceId: "source-1" }),
      ticketFixture({ id: "scope-only", marketRegionId: 10_000_002, marketLocationId: 60_003_760, priceSourceId: null }),
    ];

    const groups = deriveOrderAcquisitionGroups(tickets);

    expect(groups).toHaveLength(1);
    expect(groups[0].key).toBe("scope:10000002:60003760");
  });
});

describe("deriveOrderAcquisitionGroups — no pricing identity", () => {
  it("collects tickets with neither a market scope nor a price list into a single, non-batchable group", () => {
    const tickets = [
      ticketFixture({ id: "t1", marketRegionId: null, marketLocationId: null, priceSourceId: null }),
      ticketFixture({ id: "t2", marketRegionId: null, marketLocationId: null, priceSourceId: null }),
    ];

    const groups = deriveOrderAcquisitionGroups(tickets);

    expect(groups).toHaveLength(1);
    expect(groups[0].key).toBe("none");
    expect(groups[0].batchable).toBe(false);
    expect(groups[0].marketRegionId).toBeNull();
    expect(groups[0].priceSourceId).toBeNull();
  });

  it("never merges the no-pricing group with a market scope or a price list", () => {
    const tickets = [
      ticketFixture({ id: "none", marketRegionId: null, marketLocationId: null, priceSourceId: null }),
      ticketFixture({ id: "scoped", marketRegionId: 10_000_002, marketLocationId: 60_003_760, priceSourceId: null }),
      ticketFixture({ id: "manual", marketRegionId: null, marketLocationId: null, priceSourceId: "source-1" }),
    ];

    const groups = deriveOrderAcquisitionGroups(tickets);

    expect(groups.map((group) => group.key).sort()).toEqual([
      "list:source-1",
      "none",
      "scope:10000002:60003760",
    ]);
  });
});

describe("deriveOrderAcquisitionGroups — membership", () => {
  it("excludes non-acquisition tickets and already-batched tickets", () => {
    const tickets = [
      ticketFixture({ id: "t1" }),
      ticketFixture({ id: "t2", kind: "manufacturing" }),
      ticketFixture({ id: "t3", acquisitionRunId: "run-1" }),
    ];

    const groups = deriveOrderAcquisitionGroups(tickets);

    expect(groups).toHaveLength(1);
    expect(groups[0].tickets.map((ticket) => ticket.id)).toEqual(["t1"]);
  });

  it("preserves first-seen group order", () => {
    const tickets = [
      ticketFixture({ id: "t1", marketRegionId: null, marketLocationId: null, priceSourceId: "source-2" }),
      ticketFixture({ id: "t2", marketRegionId: 10_000_002, marketLocationId: 60_003_760 }),
      ticketFixture({ id: "t3", marketRegionId: null, marketLocationId: null, priceSourceId: "source-2" }),
    ];

    const groups = deriveOrderAcquisitionGroups(tickets);

    expect(groups.map((group) => group.key)).toEqual(["list:source-2", "scope:10000002:60003760"]);
  });
});
