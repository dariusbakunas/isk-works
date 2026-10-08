import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, test, vi } from "vitest";

import { TransactionsPage } from "../transactions-page";

const page = {
  rows: [{
    observationId: "obs-1",
    transactionId: 42,
    connectionId: "connection-1",
    characterName: "Aura Valex",
    transactionType: "marketSell",
    typeId: 587,
    typeName: "Rifter",
    quantity: 5,
    unitPrice: "700000.0000",
    totalPrice: "3500000.0000",
    transactedAt: "2026-08-05T18:42:00Z",
    counterpartyName: "Caldari Navy",
    locationName: "Jita IV - Moon 4 - Caldari Navy Assembly Plant",
    regionName: "The Forge",
    inventoryRecording: null,
  }],
  summary: {
    walletBalance: "25240000000.0000",
    income: "3500000.0000",
    expenses: "0.0000",
    netIsk: "3500000.0000",
    transactionCount: 1,
    averageDailyIsk: "1750000.0000",
  },
  availableCharacters: [{
    connectionId: "connection-1",
    characterName: "Aura Valex",
    walletBalance: "25240000000.0000",
    balanceObservedAt: "2026-08-05T18:45:00Z",
  }],
  totalCount: 1,
  page: 1,
  pageSize: 100,
};

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("TransactionsPage", () => {
  test("uses the approved filter section defaults and collapses sections without refetching", async () => {
    const fetchMock = vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
      if (String(input).includes("saved-filters")) return new Response("[]", { status: 200 });
      return new Response(JSON.stringify(page), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    });

    render(<MemoryRouter><TransactionsPage /></MemoryRouter>);
    expect(await screen.findByText("Rifter")).toBeInTheDocument();

    for (const name of ["Characters filters", "Date range filters", "Direction filters", "Type filters", "Saved filters"]) {
      expect(screen.getByRole("button", { name: new RegExp(`^${name}$`, "i") })).toHaveAttribute("aria-expanded", "true");
    }
    // No empty "More filters" section.
    expect(screen.queryByRole("button", { name: /^more filters$/i })).not.toBeInTheDocument();

    const transactionCalls = fetchMock.mock.calls.filter(([url]) => String(url).includes("/api/finance/transactions?")).length;
    fireEvent.click(screen.getByRole("button", { name: /^type filters$/i }));
    expect(screen.queryByLabelText("Market buy")).not.toBeInTheDocument();
    expect(fetchMock.mock.calls.filter(([url]) => String(url).includes("/api/finance/transactions?")).length).toBe(transactionCalls);
  });

  test("selects all or no transaction types from the type section", async () => {
    const fetchMock = vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
      if (String(input).includes("saved-filters")) return new Response("[]", { status: 200 });
      return new Response(JSON.stringify(page), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    });

    render(<MemoryRouter><TransactionsPage /></MemoryRouter>);
    expect(await screen.findByText("Rifter")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^type filters$/i })).toHaveTextContent("2/2");

    fireEvent.click(screen.getByRole("button", { name: "None" }));
    await waitFor(() => expect(fetchMock.mock.calls.some(([url]) => String(url).includes("transactionTypes="))).toBe(true));
    expect(screen.getByRole("button", { name: /^type filters$/i })).toHaveTextContent("0/2");

    fireEvent.click(screen.getByRole("button", { name: "All" }));
    await waitFor(() => expect(fetchMock.mock.calls.some(([url]) => String(url).includes("transactionTypes=marketBuy%2CmarketSell"))).toBe(true));
    expect(screen.getByRole("button", { name: /^type filters$/i })).toHaveTextContent("2/2");
  });

  test("renders server summaries and real transaction rows", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response(JSON.stringify(page), {
      status: 200,
      headers: { "content-type": "application/json" },
    }));

    render(<MemoryRouter><TransactionsPage /></MemoryRouter>);

    expect(await screen.findByText("Rifter")).toBeInTheDocument();
    expect(screen.getAllByText("Aura Valex")).toHaveLength(2);
    expect(screen.getByText("25,240,000,000 ISK")).toBeInTheDocument();
    expect(screen.getAllByText("3,500,000 ISK")).toHaveLength(2);
    expect(screen.getByText("The Forge")).toBeInTheDocument();
    expect(screen.getByText("Caldari Navy")).toBeInTheDocument();
    expect(screen.getByText("Jita IV - Moon 4 - Caldari Navy Assembly Plant")).toBeInTheDocument();
  });

  test("shows dashes instead of raw IDs for unresolved client and location names", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
      if (String(input).includes("saved-filters")) return new Response("[]", { status: 200 });
      return new Response(JSON.stringify({
        ...page,
        rows: [{ ...page.rows[0], counterpartyName: null, locationName: null }],
      }), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    });

    const { container } = render(<MemoryRouter><TransactionsPage /></MemoryRouter>);
    expect(await screen.findByText("Rifter")).toBeInTheDocument();
    expect(container.querySelector('td[data-finance-column="counterparty"]')).toHaveTextContent("—");
    expect(container.querySelector('td[data-finance-column="location"]')).toHaveTextContent("—");
  });

  test("uses the compact column chooser and updates visible table columns", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
      if (String(input).includes("saved-filters")) return new Response("[]", { status: 200 });
      return new Response(JSON.stringify(page), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    });

    render(<MemoryRouter><TransactionsPage /></MemoryRouter>);
    expect(await screen.findByText("Rifter")).toBeInTheDocument();

    const trigger = screen.getByRole("button", { name: "Columns" });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(trigger);

    await waitFor(() => expect(trigger).toHaveAttribute("aria-expanded", "true"));
    expect(screen.getByText("Visible columns")).toBeInTheDocument();
    expect(screen.getByTestId("finance-column-options")).toHaveClass("grid-cols-2");
    expect(screen.getByTestId("finance-column-options").querySelectorAll('input[type="checkbox"]')).toHaveLength(12);

    fireEvent.click(screen.getByLabelText("Region"));
    expect(screen.queryByRole("columnheader", { name: /Region/ })).not.toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: /Client/ })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: /Where/ })).toBeInTheDocument();
  });

  test("debounces search and sends it to the server", async () => {
    vi.useFakeTimers();
    const fetchMock = vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response(JSON.stringify(page), {
      status: 200,
      headers: { "content-type": "application/json" },
    }));

    render(<MemoryRouter><TransactionsPage /></MemoryRouter>);
    await vi.runAllTimersAsync();
    const transactionCallsBeforeSearch = fetchMock.mock.calls.filter(([url]) => String(url).includes("/api/finance/transactions?")).length;
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "Rifter" } });
    expect(fetchMock.mock.calls.filter(([url]) => String(url).includes("/api/finance/transactions?")).length).toBe(transactionCallsBeforeSearch);
    await act(async () => { await vi.advanceTimersByTimeAsync(300); });
    expect(fetchMock.mock.calls.some(([url]) => String(url).includes("search=Rifter"))).toBe(true);
    vi.useRealTimers();
  });

  test("loads the next page when the table sentinel approaches view", async () => {
    let observerCallback: IntersectionObserverCallback | undefined;
    class ObserverStub {
      constructor(callback: IntersectionObserverCallback) { observerCallback = callback; }
      disconnect = vi.fn();
      observe = vi.fn();
      takeRecords = vi.fn(() => []);
      unobserve = vi.fn();
      root = null;
      rootMargin = "0px";
      thresholds = [0];
    }
    vi.stubGlobal("IntersectionObserver", ObserverStub);
    const firstPage = { ...page, rows: page.rows.slice(0, 1), totalCount: 2, pageSize: 1 };
    const secondPage = {
      ...firstPage,
      page: 2,
      rows: [{ ...page.rows[0], observationId: "obs-2", transactionId: 43, typeName: "Merlin" }],
    };
    const fetchMock = vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
      const url = String(input);
      if (url.includes("saved-filters")) return new Response("[]", { status: 200 });
      return new Response(JSON.stringify(url.includes("page=2") ? secondPage : firstPage), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    });

    render(<MemoryRouter><TransactionsPage /></MemoryRouter>);
    expect(await screen.findByText("Rifter")).toBeInTheDocument();

    act(() => observerCallback?.([{ isIntersecting: true } as IntersectionObserverEntry], {} as IntersectionObserver));

    expect(await screen.findByText("Merlin")).toBeInTheDocument();
    expect(fetchMock.mock.calls.some(([url]) => String(url).includes("page=2"))).toBe(true);
    expect(screen.queryByRole("button", { name: "Next" })).not.toBeInTheDocument();
    expect(screen.getByText("All matching transactions loaded.")).toBeInTheDocument();
  });
});
