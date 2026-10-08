import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, expect, test } from "vitest";

import type { InventoryItem, InventoryReservation } from "../../../../api/inventory";
import { InventoryReservationsTab } from "../inventory-reservations-tab";

function item(overrides: Partial<InventoryItem> = {}): InventoryItem {
  return {
    balance: {
      key: { workspaceId: "workspace-1", ownerId: "owner-1", typeId: 34 },
      typeName: "Tritanium",
      quantity: 800,
      totalHistoricalCost: "800.0000",
      averageUnitCost: "1.0000",
      revision: 1,
      lastActivityAt: null,
    },
    groupName: "Mineral",
    packagedVolumeM3: "0.01",
    totalVolumeM3: "8.00",
    reservedQuantity: 1_200,
    availableQuantity: -400,
    costQuality: "known",
    currentPrice: "1.0000",
    currentValue: "800.0000",
    historicalDifference: "0.0000",
    historicalComparisonComplete: true,
    priceSourceId: null,
    priceSourceName: null,
    priceSourceUpdatedAt: null,
    marketRegionId: null,
    marketLocationId: null,
    esiObservedQuantity: null,
    esiObservedAt: null,
    reconciliationDifference: null,
    warnings: [],
    ...overrides,
  };
}

function reservation(overrides: Partial<InventoryReservation> = {}): InventoryReservation {
  return {
    allocationId: "allocation-1",
    quantity: 400,
    createdAt: "2026-07-25T12:00:00Z",
    source: { kind: "order", orderId: "order-1", displayName: "Manufacture Ishtar", status: "inProgress" },
    ...overrides,
  };
}

function renderTab(reservations: InventoryReservation[], itemOverrides: Partial<InventoryItem> = {}) {
  render(
    <MemoryRouter>
      <InventoryReservationsTab item={item(itemOverrides)} reservations={reservations} />
    </MemoryRouter>,
  );
}

describe("InventoryReservationsTab", () => {
  test("shows an empty state with no reservations", () => {
    renderTab([]);
    expect(screen.getByText("No active reservations")).toBeInTheDocument();
  });

  test("renders an Order reservation with its status and a link to the order", () => {
    renderTab([reservation()], { availableQuantity: 400, reservedQuantity: 400 });

    expect(screen.getByText("ORD")).toBeInTheDocument();
    expect(screen.getByText("Manufacture Ishtar")).toBeInTheDocument();
    // Summary line ("400 reserved") plus the card's own quantity figure.
    expect(screen.getAllByText("400")).toHaveLength(2);
    expect(screen.getByText("In Progress")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Navigate to order →" })).toHaveAttribute("href", "/orders/order-1");
    expect(screen.queryByText(/Shortfall/)).not.toBeInTheDocument();
  });

  test("renders a Ticket reservation with its status and a link to the board", () => {
    renderTab([
      reservation({
        source: { kind: "ticket", ticketId: "ticket-1", displayId: "ISK-1852", status: "todo" },
      }),
    ]);

    expect(screen.getByText("TKT")).toBeInTheDocument();
    expect(screen.getByText("ISK-1852")).toBeInTheDocument();
    expect(screen.getByText("To Do")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Navigate to ticket →" })).toHaveAttribute("href", "/board?ticket=ticket-1");
  });

  test("shows a shortfall banner when available quantity is negative", () => {
    renderTab([reservation({ quantity: 1_200 })]);

    expect(screen.getByText("Shortfall: 400 units")).toBeInTheDocument();
    expect(screen.getByText(/Owned 800, reserved 1,200/)).toBeInTheDocument();
  });

  test("does not show a shortfall banner when reservations exactly match owned quantity", () => {
    renderTab([reservation({ quantity: 800 })], { availableQuantity: 0, reservedQuantity: 800 });
    expect(screen.queryByText(/Shortfall/)).not.toBeInTheDocument();
  });
});
