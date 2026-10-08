import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, test, vi } from "vitest";

import { TransactionsPage } from "../transactions-page";

const page = {
  rows: [],
  summary: {
    walletBalance: "0.0000",
    income: "0.0000",
    expenses: "0.0000",
    netIsk: "0.0000",
    transactionCount: 0,
    averageDailyIsk: "0.0000",
  },
  availableCharacters: [],
  totalCount: 0,
  page: 1,
  pageSize: 100,
};

function stubFetch() {
  return vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
    if (String(input).includes("saved-filters")) return new Response("[]", { status: 200 });
    return new Response(JSON.stringify(page), { status: 200, headers: { "content-type": "application/json" } });
  });
}

function transactionUrls(fetchMock: ReturnType<typeof stubFetch>) {
  return fetchMock.mock.calls
    .map(([url]) => String(url))
    .filter((url) => url.includes("/api/finance/transactions?"))
    .map((url) => new URL(url, "http://x").searchParams);
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("Transactions deep links from Analytics", () => {
  test("applies the URL filters to the first request", async () => {
    const fetchMock = stubFetch();

    render(
      <MemoryRouter initialEntries={["/finance/transactions?dateFrom=2026-09-01&dateTo=2026-09-15&direction=income&category=Ships&typeId=587&locationId=60003760"]}>
        <TransactionsPage />
      </MemoryRouter>,
    );

    await waitFor(() => expect(transactionUrls(fetchMock).length).toBeGreaterThan(0));
    const first = transactionUrls(fetchMock)[0];
    expect(first.get("dateFrom")).toBe("2026-09-01");
    expect(first.get("dateTo")).toBe("2026-09-15");
    expect(first.get("direction")).toBe("income");
    expect(first.get("category")).toBe("Ships");
    expect(first.get("typeId")).toBe("587");
    expect(first.get("locationId")).toBe("60003760");
  });

  test("shows removable chips for filters the rail has no control for", async () => {
    const fetchMock = stubFetch();
    render(
      <MemoryRouter initialEntries={["/finance/transactions?category=Ships&typeId=587"]}>
        <TransactionsPage />
      </MemoryRouter>,
    );

    expect(await screen.findByText("Category: Ships")).toBeInTheDocument();
    expect(screen.getByText("Item #587")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Remove category filter" }));

    await waitFor(() => {
      const latest = transactionUrls(fetchMock).at(-1)!;
      expect(latest.has("category")).toBe(false);
      expect(latest.get("typeId")).toBe("587");
    });
    expect(screen.queryByText("Category: Ships")).not.toBeInTheDocument();
  });

  test("labels item and location chips with the names Analytics passed along", async () => {
    stubFetch();
    render(
      <MemoryRouter initialEntries={["/finance/transactions?typeId=34&itemLabel=Tritanium&locationId=60003760&locationLabel=Jita%20IV"]}>
        <TransactionsPage />
      </MemoryRouter>,
    );

    expect(await screen.findByText("Item: Tritanium")).toBeInTheDocument();
    expect(screen.getByText("Location: Jita IV")).toBeInTheDocument();
  });

  test("applies and can drop the Inventory-buys exclusion", async () => {
    const fetchMock = stubFetch();
    render(
      <MemoryRouter initialEntries={["/finance/transactions?excludeInventoryBuys=true"]}>
        <TransactionsPage />
      </MemoryRouter>,
    );

    expect(await screen.findByText("Excluding Inventory buys")).toBeInTheDocument();
    await waitFor(() => expect(transactionUrls(fetchMock)[0].get("excludeInventoryBuys")).toBe("true"));

    fireEvent.click(screen.getByRole("button", { name: "Remove inventory filter" }));

    await waitFor(() => expect(transactionUrls(fetchMock).at(-1)!.has("excludeInventoryBuys")).toBe(false));
    expect(screen.queryByText("Excluding Inventory buys")).not.toBeInTheDocument();
  });

  test("a plain visit keeps the default last-30-days filter and shows no chips", async () => {
    const fetchMock = stubFetch();
    render(<MemoryRouter><TransactionsPage /></MemoryRouter>);

    await waitFor(() => expect(transactionUrls(fetchMock).length).toBeGreaterThan(0));
    const first = transactionUrls(fetchMock)[0];
    expect(first.has("category")).toBe(false);
    expect(first.get("direction")).toBe("all");
    expect(screen.queryByRole("button", { name: /^Remove .* filter$/ })).not.toBeInTheDocument();
  });
});
