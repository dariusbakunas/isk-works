import { act, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { MarketItemOrders, MarketOrderRow } from "../../../../api/industry";
import { MarketItemDetailPanel } from "../market-item-detail-panel";

const api = vi.hoisted(() => ({
  getMarketItemOrders: vi.fn(),
  getMarketItemsFreshness: vi.fn(),
  requestMarketData: vi.fn(),
}));

vi.mock("../../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../../api/industry")>("../../../../api/industry");
  return {
    ...actual,
    getMarketItemOrders: api.getMarketItemOrders,
    getMarketItemsFreshness: api.getMarketItemsFreshness,
    requestMarketData: api.requestMarketData,
  };
});

// Recent-enough to read as "fresh" under the panel's own staleness window,
// and old-enough to read as "stale" -- computed relative to the real clock
// rather than a fixed date, since most of these tests deliberately don't
// mock time.
function freshIso(): string {
  return new Date(Date.now() - 5 * 60 * 1000).toISOString();
}

function staleIso(): string {
  return new Date(Date.now() - 20 * 60 * 1000).toISOString();
}

function sellOrder(overrides: Partial<MarketOrderRow> = {}): MarketOrderRow {
  return {
    price: "17.6100",
    quantity: 34_941_615,
    minQuantity: 1,
    locationId: 60_003_760,
    locationName: "Jita IV - Moon 4 - Caldari Navy Assembly Plant (0.9)",
    orderRange: "Station",
    observedAt: "2026-08-26T11:55:00Z",
    expiresAt: new Date(Date.now() + 90 * 24 * 60 * 60 * 1000).toISOString(),
    ...overrides,
  };
}

function buyOrder(overrides: Partial<MarketOrderRow> = {}): MarketOrderRow {
  return {
    price: "17.3100",
    quantity: 10_938_957,
    minQuantity: 1,
    locationId: 60_003_760,
    locationName: "Jita IV - Moon 4 - Caldari Navy Assembly Plant (0.9)",
    orderRange: "Region",
    observedAt: "2026-08-26T11:55:00Z",
    expiresAt: new Date(Date.now() + 90 * 24 * 60 * 60 * 1000).toISOString(),
    ...overrides,
  };
}

const withData: MarketItemOrders = {
  typeId: 35,
  typeName: "Pyerite",
  marketGroupId: 1_361,
  summary: {
    bestSell: "17.6100",
    bestBuy: "17.3100",
    spread: "0.3000",
    sellOrderCount: 1,
    buyOrderCount: 1,
    sellVolume: 34_941_615,
    observedAt: "2026-08-26T11:55:00Z",
  },
  sellOrders: [sellOrder()],
  buyOrders: [buyOrder()],
};

const confirmedEmpty: MarketItemOrders = {
  typeId: 35,
  typeName: "Pyerite",
  marketGroupId: null,
  summary: {
    bestSell: null,
    bestBuy: null,
    spread: null,
    sellOrderCount: 0,
    buyOrderCount: 0,
    sellVolume: 0,
    observedAt: null,
  },
  sellOrders: [],
  buyOrders: [],
};

function renderPanel(overrides: Partial<Parameters<typeof MarketItemDetailPanel>[0]> = {}) {
  return render(
    <MarketItemDetailPanel locationId={60_003_760} regionId={10_000_002} typeId={35} typeName="Pyerite" {...overrides} />,
  );
}

describe("MarketItemDetailPanel", () => {
  beforeEach(() => {
    vi.useRealTimers();
    vi.resetAllMocks();
  });

  test("fresh data renders immediately without triggering a synchronous refresh", async () => {
    api.getMarketItemsFreshness.mockResolvedValue({ mostRecentUpdatedAt: freshIso() });
    api.getMarketItemOrders.mockResolvedValue(withData);
    renderPanel();

    expect(await screen.findByRole("table", { name: "Sell orders" })).toBeInTheDocument();
    expect(api.requestMarketData).not.toHaveBeenCalled();
  });

  test("missing data (never observed) triggers the exact-item refresh exactly once", async () => {
    api.getMarketItemsFreshness.mockResolvedValue({ mostRecentUpdatedAt: null });
    api.getMarketItemOrders.mockResolvedValue(withData);
    api.requestMarketData.mockResolvedValue({ requested: true, sourcesNotified: 1 });
    renderPanel();

    await screen.findByRole("table", { name: "Sell orders" });

    expect(api.requestMarketData).toHaveBeenCalledTimes(1);
    expect(api.requestMarketData).toHaveBeenCalledWith(35, { regionId: 10_000_002, locationId: 60_003_760 });
  });

  test("stale data triggers the exact-item refresh exactly once", async () => {
    api.getMarketItemsFreshness.mockResolvedValue({ mostRecentUpdatedAt: staleIso() });
    api.getMarketItemOrders.mockResolvedValue(withData);
    api.requestMarketData.mockResolvedValue({ requested: true, sourcesNotified: 1 });
    renderPanel();

    await screen.findByRole("table", { name: "Sell orders" });

    expect(api.requestMarketData).toHaveBeenCalledTimes(1);
  });

  test("a successful refresh reloads and renders the returned order data", async () => {
    api.getMarketItemsFreshness.mockResolvedValue({ mostRecentUpdatedAt: null });
    api.getMarketItemOrders.mockResolvedValue(withData);
    api.requestMarketData.mockResolvedValue({ requested: true, sourcesNotified: 1 });
    renderPanel();

    expect(await screen.findByRole("table", { name: "Sell orders" })).toBeInTheDocument();
    expect(screen.getByRole("table", { name: "Buy orders" })).toBeInTheDocument();
    // Missing data has nothing cached to render first, so exactly one
    // getMarketItemOrders call happens -- after the refresh completes.
    expect(api.getMarketItemOrders).toHaveBeenCalledTimes(1);
  });

  test("fresh confirmed-empty data renders honest empty states on both sides without refreshing or refetching in a loop", async () => {
    vi.useFakeTimers();
    api.getMarketItemsFreshness.mockResolvedValue({ mostRecentUpdatedAt: new Date().toISOString() });
    api.getMarketItemOrders.mockResolvedValue(confirmedEmpty);
    renderPanel();
    await act(() => vi.advanceTimersByTimeAsync(0));

    expect(screen.getByText("No sell orders at this market scope.")).toBeInTheDocument();
    expect(screen.getByText("No buy orders at this market scope.")).toBeInTheDocument();
    expect(api.requestMarketData).not.toHaveBeenCalled();

    await act(() => vi.advanceTimersByTimeAsync(5 * 60 * 1000));

    expect(api.getMarketItemsFreshness).toHaveBeenCalledTimes(1);
    expect(api.getMarketItemOrders).toHaveBeenCalledTimes(1);
    expect(api.requestMarketData).not.toHaveBeenCalled();
  });

  test("a refresh failure surfaces an error while preserving previously-cached (stale) data", async () => {
    api.getMarketItemsFreshness.mockResolvedValue({ mostRecentUpdatedAt: staleIso() });
    api.getMarketItemOrders.mockResolvedValue(withData);
    api.requestMarketData.mockRejectedValue(new Error("boom"));
    renderPanel();

    expect(await screen.findByText("Couldn't refresh prices")).toBeInTheDocument();
    expect(screen.getByRole("table", { name: "Sell orders" })).toBeInTheDocument();
  });

  test("a refresh failure with no cached data shows the unavailable state, not a stale-data message", async () => {
    api.getMarketItemsFreshness.mockResolvedValue({ mostRecentUpdatedAt: null });
    api.getMarketItemOrders.mockRejectedValue(new Error("boom"));
    api.requestMarketData.mockRejectedValue(new Error("boom"));
    renderPanel();

    expect(await screen.findByText("Market data unavailable")).toBeInTheDocument();
    expect(screen.queryByRole("table", { name: "Sell orders" })).not.toBeInTheDocument();
  });

  test("changing the market scope reloads the same selected item for the new scope", async () => {
    api.getMarketItemsFreshness.mockResolvedValue({ mostRecentUpdatedAt: freshIso() });
    api.getMarketItemOrders.mockResolvedValue(withData);
    const { rerender } = renderPanel();
    await screen.findByRole("table", { name: "Sell orders" });
    api.getMarketItemsFreshness.mockClear();
    api.getMarketItemOrders.mockClear();

    rerender(<MarketItemDetailPanel locationId={undefined} regionId={10_000_030} typeId={35} typeName="Pyerite" />);

    await vi.waitFor(() =>
      expect(api.getMarketItemsFreshness).toHaveBeenCalledWith([35], { regionId: 10_000_030, locationId: undefined }),
    );
    expect(api.getMarketItemOrders).toHaveBeenCalledWith(35, 10_000_030, undefined);
  });

  describe("stacked order-book presentation", () => {
    test("sellers and buyers render simultaneously, with no sell/buy tab interaction required", async () => {
      api.getMarketItemsFreshness.mockResolvedValue({ mostRecentUpdatedAt: freshIso() });
      api.getMarketItemOrders.mockResolvedValue(withData);
      renderPanel();

      expect(await screen.findByRole("table", { name: "Sell orders" })).toBeInTheDocument();
      expect(screen.getByRole("table", { name: "Buy orders" })).toBeInTheDocument();
      expect(screen.getByText("Sellers")).toBeInTheDocument();
      expect(screen.getByText("Buyers")).toBeInTheDocument();
      expect(screen.queryByRole("tab")).not.toBeInTheDocument();
      expect(screen.queryByRole("tablist")).not.toBeInTheDocument();
    });

    test("sellers show Quantity, Price, Location, and Expires in -- no Range or Min Volume", async () => {
      api.getMarketItemsFreshness.mockResolvedValue({ mostRecentUpdatedAt: freshIso() });
      api.getMarketItemOrders.mockResolvedValue(withData);
      renderPanel();

      const table = await screen.findByRole("table", { name: "Sell orders" });
      expect(within(table).getByRole("columnheader", { name: "Quantity" })).toBeInTheDocument();
      expect(within(table).getByRole("columnheader", { name: "Price" })).toBeInTheDocument();
      expect(within(table).getByRole("columnheader", { name: "Location" })).toBeInTheDocument();
      expect(within(table).getByRole("columnheader", { name: "Expires in" })).toBeInTheDocument();
      expect(within(table).queryByRole("columnheader", { name: "Range" })).not.toBeInTheDocument();
      expect(within(table).queryByRole("columnheader", { name: "Min Volume" })).not.toBeInTheDocument();
    });

    test("buyers show Quantity, Price, Range, Location, Min Volume, and Expires in", async () => {
      api.getMarketItemsFreshness.mockResolvedValue({ mostRecentUpdatedAt: freshIso() });
      api.getMarketItemOrders.mockResolvedValue(withData);
      renderPanel();

      const table = await screen.findByRole("table", { name: "Buy orders" });
      expect(within(table).getByRole("columnheader", { name: "Quantity" })).toBeInTheDocument();
      expect(within(table).getByRole("columnheader", { name: "Price" })).toBeInTheDocument();
      expect(within(table).getByRole("columnheader", { name: "Range" })).toBeInTheDocument();
      expect(within(table).getByRole("columnheader", { name: "Location" })).toBeInTheDocument();
      expect(within(table).getByRole("columnheader", { name: "Min Volume" })).toBeInTheDocument();
      expect(within(table).getByRole("columnheader", { name: "Expires in" })).toBeInTheDocument();
    });

    test("sell orders render cheapest-first, without client-side resorting", async () => {
      api.getMarketItemsFreshness.mockResolvedValue({ mostRecentUpdatedAt: freshIso() });
      api.getMarketItemOrders.mockResolvedValue({
        ...withData,
        // Deliberately NOT price-sorted (20 before 10) so the test proves
        // the panel trusts the API's own order rather than resorting.
        sellOrders: [
          sellOrder({ price: "20.0000", locationId: 60_003_760 }),
          sellOrder({ price: "10.0000", locationId: 60_003_761, locationName: "Perimeter - Ostingele Watch" }),
        ],
      });
      renderPanel();

      const table = await screen.findByRole("table", { name: "Sell orders" });
      const rows = within(table).getAllByRole("row");
      expect(within(rows[1]).getByTitle("20 ISK")).toBeInTheDocument();
      expect(within(rows[2]).getByTitle("10 ISK")).toBeInTheDocument();
    });

    test("buy orders render highest-first, without client-side resorting", async () => {
      api.getMarketItemsFreshness.mockResolvedValue({ mostRecentUpdatedAt: freshIso() });
      api.getMarketItemOrders.mockResolvedValue({
        ...withData,
        // Deliberately NOT price-sorted (10 before 20).
        buyOrders: [
          buyOrder({ price: "10.0000", locationId: 60_003_760 }),
          buyOrder({ price: "20.0000", locationId: 60_003_761, locationName: "Perimeter - Ostingele Watch" }),
        ],
      });
      renderPanel();

      const table = await screen.findByRole("table", { name: "Buy orders" });
      const rows = within(table).getAllByRole("row");
      expect(within(rows[1]).getByTitle("10 ISK")).toBeInTheDocument();
      expect(within(rows[2]).getByTitle("20 ISK")).toBeInTheDocument();
    });

    test("resolved location names render instead of raw numeric location ids", async () => {
      api.getMarketItemsFreshness.mockResolvedValue({ mostRecentUpdatedAt: freshIso() });
      api.getMarketItemOrders.mockResolvedValue(withData);
      renderPanel();

      await screen.findByRole("table", { name: "Sell orders" });

      expect(screen.getAllByText("Jita IV - Moon 4 - Caldari Navy Assembly Plant (0.9)").length).toBeGreaterThan(0);
      expect(screen.queryByText("60003760")).not.toBeInTheDocument();
    });

    test("expiration renders as a human-readable remaining duration derived from expiresAt", async () => {
      vi.useFakeTimers();
      api.getMarketItemsFreshness.mockResolvedValue({ mostRecentUpdatedAt: new Date().toISOString() });
      api.getMarketItemOrders.mockResolvedValue({
        ...withData,
        sellOrders: [
          sellOrder({
            expiresAt: new Date(Date.now() + 2 * 86_400_000 + 21 * 3_600_000 + 14 * 60_000).toISOString(),
          }),
        ],
      });
      renderPanel();
      await act(() => vi.advanceTimersByTimeAsync(0));

      expect(screen.getByText("2d 21h 14m")).toBeInTheDocument();
    });

    test("an order whose computed expiration has already passed renders as Expired rather than a negative duration", async () => {
      vi.useFakeTimers();
      api.getMarketItemsFreshness.mockResolvedValue({ mostRecentUpdatedAt: new Date().toISOString() });
      api.getMarketItemOrders.mockResolvedValue({
        ...withData,
        sellOrders: [sellOrder({ expiresAt: new Date(Date.now() - 60_000).toISOString() })],
      });
      renderPanel();
      await act(() => vi.advanceTimersByTimeAsync(0));

      expect(screen.getByText("Expired")).toBeInTheDocument();
    });

    test("a sell-only order book shows populated sellers and an empty buyers state", async () => {
      api.getMarketItemsFreshness.mockResolvedValue({ mostRecentUpdatedAt: freshIso() });
      api.getMarketItemOrders.mockResolvedValue({ ...withData, buyOrders: [] });
      renderPanel();

      expect(await screen.findByRole("table", { name: "Sell orders" })).toBeInTheDocument();
      expect(screen.queryByRole("table", { name: "Buy orders" })).not.toBeInTheDocument();
      expect(screen.getByText("No buy orders at this market scope.")).toBeInTheDocument();
    });

    test("a buy-only order book shows populated buyers and an empty sellers state", async () => {
      api.getMarketItemsFreshness.mockResolvedValue({ mostRecentUpdatedAt: freshIso() });
      api.getMarketItemOrders.mockResolvedValue({ ...withData, sellOrders: [] });
      renderPanel();

      expect(await screen.findByRole("table", { name: "Buy orders" })).toBeInTheDocument();
      expect(screen.queryByRole("table", { name: "Sell orders" })).not.toBeInTheDocument();
      expect(screen.getByText("No sell orders at this market scope.")).toBeInTheDocument();
    });

    test("a fully populated order book renders both sides at once", async () => {
      api.getMarketItemsFreshness.mockResolvedValue({ mostRecentUpdatedAt: freshIso() });
      api.getMarketItemOrders.mockResolvedValue(withData);
      renderPanel();

      expect(await screen.findByRole("table", { name: "Sell orders" })).toBeInTheDocument();
      expect(screen.getByRole("table", { name: "Buy orders" })).toBeInTheDocument();
      expect(screen.queryByText("No sell orders at this market scope.")).not.toBeInTheDocument();
      expect(screen.queryByText("No buy orders at this market scope.")).not.toBeInTheDocument();
    });
  });
});
