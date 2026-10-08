import { describe, expect, test } from "vitest";

import { parseTransactionsUrlFilter, parseTransactionsUrlLabels } from "../transactions-url-state";

const UUID = "0f8fad5b-d9cb-469f-a165-70867728950e";

describe("parseTransactionsUrlFilter", () => {
  test("returns nothing for a bare URL so the page keeps its own defaults", () => {
    expect(parseTransactionsUrlFilter(new URLSearchParams(""))).toEqual({});
  });

  test("reads the filters a deep link can carry", () => {
    expect(
      parseTransactionsUrlFilter(
        new URLSearchParams(
          `connectionIds=${UUID}&dateFrom=2026-09-01&dateTo=2026-09-30&direction=income&category=Ships&locationId=60003760&typeId=34`,
        ),
      ),
    ).toEqual({
      connectionIds: [UUID],
      dateFrom: "2026-09-01",
      dateTo: "2026-09-30",
      direction: "income",
      category: "Ships",
      locationId: 60003760,
      typeId: 34,
    });
  });

  test("drops malformed values", () => {
    expect(
      parseTransactionsUrlFilter(
        new URLSearchParams("connectionIds=nope&dateFrom=yesterday&direction=sideways&locationId=abc&typeId=-1"),
      ),
    ).toEqual({});
  });
});

describe("Analytics extras", () => {
  test("reads the inventory exclusion and display labels", () => {
    const params = new URLSearchParams("excludeInventoryBuys=true&itemLabel=Tritanium&locationLabel=Jita%20IV");
    expect(parseTransactionsUrlFilter(params)).toEqual({ excludeInventoryBuys: true });
    expect(parseTransactionsUrlLabels(params)).toEqual({ item: "Tritanium", location: "Jita IV" });
  });

  test("ignores anything but an explicit true", () => {
    expect(parseTransactionsUrlFilter(new URLSearchParams("excludeInventoryBuys=1"))).toEqual({});
    expect(parseTransactionsUrlLabels(new URLSearchParams(""))).toEqual({});
  });
});
