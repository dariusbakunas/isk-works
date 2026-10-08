import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { InventoryItem, InventoryReservation } from "../../../../api/inventory";
import { InventoryInspector } from "../inventory-inspector";

const api = vi.hoisted(() => ({
  getInventoryItem: vi.fn(),
  reverseInventoryEvent: vi.fn(),
  previewAdjustment: vi.fn(),
  postAdjustment: vi.fn(),
}));

vi.mock("../../../../api/inventory", async () => {
  const actual = await vi.importActual<typeof import("../../../../api/inventory")>("../../../../api/inventory");
  return {
    ...actual,
    getInventoryItem: api.getInventoryItem,
    reverseInventoryEvent: api.reverseInventoryEvent,
    previewAdjustment: api.previewAdjustment,
    postAdjustment: api.postAdjustment,
  };
});

function item(overrides: Partial<InventoryItem> = {}): InventoryItem {
  return {
    balance: {
      key: { workspaceId: "workspace-1", ownerId: "owner-1", typeId: 34 },
      typeName: "Tritanium",
      quantity: 150,
      totalHistoricalCost: "1800.0000",
      averageUnitCost: "12.0000",
      revision: 2,
      lastActivityAt: "2026-07-25T12:00:00Z",
    },
    groupName: "Mineral",
    packagedVolumeM3: "0.01",
    totalVolumeM3: "1.50",
    reservedQuantity: 40,
    availableQuantity: 110,
    costQuality: "known",
    currentPrice: "13.0000",
    currentValue: "1950.0000",
    historicalDifference: "150.0000",
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

function event(overrides: Partial<ReturnType<typeof baseEvent>> = {}) {
  return { ...baseEvent(), ...overrides };
}

function baseEvent() {
  return {
    id: "event-1",
    kind: "openingBalance" as const,
    quantityDelta: 150,
    totalCostDelta: "1800.0000",
    unitCost: "12.0000",
    costQuality: "known" as const,
    sourceReference: "Fixture",
    note: "",
    effectiveAt: "2026-07-25T12:00:00Z",
    recordedAt: "2026-07-25T12:00:01Z",
    sequence: 1,
    reversesEventId: null,
    reversedByEventId: null,
    buildId: null,
    buildCompletionId: null,
    resultingBalance: {
      key: { workspaceId: "workspace-1", ownerId: "owner-1", typeId: 34 },
      typeName: "Tritanium",
      quantity: 150,
      totalHistoricalCost: "1800.0000",
      averageUnitCost: "12.0000",
      revision: 2,
      lastActivityAt: "2026-07-25T12:00:00Z",
    },
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

function renderInspector(overrides: Partial<InventoryItem> = {}) {
  const onChanged = vi.fn();
  const onClose = vi.fn();
  const onOpenPosting = vi.fn();
  const onOpenAdjustment = vi.fn();
  const onViewEsiHoldings = vi.fn();
  render(
    <MemoryRouter>
      <InventoryInspector
        item={item(overrides)}
        onChanged={onChanged}
        onClose={onClose}
        onOpenAdjustment={onOpenAdjustment}
        onOpenPosting={onOpenPosting}
        onViewEsiHoldings={onViewEsiHoldings}
      />
    </MemoryRouter>,
  );
  return { onChanged, onClose, onOpenPosting, onOpenAdjustment, onViewEsiHoldings };
}

describe("InventoryInspector", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.getInventoryItem.mockResolvedValue({ ...item(), events: [event()], reservations: [] });
  });

  test("renders balance and cost immediately from the summary item, without waiting on a fetch", () => {
    renderInspector();

    expect(screen.getByRole("complementary", { name: "Tritanium" })).toBeInTheDocument();
    expect(screen.getByText("40")).toBeInTheDocument();
    expect(screen.getByText("110")).toBeInTheDocument();
    expect(screen.getByText("1,950 ISK")).toBeInTheDocument();
  });

  test("defaults to the Overview tab and loads event history/reservations in the background", async () => {
    renderInspector();

    expect(screen.getByRole("tab", { name: "Overview" })).toHaveAttribute("aria-selected", "true");
    await screen.findByRole("button", { name: "Record Purchase" });
    expect(api.getInventoryItem).toHaveBeenCalledWith(34, undefined);
  });

  test("switching to the History tab renders event history", async () => {
    const user = userEvent.setup();
    renderInspector();

    await user.click(screen.getByRole("tab", { name: "History" }));
    expect(await screen.findByText("Opening Balance")).toBeInTheDocument();
  });

  test("history only badges costs the system estimated, not ordinary entered costs", async () => {
    api.getInventoryItem.mockResolvedValue({
      ...item(),
      events: [event(), { ...event(), id: "evt-estimated", sequence: 2, costQuality: "estimated" }],
      reservations: [],
    });
    const user = userEvent.setup();
    renderInspector();

    await user.click(screen.getByRole("tab", { name: "History" }));
    expect(await screen.findByText("Estimated cost")).toBeInTheDocument();
    expect(screen.queryByText("Known cost")).not.toBeInTheDocument();
  });

  test("record purchase and opening balance buttons call onOpenPosting", async () => {
    const user = userEvent.setup();
    const { onOpenPosting } = renderInspector();

    await user.click(screen.getByRole("button", { name: "Record Purchase" }));
    expect(onOpenPosting).toHaveBeenCalledWith("purchase");
    await user.click(screen.getByRole("button", { name: "Opening Balance" }));
    expect(onOpenPosting).toHaveBeenCalledWith("opening");
  });

  test("adjust inventory button calls onOpenAdjustment", async () => {
    const user = userEvent.setup();
    const { onOpenAdjustment } = renderInspector();

    await user.click(screen.getByRole("button", { name: "Adjust Inventory" }));
    expect(onOpenAdjustment).toHaveBeenCalled();
  });

  test("reverses the latest event and refreshes both the panel and the parent list", async () => {
    api.reverseInventoryEvent.mockResolvedValue({ balance: item().balance, events: [] });
    const user = userEvent.setup();
    const { onChanged } = renderInspector();

    await user.click(screen.getByRole("tab", { name: "History" }));
    await user.click(await screen.findByRole("button", { name: "Reverse Event" }));
    const dialog = await screen.findByRole("dialog", { name: "Reverse Event" });
    await user.type(within(dialog).getByLabelText("Reversal reason"), "Mistaken entry");
    await user.click(within(dialog).getByRole("button", { name: "Reverse Event" }));

    expect(api.reverseInventoryEvent).toHaveBeenCalledWith(34, "event-1", 2, "Mistaken entry");
    expect(onChanged).toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  test("closing the inspector calls onClose", async () => {
    const user = userEvent.setup();
    const { onClose } = renderInspector();

    await user.click(screen.getByRole("button", { name: "Close item inspector" }));
    expect(onClose).toHaveBeenCalled();
  });

  test("the Reservations tab has no count badge and shows an empty state with no active reservations", async () => {
    const user = userEvent.setup();
    renderInspector();

    await user.click(screen.getByRole("tab", { name: "Reservations" }));
    expect(await screen.findByText("No active reservations")).toBeInTheDocument();
    // Exact-name match: fails if a stray count badge got appended to the tab's accessible name.
    expect(screen.getByRole("tab", { name: "Reservations" })).toBeInTheDocument();
  });

  test("the Reservations tab shows a count badge and lists Order/Ticket sources with a navigate link", async () => {
    api.getInventoryItem.mockResolvedValue({
      ...item(),
      events: [event()],
      reservations: [
        reservation(),
        reservation({
          allocationId: "allocation-2",
          quantity: 800,
          source: { kind: "ticket", ticketId: "ticket-1", displayId: "ISK-1852", status: "todo" },
        }),
      ],
    });
    const user = userEvent.setup();
    renderInspector();

    await screen.findByText("2"); // the count badge on the Reservations tab
    await user.click(screen.getByRole("tab", { name: /^Reservations/ }));

    expect(screen.getByText("Manufacture Ishtar")).toBeInTheDocument();
    expect(screen.getByText("ISK-1852")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Navigate to order →" })).toHaveAttribute("href", "/orders/order-1");
    expect(screen.getByRole("link", { name: "Navigate to ticket →" })).toHaveAttribute("href", "/board?ticket=ticket-1");
  });

  test("a negative-available item shows the shortfall banner on the Reservations tab", async () => {
    api.getInventoryItem.mockResolvedValue({
      ...item({ availableQuantity: -400, reservedQuantity: 1_200 }),
      events: [event()],
      reservations: [reservation({ quantity: 1_200 })],
    });
    const user = userEvent.setup();
    renderInspector({ availableQuantity: -400, reservedQuantity: 1_200 });

    await user.click(screen.getByRole("tab", { name: /^Reservations/ }));
    expect(await screen.findByText("Shortfall: 400 units")).toBeInTheDocument();
  });

  test("Overview shows no ESI observation exists when esiObservedQuantity is null", () => {
    renderInspector({ esiObservedQuantity: null });
    expect(screen.getByText("No ESI observation exists for this item yet.")).toBeInTheDocument();
  });

  test("Overview shows a healthy Match when ESI observation equals owned quantity", () => {
    renderInspector({
      balance: { ...item().balance, quantity: 42_000 },
      esiObservedQuantity: 42_000,
      esiObservedAt: "2026-08-23T09:00:00Z",
    });

    expect(screen.getByText("Match")).toBeInTheDocument();
    expect(screen.getByText(/As of/)).toBeInTheDocument();
  });

  test("Overview shows a signed difference for an ESI discrepancy", () => {
    renderInspector({
      balance: { ...item().balance, quantity: 2_800 },
      esiObservedQuantity: 3_200,
      reconciliationDifference: 400,
    });

    expect(screen.getByText("+400 units")).toBeInTheDocument();
  });

  test("no Review discrepancy button is shown when ESI matches accounting", () => {
    renderInspector({
      balance: { ...item().balance, quantity: 42_000 },
      esiObservedQuantity: 42_000,
    });
    expect(screen.queryByRole("button", { name: "Review discrepancy" })).not.toBeInTheDocument();
  });

  test("View ESI holdings button is shown for a Match row and calls onViewEsiHoldings", async () => {
    const user = userEvent.setup();
    const { onViewEsiHoldings } = renderInspector({
      balance: { ...item().balance, quantity: 42_000 },
      esiObservedQuantity: 42_000,
    });

    const button = screen.getByRole("button", { name: "View ESI holdings" });
    await user.click(button);
    expect(onViewEsiHoldings).toHaveBeenCalled();
  });

  test("the inspector delegates discrepancy review to ESI holdings", () => {
    renderInspector({
      balance: { ...item().balance, quantity: 2_800 },
      esiObservedQuantity: 3_200,
      reconciliationDifference: 400,
    });

    expect(screen.getByRole("button", { name: "View ESI holdings" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Review discrepancy" })).not.toBeInTheDocument();
  });

  test("no View ESI holdings button when there is no ESI observation for this item", () => {
    renderInspector({ esiObservedQuantity: null });
    expect(screen.queryByRole("button", { name: "View ESI holdings" })).not.toBeInTheDocument();
  });

});
