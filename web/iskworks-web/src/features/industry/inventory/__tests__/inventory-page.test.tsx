import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { InventoryItem } from "../../../../api/inventory";
import type { PriceSource } from "../../../../api/industry";
import { InventoryPage } from "../inventory-page";

const api = vi.hoisted(() => ({
  listInventory: vi.fn(),
  exportInventory: vi.fn(),
  getInventoryItem: vi.fn(),
  getEsiHoldings: vi.fn(),
  previewAdjustment: vi.fn(),
}));

vi.mock("../../../../api/inventory", async () => {
  const actual = await vi.importActual<typeof import("../../../../api/inventory")>("../../../../api/inventory");
  return {
    ...actual,
    listInventory: api.listInventory,
    exportInventory: api.exportInventory,
    getInventoryItem: api.getInventoryItem,
    getEsiHoldings: api.getEsiHoldings,
    previewAdjustment: api.previewAdjustment,
  };
});

const industryApi = vi.hoisted(() => ({
  listPriceSources: vi.fn(),
}));

vi.mock("../../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../../api/industry")>("../../../../api/industry");
  return { ...actual, listPriceSources: industryApi.listPriceSources };
});

function item({
  typeId,
  typeName,
  groupName,
  quantity,
  reservedQuantity,
  availableQuantity,
  esiObservedQuantity = null,
}: {
  typeId: number;
  typeName: string;
  groupName: string | null;
  quantity: number;
  reservedQuantity: number;
  availableQuantity: number;
  esiObservedQuantity?: number | null;
}): InventoryItem {
  return {
    balance: {
      key: { workspaceId: "workspace-1", ownerId: "owner-1", typeId },
      typeName,
      quantity,
      totalHistoricalCost: "400.0000",
      averageUnitCost: "4.0000",
      revision: 1,
      lastActivityAt: null,
    },
    groupName,
    packagedVolumeM3: "0.01",
    totalVolumeM3: "1.00",
    reservedQuantity,
    availableQuantity,
    costQuality: "known",
    currentPrice: "5.0000",
    currentValue: "500.0000",
    historicalDifference: "100.0000",
    historicalComparisonComplete: true,
    priceSourceId: null,
    priceSourceName: null,
    priceSourceUpdatedAt: null,
    marketRegionId: null,
    marketLocationId: null,
    esiObservedQuantity,
    esiObservedAt: esiObservedQuantity === null ? null : "2026-08-23T12:00:00Z",
    reconciliationDifference: esiObservedQuantity === null ? null : esiObservedQuantity - quantity,
    warnings: [],
  };
}

const tritanium = item({ typeId: 34, typeName: "Tritanium", groupName: "Mineral", quantity: 100, reservedQuantity: 0, availableQuantity: 100 });
const pyerite = item({ typeId: 35, typeName: "Pyerite", groupName: "Mineral", quantity: 200, reservedQuantity: 50, availableQuantity: 150 });
const zydrine = item({ typeId: 39, typeName: "Zydrine", groupName: "Advanced Mineral", quantity: 800, reservedQuantity: 1200, availableQuantity: -400 });
const rifter = item({ typeId: 587, typeName: "Rifter", groupName: null, quantity: 5, reservedQuantity: 0, availableQuantity: 5 });

const priceSources: PriceSource[] = [];

function renderPage(initialEntries: string[] = ["/inventory"]) {
  return render(
    <MemoryRouter initialEntries={initialEntries}>
      <Routes>
        <Route element={<InventoryPage />} path="/inventory" />
      </Routes>
    </MemoryRouter>,
  );
}

async function expandAll(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("button", { name: "Expand all groups" }));
}

describe("InventoryPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.listInventory.mockResolvedValue([tritanium, pyerite, zydrine, rifter]);
    api.getInventoryItem.mockResolvedValue({ ...tritanium, events: [], reservations: [] });
    industryApi.listPriceSources.mockResolvedValue(priceSources);
  });

  test("search filters rows by item name and hides categories with no matches", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("table", { name: "Inventory" });
    await expandAll(user);
    expect(screen.getByRole("row", { name: /Pyerite/ })).toBeInTheDocument();

    await user.type(screen.getByRole("searchbox", { name: "Search inventory" }), "p");

    expect(screen.getByRole("row", { name: /Pyerite/ })).toBeInTheDocument();
    expect(screen.queryByRole("row", { name: /Tritanium/ })).not.toBeInTheDocument();
    expect(screen.queryByText("Advanced Mineral")).not.toBeInTheDocument();
    expect(screen.queryByText("Other")).not.toBeInTheDocument();
    expect(screen.getByText("Mineral")).toBeInTheDocument();
  });

  test("Tracked filter shows every tracked item and fetches the tracked scope by default", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("table", { name: "Inventory" });
    await user.click(screen.getByRole("tab", { name: "Tracked" }));
    await expandAll(user);

    for (const name of [/Tritanium/, /Pyerite/, /Zydrine/, /Rifter/]) {
      expect(screen.getByRole("row", { name })).toBeInTheDocument();
    }
    expect(api.listInventory).toHaveBeenCalledWith(undefined, "tracked");
  });

  test("Negative available filter shows only the shortfall item", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("table", { name: "Inventory" });
    await user.click(screen.getByRole("tab", { name: "Negative available" }));
    await expandAll(user);

    expect(screen.getByRole("row", { name: /Zydrine/ })).toBeInTheDocument();
    expect(screen.queryByRole("row", { name: /Tritanium/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("row", { name: /Pyerite/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("row", { name: /Rifter/ })).not.toBeInTheDocument();
  });

  test("With reservations filter keeps a partially-matching category and drops an empty one", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("table", { name: "Inventory" });
    await user.click(screen.getByRole("tab", { name: "With reservations" }));
    await expandAll(user);

    expect(screen.getByRole("row", { name: /Pyerite/ })).toBeInTheDocument();
    expect(screen.getByRole("row", { name: /Zydrine/ })).toBeInTheDocument();
    expect(screen.queryByRole("row", { name: /Tritanium/ })).not.toBeInTheDocument();
    expect(screen.queryByText("Other")).not.toBeInTheDocument();
    expect(screen.getByText("Mineral")).toBeInTheDocument();
  });

  test("Needs attention matches a negative-available shortfall", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("table", { name: "Inventory" });
    await user.click(screen.getByRole("tab", { name: "Needs attention" }));
    await expandAll(user);

    expect(screen.getByRole("row", { name: /Zydrine/ })).toBeInTheDocument();
    expect(screen.queryByRole("row", { name: /Pyerite/ })).not.toBeInTheDocument();
  });

  test("Needs attention also matches a plain ESI discrepancy with available >= 0, which Negative available does not", async () => {
    const nocxium = item({
      typeId: 36,
      typeName: "Nocxium",
      groupName: "Mineral",
      quantity: 2_800,
      reservedQuantity: 0,
      availableQuantity: 2_800,
      esiObservedQuantity: 3_200,
    });
    api.listInventory.mockResolvedValue([tritanium, nocxium]);
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("table", { name: "Inventory" });

    await user.click(screen.getByRole("tab", { name: "Needs attention" }));
    await expandAll(user);
    expect(screen.getByRole("row", { name: /Nocxium/ })).toBeInTheDocument();
    expect(screen.queryByRole("row", { name: /Tritanium/ })).not.toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "Negative available" }));
    expect(screen.queryByRole("row", { name: /Nocxium/ })).not.toBeInTheDocument();
  });

  test("Untracked observed fetches the untracked scope separately and shows only ESI-only rows", async () => {
    const phantom = item({
      typeId: 44,
      typeName: "Caldari Navy Mjolnir Heavy Missile",
      groupName: "Ammo",
      quantity: 0,
      reservedQuantity: 0,
      availableQuantity: 0,
      esiObservedQuantity: 50,
    });
    api.listInventory.mockImplementation((_priceSourceId, scope) =>
      Promise.resolve(scope === "untracked" ? [phantom] : [tritanium, pyerite, zydrine, rifter]),
    );
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("table", { name: "Inventory" });

    await user.click(screen.getByRole("tab", { name: "Untracked observed" }));
    expect(api.listInventory).toHaveBeenCalledWith(undefined, "untracked");
    await expandAll(user);

    expect(await screen.findByRole("row", { name: /Caldari Navy Mjolnir Heavy Missile/ })).toBeInTheDocument();
    expect(screen.queryByRole("row", { name: /Tritanium/ })).not.toBeInTheDocument();
  });

  test("switching away from Untracked observed refetches the tracked scope", async () => {
    api.listInventory.mockImplementation((_priceSourceId, scope) =>
      Promise.resolve(scope === "untracked" ? [] : [tritanium, pyerite, zydrine, rifter]),
    );
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("table", { name: "Inventory" });

    await user.click(screen.getByRole("tab", { name: "Untracked observed" }));
    await screen.findByText("Nothing untracked");
    api.listInventory.mockClear();

    await user.click(screen.getByRole("tab", { name: "Tracked" }));
    expect(await screen.findByRole("table", { name: "Inventory" })).toBeInTheDocument();
    expect(api.listInventory).toHaveBeenCalledWith(undefined, "tracked");
  });

  test("a slow scope response that arrives after switching scopes is ignored", async () => {
    const phantom = item({
      typeId: 44,
      typeName: "Caldari Navy Mjolnir Heavy Missile",
      groupName: "Ammo",
      quantity: 0,
      reservedQuantity: 0,
      availableQuantity: 0,
      esiObservedQuantity: 50,
    });
    let resolveUntracked: (items: InventoryItem[]) => void = () => {};
    let resolveTracked: (items: InventoryItem[]) => void = () => {};
    api.listInventory.mockImplementation((_priceSourceId, scope) =>
      scope === "untracked"
        ? new Promise((resolve) => (resolveUntracked = resolve))
        : new Promise((resolve) => (resolveTracked = resolve)),
    );
    const user = userEvent.setup();
    renderPage();
    resolveTracked([tritanium, pyerite, zydrine, rifter]);
    await screen.findByRole("table", { name: "Inventory" });

    // Untracked starts loading; Tracked is clicked before it finishes.
    await user.click(screen.getByRole("tab", { name: "Untracked observed" }));
    const untracked = resolveUntracked;
    await user.click(screen.getByRole("tab", { name: "Tracked" }));
    resolveTracked([tritanium, pyerite, zydrine, rifter]);
    await screen.findByRole("table", { name: "Inventory" });
    // The slower untracked response lands last.
    untracked([phantom]);
    await new Promise((resolve) => setTimeout(resolve, 0));

    await expandAll(user);
    expect(screen.getByRole("row", { name: /Tritanium/ })).toBeInTheDocument();
    expect(screen.queryByRole("row", { name: /Caldari Navy Mjolnir Heavy Missile/ })).not.toBeInTheDocument();
  });

  test("a ?item= deep-link outside the current scope is fetched directly so the inspector still opens", async () => {
    const untrackedItem = { ...item({ typeId: 44, typeName: "Zydrite Ore", groupName: "Ore", quantity: 0, reservedQuantity: 0, availableQuantity: 0, esiObservedQuantity: 12 }), events: [], reservations: [] };
    api.getInventoryItem.mockImplementation((typeId: number) =>
      typeId === 44 ? Promise.resolve(untrackedItem) : Promise.resolve({ ...tritanium, events: [], reservations: [] }),
    );

    renderPage(["/inventory?item=44"]);

    expect(await screen.findByRole("heading", { name: "Zydrite Ore" })).toBeInTheDocument();
    expect(api.getInventoryItem).toHaveBeenCalledWith(44, undefined);
  });

  test("combines search with the active operational filter", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("table", { name: "Inventory" });
    await user.click(screen.getByRole("tab", { name: "With reservations" }));
    await user.type(screen.getByRole("searchbox", { name: "Search inventory" }), "zyd");
    await expandAll(user);

    expect(screen.getByRole("row", { name: /Zydrine/ })).toBeInTheDocument();
    expect(screen.queryByRole("row", { name: /Pyerite/ })).not.toBeInTheDocument();
  });

  test("Record menu opens the existing Opening Balance flow", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("table", { name: "Inventory" });
    await user.click(screen.getByRole("button", { name: "Record" }));
    await user.click(screen.getByRole("menuitem", { name: "Opening Balance" }));
    expect(await screen.findByRole("heading", { name: "Add Opening Balance" })).toBeInTheDocument();
  });

  test("Record menu opens the existing Purchase flow", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("table", { name: "Inventory" });
    await user.click(screen.getByRole("button", { name: "Record" }));
    await user.click(screen.getByRole("menuitem", { name: "Purchase" }));
    expect(await screen.findByRole("heading", { name: "Record Purchase" })).toBeInTheDocument();
  });

  test("Record menu opens the existing Adjustment flow", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("table", { name: "Inventory" });
    await user.click(screen.getByRole("button", { name: "Record" }));
    await user.click(screen.getByRole("menuitem", { name: "Adjustment" }));
    expect(await screen.findByRole("heading", { name: "Adjust Inventory" })).toBeInTheDocument();
  });

  test("overflow menu keeps Import and Export reachable", async () => {
    const user = userEvent.setup();
    api.exportInventory.mockResolvedValue({ exportedAt: "2026-08-23T00:00:00Z", items: [] });
    vi.stubGlobal("URL", class extends URL {
      static createObjectURL = vi.fn(() => "blob:mock");
      static revokeObjectURL = vi.fn();
    });
    renderPage();
    await screen.findByRole("table", { name: "Inventory" });

    await user.click(screen.getByRole("button", { name: "More actions" }));
    await user.click(screen.getByRole("menuitem", { name: "Export" }));
    await waitFor(() => expect(api.exportInventory).toHaveBeenCalledOnce());

    await user.click(screen.getByRole("button", { name: "More actions" }));
    await user.click(screen.getByRole("menuitem", { name: "Import" }));
    expect(await screen.findByRole("heading", { name: "Import Inventory" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Close" }));

    await user.click(screen.getByRole("button", { name: "More actions" }));
    expect(screen.queryByRole("menuitem", { name: "Wallet Imports" })).not.toBeInTheDocument();

    vi.unstubAllGlobals();
  });

  test("selecting a row opens the inspector and writes ?item=, closing clears it", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("table", { name: "Inventory" });
    await expandAll(user);

    await user.click(screen.getByRole("row", { name: /Tritanium/ }));
    const inspector = await screen.findByRole("heading", { name: "Tritanium" });
    expect(inspector).toBeInTheDocument();
    expect(api.getInventoryItem).toHaveBeenCalledWith(34, undefined);

    await user.click(screen.getByRole("button", { name: "Close item inspector" }));
    expect(screen.queryByRole("heading", { name: "Tritanium" })).not.toBeInTheDocument();
  });

  test("?item= deep-link opens the inspector for that item on load", async () => {
    renderPage(["/inventory?item=34"]);
    expect(await screen.findByRole("heading", { name: "Tritanium" })).toBeInTheDocument();
  });

  test("View ESI holdings opens a modal without changing search, filter, expansion, or the selected item", async () => {
    const nocxium = item({
      typeId: 36,
      typeName: "Nocxium",
      groupName: "Mineral",
      quantity: 2_800,
      reservedQuantity: 0,
      availableQuantity: 2_800,
      esiObservedQuantity: 3_200,
    });
    api.listInventory.mockResolvedValue([tritanium, pyerite, zydrine, rifter, nocxium]);
    api.getEsiHoldings.mockResolvedValue({ typeId: 36, observedQuantity: 3_200, observedAt: "2026-08-23T09:00:00Z", contributors: [] });
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("table", { name: "Inventory" });
    await user.click(screen.getByRole("tab", { name: "Needs attention" }));
    await user.type(screen.getByRole("searchbox", { name: "Search inventory" }), "no");
    await expandAll(user);

    await user.click(screen.getByRole("button", { name: "View ESI holdings" }));
    expect(await screen.findByRole("heading", { name: "Nocxium" })).toBeInTheDocument();

    expect(screen.getByRole("tab", { name: "Needs attention" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("searchbox", { name: "Search inventory" })).toHaveValue("no");
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("heading", { name: "Nocxium" })).not.toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Needs attention" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("searchbox", { name: "Search inventory" })).toHaveValue("no");
  });

  test("Untracked observed rows use the same ESI holdings modal", async () => {
    const phantom = item({
      typeId: 44,
      typeName: "Caldari Navy Mjolnir Heavy Missile",
      groupName: "Ammo",
      quantity: 0,
      reservedQuantity: 0,
      availableQuantity: 0,
      esiObservedQuantity: 50,
    });
    api.listInventory.mockImplementation((_priceSourceId, scope) =>
      Promise.resolve(scope === "untracked" ? [phantom] : [tritanium, pyerite, zydrine, rifter]),
    );
    api.getEsiHoldings.mockResolvedValue({ typeId: 44, observedQuantity: 50, observedAt: "2026-08-23T09:00:00Z", contributors: [] });
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("table", { name: "Inventory" });

    await user.click(screen.getByRole("tab", { name: "Untracked observed" }));
    await expandAll(user);
    await screen.findByRole("row", { name: /Caldari Navy Mjolnir Heavy Missile/ });

    await user.click(screen.getByRole("button", { name: "View ESI holdings" }));
    expect(await screen.findByRole("heading", { name: "Caldari Navy Mjolnir Heavy Missile" })).toBeInTheDocument();
    expect(api.getEsiHoldings).toHaveBeenCalledWith(44);
  });

  test("Review adjustment stays in the holdings modal for that item", async () => {
    const nocxium = item({
      typeId: 36,
      typeName: "Nocxium",
      groupName: "Mineral",
      quantity: 2_800,
      reservedQuantity: 0,
      availableQuantity: 2_800,
      esiObservedQuantity: 3_200,
    });
    api.listInventory.mockResolvedValue([tritanium, pyerite, zydrine, rifter, nocxium]);
    api.getInventoryItem.mockImplementation((typeId: number) =>
      Promise.resolve(typeId === 36 ? { ...nocxium, events: [], reservations: [] } : { ...tritanium, events: [], reservations: [] }),
    );
    api.getEsiHoldings.mockResolvedValue({ typeId: 36, observedQuantity: 3_200, observedAt: "2026-08-23T09:00:00Z", contributors: [] });
    api.previewAdjustment.mockResolvedValue({
      current: nocxium.balance,
      posting: { kind: "adjustment", quantityDelta: 400, totalCostDelta: "800.0000", unitCost: "2.0000", costQuality: "known" },
      resulting: { ...nocxium.balance, quantity: 3_200 },
      warnings: [],
    });
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("table", { name: "Inventory" });
    await expandAll(user);

    await user.click(screen.getByRole("button", { name: "View ESI holdings" }));
    await screen.findByRole("heading", { name: "Nocxium" });
    await user.click(screen.getByRole("button", { name: "Review adjustment" }));

    expect(screen.getByText("ESI holdings")).toBeInTheDocument();
    expect(await screen.findByText("Adjustment value")).toBeInTheDocument();
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
  });

  test("Expand All / Collapse All toggle every category", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("table", { name: "Inventory" });

    expect(screen.queryByRole("row", { name: /Tritanium/ })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Expand all groups" }));
    expect(screen.getByRole("row", { name: /Tritanium/ })).toBeInTheDocument();
    expect(screen.getByRole("row", { name: /Rifter/ })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Collapse all groups" }));
    expect(screen.queryByRole("row", { name: /Tritanium/ })).not.toBeInTheDocument();
  });
});
