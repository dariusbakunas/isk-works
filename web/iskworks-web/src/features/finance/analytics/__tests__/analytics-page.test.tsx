import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { AnalyticsPage } from "../analytics-page";
import { CHARACTER_A, emptyAnalytics, sampleAnalytics } from "./fixtures";

const NOW = new Date("2026-09-29T15:00:00Z");

function LocationProbe() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}{location.search}</div>;
}

function stubFetch(payload = sampleAnalytics()) {
  return vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
    const url = String(input);
    if (url.includes("/api/finance/analytics/export")) {
      return new Response("Period start,Income\n", { status: 200, headers: { "content-type": "text/csv" } });
    }
    return new Response(JSON.stringify(payload), { status: 200, headers: { "content-type": "application/json" } });
  });
}

function analyticsCalls(fetchMock: ReturnType<typeof stubFetch>) {
  return fetchMock.mock.calls
    .map(([url]) => String(url))
    .filter((url) => url.includes("/api/finance/analytics?"))
    .map((url) => new URL(url, "http://x").searchParams);
}

function renderPage(path = "/finance/analytics") {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route element={<AnalyticsPage />} path="/finance/analytics" />
        <Route element={<LocationProbe />} path="/finance/transactions" />
      </Routes>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(NOW);
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("AnalyticsPage", () => {
  test("shows a skeleton first, then the KPI cards and cash flow chart", async () => {
    stubFetch();
    renderPage();

    expect(screen.getByRole("status", { name: "Loading analytics" })).toBeInTheDocument();
    const cards = await screen.findAllByTestId("kpi-card");
    expect(cards).toHaveLength(6);
    expect(screen.queryByRole("status", { name: "Loading analytics" })).not.toBeInTheDocument();
    expect(screen.getByRole("img", { name: /^Cash flow by week: 2 periods, income 1\.5B ISK, expenses 300M ISK\./ })).toBeInTheDocument();
  });

  test("abbreviates ISK on cards and keeps the exact value in the tooltip", async () => {
    stubFetch();
    renderPage();
    const [income, expenses, net, margin, , wallet] = await screen.findAllByTestId("kpi-card");

    expect(within(income).getByText("1.5B ISK")).toHaveAttribute("title", "1,500,000,000 ISK");
    expect(within(expenses).getByText("300M ISK")).toBeInTheDocument();
    expect(within(net).getByText("+1.2B ISK")).toBeInTheDocument();
    expect(within(margin).getByText("80.0%")).toBeInTheDocument();
    expect(within(wallet).getByText("2.62B ISK")).toBeInTheDocument();
    // Never the prototype's doubled suffix ("1.24BM"): check each value on its own,
    // since separate elements sit side by side in the page text.
    const values = [...document.querySelectorAll("*")].filter((element) => element.children.length === 0).map((element) => element.textContent ?? "");
    expect(values.filter((text) => /\d[KMBT][KMBT]\b/.test(text))).toEqual([]);
  });

  test("delta arrows follow direction while costs colour inversely", async () => {
    stubFetch();
    renderPage();
    const [income, expenses, , margin] = await screen.findAllByTestId("kpi-card");

    expect(within(income).getByText("+275.0%").parentElement).toHaveClass("text-income");
    // Expenses rose 20%: arrow up, but that is bad news.
    expect(within(expenses).getByText("+20.0%").parentElement).toHaveClass("text-expense");
    expect(within(margin).getByText("+42.5 pp")).toBeInTheDocument();
  });

  test("hides the wallet delta when there is no earlier balance", async () => {
    const payload = sampleAnalytics();
    payload.kpis.walletBalance = { value: "2622608840.1500", delta: null, sparkline: [] };
    stubFetch(payload);
    renderPage();
    const wallet = (await screen.findAllByTestId("kpi-card"))[5];
    expect(within(wallet).getByText("No earlier balance")).toBeInTheDocument();
    expect(within(wallet).queryByText("vs prev")).not.toBeInTheDocument();
  });

  test("requests the default 30-day window with comparison and automatic granularity", async () => {
    const fetchMock = stubFetch();
    renderPage();
    await screen.findAllByTestId("kpi-card");

    const first = analyticsCalls(fetchMock)[0];
    expect(first.get("dateFrom")).toBe("2026-08-31");
    expect(first.get("dateTo")).toBe("2026-09-29");
    expect(first.get("granularity")).toBe("week");
    expect(first.get("comparePrevious")).toBe("true");
    expect(first.has("connectionIds")).toBe(false);
  });

  test("changing the preset, compare toggle and granularity refetches with those filters", async () => {
    const fetchMock = stubFetch();
    renderPage();
    await screen.findAllByTestId("kpi-card");

    fireEvent.click(screen.getByRole("button", { name: "Last 7 days" }));
    await waitFor(() => expect(analyticsCalls(fetchMock).at(-1)?.get("dateFrom")).toBe("2026-09-23"));
    // A 7-day window switches to daily buckets on its own.
    expect(analyticsCalls(fetchMock).at(-1)?.get("granularity")).toBe("day");

    fireEvent.click(screen.getByRole("button", { name: /compare prev/i }));
    await waitFor(() => expect(analyticsCalls(fetchMock).at(-1)?.get("comparePrevious")).toBe("false"));

    fireEvent.click(screen.getByRole("button", { name: "Group by month" }));
    await waitFor(() => expect(analyticsCalls(fetchMock).at(-1)?.get("granularity")).toBe("month"));
  });

  test("a custom range shows date fields seeded from the current window", async () => {
    const fetchMock = stubFetch();
    renderPage();
    await screen.findAllByTestId("kpi-card");

    fireEvent.click(screen.getByRole("button", { name: "Custom date range" }));

    const from = await screen.findByLabelText("From");
    expect(from).toHaveValue("2026-08-31");
    fireEvent.change(from, { target: { value: "2026-09-10" } });
    await waitFor(() => expect(analyticsCalls(fetchMock).at(-1)?.get("dateFrom")).toBe("2026-09-10"));
    expect(analyticsCalls(fetchMock).at(-1)?.get("dateTo")).toBe("2026-09-29");
  });

  test("filters by character from the multi-select", async () => {
    const fetchMock = stubFetch();
    renderPage();
    await screen.findAllByTestId("kpi-card");

    fireEvent.click(screen.getByRole("button", { name: /Filter by character: All 2 chars/ }));
    fireEvent.click(screen.getByRole("menuitemcheckbox", { name: "Aura Valex" }));

    await waitFor(() => expect(analyticsCalls(fetchMock).at(-1)?.get("connectionIds")).toBe(CHARACTER_A));
    expect(screen.getByRole("button", { name: /Filter by character: Aura Valex/ })).toBeInTheDocument();
  });

  test("keeps state in the URL so a reload or shared link reproduces the view", async () => {
    const fetchMock = stubFetch();
    renderPage(`/finance/analytics?range=90d&compare=0&granularity=month&category=Ships`);
    await screen.findAllByTestId("kpi-card");

    const first = analyticsCalls(fetchMock)[0];
    expect(first.get("dateFrom")).toBe("2026-07-02");
    expect(first.get("comparePrevious")).toBe("false");
    expect(first.get("granularity")).toBe("month");
    expect(first.get("category")).toBe("Ships");
    expect(screen.getByText("Category: Ships")).toBeInTheDocument();
  });

  test("a category chip can be removed", async () => {
    const fetchMock = stubFetch();
    renderPage(`/finance/analytics?category=Ships`);
    await screen.findAllByTestId("kpi-card");

    fireEvent.click(screen.getByRole("button", { name: "Remove category filter" }));

    await waitFor(() => expect(analyticsCalls(fetchMock).at(-1)?.has("category")).toBe(false));
    expect(screen.queryByText("Category: Ships")).not.toBeInTheDocument();
  });

  test("shows an empty state with recovery actions when nothing traded", async () => {
    const fetchMock = stubFetch(emptyAnalytics());
    renderPage(`/finance/analytics?range=7d&characters=${CHARACTER_A}`);

    expect(await screen.findByText("No transactions in this period")).toBeInTheDocument();
    expect(screen.getByText(/between Sep 23 – Sep 29, 2026/)).toBeInTheDocument();
    // The header stays so the range can be changed.
    expect(screen.getByRole("button", { name: "Last 30 days" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Expand to 30d" }));
    await waitFor(() => expect(analyticsCalls(fetchMock).at(-1)?.get("dateFrom")).toBe("2026-08-31"));
  });

  test("offers All characters when the empty result is character-filtered", async () => {
    const fetchMock = stubFetch(emptyAnalytics());
    renderPage(`/finance/analytics?characters=${CHARACTER_A}`);
    await screen.findByText("No transactions in this period");

    fireEvent.click(screen.getByRole("button", { name: "All characters" }));

    await waitFor(() => expect(analyticsCalls(fetchMock).at(-1)?.has("connectionIds")).toBe(false));
  });

  test("shows the API error with a retry", async () => {
    let fail = true;
    vi.spyOn(globalThis, "fetch").mockImplementation(async () => {
      if (fail) {
        return new Response(JSON.stringify({ error: { code: "boom", message: "The ledger is unavailable." } }), { status: 500 });
      }
      return new Response(JSON.stringify(sampleAnalytics()), { status: 200 });
    });
    renderPage();

    expect(await screen.findByText(/The ledger is unavailable\./)).toBeInTheDocument();
    fail = false;
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));

    expect(await screen.findAllByTestId("kpi-card")).toHaveLength(6);
  });

  test("explains what the numbers cover and what was left out", async () => {
    stubFetch();
    renderPage();
    await screen.findAllByTestId("kpi-card");

    expect(screen.getByText(/market trades only; taxes & fees come from the wallet journal/)).toBeInTheDocument();
    expect(screen.getByText(/3 trades between your own characters \(200,000,000 ISK\) are excluded/)).toBeInTheDocument();
    // History starts 2026-07-05, well before the 30-day window, so no coverage warning.
    expect(screen.queryByText(/Synced history starts/)).not.toBeInTheDocument();
  });

  test("warns when synced history starts after the range does", async () => {
    stubFetch({ ...sampleAnalytics(), earliestObservedAt: "2026-09-10T00:00:00Z" });
    renderPage();
    await screen.findAllByTestId("kpi-card");

    expect(screen.getByText("Synced history starts 2026-09-10; earlier days in this range have no data.")).toBeInTheDocument();
  });

  test("View transactions deep-links to Transactions with the page's filters", async () => {
    stubFetch();
    renderPage(`/finance/analytics?characters=${CHARACTER_A}&category=Ships`);
    await screen.findAllByTestId("kpi-card");

    fireEvent.click(screen.getByRole("button", { name: "Cash Flow options" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "View transactions" }));

    const location = new URL(`http://x${screen.getByTestId("location").textContent}`);
    expect(location.pathname).toBe("/finance/transactions");
    expect(location.searchParams.get("connectionIds")).toBe(CHARACTER_A);
    expect(location.searchParams.get("dateFrom")).toBe("2026-08-31");
    expect(location.searchParams.get("dateTo")).toBe("2026-09-29");
    expect(location.searchParams.get("category")).toBe("Ships");
  });

  test("Export CSV downloads the section with the current filters", async () => {
    const fetchMock = stubFetch();
    const createObjectURL = vi.fn(() => "blob:csv");
    Object.assign(URL, { createObjectURL, revokeObjectURL: vi.fn() });
    const click = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => undefined);
    renderPage(`/finance/analytics?range=90d`);
    await screen.findAllByTestId("kpi-card");

    fireEvent.click(screen.getByRole("button", { name: "Cash Flow options" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Export CSV" }));

    await waitFor(() => expect(click).toHaveBeenCalled());
    const exportCall = fetchMock.mock.calls.map(([url]) => String(url)).find((url) => url.includes("/analytics/export"))!;
    const params = new URL(exportCall, "http://x").searchParams;
    expect(params.get("section")).toBe("cashFlow");
    expect(params.get("dateFrom")).toBe("2026-07-02");
  });
});

describe("AnalyticsPage breakdowns", () => {
  test("lists every category with abbreviated values and colour swatches", async () => {
    stubFetch();
    renderPage();
    const spending = await screen.findByRole("list", { name: "Spending categories" });

    expect(within(spending).getByText("Ships")).toBeInTheDocument();
    expect(within(spending).getByText("180M")).toBeInTheDocument();
    // Spending on Ships rose 80%: an increase in cost reads as bad.
    expect(within(spending).getByText("+80.0%")).toHaveClass("text-expense");
    // Modules spend fell 33%: good.
    expect(within(spending).getByText("-33.3%")).toHaveClass("text-income");
    const income = screen.getByRole("list", { name: "Income categories" });
    expect(within(income).getByText("1.2B")).toBeInTheDocument();
  });

  test("clicking a category filters the whole page and shows the chip", async () => {
    const fetchMock = stubFetch();
    renderPage();
    const spending = await screen.findByRole("list", { name: "Spending categories" });

    fireEvent.click(within(spending).getByRole("button", { name: /Ships/ }));

    await waitFor(() => expect(analyticsCalls(fetchMock).at(-1)?.get("category")).toBe("Ships"));
    expect(await screen.findByText("Category: Ships")).toBeInTheDocument();
    // Selecting again clears it.
    fireEvent.click(within(await screen.findByRole("list", { name: "Spending categories" })).getByRole("button", { name: /Ships/ }));
    await waitFor(() => expect(analyticsCalls(fetchMock).at(-1)?.has("category")).toBe(false));
  });

  test("the selected category is marked and the folded Other cannot be a filter", async () => {
    stubFetch();
    renderPage("/finance/analytics?category=Ships");
    const spending = await screen.findByRole("list", { name: "Spending categories" });

    expect(within(spending).getByRole("button", { name: /Ships/ })).toHaveAttribute("aria-pressed", "true");
    expect(within(spending).getByRole("button", { name: /Modules/ })).toHaveAttribute("aria-pressed", "false");
    expect(within(spending).getByRole("button", { name: /Other/ })).toBeDisabled();
  });

  test("the donut cards link to Transactions without the page's category, other cards keep it", async () => {
    stubFetch();
    renderPage("/finance/analytics?category=Ships");
    await screen.findByRole("list", { name: "Spending categories" });

    fireEvent.click(screen.getByRole("button", { name: "Spending by Category options" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "View transactions" }));
    const link = new URL(`http://x${screen.getByTestId("location").textContent}`);
    expect(link.searchParams.get("direction")).toBe("expense");
    expect(link.searchParams.has("category")).toBe(false);
  });

  test("the inventory toggle refetches and explains what it left out", async () => {
    const fetchMock = stubFetch({
      ...sampleAnalytics(),
      excludedInventoryBuys: { transactionCount: 2, totalIsk: "50000000.0000" },
    });
    renderPage();
    await screen.findAllByTestId("kpi-card");
    expect(screen.queryByText(/already recorded into Inventory/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /exclude inventory buys/i }));

    await waitFor(() => expect(analyticsCalls(fetchMock).at(-1)?.get("excludeInventoryBuys")).toBe("true"));
    expect(screen.getByRole("button", { name: /exclude inventory buys/i })).toHaveAttribute("aria-pressed", "true");
  });

  test("shows income and expense bars and net per character", async () => {
    stubFetch();
    renderPage();
    await screen.findByRole("region", { name: "By Character" });

    const card = screen.getByRole("region", { name: "By Character" });
    expect(within(card).getByRole("img", { name: "Aura Valex: income 1B ISK, expenses 100M ISK" })).toBeInTheDocument();
    expect(within(card).getByText("+900M")).toBeInTheDocument();
    expect(within(card).getByText("+300M")).toBeInTheDocument();
  });

  test("locations table shows volume rows and a region sub-line", async () => {
    stubFetch();
    renderPage();
    const card = await screen.findByRole("region", { name: "By Location" });

    expect(within(card).getByText("Jita IV - Moon 4")).toBeInTheDocument();
    expect(within(card).getByText("The Forge")).toBeInTheDocument();
    expect(within(card).getByText("847")).toBeInTheDocument();
    // Tables carry the exact figure, not the abbreviated one used on cards and charts.
    expect(within(card).getByText("900,000,000")).toBeInTheDocument();
    expect(within(card).getByText("+700,000,000")).toBeInTheDocument();
  });

  test("opening a location deep-links with its id and name", async () => {
    stubFetch();
    renderPage();
    const card = await screen.findByRole("region", { name: "By Location" });

    fireEvent.click(within(card).getByRole("button", { name: "Jita IV - Moon 4" }));

    const link = new URL(`http://x${screen.getByTestId("location").textContent}`);
    expect(link.pathname).toBe("/finance/transactions");
    expect(link.searchParams.get("locationId")).toBe("60003760");
    expect(link.searchParams.get("locationLabel")).toBe("Jita IV - Moon 4");
  });

  test("top tables label their share column correctly and link to the item", async () => {
    stubFetch();
    renderPage();
    const expenses = await screen.findByRole("region", { name: "Top Expenses" });
    const earners = screen.getByRole("region", { name: "Top Earners" });

    expect(within(expenses).getByText("% of spend")).toBeInTheDocument();
    // The prototype said "% spend" on both tables.
    expect(within(earners).getByText("% of income")).toBeInTheDocument();
    expect(within(earners).queryByText(/% spend/)).not.toBeInTheDocument();
    expect(within(earners).getByText("42.5%")).toBeInTheDocument();
    // Average price is exact ISK per unit; quantity and total are exact too.
    expect(within(expenses).getByText("8,420")).toBeInTheDocument();
    expect(within(expenses).getByText("45,000")).toBeInTheDocument();
    expect(within(expenses).getByText("379,000,000")).toBeInTheDocument();

    fireEvent.click(within(earners).getByRole("button", { name: "Rifter" }));
    const link = new URL(`http://x${screen.getByTestId("location").textContent}`);
    expect(link.searchParams.get("typeId")).toBe("587");
    expect(link.searchParams.get("direction")).toBe("income");
    expect(link.searchParams.get("itemLabel")).toBe("Rifter");
  });

  test("a side with no activity says so instead of drawing an empty chart", async () => {
    stubFetch({ ...sampleAnalytics(), incomeByCategory: [], topEarners: [] });
    renderPage();

    expect(await screen.findByText("No income in this period.")).toBeInTheDocument();
    expect(screen.getByText("No sales in this period.")).toBeInTheDocument();
  });

  test("every breakdown card exports its own CSV section", async () => {
    const fetchMock = stubFetch();
    Object.assign(URL, { createObjectURL: vi.fn(() => "blob:csv"), revokeObjectURL: vi.fn() });
    const click = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => undefined);
    renderPage();
    await screen.findByRole("region", { name: "Top Earners" });

    fireEvent.click(screen.getByRole("button", { name: "Top Earners options" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Export CSV" }));

    await waitFor(() => expect(click).toHaveBeenCalled());
    const exportUrl = fetchMock.mock.calls.map(([url]) => String(url)).find((url) => url.includes("/analytics/export"))!;
    expect(new URL(exportUrl, "http://x").searchParams.get("section")).toBe("topEarners");
  });
});

describe("AnalyticsPage flow, heatmap and insights", () => {
  test("Money Flow lists sources, destinations and what was kept, with exact values on hover", async () => {
    stubFetch();
    renderPage();
    const card = await screen.findByRole("region", { name: "Money Flow" });

    // 1.5B in, 300M out: the wallet kept 1.2B.
    expect(within(card).getAllByText("Net saved").length).toBeGreaterThan(0);
    expect(within(card).getAllByText("Ships").length).toBe(2);
    expect(within(card).getAllByText("1.2B").length).toBe(2);
    expect(card.querySelector("title")).not.toBeNull();
    expect(within(card).getAllByText(/^Net saved: 1,200,000,000 ISK$/).length).toBeGreaterThan(0);
  });

  test("labels which column is income and which is spending, since a category can be on both", async () => {
    stubFetch();
    renderPage();
    const card = await screen.findByRole("region", { name: "Money Flow" });

    expect(within(card).getByText("INCOME SOURCES")).toBeInTheDocument();
    expect(within(card).getByText("SPENDING")).toBeInTheDocument();
    // Ships is both earned and spent: each node says which it is, in its name and tooltip.
    expect(within(card).getByRole("button", { name: "Ships, income, 1.2B ISK" })).toBeInTheDocument();
    expect(within(card).getByRole("button", { name: "Ships, spending, 180M ISK" })).toBeInTheDocument();
    expect(within(card).getAllByText("Ships (income): 1,200,000,000 ISK").length).toBeGreaterThan(0);
    expect(within(card).getAllByText("Ships (spending): 180,000,000 ISK").length).toBeGreaterThan(0);
  });

  test("clicking a Money Flow node filters the page, and Other and Net saved are not filters", async () => {
    const fetchMock = stubFetch();
    renderPage();
    const card = await screen.findByRole("region", { name: "Money Flow" });

    const diagram = card.querySelector('svg[role="group"]') as unknown as HTMLElement;
    const buttons = within(diagram).getAllByRole("button");
    // Ships and Modules on the income side, Ships and Modules on the spending side.
    expect(buttons).toHaveLength(4);
    expect(within(card).queryByRole("button", { name: /Other/ })).not.toBeInTheDocument();
    expect(within(card).queryByRole("button", { name: /Net saved/ })).not.toBeInTheDocument();

    fireEvent.click(within(card).getAllByRole("button", { name: /^Ships, income/ })[0]);
    await waitFor(() => expect(analyticsCalls(fetchMock).at(-1)?.get("category")).toBe("Ships"));
  });

  test("shows a Drawdown source when spending exceeds income", async () => {
    stubFetch({
      ...sampleAnalytics(),
      incomeByCategory: [{ category: "Ships", total: "100000000.0000", previous: null }],
      spendingByCategory: [{ category: "Modules", total: "400000000.0000", previous: null }],
    });
    renderPage();
    const card = await screen.findByRole("region", { name: "Money Flow" });

    expect(within(card).getAllByText("Drawdown").length).toBeGreaterThan(0);
    expect(within(card).queryByText("Net saved")).not.toBeInTheDocument();
  });

  test("Daily Net draws one fixed square cell per day with the exact net on hover", async () => {
    stubFetch();
    renderPage();
    const card = await screen.findByRole("region", { name: "Daily Net" });

    const cells = within(card).getAllByTestId("heatmap-cell");
    expect(cells).toHaveLength(5);
    for (const cell of cells) {
      // Square and fixed: the prototype's cells stretched into wide bars.
      expect(cell).toHaveStyle({ width: "13px", height: "13px" });
    }
    expect(within(card).getByRole("img", { name: "Sep 26: +42,000,000 ISK" })).toBeInTheDocument();
    expect(within(card).getByRole("img", { name: "Sep 27: -21,000,000 ISK" })).toBeInTheDocument();
  });

  test("Daily Net colours gains green, losses red and empty days neutral", async () => {
    stubFetch();
    renderPage();
    const card = await screen.findByRole("region", { name: "Daily Net" });

    const style = (label: string) => within(card).getByRole("img", { name: label }).getAttribute("style") ?? "";
    expect(style("Sep 26: +42,000,000 ISK")).toContain("--color-income");
    expect(style("Sep 27: -21,000,000 ISK")).toContain("--color-expense");
    expect(style("Sep 25: 0 ISK")).toContain("--color-panel-strong");
  });

  test("Daily Net aligns days to the real weekday", async () => {
    stubFetch();
    renderPage();
    const grid = await screen.findByTestId("heatmap-grid");

    // Sep 25 2026 is a Friday: four blank cells (Mon-Thu) come first in the column.
    const slots = [...grid.children];
    expect(slots.slice(0, 4).every((slot) => slot.getAttribute("aria-hidden") === "true")).toBe(true);
    expect(slots[4].getAttribute("aria-label")).toBe("Sep 25: 0 ISK");
  });

  test("Daily Net links to the transactions of its own 91-day window, not the page range", async () => {
    stubFetch();
    renderPage("/finance/analytics?range=7d");
    await screen.findByRole("region", { name: "Daily Net" });

    fireEvent.click(screen.getByRole("button", { name: "Daily Net options" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "View transactions" }));

    const link = new URL(`http://x${screen.getByTestId("location").textContent}`);
    expect(link.searchParams.get("dateFrom")).toBe("2026-09-25");
    expect(link.searchParams.get("dateTo")).toBe("2026-09-29");
  });

  test("insights are written from the server's numbers", async () => {
    stubFetch();
    renderPage();
    const section = await screen.findByRole("region", { name: "Insights" });

    expect(within(section).getByText("Ships spending up 80%")).toBeInTheDocument();
    expect(within(section).getByText("180M vs 100M in the previous period.")).toBeInTheDocument();
    expect(within(section).getByText("67% of spending at Jita IV - Moon 4")).toBeInTheDocument();
    expect(within(section).getByText("Aura Valex earned 67% of income")).toBeInTheDocument();
    expect(within(section).getByText("1B of 1.5B total income.")).toBeInTheDocument();
  });

  test("says so when nothing stands out instead of showing canned tips", async () => {
    stubFetch({ ...sampleAnalytics(), insights: [] });
    renderPage();

    expect(await screen.findByText("Nothing stands out in this period.")).toBeInTheDocument();
  });

  test("Money Flow and Daily Net export their own CSV sections", async () => {
    const fetchMock = stubFetch();
    Object.assign(URL, { createObjectURL: vi.fn(() => "blob:csv"), revokeObjectURL: vi.fn() });
    const click = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => undefined);
    renderPage();
    await screen.findByRole("region", { name: "Money Flow" });

    for (const [card, section] of [["Money Flow", "flow"], ["Daily Net", "heatmap"]] as const) {
      fireEvent.click(screen.getByRole("button", { name: `${card} options` }));
      fireEvent.click(screen.getByRole("menuitem", { name: "Export CSV" }));
      await waitFor(() => {
        const sections = fetchMock.mock.calls
          .map(([url]) => String(url))
          .filter((url) => url.includes("/analytics/export"))
          .map((url) => new URL(url, "http://x").searchParams.get("section"));
        expect(sections).toContain(section);
      });
    }
    expect(click).toHaveBeenCalledTimes(2);
  });
});

describe("AnalyticsPage taxes and fees", () => {
  const card = async (label: string) =>
    (await screen.findAllByTestId("kpi-card")).find((element) => within(element).queryByText(label, { exact: false }) !== null)!;

  test("shows a Taxes & fees card between margin and wallet, with a cost-coloured delta", async () => {
    stubFetch();
    renderPage();
    const cards = await screen.findAllByTestId("kpi-card");

    expect(cards.map((element) => within(element).getByText(/^(Income|Expenses|Net ISK|Profit margin|Taxes & fees|Wallet balance)$/).textContent)).toEqual([
      "Income", "Expenses", "Net ISK", "Profit margin", "Taxes & fees", "Wallet balance",
    ]);
    const fees = await card("Taxes & fees");
    expect(within(fees).getByText("312M ISK")).toHaveAttribute("title", expect.stringContaining("312,000,000 ISK"));
    // Fees rose 4%: the arrow goes up and it is bad news.
    expect(within(fees).getByText("+4.0%").parentElement).toHaveClass("text-expense");
  });

  test("the fee tooltip breaks the total down by kind", async () => {
    stubFetch();
    renderPage();
    const fees = await card("Taxes & fees");

    const title = within(fees).getByText("312M ISK").getAttribute("title")!;
    expect(title).toContain("Broker fees 100,000,000 ISK");
    expect(title).toContain("Sales tax 200,000,000 ISK");
    expect(title).toContain("Structure market fees 12,000,000 ISK");
  });

  test("there is no fee card without journal data or while a category is selected", async () => {
    const payload = sampleAnalytics();
    payload.kpis.fees = null;
    stubFetch(payload);
    renderPage();

    expect(await screen.findAllByTestId("kpi-card")).toHaveLength(5);
    expect(screen.queryByText("Taxes & fees")).not.toBeInTheDocument();
  });

  test("says when the journal only covers part of the range instead of comparing", async () => {
    const payload = sampleAnalytics();
    payload.kpis.fees = { ...payload.kpis.fees!, delta: null, availableFrom: "2026-09-10" };
    stubFetch(payload);
    renderPage();
    const fees = await card("Taxes & fees");

    expect(within(fees).getByText("Journal from Sep 10")).toBeInTheDocument();
    expect(within(fees).queryByText("vs prev")).not.toBeInTheDocument();
    expect(screen.getByText("Taxes & fees are available from 2026-09-10; earlier days in this range are not included.")).toBeInTheDocument();
  });

  test("the footnote no longer claims the journal is missing once it is used", async () => {
    stubFetch();
    renderPage();
    await screen.findAllByTestId("kpi-card");

    expect(screen.getByText(/taxes & fees come from the wallet journal/i)).toBeInTheDocument();
    expect(screen.queryByText(/wallet journal is not included/)).not.toBeInTheDocument();
  });

  test("still explains the gap when there is no journal data", async () => {
    const payload = sampleAnalytics();
    payload.kpis.fees = null;
    stubFetch(payload);
    renderPage();
    await screen.findAllByTestId("kpi-card");

    expect(screen.getByText(/wallet journal is not included/)).toBeInTheDocument();
  });
});

describe("AnalyticsPage card order", () => {
  test("income is always on the left and spending on the right, matching Money Flow", async () => {
    stubFetch();
    renderPage();
    await screen.findByRole("region", { name: "Money Flow" });

    const order = screen.getAllByRole("region").map((region) => region.getAttribute("aria-label"));
    const before = (first: string, second: string) => order.indexOf(first) < order.indexOf(second);
    expect(before("Income by Category", "Spending by Category")).toBe(true);
    expect(before("Top Earners", "Top Expenses")).toBe(true);
  });
});
