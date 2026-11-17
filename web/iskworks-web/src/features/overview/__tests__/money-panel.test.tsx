import { render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { sampleAnalytics } from "../../finance/analytics/__tests__/fixtures";
import { deltaLabel, MoneyPanel, totalWallet } from "../money-panel";
import { OverviewDataProvider } from "../overview-data";
import { fetchByUrl, hoursFromNow, NOW, rosterEntry } from "./overview-fixtures";

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(NOW);
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

function renderPanel() {
  return render(
    <MemoryRouter>
      <OverviewDataProvider>
        <MoneyPanel />
      </OverviewDataProvider>
    </MemoryRouter>,
  );
}

describe("totalWallet", () => {
  test("adds wallets exactly, skipping unknown ones", () => {
    expect(
      totalWallet([
        rosterEntry({ walletBalance: "9007199254740993.1234" }),
        rosterEntry({ walletBalance: "0.0001" }),
        rosterEntry({ walletBalance: null }),
      ]),
    ).toBe("9007199254740993.1235");
    expect(totalWallet([rosterEntry({ walletBalance: null })])).toBeNull();
  });
});

describe("deltaLabel", () => {
  test("up, down, new, or nothing to compare with", () => {
    expect(deltaLabel({ previous: "1", percent: 18.4, isNew: false })).toEqual({ text: "▲ 18% vs prev 30d", tone: "positive" });
    expect(deltaLabel({ previous: "1", percent: -7, isNew: false })).toEqual({ text: "▼ 7% vs prev 30d", tone: "danger" });
    expect(deltaLabel({ previous: "0", percent: null, isNew: true })).toEqual({ text: "new vs prev 30d", tone: "positive" });
    expect(deltaLabel({ previous: "0", percent: null, isNew: false })).toBeNull();
    expect(deltaLabel(null)).toBeNull();
  });
});

describe("MoneyPanel", () => {
  test("total wallet, 30-day net with its change, net per day and PI income", async () => {
    fetchByUrl({
      characters: [
        rosterEntry({ walletBalance: "3000000000.0000", lastSyncedAt: hoursFromNow(-0.1) }),
        rosterEntry({ walletBalance: "1500000000.5000", lastSyncedAt: hoursFromNow(-0.5) }),
      ],
    });
    renderPanel();

    const panel = await screen.findByRole("region", { name: "Money" });
    expect(within(panel).getByText("4.5B")).toHaveAttribute("title", expect.stringContaining("4,500,000,000.5"));
    expect(within(panel).getByText("wallets 30 min ago")).toBeInTheDocument();
    expect(await within(panel).findByText("1.2B")).toBeInTheDocument();
    expect(within(panel).getByText("▲ 700% vs prev 30d")).toBeInTheDocument();
    expect(within(panel).getByRole("img", { name: /Net per day/ })).toBeInTheDocument();
    expect(within(panel).getByText("412M")).toBeInTheDocument();
    expect(within(panel).getByRole("link", { name: "View all Money" })).toHaveAttribute("href", "/finance/analytics");
  });

  test("a finance failure leaves the wallet total and shows a retry where the net was", async () => {
    fetchByUrl({ finance: new Error("Finance analytics request failed.") });
    renderPanel();

    const panel = await screen.findByRole("region", { name: "Money" });
    expect(within(panel).getByText("4.8B")).toBeInTheDocument();
    expect(await within(panel).findByRole("button", { name: "Retry" })).toBeInTheDocument();
  });

  test("a net loss reads as negative", async () => {
    const analytics = sampleAnalytics();
    analytics.kpis.net = { value: "-250000000.0000", delta: null, sparkline: [] };
    fetchByUrl({ finance: analytics });
    renderPanel();

    const panel = await screen.findByRole("region", { name: "Money" });
    expect(await within(panel).findByText("-250M")).toHaveClass("text-danger");
  });
});
