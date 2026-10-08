import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { InventoryItem, InventoryPreview } from "../../../../api/inventory";
import { InventoryReviewDiscrepancyPanel } from "../inventory-review-discrepancy-panel";

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
      typeName: "Nocxium",
      quantity: 2_800,
      totalHistoricalCost: "5600.0000",
      averageUnitCost: "2.0000",
      revision: 3,
      lastActivityAt: "2026-08-20T00:00:00Z",
    },
    groupName: "Mineral",
    packagedVolumeM3: "0.01",
    totalVolumeM3: "28.00",
    reservedQuantity: 0,
    availableQuantity: 2_800,
    costQuality: "known",
    currentPrice: "3.0000",
    currentValue: "8400.0000",
    historicalDifference: "2800.0000",
    historicalComparisonComplete: true,
    priceSourceId: null,
    priceSourceName: null,
    priceSourceUpdatedAt: null,
    marketRegionId: null,
    marketLocationId: null,
    esiObservedQuantity: 3_200,
    esiObservedAt: "2026-08-23T12:00:00Z",
    reconciliationDifference: 400,
    warnings: [],
    ...overrides,
  };
}

function preview(overrides: Partial<InventoryPreview> = {}): InventoryPreview {
  return {
    current: item().balance,
    posting: {
      kind: "adjustment",
      quantityDelta: 400,
      totalCostDelta: "800.0000",
      unitCost: "2.0000",
      costQuality: "known",
    },
    resulting: {
      ...item().balance,
      quantity: 3_200,
      totalHistoricalCost: "6400.0000",
      revision: 4,
    },
    warnings: [],
    ...overrides,
  };
}

function renderPanel(itemOverrides: Partial<InventoryItem> = {}) {
  const onCancel = vi.fn();
  const onSaved = vi.fn();
  render(<InventoryReviewDiscrepancyPanel item={item(itemOverrides)} onCancel={onCancel} onSaved={onSaved} />);
  return { onCancel, onSaved };
}

describe("InventoryReviewDiscrepancyPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  test("a positive discrepancy on an item with an existing cost basis previews automatically with no unit cost", async () => {
    api.previewAdjustment.mockResolvedValue(preview());
    renderPanel();

    expect(await screen.findByText("Adjustment value")).toBeInTheDocument();
    expect(api.previewAdjustment).toHaveBeenCalledWith(
      expect.objectContaining({ typeId: 34, quantityDelta: 400, unitCost: null, expectedRevision: 3 }),
    );
    expect(screen.getAllByText("+400").length).toBeGreaterThan(0);
    expect(screen.getAllByText("3,200").length).toBeGreaterThan(0);
  });

  test("a negative discrepancy previews automatically and never carries a unit cost", async () => {
    api.previewAdjustment.mockResolvedValue(
      preview({
        posting: { kind: "adjustment", quantityDelta: -600, totalCostDelta: "-1200.0000", unitCost: "2.0000", costQuality: "known" },
        resulting: { ...preview().resulting, quantity: 2_200, revision: 4 },
      }),
    );
    renderPanel({ esiObservedQuantity: 2_200, reconciliationDifference: -600 });

    expect(await screen.findByText("Adjustment value")).toBeInTheDocument();
    expect(api.previewAdjustment).toHaveBeenCalledWith(
      expect.objectContaining({ quantityDelta: -600, unitCost: null }),
    );
    expect(screen.getAllByText("-600").length).toBeGreaterThan(0);
  });

  test("confirming the review shows the before/after preview, and Post adjustment records it", async () => {
    api.previewAdjustment.mockResolvedValue(preview());
    api.postAdjustment.mockResolvedValue({ balance: preview().resulting, events: [] });
    const user = userEvent.setup();
    const { onSaved } = renderPanel();

    await screen.findByText("Adjustment value");
    await user.click(screen.getByRole("button", { name: "Confirm adjustment" }));

    expect(await screen.findByText("Posting preview")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Post adjustment" }));

    expect(api.postAdjustment).toHaveBeenCalledWith(
      expect.objectContaining({ typeId: 34, quantityDelta: 400 }),
    );
    expect(onSaved).toHaveBeenCalled();
  });

  test("a brand-new item with no cost basis requires a unit cost before it will preview", async () => {
    const user = userEvent.setup();
    renderPanel({
      balance: { ...item().balance, quantity: 0, averageUnitCost: null, totalHistoricalCost: "0.0000" },
      esiObservedQuantity: 50,
      reconciliationDifference: 50,
    });

    expect(screen.getByText("New item -- cost required")).toBeInTheDocument();
    expect(screen.getByText("Market reference: 3 ISK / unit", { exact: false })).toBeInTheDocument();
    expect(api.previewAdjustment).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Create adjustment" })).toBeDisabled();

    await user.type(screen.getByLabelText("Unit cost (ISK)"), "82400");
    expect(screen.getByRole("button", { name: "Create adjustment" })).toBeEnabled();
    expect(screen.getAllByText("+50").length).toBeGreaterThan(0);

    api.previewAdjustment.mockResolvedValue(
      preview({
        posting: { kind: "adjustment", quantityDelta: 50, totalCostDelta: "4120000.0000", unitCost: "82400.0000", costQuality: "known" },
        resulting: { ...preview().resulting, quantity: 50, averageUnitCost: "82400.0000" },
      }),
    );
    await user.click(screen.getByRole("button", { name: "Create adjustment" }));

    expect(api.previewAdjustment).toHaveBeenCalledWith(
      expect.objectContaining({ quantityDelta: 50, unitCost: "82400" }),
    );
    expect(await screen.findByText("Posting preview")).toBeInTheDocument();
  });

  describe("Unit cost (ISK) via MoneyInput", () => {
    function newItemPanel() {
      return renderPanel({
        balance: { ...item().balance, quantity: 0, averageUnitCost: null, totalHistoricalCost: "0.0000" },
        esiObservedQuantity: 50,
        reconciliationDifference: 50,
      });
    }

    test("accepts comma grouping and previews with the canonical unit cost", async () => {
      const user = userEvent.setup();
      newItemPanel();
      api.previewAdjustment.mockResolvedValue(preview());

      await user.type(screen.getByLabelText("Unit cost (ISK)"), "1,000");
      expect(screen.getByRole("button", { name: "Create adjustment" })).toBeEnabled();
      await user.click(screen.getByRole("button", { name: "Create adjustment" }));

      expect(api.previewAdjustment).toHaveBeenCalledWith(
        expect.objectContaining({ quantityDelta: 50, unitCost: "1000" }),
      );
    });

    test("rejects malformed grouping locally and blocks Create", async () => {
      const user = userEvent.setup();
      newItemPanel();

      await user.type(screen.getByLabelText("Unit cost (ISK)"), "1,00");
      screen.getByLabelText("Unit cost (ISK)").blur();

      expect(await screen.findByRole("alert")).toHaveTextContent(/commas only every 3 digits/i);
      expect(screen.getByRole("button", { name: "Create adjustment" })).toBeDisabled();
      expect(api.previewAdjustment).not.toHaveBeenCalled();
    });

    test("keeps the stricter positive-only rule -- zero is not accepted", async () => {
      const user = userEvent.setup();
      newItemPanel();

      await user.type(screen.getByLabelText("Unit cost (ISK)"), "0");

      expect(screen.getByRole("button", { name: "Create adjustment" })).toBeDisabled();
      expect(api.previewAdjustment).not.toHaveBeenCalled();
    });

    test("rejects five fractional digits locally", async () => {
      const user = userEvent.setup();
      newItemPanel();

      await user.type(screen.getByLabelText("Unit cost (ISK)"), "1.234567");

      expect(await screen.findByRole("alert")).toHaveTextContent(/4 decimal places/i);
      expect(screen.getByRole("button", { name: "Create adjustment" })).toBeDisabled();
    });
  });

  test("Cancel calls onCancel from the review step", async () => {
    api.previewAdjustment.mockResolvedValue(preview());
    const user = userEvent.setup();
    const { onCancel } = renderPanel();

    await screen.findByText("Adjustment value");
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalled();
  });

  test("a failed preview shows an error instead of crashing", async () => {
    api.previewAdjustment.mockRejectedValue(new Error("boom"));
    renderPanel();

    expect(await screen.findByText("Adjustment not ready")).toBeInTheDocument();
  });
});
