import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { MarketScope, TicketSummary } from "../../../api/industry";
import { acquisitionPricingIdentity } from "../acquisition-pricing";
import type { OrderAcquisitionGroup } from "../order-acq-group";
import { OrderAcqGroupCard } from "../order-acq-group-card";
import { makeDataTransfer } from "./fixtures";

// Resolve market-scope names synchronously so the descriptor logic (which
// is what these tests exercise) runs against real names, without touching
// the network.
vi.mock("../../../hooks/use-market-scope-label", () => ({
  useMarketScopeLabel: (scope: MarketScope | null) => {
    if (scope === null) return { regionName: "", locationName: "" };
    const regionName = scope.regionId === 10_000_002 ? "The Forge" : `Region ${scope.regionId}`;
    if (scope.locationId === undefined) return { regionName, locationName: "All locations" };
    const locationName =
      scope.locationId === 60_003_760
        ? "Jita IV - Moon 4 - Caldari Navy Assembly Plant"
        : `Location ${scope.locationId}`;
    return { regionName, locationName };
  },
}));

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

function groupFrom(tickets: TicketSummary[]): OrderAcquisitionGroup {
  const identity = acquisitionPricingIdentity(tickets[0]);
  return {
    key:
      identity.kind === "scope"
        ? `scope:${identity.marketRegionId}:${identity.marketLocationId ?? ""}`
        : identity.kind === "list"
          ? `list:${identity.priceSourceId}`
          : "none",
    marketRegionId: identity.kind === "scope" ? identity.marketRegionId : null,
    marketLocationId: identity.kind === "scope" ? identity.marketLocationId : null,
    priceSourceId: identity.kind === "list" ? identity.priceSourceId : null,
    batchable: identity.kind !== "none",
    tickets,
  };
}

const manualTicket = (overrides: Partial<TicketSummary> = {}) =>
  ticketFixture({ marketRegionId: null, marketLocationId: null, priceSourceId: "source-1", ...overrides });
const noSourceTicket = (overrides: Partial<TicketSummary> = {}) =>
  ticketFixture({ marketRegionId: null, marketLocationId: null, priceSourceId: null, ...overrides });

describe("OrderAcqGroupCard pricing descriptor", () => {
  it("renders a market scope as a station, not 'Unknown source'", () => {
    render(
      <OrderAcqGroupCard expanded={false} group={groupFrom([ticketFixture()])} onToggleExpand={() => {}} />,
    );

    expect(screen.getAllByText("Priced at Jita IV - Moon 4 - Caldari Navy Assembly Plant").length).toBeGreaterThan(0);
    expect(screen.queryByText("Unknown source")).not.toBeInTheDocument();
  });

  it("renders a region-wide market scope as 'Priced in <region>'", () => {
    render(
      <OrderAcqGroupCard
        expanded={false}
        group={groupFrom([ticketFixture({ marketLocationId: null })])}
        onToggleExpand={() => {}}
      />,
    );

    expect(screen.getAllByText("Priced in The Forge").length).toBeGreaterThan(0);
  });

  it("renders a manual price list as a list, not a location", () => {
    render(
      <OrderAcqGroupCard
        expanded={false}
        group={groupFrom([manualTicket()])}
        onToggleExpand={() => {}}
        priceSourceName="Weekend buy list"
      />,
    );

    expect(screen.getAllByText("Price list: Weekend buy list").length).toBeGreaterThan(0);
    expect(screen.queryByText("source-1")).not.toBeInTheDocument();
    expect(screen.queryByText("Unknown source")).not.toBeInTheDocument();
  });

  it("renders tickets with no scope and no list as 'No pricing source' and warns they can't be batched", () => {
    render(
      <OrderAcqGroupCard expanded={false} group={groupFrom([noSourceTicket()])} onToggleExpand={() => {}} />,
    );

    expect(screen.getAllByText("No pricing source").length).toBeGreaterThan(0);
    expect(screen.getByText(/can.t be added to an Acquisition Run/i)).toBeInTheDocument();
  });
});

describe("OrderAcqGroupCard collapsed state", () => {
  it("shows a material count, up to 3 preview rows, and a remainder line beyond the limit", () => {
    const tickets = [
      ticketFixture({ id: "t1", capturedName: "Zydrine", quantity: 100 }),
      ticketFixture({ id: "t2", capturedName: "Nocxium", quantity: 50 }),
      ticketFixture({ id: "t3", capturedName: "Megacyte", quantity: 20 }),
      ticketFixture({ id: "t4", capturedName: "Isogen", quantity: 10 }),
    ];
    render(<OrderAcqGroupCard expanded={false} group={groupFrom(tickets)} onToggleExpand={() => {}} />);

    expect(screen.getByText("ACQ GROUP")).toBeInTheDocument();
    expect(screen.getByText("4 materials")).toBeInTheDocument();
    expect(screen.getByText("Zydrine")).toBeInTheDocument();
    expect(screen.getByText("Nocxium")).toBeInTheDocument();
    expect(screen.getByText("Megacyte")).toBeInTheDocument();
    expect(screen.queryByText("Isogen")).not.toBeInTheDocument();
    expect(screen.getByText("+ 1 more")).toBeInTheDocument();
  });

  it("labels a fully-complete group ACQUIRED", () => {
    render(
      <OrderAcqGroupCard
        expanded={false}
        group={groupFrom([ticketFixture({ status: "complete" })])}
        onToggleExpand={() => {}}
      />,
    );

    expect(screen.getByText("ACQUIRED")).toBeInTheDocument();
  });

  it("toggles expansion when the collapsed card is clicked outside selection mode", async () => {
    const onToggleExpand = vi.fn();
    render(
      <OrderAcqGroupCard expanded={false} group={groupFrom([ticketFixture()])} onToggleExpand={onToggleExpand} />,
    );

    await userEvent.click(screen.getByText("ACQ GROUP"));
    expect(onToggleExpand).toHaveBeenCalledTimes(1);
  });
});

describe("OrderAcqGroupCard expanded state", () => {
  it("reveals each member ticket", () => {
    const tickets = [
      ticketFixture({ id: "t1", displayId: "ISK-2000", capturedName: "Zydrine" }),
      ticketFixture({ id: "t2", displayId: "ISK-2001", capturedName: "Isogen" }),
    ];
    render(<OrderAcqGroupCard expanded group={groupFrom(tickets)} onToggleExpand={() => {}} />);

    expect(screen.getByText("ISK-2000")).toBeInTheDocument();
    expect(screen.getByText("ISK-2001")).toBeInTheDocument();
    expect(screen.getByText("Zydrine")).toBeInTheDocument();
    expect(screen.getByText("Isogen")).toBeInTheDocument();
  });

  it("selects and deselects an individual member ticket", async () => {
    const onToggleSelect = vi.fn();
    render(
      <OrderAcqGroupCard
        expanded
        group={groupFrom([ticketFixture({ id: "t1", displayId: "ISK-2000" })])}
        onToggleExpand={() => {}}
        onToggleSelect={onToggleSelect}
        selectedIds={new Set()}
        selecting
      />,
    );

    await userEvent.click(screen.getByLabelText("Select ISK-2000"));
    expect(onToggleSelect).toHaveBeenCalledWith("t1");
  });

  it("only Acquisition tickets are selectable -- a stray non-Acquisition member is never batchable", () => {
    render(
      <OrderAcqGroupCard
        expanded
        group={groupFrom([ticketFixture({ id: "t1", displayId: "ISK-2000", kind: "manufacturing" })])}
        onToggleExpand={() => {}}
        selectedIds={new Set()}
        selecting
      />,
    );

    expect(screen.getByLabelText("Select ISK-2000")).toBeDisabled();
  });

  it("'Select N actionable' selects every batchable member at once", async () => {
    const onSelectAll = vi.fn();
    const tickets = [
      ticketFixture({ id: "t1", displayId: "ISK-2000" }),
      ticketFixture({ id: "t2", displayId: "ISK-2001" }),
    ];
    render(
      <OrderAcqGroupCard
        expanded
        group={groupFrom(tickets)}
        onSelectAll={onSelectAll}
        onToggleExpand={() => {}}
        selectedIds={new Set()}
        selecting
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: "Select 2 actionable" }));
    expect(onSelectAll).toHaveBeenCalledWith(["t1", "t2"]);
  });

  it("a non-batchable 'none' group offers no select-all and no per-row checkboxes", () => {
    const tickets = [
      noSourceTicket({ id: "t1", displayId: "ISK-2000" }),
      noSourceTicket({ id: "t2", displayId: "ISK-2001" }),
    ];
    render(
      <OrderAcqGroupCard
        expanded
        group={groupFrom(tickets)}
        onSelectAll={vi.fn()}
        onToggleExpand={() => {}}
        selectedIds={new Set()}
        selecting
      />,
    );

    expect(screen.queryByRole("button", { name: /Select \d+ actionable/ })).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Select ISK-2000")).not.toBeInTheDocument();
    expect(screen.getByText(/can.t be added to an Acquisition Run/i)).toBeInTheDocument();
  });

  it("a Ready member ticket is draggable and reports drag start/end", () => {
    const onTicketDragStart = vi.fn();
    const onTicketDragEnd = vi.fn();
    render(
      <OrderAcqGroupCard
        expanded
        group={groupFrom([ticketFixture({ id: "t1", displayId: "ISK-2000", status: "todo" })])}
        onToggleExpand={() => {}}
        onTicketDragEnd={onTicketDragEnd}
        onTicketDragStart={onTicketDragStart}
      />,
    );

    const row = screen.getByText("ISK-2000").closest("[draggable]") as HTMLElement;
    expect(row).toHaveAttribute("draggable", "true");

    fireEvent.dragStart(row, { dataTransfer: makeDataTransfer() });
    fireEvent.dragEnd(row);

    expect(onTicketDragStart).toHaveBeenCalledWith("t1");
    expect(onTicketDragEnd).toHaveBeenCalled();
  });

  it("a Complete member ticket stays draggable -- backward lane moves are allowed", () => {
    render(
      <OrderAcqGroupCard
        expanded
        group={groupFrom([ticketFixture({ id: "t1", displayId: "ISK-2000", status: "complete" })])}
        onToggleExpand={() => {}}
      />,
    );

    expect(screen.getByText("ISK-2000").closest("[draggable]")).toHaveAttribute("draggable", "true");
  });

  it("is not draggable while in selection mode", () => {
    render(
      <OrderAcqGroupCard
        expanded
        group={groupFrom([ticketFixture({ id: "t1", displayId: "ISK-2000", status: "todo" })])}
        onToggleExpand={() => {}}
        selectedIds={new Set()}
        selecting
      />,
    );

    expect(screen.getByText("ISK-2000").closest("[draggable]")).toHaveAttribute("draggable", "false");
  });
});

describe("OrderAcqGroupCard inspected ticket", () => {
  const tickets = [
    ticketFixture({ id: "t1", displayId: "ISK-2000" }),
    ticketFixture({ id: "t2", displayId: "ISK-2001", capturedName: "Pyerite", typeId: 35 }),
  ];
  const marked = () => [...document.querySelectorAll('[aria-current="true"]')];

  it("marks the group while collapsed", () => {
    render(
      <OrderAcqGroupCard activeTicketId="t2" expanded={false} group={groupFrom(tickets)} onToggleExpand={() => {}} />,
    );

    expect(marked()).toHaveLength(1);
    expect(marked()[0]).toHaveTextContent("ACQ GROUP");
  });

  it("marks the ticket's own row once expanded", () => {
    render(<OrderAcqGroupCard activeTicketId="t2" expanded group={groupFrom(tickets)} onToggleExpand={() => {}} />);

    expect(marked()).toHaveLength(1);
    expect(marked()[0]).toHaveTextContent("ISK-2001");
  });

  it("marks nothing when the inspected ticket is elsewhere", () => {
    render(
      <OrderAcqGroupCard activeTicketId="other" expanded={false} group={groupFrom(tickets)} onToggleExpand={() => {}} />,
    );

    expect(marked()).toHaveLength(0);
  });
});
