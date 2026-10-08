import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, test, vi } from "vitest";

import type { InventoryItem } from "../../../../api/inventory";
import { InventoryOperationalTable } from "../inventory-operational-table";

function inventoryItem({
  typeId,
  typeName,
  groupName,
  currentPrice = "5.0000",
  currentValue = "500.0000",
  historicalComparisonComplete = true,
  quantity = 100,
  esiObservedQuantity = null,
  esiObservedAt = null,
  reconciliationDifference = null,
}: {
  typeId: number;
  typeName: string;
  groupName: string | null;
  currentPrice?: string | null;
  currentValue?: string | null;
  historicalComparisonComplete?: boolean;
  quantity?: number;
  esiObservedQuantity?: number | null;
  esiObservedAt?: string | null;
  reconciliationDifference?: number | null;
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
    reservedQuantity: 10,
    availableQuantity: 90,
    costQuality: "known",
    currentPrice,
    currentValue,
    historicalDifference: "100.0000",
    historicalComparisonComplete,
    priceSourceId: null,
    priceSourceName: null,
    priceSourceUpdatedAt: null,
    marketRegionId: null,
    marketLocationId: null,
    esiObservedQuantity,
    esiObservedAt,
    reconciliationDifference,
    warnings: [],
  };
}

/** Controlled-expand-state wrapper mirroring how InventoryPage drives the table. */
function ControlledTable({
  items,
  onSelectItem = vi.fn(),
  onViewEsiHoldings = vi.fn(),
  selectedTypeId = null,
  initialExpanded = [],
}: {
  items: InventoryItem[];
  onSelectItem?: (typeId: number) => void;
  onViewEsiHoldings?: (item: InventoryItem) => void;
  selectedTypeId?: number | null;
  initialExpanded?: string[];
}) {
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(() => new Set(initialExpanded));
  return (
    <InventoryOperationalTable
      expandedGroups={expandedGroups}
      items={items}
      onSelectItem={onSelectItem}
      onToggleGroup={(groupKey, expanded) => {
        setExpandedGroups((current) => {
          const next = new Set(current);
          if (expanded) next.add(groupKey);
          else next.delete(groupKey);
          return next;
        });
      }}
      onViewEsiHoldings={onViewEsiHoldings}
      selectedTypeId={selectedTypeId}
    />
  );
}

function renderTable(items: InventoryItem[], onSelectItem = vi.fn(), selectedTypeId: number | null = null, initialExpanded: string[] = []) {
  const onViewEsiHoldings = vi.fn();
  render(
    <ControlledTable
      initialExpanded={initialExpanded}
      items={items}
      onSelectItem={onSelectItem}
      onViewEsiHoldings={onViewEsiHoldings}
      selectedTypeId={selectedTypeId}
    />,
  );
  return { onSelectItem, onViewEsiHoldings };
}

function rowStatus(name: string | RegExp): string | null {
  return screen.getByRole("row", { name }).querySelector("td")?.getAttribute("data-status") ?? null;
}

describe("InventoryOperationalTable", () => {
  test("groups items alphabetically, starts collapsed, and places Other last", async () => {
    const user = userEvent.setup();
    renderTable([
      inventoryItem({ typeId: 35, typeName: "Pyerite", groupName: "Mineral" }),
      inventoryItem({ typeId: 587, typeName: "Rifter", groupName: null }),
      inventoryItem({ typeId: 34, typeName: "Tritanium", groupName: "Mineral" }),
      inventoryItem({ typeId: 11_399, typeName: "Morphite", groupName: "Advanced Mineral" }),
    ]);

    const groupButtons = screen
      .getAllByRole("button", { name: /^Expand / })
      .filter((button) => button.getAttribute("aria-expanded") !== null);
    expect(groupButtons.map((button) => button.textContent)).toEqual([
      expect.stringContaining("Advanced Mineral"),
      expect.stringContaining("Mineral"),
      expect.stringContaining("Other"),
    ]);

    expect(screen.queryByRole("row", { name: /Tritanium/ })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Expand Mineral" }));
    const mineralRows = within(screen.getByRole("table")).getAllByRole("row", { name: /Pyerite|Tritanium/ });
    expect(mineralRows[0]).toHaveTextContent("Pyerite");
    expect(mineralRows[1]).toHaveTextContent("Tritanium");

    await user.click(screen.getByRole("button", { name: "Expand Other" }));
    expect(screen.getByRole("row", { name: /Rifter/ })).toBeInTheDocument();
  });

  test("toggles an individual group open and closed, and selects a row by click or keyboard", async () => {
    const user = userEvent.setup();
    const { onSelectItem } = renderTable([
      inventoryItem({ typeId: 35, typeName: "Pyerite", groupName: "Mineral" }),
      inventoryItem({ typeId: 34, typeName: "Tritanium", groupName: "Mineral" }),
    ]);

    expect(screen.getByRole("button", { name: "Expand Mineral" })).toHaveAttribute("aria-expanded", "false");

    await user.click(screen.getByRole("button", { name: "Expand Mineral" }));
    await user.click(screen.getByRole("row", { name: /Pyerite/ }));
    expect(onSelectItem).toHaveBeenCalledWith(35);

    const tritaniumRow = screen.getByRole("row", { name: /Tritanium/ });
    tritaniumRow.focus();
    await user.keyboard("{Enter}");
    expect(onSelectItem).toHaveBeenCalledWith(34);

    await user.click(screen.getByRole("button", { name: "Collapse Mineral" }));
    expect(screen.queryByRole("row", { name: /Tritanium/ })).not.toBeInTheDocument();
  });

  test("respects externally-controlled expanded state (e.g. an Expand All driven by the page)", () => {
    renderTable(
      [
        inventoryItem({ typeId: 35, typeName: "Pyerite", groupName: "Mineral" }),
        inventoryItem({ typeId: 34, typeName: "Tritanium", groupName: "Mineral" }),
      ],
      vi.fn(),
      null,
      ["Mineral"],
    );

    expect(screen.getByRole("button", { name: "Collapse Mineral" })).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("row", { name: /Tritanium/ })).toBeInTheDocument();
  });

  test("applies warning styling when current price or historical comparison is incomplete", async () => {
    const user = userEvent.setup();
    renderTable([
      inventoryItem({ typeId: 34, typeName: "Tritanium", groupName: "Mineral", currentPrice: null, currentValue: null }),
      inventoryItem({ typeId: 35, typeName: "Pyerite", groupName: "Mineral", historicalComparisonComplete: false }),
    ]);

    await user.click(screen.getByRole("button", { name: "Expand Mineral" }));
    expect(rowStatus(/Tritanium/)).toBe("warning");
    expect(rowStatus(/Pyerite/)).toBe("warning");
    expect(screen.getAllByText("No price").length).toBeGreaterThan(0);
  });

  test("highlights the row matching selectedTypeId", async () => {
    const user = userEvent.setup();
    renderTable(
      [
        inventoryItem({ typeId: 35, typeName: "Pyerite", groupName: "Mineral" }),
        inventoryItem({ typeId: 34, typeName: "Tritanium", groupName: "Mineral" }),
      ],
      vi.fn(),
      34,
    );

    await user.click(screen.getByRole("button", { name: "Expand Mineral" }));
    expect(screen.getByRole("row", { name: /Tritanium/ })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("row", { name: /Pyerite/ })).toHaveAttribute("aria-selected", "false");
  });

  test("renders the designed column set (no Volume, adds Market Price) and per-group value totals", async () => {
    const user = userEvent.setup();
    renderTable([
      inventoryItem({ typeId: 34, typeName: "Tritanium", groupName: "Mineral", currentPrice: "12.3400", currentValue: "617.0000" }),
      inventoryItem({ typeId: 35, typeName: "Pyerite", groupName: "Mineral", currentValue: null }),
    ]);

    for (const label of ["Item", "Owned", "Reserved", "Available", "Avg Cost", "Historical Value", "Market Price", "Market Value", "ESI"]) {
      expect(screen.getByRole("columnheader", { name: label })).toBeInTheDocument();
    }
    expect(screen.queryByRole("columnheader", { name: "Volume" })).not.toBeInTheDocument();

    expect(screen.getByText(/2 items · 617 ISK partial/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Expand Mineral" }));
    const row = screen.getByRole("row", { name: /Tritanium/ });
    const cellTexts = [...row.querySelectorAll("td")].map((cell) => cell.textContent);
    expect(cellTexts[6]).toContain("12.34"); // Market Price column
    expect(cellTexts[7]).toContain("617"); // Market Value column
  });

  test("ESI column shows a dash with no observation, a check when it matches, and a signed difference otherwise", async () => {
    const user = userEvent.setup();
    renderTable([
      inventoryItem({ typeId: 34, typeName: "Tritanium", groupName: "Mineral", quantity: 100, esiObservedQuantity: null }),
      inventoryItem({ typeId: 35, typeName: "Pyerite", groupName: "Mineral", quantity: 42_000, esiObservedQuantity: 42_000, reconciliationDifference: 0 }),
      inventoryItem({ typeId: 36, typeName: "Nocxium", groupName: "Mineral", quantity: 2_800, esiObservedQuantity: 3_200, reconciliationDifference: 400 }),
      inventoryItem({ typeId: 37, typeName: "Zydrine", groupName: "Mineral", quantity: 4_800, esiObservedQuantity: 4_200, reconciliationDifference: -600 }),
    ]);

    await user.click(screen.getByRole("button", { name: "Expand Mineral" }));

    expect(screen.getByRole("row", { name: /Tritanium/ })).toHaveTextContent("—");
    expect(rowStatus(/Tritanium/)).toBe("neutral");

    const pyeriteRow = screen.getByRole("row", { name: /Pyerite/ });
    expect(pyeriteRow.querySelector('[title="ESI observation matches accounting inventory"]')).toBeInTheDocument();
    expect(rowStatus(/Pyerite/)).toBe("neutral");

    expect(screen.getByRole("row", { name: /Nocxium/ })).toHaveTextContent("+400");
    expect(rowStatus(/Nocxium/)).toBe("warning");

    expect(screen.getByRole("row", { name: /Zydrine/ })).toHaveTextContent("-600");
    expect(rowStatus(/Zydrine/)).toBe("warning");
  });

  test("the View ESI holdings affordance appears for both Match and discrepancy rows, and never triggers row selection", async () => {
    const user = userEvent.setup();
    const { onSelectItem, onViewEsiHoldings } = renderTable([
      inventoryItem({ typeId: 35, typeName: "Pyerite", groupName: "Mineral", quantity: 42_000, esiObservedQuantity: 42_000, reconciliationDifference: 0 }),
      inventoryItem({ typeId: 36, typeName: "Nocxium", groupName: "Mineral", quantity: 2_800, esiObservedQuantity: 3_200, reconciliationDifference: 400 }),
    ]);

    await user.click(screen.getByRole("button", { name: "Expand Mineral" }));

    const pyeriteRow = screen.getByRole("row", { name: /Pyerite/ });
    const nocxiumRow = screen.getByRole("row", { name: /Nocxium/ });
    expect(within(pyeriteRow).getByRole("button", { name: "View ESI holdings" })).toBeInTheDocument();
    expect(within(nocxiumRow).getByRole("button", { name: "View ESI holdings" })).toBeInTheDocument();

    await user.click(within(nocxiumRow).getByRole("button", { name: "View ESI holdings" }));
    expect(onViewEsiHoldings).toHaveBeenCalledTimes(1);
    expect(onViewEsiHoldings.mock.calls[0][0]).toMatchObject({ balance: { key: { typeId: 36 } } });
    expect(onSelectItem).not.toHaveBeenCalled();
  });

  test("no View ESI holdings affordance when there is no ESI observation for the item", async () => {
    const user = userEvent.setup();
    renderTable([inventoryItem({ typeId: 34, typeName: "Tritanium", groupName: "Mineral", esiObservedQuantity: null })]);

    await user.click(screen.getByRole("button", { name: "Expand Mineral" }));
    const row = screen.getByRole("row", { name: /Tritanium/ });
    expect(within(row).queryByRole("button", { name: "View ESI holdings" })).not.toBeInTheDocument();
  });
});
