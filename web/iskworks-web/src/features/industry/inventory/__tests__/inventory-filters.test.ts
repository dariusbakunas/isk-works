import { describe, expect, test } from "vitest";

import type { InventoryItem } from "../../../../api/inventory";
import { filterScope, hasEsiDiscrepancy, matchesInventoryFilter, matchesInventorySearch, needsAttention } from "../inventory-filters";

function item(overrides: Partial<InventoryItem> = {}): InventoryItem {
  return {
    balance: {
      key: { workspaceId: "workspace-1", ownerId: "owner-1", typeId: 34 },
      typeName: "Tritanium",
      quantity: 100,
      totalHistoricalCost: "400.0000",
      averageUnitCost: "4.0000",
      revision: 1,
      lastActivityAt: null,
    },
    groupName: "Mineral",
    packagedVolumeM3: "0.01",
    totalVolumeM3: "1.00",
    reservedQuantity: 0,
    availableQuantity: 100,
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
    esiObservedQuantity: null,
    esiObservedAt: null,
    reconciliationDifference: null,
    warnings: [],
    ...overrides,
  };
}

describe("needsAttention", () => {
  test("a full reservation (available == 0) is not Needs Attention", () => {
    expect(needsAttention(item({ reservedQuantity: 840, availableQuantity: 0, balance: { ...item().balance, quantity: 840 } }))).toBe(false);
  });

  test("a shortfall (available < 0) is Needs Attention", () => {
    expect(needsAttention(item({ reservedQuantity: 1200, availableQuantity: -400, balance: { ...item().balance, quantity: 800 } }))).toBe(true);
  });

  test("a healthy item with no reservations is not Needs Attention", () => {
    expect(needsAttention(item({ reservedQuantity: 0, availableQuantity: 100 }))).toBe(false);
  });

  test("a meaningful ESI discrepancy is Needs Attention even with available >= 0", () => {
    expect(
      needsAttention(item({ balance: { ...item().balance, quantity: 2_800 }, esiObservedQuantity: 3_200, reconciliationDifference: 400 })),
    ).toBe(true);
  });

  test("no ESI observation is not treated as a discrepancy", () => {
    expect(needsAttention(item({ esiObservedQuantity: null }))).toBe(false);
  });

  test("an ESI observation equal to owned quantity is healthy, not Needs Attention", () => {
    expect(
      needsAttention(item({ balance: { ...item().balance, quantity: 42_000 }, esiObservedQuantity: 42_000 })),
    ).toBe(false);
  });
});

describe("hasEsiDiscrepancy", () => {
  test("a raw mismatch fully explained by ignored holdings is reconciled", () => {
    expect(hasEsiDiscrepancy(item({
      balance: { ...item().balance, quantity: 1_395 },
      esiObservedQuantity: 1_495,
      ignoredEsiQuantity: 100,
      includedEsiQuantity: 1_395,
      reconciliationDifference: 0,
    }))).toBe(false);
  });
  test("no observation is not a discrepancy", () => {
    expect(hasEsiDiscrepancy(item({ esiObservedQuantity: null }))).toBe(false);
  });

  test("observed quantity equal to owned is not a discrepancy", () => {
    expect(
      hasEsiDiscrepancy(item({ balance: { ...item().balance, quantity: 100 }, esiObservedQuantity: 100 })),
    ).toBe(false);
  });

  test("observed quantity greater than owned (positive discrepancy)", () => {
    expect(
      hasEsiDiscrepancy(item({ balance: { ...item().balance, quantity: 2_800 }, esiObservedQuantity: 3_200, reconciliationDifference: 400 })),
    ).toBe(true);
  });

  test("observed quantity less than owned (negative discrepancy)", () => {
    expect(
      hasEsiDiscrepancy(item({ balance: { ...item().balance, quantity: 4_800 }, esiObservedQuantity: 4_200, reconciliationDifference: -600 })),
    ).toBe(true);
  });
});

describe("matchesInventoryFilter", () => {
  const healthy = item({ reservedQuantity: 0, availableQuantity: 100 });
  const reserved = item({ reservedQuantity: 40, availableQuantity: 60 });
  const shortfall = item({ reservedQuantity: 1200, availableQuantity: -400 });

  test("tracked matches every item regardless of state", () => {
    for (const candidate of [healthy, reserved, shortfall]) {
      expect(matchesInventoryFilter(candidate, "tracked")).toBe(true);
    }
  });

  test("untrackedObserved matches every item -- the fetched list is already scoped server-side", () => {
    for (const candidate of [healthy, reserved, shortfall]) {
      expect(matchesInventoryFilter(candidate, "untrackedObserved")).toBe(true);
    }
  });

  test("negativeAvailable matches only items with availableQuantity < 0", () => {
    expect(matchesInventoryFilter(healthy, "negativeAvailable")).toBe(false);
    expect(matchesInventoryFilter(reserved, "negativeAvailable")).toBe(false);
    expect(matchesInventoryFilter(shortfall, "negativeAvailable")).toBe(true);
  });

  test("withReservations matches only items with reservedQuantity > 0", () => {
    expect(matchesInventoryFilter(healthy, "withReservations")).toBe(false);
    expect(matchesInventoryFilter(reserved, "withReservations")).toBe(true);
    expect(matchesInventoryFilter(shortfall, "withReservations")).toBe(true);
  });

  test("needsAttention matches a shortfall or a meaningful ESI discrepancy, not a plain reservation", () => {
    const esiDiscrepancy = item({
      balance: { ...item().balance, quantity: 2_800 },
      esiObservedQuantity: 3_200,
      reconciliationDifference: 400,
    });
    expect(matchesInventoryFilter(healthy, "needsAttention")).toBe(false);
    expect(matchesInventoryFilter(reserved, "needsAttention")).toBe(false);
    expect(matchesInventoryFilter(shortfall, "needsAttention")).toBe(true);
    expect(matchesInventoryFilter(esiDiscrepancy, "needsAttention")).toBe(true);
  });
});

describe("filterScope", () => {
  test("only untrackedObserved requests the untracked scope", () => {
    expect(filterScope("untrackedObserved")).toBe("untracked");
    expect(filterScope("tracked")).toBe("tracked");
    expect(filterScope("needsAttention")).toBe("tracked");
    expect(filterScope("negativeAvailable")).toBe("tracked");
    expect(filterScope("withReservations")).toBe("tracked");
  });
});

describe("matchesInventorySearch", () => {
  test("empty query matches everything", () => {
    expect(matchesInventorySearch(item({ balance: { ...item().balance, typeName: "Tritanium" } }), "")).toBe(true);
  });

  test("matches case-insensitively against the item name", () => {
    const tritanium = item({ balance: { ...item().balance, typeName: "Tritanium" } });
    expect(matchesInventorySearch(tritanium, "trit")).toBe(true);
    expect(matchesInventorySearch(tritanium, "TRIT")).toBe(true);
    expect(matchesInventorySearch(tritanium, "pyerite")).toBe(false);
  });
});
