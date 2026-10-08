import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi, beforeEach } from "vitest";

import type { EsiHoldings, InventoryItem } from "../../../../api/inventory";
import { InventoryEsiHoldingsModal } from "../inventory-esi-holdings-modal";

const api = vi.hoisted(() => ({
  getEsiHoldings: vi.fn(),
  getInventoryItem: vi.fn(),
  setEsiHoldingIncluded: vi.fn(),
}));

vi.mock("../../../../api/inventory", async () => {
  const actual = await vi.importActual<typeof import("../../../../api/inventory")>("../../../../api/inventory");
  return {
    ...actual,
    getEsiHoldings: api.getEsiHoldings,
    getInventoryItem: api.getInventoryItem,
    setEsiHoldingIncluded: api.setEsiHoldingIncluded,
  };
});

function holdings(overrides: Partial<EsiHoldings> = {}): EsiHoldings {
  return {
    typeId: 34,
    observedQuantity: 100,
    ignoredQuantity: 0,
    includedQuantity: 100,
    observedAt: "2026-08-23T09:00:00Z",
    contributors: [
      {
        connectionId: "conn-1",
        eveCharacterId: 1001,
        characterName: "Corvin",
        locationId: 60_003_760,
        locationName: "Jita IV - Moon 4 - CNAP",
        locationFlag: "Hangar",
        quantity: 100,
        ignoredForReconciliation: false,
      },
    ],
    ...overrides,
  };
}

function inventoryItem(quantity = 100, difference = 0, typeId = 34): InventoryItem {
  return {
    balance: { key: { workspaceId: "workspace-1", ownerId: "owner-1", typeId }, typeName: "Tritanium", quantity, totalHistoricalCost: "400", averageUnitCost: "4", revision: 1, lastActivityAt: null },
    groupName: "Mineral", packagedVolumeM3: null, totalVolumeM3: null, reservedQuantity: 0, availableQuantity: quantity,
    costQuality: "known", currentPrice: "5", currentValue: null, historicalDifference: null, historicalComparisonComplete: true,
    priceSourceId: null, priceSourceName: null, marketRegionId: null, marketLocationId: null, priceSourceUpdatedAt: null,
    esiObservedQuantity: quantity + difference, ignoredEsiQuantity: 0, includedEsiQuantity: quantity + difference,
    esiObservedAt: "2026-08-23T09:00:00Z", reconciliationDifference: difference, warnings: [],
  };
}

type LegacyOverrides = Partial<Parameters<typeof InventoryEsiHoldingsModal>[0]> & { accountedQuantity?: number; typeId?: number; typeName?: string; onReviewAdjustment?: () => void };
function renderModal(overrides: LegacyOverrides = {}) {
  const onClose = vi.fn();
  const onReviewAdjustment = vi.fn();
  const accounted = overrides.accountedQuantity ?? 100;
  const item = overrides.item ?? inventoryItem(accounted, 0, overrides.typeId ?? 34);
  render(
    <InventoryEsiHoldingsModal
      item={item}
      onClose={onClose}
      onChanged={vi.fn()}
    />,
  );
  return { onClose, onReviewAdjustment };
}

describe("InventoryEsiHoldingsModal", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.getInventoryItem.mockResolvedValue({ ...inventoryItem(), events: [], reservations: [] });
  });

  test("shows a loading state before the holdings fetch resolves", () => {
    api.getEsiHoldings.mockReturnValue(new Promise(() => {}));
    renderModal();

    expect(screen.getByRole("status")).toHaveTextContent("Loading ESI holdings...");
  });

  test("lazy-loads holdings for the given type only after mount", () => {
    api.getEsiHoldings.mockReturnValue(new Promise(() => {}));
    renderModal({ typeId: 587 });

    expect(api.getEsiHoldings).toHaveBeenCalledWith(587);
    expect(api.getEsiHoldings).toHaveBeenCalledTimes(1);
  });

  test("shows an error state when the fetch fails", async () => {
    api.getEsiHoldings.mockRejectedValue(new Error("boom"));
    renderModal();

    expect(await screen.findByText("ESI holdings unavailable")).toBeInTheDocument();
  });

  test("shows an empty state when there are no contributors", async () => {
    api.getEsiHoldings.mockResolvedValue(holdings({ observedQuantity: 0, contributors: [] }));
    renderModal({ accountedQuantity: 0 });

    expect(await screen.findByText(/No contributing holdings/)).toBeInTheDocument();
  });

  test("renders a positive discrepancy with a plus sign", async () => {
    api.getEsiHoldings.mockResolvedValue(holdings({ observedQuantity: 3_200 }));
    renderModal({ item: inventoryItem(2_800, 400) });

    expect(await screen.findByText("+400")).toBeInTheDocument();
  });

  test("renders a negative discrepancy with a minus sign", async () => {
    api.getEsiHoldings.mockResolvedValue(holdings({ observedQuantity: 4_200 }));
    renderModal({ item: inventoryItem(4_800, -600) });

    expect(await screen.findByText("-600")).toBeInTheDocument();
  });

  test("renders Match when observed quantity equals accounted quantity", async () => {
    api.getEsiHoldings.mockResolvedValue(holdings({ observedQuantity: 100 }));
    renderModal({ accountedQuantity: 100 });

    expect(await screen.findByText("Match")).toBeInTheDocument();
  });

  test("groups multiple characters and multiple locations, each as a separate row", async () => {
    api.getEsiHoldings.mockResolvedValue(
      holdings({
        observedQuantity: 125_000,
        contributors: [
          {
            connectionId: "conn-1",
            eveCharacterId: 1001,
            characterName: "Corvin",
            locationId: 60_003_760,
            locationName: "Jita IV - Moon 4 - CNAP",
            locationFlag: "Hangar",
            quantity: 80_000,
            ignoredForReconciliation: false,
          },
          {
            connectionId: "conn-1",
            eveCharacterId: 1001,
            characterName: "Corvin",
            locationId: 1_000_000_000_001,
            locationName: null,
            locationFlag: "Hangar",
            quantity: 30_000,
            ignoredForReconciliation: false,
          },
          {
            connectionId: "conn-2",
            eveCharacterId: 1002,
            characterName: "Hauler Alt",
            locationId: 1_000_000_000_002,
            locationName: "Some Citadel",
            locationFlag: "Hangar",
            quantity: 15_000,
            ignoredForReconciliation: false,
          },
        ],
      }),
    );
    renderModal({ accountedQuantity: 125_000 });

    const rows = await screen.findAllByRole("row");
    // header row + 3 contributor rows (footer is not a <tr role="row"> distinct concern here, it still counts).
    const characterCells = rows.map((row) => row.textContent ?? "");
    expect(characterCells.some((text) => text.includes("Corvin") && text.includes("Jita"))).toBe(true);
    expect(characterCells.some((text) => text.includes("Corvin") && text.includes("Unknown location 1000000000001"))).toBe(true);
    expect(characterCells.some((text) => text.includes("Hauler Alt") && text.includes("Some Citadel"))).toBe(true);
  });

  test("the itemized contributor total matches the displayed ESI observed quantity", async () => {
    api.getEsiHoldings.mockResolvedValue(
      holdings({
        observedQuantity: 125_000,
        contributors: [
          {
            connectionId: "conn-1",
            eveCharacterId: 1001,
            characterName: "Corvin",
            locationId: 60_003_760,
            locationName: "Jita IV - Moon 4 - CNAP",
            locationFlag: "Hangar",
            quantity: 80_000,
            ignoredForReconciliation: false,
          },
          {
            connectionId: "conn-2",
            eveCharacterId: 1002,
            characterName: "Hauler Alt",
            locationId: 1_000_000_000_002,
            locationName: "Some Citadel",
            locationFlag: "Hangar",
            quantity: 45_000,
            ignoredForReconciliation: false,
          },
        ],
      }),
    );
    renderModal({ accountedQuantity: 125_000 });

    const table = await screen.findByRole("table");
    const footer = within(table).getByText("Total").closest("tr");
    expect(footer).not.toBeNull();
    expect(within(footer as HTMLElement).getByText("125,000")).toBeInTheDocument();
  });

  test("ignoring one location persists its exact identity and refreshes totals in place", async () => {
    api.getEsiHoldings.mockResolvedValue(holdings());
    api.setEsiHoldingIncluded.mockResolvedValue(holdings({ ignoredQuantity: 100, includedQuantity: 0, contributors: [{ ...holdings().contributors[0], ignoredForReconciliation: true }] }));
    api.getInventoryItem.mockResolvedValue({ ...inventoryItem(100, -100), ignoredEsiQuantity: 100, includedEsiQuantity: 0, events: [], reservations: [] });
    const user = userEvent.setup();
    renderModal();

    await user.click(await screen.findByRole("button", { name: "Ignore Tritanium at this location for reconciliation" }));

    expect(api.setEsiHoldingIncluded).toHaveBeenCalledWith(34, 1001, 60_003_760, false);
    expect(api.setEsiHoldingIncluded.mock.invocationCallOrder[0]).toBeLessThan(api.getInventoryItem.mock.invocationCallOrder[0]);
    expect(await screen.findByRole("button", { name: "Include Tritanium at this location in reconciliation" })).toBeInTheDocument();
    expect(screen.getByText("Ignored for reconciliation")).toBeInTheDocument();
    expect(screen.getByText("-100")).toBeInTheDocument();
  });

  test("a failed inclusion update keeps the holdings rows available for retry", async () => {
    api.getEsiHoldings.mockResolvedValue(holdings());
    api.setEsiHoldingIncluded.mockRejectedValue(new Error("save failed"));
    const user = userEvent.setup();
    renderModal();

    await user.click(await screen.findByRole("button", { name: "Ignore Tritanium at this location for reconciliation" }));

    expect(await screen.findByText("Reconciliation data not refreshed")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Ignore Tritanium at this location for reconciliation" })).toBeInTheDocument();
    expect(screen.getByText("Jita IV - Moon 4 - CNAP")).toBeInTheDocument();
  });

  test("Review adjustment stays inside the holdings dialog", async () => {
    api.getEsiHoldings.mockResolvedValue(holdings({ observedQuantity: 3_200 }));
    const user = userEvent.setup();
    renderModal({ item: inventoryItem(2_800, 400) });

    const button = await screen.findByRole("button", { name: "Review adjustment" });
    await user.click(button);
    expect(await screen.findByText("Review discrepancy")).toBeInTheDocument();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Back" }));
    expect(await screen.findByRole("button", { name: "Review adjustment" })).toBeInTheDocument();
  });

  test("no Review adjustment button when there is no discrepancy (Match)", async () => {
    api.getEsiHoldings.mockResolvedValue(holdings({ observedQuantity: 100 }));
    renderModal({ item: inventoryItem(100, 0) });

    await screen.findByText("Match");
    expect(screen.queryByRole("button", { name: "Review adjustment" })).not.toBeInTheDocument();
  });

  test("closing the modal calls onClose", async () => {
    api.getEsiHoldings.mockResolvedValue(holdings());
    const user = userEvent.setup();
    const { onClose } = renderModal();

    await screen.findByText("Match");
    await user.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalled();
  });
});
