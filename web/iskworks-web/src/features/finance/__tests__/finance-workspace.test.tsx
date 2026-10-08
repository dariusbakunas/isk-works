import { fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterEach, describe, expect, test, vi } from "vitest";

import { FinanceWorkspace } from "../finance-workspace";

import { emptyAnalytics } from "../analytics/__tests__/fixtures";

const analyticsPayload = emptyAnalytics();

const transactionPage = {
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
  vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
    if (String(input).includes("saved-filters")) return new Response("[]", { status: 200 });
    if (String(input).includes("/api/finance/analytics")) {
      return new Response(JSON.stringify(analyticsPayload), { status: 200, headers: { "content-type": "application/json" } });
    }
    return new Response(JSON.stringify(transactionPage), {
      status: 200,
      headers: { "content-type": "application/json" },
    });
  });
}

function renderFinance(path: string) {
  render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route path="/finance/*" element={<FinanceWorkspace />} />
      </Routes>
    </MemoryRouter>,
  );
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("FinanceWorkspace", () => {
  test("the finance index lands on Transactions with no placeholder tab bar", async () => {
    stubFetch();

    renderFinance("/finance");

    expect(screen.getByRole("heading", { name: "Finance" })).toBeInTheDocument();
    expect(
      await screen.findByRole("searchbox", { name: "Search transactions" }),
    ).toBeInTheDocument();
    expect(screen.queryByText(/is not available yet\./i)).not.toBeInTheDocument();
    // Only the two real views are tabs; no placeholder tabs.
    expect(screen.getByRole("link", { name: "Transactions" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Analytics" })).toBeInTheDocument();
    for (const removed of ["Overview", "Journal", "Assets"]) {
      expect(screen.queryByRole("link", { name: removed })).not.toBeInTheDocument();
    }
  });

  test("the Analytics tab loads the analytics view on demand", async () => {
    stubFetch();

    renderFinance("/finance/analytics");

    expect(await screen.findByRole("heading", { name: "Analytics" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Analytics" })).toHaveAttribute("aria-current", "page");
    // Tab links resolve from /finance, not from the current tab's subpath.
    expect(screen.getByRole("link", { name: "Transactions" })).toHaveAttribute("href", "/finance/transactions");
    expect(screen.getByRole("link", { name: "Analytics" })).toHaveAttribute("href", "/finance/analytics");
  });

  test("switching tabs moves between Transactions and Analytics", async () => {
    stubFetch();
    renderFinance("/finance/transactions");
    await screen.findByRole("searchbox", { name: "Search transactions" });

    fireEvent.click(screen.getByRole("link", { name: "Analytics" }));

    expect(await screen.findByRole("heading", { name: "Analytics" })).toBeInTheDocument();
    expect(screen.queryByRole("searchbox", { name: "Search transactions" })).not.toBeInTheDocument();
  });

  test("an old placeholder subpath redirects to Transactions instead of a dead pane", async () => {
    stubFetch();

    renderFinance("/finance/journal");

    expect(
      await screen.findByRole("searchbox", { name: "Search transactions" }),
    ).toBeInTheDocument();
    expect(screen.queryByText("Journal is not available yet.")).not.toBeInTheDocument();
  });
});
