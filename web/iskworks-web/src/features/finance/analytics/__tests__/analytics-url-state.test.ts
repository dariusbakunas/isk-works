import { describe, expect, test } from "vitest";

import {
  autoGranularity,
  parseAnalyticsUrlState,
  resolveDateRange,
  serializeAnalyticsUrlState,
  toAnalyticsQuery,
  transactionsLink,
} from "../analytics-url-state";

const NOW = new Date("2026-09-29T15:00:00Z");
const UUID = "0f8fad5b-d9cb-469f-a165-70867728950e";

describe("analytics url state", () => {
  test("defaults to 30 days, comparison on, all characters, automatic granularity", () => {
    const state = parseAnalyticsUrlState(new URLSearchParams());
    expect(state).toEqual({
      preset: "30d",
      dateFrom: null,
      dateTo: null,
      compare: true,
      characters: [],
      granularity: null,
      category: null,
      excludeInventory: false,
    });
    expect(serializeAnalyticsUrlState(state).toString()).toBe("");
  });

  test("round-trips every field", () => {
    const params = new URLSearchParams(
      `range=custom&from=2026-09-01&to=2026-09-15&compare=0&characters=${UUID}&granularity=month&category=Ships&excludeInventory=1`,
    );
    const state = parseAnalyticsUrlState(params);
    expect(state).toEqual({
      preset: "custom",
      dateFrom: "2026-09-01",
      dateTo: "2026-09-15",
      compare: false,
      characters: [UUID],
      granularity: "month",
      category: "Ships",
      excludeInventory: true,
    });
    expect(new URLSearchParams(serializeAnalyticsUrlState(state)).toString()).toBe(params.toString());
  });

  test("ignores junk instead of throwing", () => {
    const state = parseAnalyticsUrlState(
      new URLSearchParams("range=forever&from=nope&characters=abc,,zzz&granularity=hour&compare=maybe"),
    );
    expect(state.preset).toBe("30d");
    expect(state.characters).toEqual([]);
    expect(state.granularity).toBeNull();
    expect(state.compare).toBe(true);
  });

  test("a custom range needs both valid dates, otherwise it falls back to 30d", () => {
    expect(parseAnalyticsUrlState(new URLSearchParams("range=custom&from=2026-09-01")).preset).toBe("30d");
    expect(parseAnalyticsUrlState(new URLSearchParams("range=custom&from=2026-09-05&to=2026-09-01")).preset).toBe("30d");
  });

  test("presets resolve to inclusive UTC windows ending today", () => {
    const base = parseAnalyticsUrlState(new URLSearchParams());
    expect(resolveDateRange({ ...base, preset: "7d" }, NOW)).toEqual({ dateFrom: "2026-09-23", dateTo: "2026-09-29" });
    expect(resolveDateRange({ ...base, preset: "30d" }, NOW)).toEqual({ dateFrom: "2026-08-31", dateTo: "2026-09-29" });
    expect(resolveDateRange({ ...base, preset: "90d" }, NOW)).toEqual({ dateFrom: "2026-07-02", dateTo: "2026-09-29" });
    expect(resolveDateRange({ ...base, preset: "ytd" }, NOW)).toEqual({ dateFrom: "2026-01-01", dateTo: "2026-09-29" });
    expect(
      resolveDateRange({ ...base, preset: "custom", dateFrom: "2026-09-01", dateTo: "2026-09-15" }, NOW),
    ).toEqual({ dateFrom: "2026-09-01", dateTo: "2026-09-15" });
  });

  test("granularity is chosen from the range length unless set", () => {
    expect(autoGranularity("2026-09-23", "2026-09-29")).toBe("day");
    expect(autoGranularity("2026-08-31", "2026-09-29")).toBe("week");
    expect(autoGranularity("2026-01-01", "2026-09-29")).toBe("week");
    expect(autoGranularity("2025-01-01", "2026-09-29")).toBe("month");
    const state = parseAnalyticsUrlState(new URLSearchParams("granularity=month"));
    expect(toAnalyticsQuery(state, NOW).granularity).toBe("month");
    expect(toAnalyticsQuery(parseAnalyticsUrlState(new URLSearchParams("range=7d")), NOW).granularity).toBe("day");
  });

  test("builds the API query from state", () => {
    const state = parseAnalyticsUrlState(new URLSearchParams(`characters=${UUID}&compare=0&category=Ships`));
    expect(toAnalyticsQuery(state, NOW)).toEqual({
      connectionIds: [UUID],
      dateFrom: "2026-08-31",
      dateTo: "2026-09-29",
      granularity: "week",
      comparePrevious: false,
      category: "Ships",
      excludeInventoryBuys: false,
    });
  });

  test("the inventory exclusion reaches both the API query and the Transactions link", () => {
    const state = parseAnalyticsUrlState(new URLSearchParams("excludeInventory=1"));
    expect(toAnalyticsQuery(state, NOW).excludeInventoryBuys).toBe(true);
    const link = new URL(transactionsLink(state, NOW, { direction: "expense" }), "http://x");
    expect(link.searchParams.get("excludeInventoryBuys")).toBe("true");
    const off = new URL(transactionsLink(parseAnalyticsUrlState(new URLSearchParams()), NOW), "http://x");
    expect(off.searchParams.has("excludeInventoryBuys")).toBe(false);
  });

  test("a link can override the date range, as the fixed-window heatmap does", () => {
    const state = parseAnalyticsUrlState(new URLSearchParams("range=7d"));
    const link = new URL(
      transactionsLink(state, NOW, { range: { dateFrom: "2026-06-30", dateTo: "2026-09-29" } }),
      "http://x",
    );
    expect(link.searchParams.get("dateFrom")).toBe("2026-06-30");
    expect(link.searchParams.get("dateTo")).toBe("2026-09-29");
  });

  test("links can carry display labels for the item or location they filter by", () => {
    const state = parseAnalyticsUrlState(new URLSearchParams());
    const link = new URL(
      transactionsLink(state, NOW, { direction: "expense", typeId: 34, itemLabel: "Tritanium" }),
      "http://x",
    );
    expect(link.searchParams.get("itemLabel")).toBe("Tritanium");
    const where = new URL(
      transactionsLink(state, NOW, { locationId: 60003760, locationLabel: "Jita IV - Moon 4" }),
      "http://x",
    );
    expect(where.searchParams.get("locationLabel")).toBe("Jita IV - Moon 4");
  });

  test("View transactions links carry the page filters plus the chart's own", () => {
    const state = parseAnalyticsUrlState(new URLSearchParams(`characters=${UUID}&category=Ships`));
    const link = new URL(transactionsLink(state, NOW, { direction: "income" }), "http://x");
    expect(link.pathname).toBe("/finance/transactions");
    expect(link.searchParams.get("connectionIds")).toBe(UUID);
    expect(link.searchParams.get("dateFrom")).toBe("2026-08-31");
    expect(link.searchParams.get("dateTo")).toBe("2026-09-29");
    expect(link.searchParams.get("category")).toBe("Ships");
    expect(link.searchParams.get("direction")).toBe("income");

    const item = new URL(transactionsLink(state, NOW, { direction: "expense", typeId: 34, category: null }), "http://x");
    expect(item.searchParams.get("typeId")).toBe("34");
    expect(item.searchParams.has("category")).toBe(false);
  });
});
