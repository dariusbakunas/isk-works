import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { MarketCategoryNode, MarketItemOrders, MarketItemPage } from "../../../../api/industry";
import { MarketBrowserPage } from "../market-browser-page";

const api = vi.hoisted(() => ({
  listMarketCategories: vi.fn(),
  listMarketItems: vi.fn(),
  getMarketItemOrders: vi.fn(),
  getMarketItemsFreshness: vi.fn(),
}));

vi.mock("../../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../../api/industry")>("../../../../api/industry");
  return {
    ...actual,
    listMarketCategories: api.listMarketCategories,
    listMarketItems: api.listMarketItems,
    getMarketItemOrders: api.getMarketItemOrders,
    getMarketItemsFreshness: api.getMarketItemsFreshness,
  };
});

const categories: MarketCategoryNode[] = [
  {
    marketGroupId: 4,
    name: "Ships",
    itemCount: 2,
    children: [],
  },
];

function pageWith(items: { typeId: number; typeName: string }[], totalCount = items.length): MarketItemPage {
  return {
    rows: items.map((item) => ({
      typeId: item.typeId,
      typeName: item.typeName,
      bestSell: null,
      bestBuy: null,
      spread: null,
      sellOrderCount: 0,
      buyOrderCount: 0,
      observedAt: null,
    })),
    totalCount,
    page: 1,
    pageSize: 200,
  };
}

const treeItemsPage = pageWith([
  { typeId: 587, typeName: "Rifter" },
  { typeId: 3_756, typeName: "Punisher" },
]);

function ordersFor(typeId: number, typeName: string): MarketItemOrders {
  return {
    typeId,
    typeName,
    marketGroupId: 4,
    summary: {
      bestSell: "17.6100",
      bestBuy: "17.3100",
      spread: "0.3000",
      sellOrderCount: 1,
      buyOrderCount: 1,
      sellVolume: 100,
      observedAt: "2026-08-26T11:55:00Z",
    },
    sellOrders: [
      {
        price: "17.6100",
        quantity: 100,
        minQuantity: 1,
        locationId: 60_003_760,
        locationName: "Jita IV - Moon 4 - Caldari Navy Assembly Plant (0.9)",
        orderRange: "Station",
        observedAt: "2026-08-26T11:55:00Z",
        expiresAt: "2026-11-24T11:55:00Z",
      },
    ],
    buyOrders: [
      {
        price: "17.3100",
        quantity: 100,
        minQuantity: 1,
        locationId: 60_003_760,
        locationName: "Jita IV - Moon 4 - Caldari Navy Assembly Plant (0.9)",
        orderRange: "Region",
        observedAt: "2026-08-26T11:55:00Z",
        expiresAt: "2026-11-24T11:55:00Z",
      },
    ],
  };
}

// Default listMarketItems behavior: a request carrying `search` is a
// catalog-wide search call, anything else is the tree's own per-node fetch
// -- lets most tests share one mock implementation instead of juggling
// call-order-based mockResolvedValueOnce chains.
function defaultListMarketItems(query: { search?: string }) {
  if (query.search) return Promise.resolve(pageWith([{ typeId: 11_393, typeName: "Retribution" }]));
  return Promise.resolve(treeItemsPage);
}

describe("MarketBrowserPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useRealTimers();
    api.listMarketCategories.mockResolvedValue(categories);
    api.listMarketItems.mockImplementation(defaultListMarketItems);
    api.getMarketItemsFreshness.mockResolvedValue({ mostRecentUpdatedAt: "2026-08-26T11:55:00Z" });
    api.getMarketItemOrders.mockImplementation((typeId: number, _regionId: number, _locationId?: number) =>
      Promise.resolve(ordersFor(typeId, typeId === 587 ? "Rifter" : typeId === 11_393 ? "Retribution" : "Punisher")),
    );
  });

  test("the initial page shows the category tree and an empty right pane, with no middle item table", async () => {
    render(<MarketBrowserPage />);

    await screen.findByRole("button", { name: "Ships 2" });
    expect(screen.getByText("Select an item from the market tree.")).toBeInTheDocument();
    expect(screen.queryByRole("table", { name: "Market items" })).not.toBeInTheDocument();
  });

  test("clicking a tree leaf selects that item and renders its detail in the right pane", async () => {
    const user = userEvent.setup();
    render(<MarketBrowserPage />);
    await user.click(await screen.findByRole("button", { name: "Expand Ships" }));
    await user.click(await screen.findByRole("button", { name: /Rifter/ }));

    expect(await screen.findByRole("table", { name: "Sell orders" })).toBeInTheDocument();
    expect(screen.getByText("Rifter", { selector: "strong" })).toBeInTheDocument();
  });

  test("selecting another leaf replaces the right-pane item without navigating away", async () => {
    const user = userEvent.setup();
    render(<MarketBrowserPage />);
    await user.click(await screen.findByRole("button", { name: "Expand Ships" }));
    await user.click(await screen.findByRole("button", { name: /Rifter/ }));
    await screen.findByText("Rifter", { selector: "strong" });

    await user.click(await screen.findByRole("button", { name: /Punisher/ }));

    expect(await screen.findByText("Punisher", { selector: "strong" })).toBeInTheDocument();
    expect(screen.queryByText("Select an item from the market tree.")).not.toBeInTheDocument();
  });

  test("the selected tree leaf is visibly selected", async () => {
    const user = userEvent.setup();
    render(<MarketBrowserPage />);
    await user.click(await screen.findByRole("button", { name: "Expand Ships" }));
    const rifterButton = await screen.findByRole("button", { name: /Rifter/ });

    await user.click(rifterButton);

    expect(await screen.findByRole("button", { name: /Rifter/ })).toHaveClass("bg-primary/10");
  });

  test("the old MarketItemTable is not rendered anywhere in browse mode", async () => {
    const user = userEvent.setup();
    render(<MarketBrowserPage />);
    await user.click(await screen.findByRole("button", { name: "Expand Ships" }));
    await user.click(await screen.findByRole("button", { name: /Rifter/ }));
    await screen.findByRole("table", { name: "Sell orders" });

    expect(screen.queryByRole("table", { name: "Market items" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Refresh market prices" })).not.toBeInTheDocument();
  });

  describe("catalog-wide search", () => {
    test("an empty query shows the category tree, not search results", async () => {
      render(<MarketBrowserPage />);

      await screen.findByRole("button", { name: "Ships 2" });
      expect(screen.queryByText("No items match this search.")).not.toBeInTheDocument();
    });

    test("entering a valid query switches the left pane to catalog-wide search results", async () => {
      const user = userEvent.setup();
      render(<MarketBrowserPage />);
      await screen.findByRole("button", { name: "Ships 2" });

      await user.type(screen.getByRole("textbox", { name: "Search market items" }), "ret");

      expect(await screen.findByRole("button", { name: /Retribution/ })).toBeInTheDocument();
      // The tree stays mounted (just visually hidden) so its own lazy-load
      // cache survives -- assert it's hidden rather than absent.
      expect(screen.getByRole("button", { name: "Ships 2" }).closest(".hidden")).not.toBeNull();
    });

    test("a catalog search does not scope to the currently browsed category", async () => {
      const user = userEvent.setup();
      render(<MarketBrowserPage />);
      await user.click(await screen.findByRole("button", { name: "Ships 2" }));

      await user.type(screen.getByRole("textbox", { name: "Search market items" }), "ret");
      await screen.findByRole("button", { name: /Retribution/ });

      const searchCall = api.listMarketItems.mock.calls.find(([query]) => query.search === "ret");
      expect(searchCall?.[0]).not.toHaveProperty("marketGroupId");
    });

    test("a one-character query does not search -- the tree stays put", async () => {
      const user = userEvent.setup();
      render(<MarketBrowserPage />);
      await screen.findByRole("button", { name: "Ships 2" });
      const callsBefore = api.listMarketItems.mock.calls.length;

      await user.type(screen.getByRole("textbox", { name: "Search market items" }), "r");

      expect(screen.getByRole("button", { name: "Ships 2" })).toBeInTheDocument();
      expect(api.listMarketItems.mock.calls.length).toBe(callsBefore);
    });

    test("typing several characters within the debounce window issues only one search request", async () => {
      vi.useFakeTimers();
      render(<MarketBrowserPage />);
      await act(() => vi.advanceTimersByTimeAsync(0));
      const input = screen.getByRole("textbox", { name: "Search market items" });

      fireEvent.change(input, { target: { value: "tr" } });
      await act(() => vi.advanceTimersByTimeAsync(100));
      fireEvent.change(input, { target: { value: "tri" } });
      await act(() => vi.advanceTimersByTimeAsync(100));
      fireEvent.change(input, { target: { value: "trit" } });
      await act(() => vi.advanceTimersByTimeAsync(300));

      const searchCalls = api.listMarketItems.mock.calls.filter(([query]) => query.search);
      expect(searchCalls).toHaveLength(1);
      expect(searchCalls[0][0]).toMatchObject({ search: "trit" });
    });

    test("a stale response cannot replace a newer query's results", async () => {
      vi.useFakeTimers();
      let resolveFirst!: (page: MarketItemPage) => void;
      let resolveSecond!: (page: MarketItemPage) => void;
      const first = new Promise<MarketItemPage>((resolve) => {
        resolveFirst = resolve;
      });
      const second = new Promise<MarketItemPage>((resolve) => {
        resolveSecond = resolve;
      });
      api.listMarketItems.mockImplementation((query: { search?: string }) => {
        if (!query.search) return Promise.resolve(treeItemsPage);
        return query.search === "ab" ? first : second;
      });
      render(<MarketBrowserPage />);
      await act(() => vi.advanceTimersByTimeAsync(0));
      const input = screen.getByRole("textbox", { name: "Search market items" });

      fireEvent.change(input, { target: { value: "ab" } });
      await act(() => vi.advanceTimersByTimeAsync(300));
      fireEvent.change(input, { target: { value: "abc" } });
      await act(() => vi.advanceTimersByTimeAsync(300));

      // Resolve out of order: the newer query ("abc") lands first, the
      // stale one ("ab") lands after -- it must not overwrite the newer
      // results once it finally resolves.
      resolveSecond(pageWith([{ typeId: 1, typeName: "Abaddon" }]));
      await act(() => Promise.resolve());
      resolveFirst(pageWith([{ typeId: 2, typeName: "Abandoned Module" }]));
      await act(() => Promise.resolve());

      expect(screen.getByRole("button", { name: /Abaddon/ })).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /Abandoned Module/ })).not.toBeInTheDocument();
    });

    test("clicking a search result selects the correct item and updates the existing detail pane", async () => {
      const user = userEvent.setup();
      render(<MarketBrowserPage />);
      await screen.findByRole("button", { name: "Ships 2" });

      await user.type(screen.getByRole("textbox", { name: "Search market items" }), "ret");
      await user.click(await screen.findByRole("button", { name: /Retribution/ }));

      expect(await screen.findByRole("table", { name: "Sell orders" })).toBeInTheDocument();
      expect(screen.getByText("Retribution", { selector: "strong" })).toBeInTheDocument();
      expect(api.getMarketItemOrders).toHaveBeenCalledWith(11_393, expect.any(Number), expect.anything());
    });

    test("the selected item stays selected when the search query is cleared", async () => {
      const user = userEvent.setup();
      render(<MarketBrowserPage />);
      const input = screen.getByRole("textbox", { name: "Search market items" });
      await user.type(input, "ret");
      await user.click(await screen.findByRole("button", { name: /Retribution/ }));
      await screen.findByText("Retribution", { selector: "strong" });

      await user.clear(input);

      await screen.findByRole("button", { name: "Ships 2" });
      expect(screen.getByText("Retribution", { selector: "strong" })).toBeInTheDocument();
    });

    test("clearing the search query restores the category tree", async () => {
      const user = userEvent.setup();
      render(<MarketBrowserPage />);
      const input = screen.getByRole("textbox", { name: "Search market items" });
      await user.type(input, "ret");
      await screen.findByRole("button", { name: /Retribution/ });

      await user.clear(input);

      expect(await screen.findByRole("button", { name: "Ships 2" })).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /Retribution/ })).not.toBeInTheDocument();
    });

    test("tree expansion and its lazy-loaded item cache survive a search/clear round trip", async () => {
      const user = userEvent.setup();
      render(<MarketBrowserPage />);
      await user.click(await screen.findByRole("button", { name: "Expand Ships" }));
      await screen.findByRole("button", { name: /Rifter/ });
      const treeCallsBefore = api.listMarketItems.mock.calls.filter((call) => !call[0].search).length;

      const input = screen.getByRole("textbox", { name: "Search market items" });
      await user.type(input, "ret");
      await screen.findByRole("button", { name: /Retribution/ });
      await user.clear(input);

      expect(await screen.findByRole("button", { name: /Rifter/ })).toBeInTheDocument();
      const treeCallsAfter = api.listMarketItems.mock.calls.filter((call) => !call[0].search).length;
      expect(treeCallsAfter).toBe(treeCallsBefore);
    });

    test("a query with no matches renders a clear no-results state", async () => {
      const user = userEvent.setup();
      api.listMarketItems.mockImplementation((query: { search?: string }) =>
        Promise.resolve(query.search ? pageWith([]) : treeItemsPage),
      );
      render(<MarketBrowserPage />);
      await screen.findByRole("button", { name: "Ships 2" });

      await user.type(screen.getByRole("textbox", { name: "Search market items" }), "zzznomatch");

      expect(await screen.findByText("No items match this search.")).toBeInTheDocument();
    });

    test("a search request failure is localized to the search results and does not clear the selected item's detail", async () => {
      const user = userEvent.setup();
      render(<MarketBrowserPage />);
      await user.click(await screen.findByRole("button", { name: "Expand Ships" }));
      await user.click(await screen.findByRole("button", { name: /Rifter/ }));
      await screen.findByText("Rifter", { selector: "strong" });

      api.listMarketItems.mockImplementation((query: { search?: string }) =>
        query.search ? Promise.reject(new Error("boom")) : Promise.resolve(treeItemsPage),
      );
      await user.type(screen.getByRole("textbox", { name: "Search market items" }), "ret");

      expect(await screen.findByText("ISK Works could not complete the request.")).toBeInTheDocument();
      expect(screen.getByText("Rifter", { selector: "strong" })).toBeInTheDocument();
      expect(screen.getByRole("table", { name: "Sell orders" })).toBeInTheDocument();
    });

    test("capped results show a refine-search hint when more results exist than were returned", async () => {
      const user = userEvent.setup();
      api.listMarketItems.mockImplementation((query: { search?: string }) =>
        Promise.resolve(query.search ? pageWith([{ typeId: 11_393, typeName: "Retribution" }], 5) : treeItemsPage),
      );
      render(<MarketBrowserPage />);
      await screen.findByRole("button", { name: "Ships 2" });

      await user.type(screen.getByRole("textbox", { name: "Search market items" }), "ret");

      expect(await screen.findByText("More results available — refine your search")).toBeInTheDocument();
    });
  });
});
