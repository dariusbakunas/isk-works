import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type { MarketOrdersOverview } from "../../../api/market-orders";
import { MarketOrdersPage } from "../market-orders-page";
import { character, ILSA, location, megacyteBuy, NOW, overview } from "./fixtures";

function stubFetch(body: MarketOrdersOverview = overview()) {
  return vi.spyOn(globalThis, "fetch").mockImplementation(async (input, init) => {
    if (String(input).endsWith("/api/market-orders/refresh") && init?.method === "POST") {
      return new Response(null, { status: 202 });
    }
    return new Response(JSON.stringify(body), { status: 200, headers: { "content-type": "application/json" } });
  });
}

function LocationProbe() {
  const current = useLocation();
  return <div data-testid="location">{current.pathname}{current.search}</div>;
}

function renderPage(path = "/market-orders") {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route element={<><MarketOrdersPage /><LocationProbe /></>} path="/market-orders" />
      </Routes>
    </MemoryRouter>,
  );
}

function rowFor(table: HTMLElement, name: string) {
  return within(table).getByText(name).closest("tr")!;
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(NOW);
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("Market Orders", () => {
  test("shows the summary and orders grouped by location, outbid first", async () => {
    stubFetch();
    renderPage();

    expect(screen.getByRole("status", { name: "Loading market orders" })).toBeInTheDocument();
    const kpis = await screen.findAllByTestId("kpi-card");
    expect(kpis[0]).toHaveTextContent("Outbid1");
    expect(kpis[3]).toHaveTextContent("7,639,600,000 ISK");
    expect(screen.getByText("Books as of 3m ago · Your orders synced 12m ago")).toBeInTheDocument();

    const table = screen.getByRole("table", { name: "Market orders" });
    const headers = within(table).getAllByRole("rowheader").map((header) => header.textContent);
    expect(headers[0]).toContain("Jita IV - Moon 4");
    expect(headers[0]).toContain("1 outbid");
    expect(headers[1]).toContain("K-6K16");
    expect(headers[1]).toContain("Cannot read this market");

    const ishtar = rowFor(table, "Ishtar");
    expect(within(ishtar).getByText("OUTBID")).toBeInTheDocument();
    expect(within(ishtar).getByTitle("312,400,000 ISK")).toHaveTextContent("312.4M");
    expect(within(ishtar).getByTitle("311,900,000 ISK")).toHaveTextContent("311.9M");
    expect(within(ishtar).getByText("2 ahead · 5 units")).toBeInTheDocument();
    expect(within(ishtar).getByTitle("311,800,000 ISK")).toHaveTextContent("311.8M");
    expect(within(ishtar).getByText("3m 0s")).toBeInTheDocument();
    expect(within(ishtar).getByRole("img", { name: "Undercut loses money" })).toBeInTheDocument();

    // Four significant digits keep close buy prices apart.
    const megacyte = rowFor(table, "Megacyte");
    expect(within(megacyte).getByTitle("9,210 ISK")).toHaveTextContent("9.21K");
    expect(within(megacyte).getByText("lead 10.00")).toBeInTheDocument();
    expect(within(megacyte).getByText("now")).toBeInTheDocument();

    expect(screen.getByText(/denied docking access to this structure/)).toBeInTheDocument();
  });

  test("copies the suggested price as a plain number", async () => {
    stubFetch();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    renderPage();

    const table = await screen.findByRole("table", { name: "Market orders" });
    fireEvent.click(within(rowFor(table, "Ishtar")).getByRole("button", { name: "Copy 311,800,000 ISK" }));

    expect(writeText).toHaveBeenCalledWith("311800000");
    // Copying doesn't open the inspector.
    expect(screen.getByTestId("location")).toHaveTextContent("/market-orders");
  });

  test("filters by side, outbid status and character through the URL", async () => {
    stubFetch();
    renderPage();
    const table = await screen.findByRole("table", { name: "Market orders" });

    fireEvent.click(screen.getByRole("button", { name: "Outbid only" }));
    expect(screen.getByTestId("location")).toHaveTextContent("outbid=1");
    expect(within(table).queryByText("Megacyte")).not.toBeInTheDocument();
    expect(within(table).getByText("Ishtar")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Outbid only" }));
    fireEvent.click(within(screen.getByRole("group", { name: "Side" })).getByRole("button", { name: "Buy" }));
    expect(screen.getByTestId("location")).toHaveTextContent("side=buy");
    expect(within(table).queryByText("Ishtar")).not.toBeInTheDocument();
    expect(within(table).getByText("Megacyte")).toBeInTheDocument();

    fireEvent.click(within(screen.getByRole("group", { name: "Side" })).getByRole("button", { name: "All" }));
    fireEvent.click(within(screen.getByRole("group", { name: "Characters" })).getByRole("button", { name: "Ilsa Morrow" }));
    expect(screen.getByTestId("location")).toHaveTextContent(`hide=${ILSA}`);
    expect(within(table).queryByText("Large Skill Injector")).not.toBeInTheDocument();
  });

  test("opens the inspector with the book depth, pricing and cost basis", async () => {
    stubFetch();
    renderPage();
    const table = await screen.findByRole("table", { name: "Market orders" });

    fireEvent.click(rowFor(table, "Ishtar"));

    const ladder = await screen.findByRole("list", { name: "Sell orders" });
    const rows = within(ladder).getAllByTestId("depth-row");
    expect(rows).toHaveLength(4);
    expect(rows[0]).toHaveClass("text-danger");
    expect(within(rows[1]).getByText("ALT")).toBeInTheDocument();
    expect(rows[1]).not.toHaveClass("text-danger");
    expect(within(rows[2]).getByText("YOU")).toBeInTheDocument();
    expect(screen.getByRole("list", { name: "Buy orders reaching here" })).toBeInTheDocument();
    expect(screen.getByText("Outbid since 12m ago")).toBeInTheDocument();
    expect(screen.getByText("Broker fees not included")).toBeInTheDocument();
    expect(screen.getByText("305,000,000 ISK")).toBeInTheDocument();
    expect(screen.getByText("-1.22%")).toHaveClass("text-danger");
    expect(screen.getByTestId("location")).toHaveTextContent("order=1");
  });

  test("greys out ranged buy orders that can't reach the location", async () => {
    stubFetch(overview({ locations: [location({ orders: [megacyteBuy()] })] }));
    renderPage("/market-orders?order=5");

    const ladder = await screen.findByRole("list", { name: "Buy orders" });
    const rows = within(ladder).getAllByTestId("depth-row");
    expect(within(rows[0]).getByText("out of range")).toBeInTheDocument();
    expect(rows[0]).toHaveClass("opacity-50");
    expect(rows[0]).not.toHaveClass("text-danger");
    expect(within(rows[0]).getByText("· 5 jumps")).toBeInTheDocument();
    expect(screen.getByText("Greyed rows can't reach this location.")).toBeInTheDocument();
  });

  test("asks characters without the scope to refresh permissions and flags missing stargates", async () => {
    stubFetch(
      overview({
        characters: [character(), character({ connectionId: ILSA, characterName: "Ilsa Morrow", ordersScopeMissing: true })],
        stargatesMissing: true,
      }),
    );
    renderPage();

    expect(await screen.findByText("1 character hasn't granted market order access")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Refresh permissions" })).toBeInTheDocument();
    expect(screen.getByText("Buy order range checks need stargates.")).toBeInTheDocument();
  });

  test("refresh prioritizes the books and reloads", async () => {
    const fetchMock = stubFetch();
    renderPage();
    await screen.findByRole("table", { name: "Market orders" });

    fireEvent.click(screen.getByRole("button", { name: "Refresh market orders" }));

    await waitFor(() =>
      expect(fetchMock.mock.calls.filter(([url]) => String(url).endsWith("/api/market-orders"))).toHaveLength(2),
    );
    expect(fetchMock.mock.calls.some(([url, init]) => String(url).endsWith("/refresh") && init?.method === "POST")).toBe(true);
  });

  test("says so when there are no orders", async () => {
    stubFetch(overview({ locations: [] }));
    renderPage();

    expect(await screen.findByText("No active market orders")).toBeInTheDocument();
  });
});
