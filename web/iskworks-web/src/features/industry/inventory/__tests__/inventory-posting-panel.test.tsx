import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { InventoryItem, InventoryPreview } from "../../../../api/inventory";

const api = vi.hoisted(() => ({
  previewInventory: vi.fn(),
  postInventory: vi.fn(),
}));
vi.mock("../../../../api/inventory", async () => {
  const actual = await vi.importActual<typeof import("../../../../api/inventory")>("../../../../api/inventory");
  return { ...actual, previewInventory: api.previewInventory, postInventory: api.postInventory };
});

import { InventoryPostingPanel } from "../inventory-posting-panel";

function item(): InventoryItem {
  return {
    balance: {
      key: { workspaceId: "ws-1", ownerId: "o-1", typeId: 34 },
      typeName: "Tritanium",
      quantity: 0,
      totalHistoricalCost: "0.0000",
      averageUnitCost: null,
      revision: 0,
      lastActivityAt: "2026-07-25T12:00:00Z",
    },
    groupName: "Mineral",
    packagedVolumeM3: "0.01",
    totalVolumeM3: "0",
    reservedQuantity: 0,
    availableQuantity: 0,
    costQuality: "known",
    currentPrice: null,
    currentValue: null,
    historicalDifference: null,
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
  };
}

const preview: InventoryPreview = {
  current: item().balance,
  posting: { kind: "opening", quantityDelta: 10, totalCostDelta: "10000000.0000", unitCost: "1000000.0000", costQuality: "known" },
  resulting: { ...item().balance, quantity: 10, totalHistoricalCost: "10000000.0000", averageUnitCost: "1000000.0000", revision: 1 },
  warnings: [],
} as unknown as InventoryPreview;

function renderPanel(kind: "opening" | "purchase" = "purchase") {
  render(
    <InventoryPostingPanel
      items={[item()]}
      kind={kind}
      onCancel={vi.fn()}
      onSaved={vi.fn()}
      preselectedTypeId={34}
    />,
  );
}

describe("InventoryPostingPanel -- Unit cost (ISK) via MoneyInput", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.previewInventory.mockResolvedValue(preview);
  });

  test("accepts comma grouping and previews with the canonical unit cost", async () => {
    renderPanel();

    await userEvent.type(screen.getByLabelText("Quantity"), "10");
    await userEvent.type(screen.getByLabelText("Unit cost (ISK)"), "1,000,000");
    await userEvent.click(screen.getByRole("button", { name: "Preview" }));

    await waitFor(() => expect(api.previewInventory).toHaveBeenCalled());
    expect(api.previewInventory).toHaveBeenCalledWith(
      "purchase",
      expect.objectContaining({ unitCost: "1000000" }),
    );
  });

  test("blocks Preview and shows an inline error while the unit cost is invalid", async () => {
    renderPanel();

    await userEvent.type(screen.getByLabelText("Quantity"), "10");
    await userEvent.type(screen.getByLabelText("Unit cost (ISK)"), "1.234567");

    expect(await screen.findByRole("alert")).toHaveTextContent(/4 decimal places/i);

    await userEvent.click(screen.getByRole("button", { name: "Preview" }));
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(api.previewInventory).not.toHaveBeenCalled();
  });

  test("requires a non-empty unit cost for a known/estimated treatment", async () => {
    renderPanel();

    await userEvent.type(screen.getByLabelText("Quantity"), "10");
    await userEvent.click(screen.getByRole("button", { name: "Preview" }));

    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(api.previewInventory).not.toHaveBeenCalled();
    expect(screen.getByText(/Unit cost is required/i)).toBeInTheDocument();
  });
});

describe("InventoryPostingPanel -- opening balance cost", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.previewInventory.mockResolvedValue(preview);
  });

  test("has a single cost model: no treatment choice, just a unit cost", async () => {
    renderPanel("opening");

    expect(screen.queryByText("Cost treatment")).not.toBeInTheDocument();
    expect(screen.queryByText("Estimated cost")).not.toBeInTheDocument();
    expect(screen.queryByText("Zero cost")).not.toBeInTheDocument();

    await userEvent.type(screen.getByLabelText("Quantity"), "10");
    await userEvent.type(screen.getByLabelText("Unit cost (ISK)"), "5.25");
    await userEvent.click(screen.getByRole("button", { name: "Preview" }));

    await waitFor(() => expect(api.previewInventory).toHaveBeenCalled());
    expect(api.previewInventory).toHaveBeenCalledWith(
      "opening",
      expect.objectContaining({ unitCost: "5.25", costQuality: "known", acknowledgeZeroCost: false }),
    );
  });

  test("records something genuinely free as a unit cost of 0", async () => {
    renderPanel("opening");

    await userEvent.type(screen.getByLabelText("Quantity"), "10");
    await userEvent.type(screen.getByLabelText("Unit cost (ISK)"), "0");
    await userEvent.click(screen.getByRole("button", { name: "Preview" }));

    await waitFor(() => expect(api.previewInventory).toHaveBeenCalled());
    expect(api.previewInventory).toHaveBeenCalledWith(
      "opening",
      expect.objectContaining({ unitCost: "0", costQuality: "known" }),
    );
  });
});
