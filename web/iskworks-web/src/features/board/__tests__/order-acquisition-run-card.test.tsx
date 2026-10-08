import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { MarketScope, Ticket } from "../../../api/industry";
import { OrderAcquisitionRunCard } from "../order-acquisition-run-card";
import { runFixture } from "./fixtures";

vi.mock("../../../hooks/use-market-scope-label", () => ({
  useMarketScopeLabel: (scope: MarketScope | null) =>
    scope === null
      ? { regionName: "", locationName: "" }
      : {
          regionName: "The Forge",
          locationName:
            scope.locationId === undefined
              ? "All locations"
              : "Jita IV - Moon 4 - Caldari Navy Assembly Plant",
        },
}));

function ticketFixture(overrides: Partial<Ticket> = {}): Ticket {
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
    marketRegionId: null,
    marketLocationId: null,
    priceSourceId: "source-1",
    acquisitionRunId: "run-1",
    acquiredQuantity: null,
    executionSnapshot: null,
    createdAt: "2026-08-22T00:00:00Z",
    updatedAt: "2026-08-22T00:00:00Z",
    archivedAt: null,
    ...overrides,
  };
}

describe("OrderAcquisitionRunCard", () => {
  it("shows the resolved manual price list rather than a plan count", () => {
    const tickets = [ticketFixture({ id: "t1" })];
    render(<OrderAcquisitionRunCard priceSourceName="Jita 4-4" run={runFixture()} tickets={tickets} />);

    expect(screen.getByText("Price list: Jita 4-4")).toBeInTheDocument();
    expect(screen.getByText("ACQ-0042")).toBeInTheDocument();
    expect(screen.getByText("Tuesday Jita Run")).toBeInTheDocument();
  });

  it("shows a market-scoped Run's scope, never 'Unknown source'", () => {
    const run = runFixture({ marketRegionId: 10_000_002, marketLocationId: 60_003_760, priceSourceId: null });
    render(<OrderAcquisitionRunCard run={run} tickets={[ticketFixture({ id: "t1" })]} />);

    expect(screen.getByText("Priced at Jita IV - Moon 4 - Caldari Navy Assembly Plant")).toBeInTheDocument();
    expect(screen.queryByText("Unknown source")).not.toBeInTheDocument();
  });

  it("collapses to 3 items plus a remainder count beyond the preview limit, while Ready", () => {
    const tickets = [
      ticketFixture({ id: "t1", typeId: 1, capturedName: "Zydrine", quantity: 100 }),
      ticketFixture({ id: "t2", typeId: 2, capturedName: "Nocxium", quantity: 50 }),
      ticketFixture({ id: "t3", typeId: 3, capturedName: "Megacyte", quantity: 20 }),
      ticketFixture({ id: "t4", typeId: 4, capturedName: "Isogen", quantity: 10 }),
    ];
    render(<OrderAcquisitionRunCard priceSourceName="Jita 4-4" run={runFixture()} tickets={tickets} />);

    expect(screen.getByText("+ 1 more items")).toBeInTheDocument();
  });

  it("shows progress while In Progress, including over-acquisition beyond each ticket's own demand", () => {
    const tickets = [
      ticketFixture({ id: "t1", typeId: 1, quantity: 100, acquiredQuantity: 150 }),
      ticketFixture({ id: "t2", typeId: 2, quantity: 50, acquiredQuantity: 0 }),
    ];
    render(
      <OrderAcquisitionRunCard priceSourceName="Jita 4-4" run={runFixture({ status: "inProgress" })} tickets={tickets} />,
    );

    expect(screen.getByText("1/2 items fully acquired")).toBeInTheDocument();
  });

  it("shows a closed summary once Complete", () => {
    const tickets = [ticketFixture({ id: "t1" })];
    render(
      <OrderAcquisitionRunCard priceSourceName="Jita 4-4" run={runFixture({ status: "complete" })} tickets={tickets} />,
    );

    expect(screen.getByText(/All 1 items acquired/)).toBeInTheDocument();
  });
});
