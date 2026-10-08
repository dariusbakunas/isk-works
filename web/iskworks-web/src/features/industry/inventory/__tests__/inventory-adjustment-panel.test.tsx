import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { InventoryItem, InventoryPreview } from "../../../../api/inventory";
import { InventoryAdjustmentPanel } from "../inventory-adjustment-panel";

const api = vi.hoisted(() => ({
  previewAdjustment: vi.fn(),
  postAdjustment: vi.fn(),
}));

vi.mock("../../../../api/inventory", async () => {
  const actual = await vi.importActual<typeof import("../../../../api/inventory")>("../../../../api/inventory");
  return { ...actual, previewAdjustment: api.previewAdjustment, postAdjustment: api.postAdjustment };
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
    reservedQuantity: 0,
    availableQuantity: 150,
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

function preview(overrides: Partial<InventoryPreview> = {}): InventoryPreview {
  return {
    current: item().balance,
    posting: {
      kind: "adjustment",
      quantityDelta: 20,
      totalCostDelta: "240.0000",
      unitCost: "12.0000",
      costQuality: "known",
    },
    resulting: {
      key: { workspaceId: "workspace-1", ownerId: "owner-1", typeId: 34 },
      typeName: "Tritanium",
      quantity: 170,
      totalHistoricalCost: "2040.0000",
      averageUnitCost: "12.0000",
      revision: 3,
      lastActivityAt: "2026-07-25T12:00:00Z",
    },
    warnings: [],
    ...overrides,
  };
}

function renderPanel(items: InventoryItem[] = [item()], preselectedTypeId = 34) {
  const onCancel = vi.fn();
  const onSaved = vi.fn();
  render(
    <InventoryAdjustmentPanel
      items={items}
      preselectedTypeId={preselectedTypeId}
      onCancel={onCancel}
      onSaved={onSaved}
    />,
  );
  return { onCancel, onSaved };
}

describe("InventoryAdjustmentPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  test("positive adjustment on an item with an existing cost basis previews without requiring a unit cost", async () => {
    api.previewAdjustment.mockResolvedValue(preview());
    const user = userEvent.setup();
    renderPanel();

    await user.type(screen.getByLabelText("Quantity"), "20");
    await user.click(screen.getByRole("button", { name: "Preview" }));

    expect(api.previewAdjustment).toHaveBeenCalledWith(
      expect.objectContaining({ typeId: 34, quantityDelta: 20, unitCost: null, expectedRevision: 2 }),
    );
    expect(await screen.findByText("Adjustment")).toBeInTheDocument();
    expect(screen.getByText("170")).toBeInTheDocument();
  });

  test("positive adjustment on a brand-new item requires an explicit unit cost before previewing", async () => {
    const user = userEvent.setup();
    renderPanel(
      [item({ balance: { ...item().balance, averageUnitCost: null, quantity: 0, totalHistoricalCost: "0.0000" } })],
      34,
    );

    await user.type(screen.getByLabelText("Quantity"), "20");
    await user.click(screen.getByRole("button", { name: "Preview" }));

    expect(api.previewAdjustment).not.toHaveBeenCalled();
    expect(await screen.findByText("Adjustment not ready")).toBeInTheDocument();
  });

  test("removing quantity does not show a unit cost field and previews a negative delta", async () => {
    api.previewAdjustment.mockResolvedValue(
      preview({
        posting: { kind: "adjustment", quantityDelta: -30, totalCostDelta: "-360.0000", unitCost: "12.0000", costQuality: "known" },
        resulting: { ...preview().resulting, quantity: 120, totalHistoricalCost: "1440.0000" },
      }),
    );
    const user = userEvent.setup();
    renderPanel();

    await user.click(screen.getByRole("radio", { name: "Remove quantity" }));
    expect(screen.queryByText("Unit cost (ISK)", { exact: false })).not.toBeInTheDocument();
    await user.type(screen.getByLabelText("Quantity"), "30");
    await user.click(screen.getByRole("button", { name: "Preview" }));

    expect(api.previewAdjustment).toHaveBeenCalledWith(
      expect.objectContaining({ quantityDelta: -30, unitCost: null }),
    );
    expect(await screen.findByText("120")).toBeInTheDocument();
  });

  test("accepts comma grouping and previews with the canonical unit cost", async () => {
    api.previewAdjustment.mockResolvedValue(preview());
    const user = userEvent.setup();
    renderPanel();

    await user.type(screen.getByLabelText("Quantity"), "20");
    await user.type(screen.getByLabelText("Unit cost (ISK)"), "1,000,000");
    await user.click(screen.getByRole("button", { name: "Preview" }));

    expect(api.previewAdjustment).toHaveBeenCalledWith(
      expect.objectContaining({ unitCost: "1000000" }),
    );
  });

  test("blocks Preview and shows an inline error while the unit cost is invalid", async () => {
    const user = userEvent.setup();
    renderPanel();

    await user.type(screen.getByLabelText("Quantity"), "20");
    await user.type(screen.getByLabelText("Unit cost (ISK)"), "1.234567");

    expect(await screen.findByRole("alert")).toHaveTextContent(/4 decimal places/i);

    await user.click(screen.getByRole("button", { name: "Preview" }));
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(api.previewAdjustment).not.toHaveBeenCalled();
  });

  test("clearing an optional unit cost falls back to the weighted average (null)", async () => {
    api.previewAdjustment.mockResolvedValue(preview());
    const user = userEvent.setup();
    renderPanel();

    await user.type(screen.getByLabelText("Quantity"), "20");
    await user.type(screen.getByLabelText("Unit cost (ISK)"), "500");
    await user.clear(screen.getByLabelText("Unit cost (ISK)"));
    screen.getByLabelText("Unit cost (ISK)").blur();
    await user.click(screen.getByRole("button", { name: "Preview" }));

    expect(api.previewAdjustment).toHaveBeenCalledWith(
      expect.objectContaining({ unitCost: null }),
    );
  });

  test("recording the adjustment posts and calls onSaved", async () => {
    api.previewAdjustment.mockResolvedValue(preview());
    api.postAdjustment.mockResolvedValue({ balance: preview().resulting, events: [] });
    const user = userEvent.setup();
    const { onSaved } = renderPanel();

    await user.type(screen.getByLabelText("Quantity"), "20");
    await user.click(screen.getByRole("button", { name: "Preview" }));
    await screen.findByText("Adjustment");
    await user.click(screen.getByRole("button", { name: "Record Event" }));

    expect(api.postAdjustment).toHaveBeenCalledWith(
      expect.objectContaining({ typeId: 34, quantityDelta: 20 }),
    );
    expect(onSaved).toHaveBeenCalled();
  });
});
