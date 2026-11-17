import { render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { location, NOW, outbidIshtar, overview, row } from "../../market-orders/__tests__/fixtures";
import { MarketPanel, outbidRows } from "../market-panel";
import { OverviewDataProvider } from "../overview-data";
import { fetchByUrl } from "./overview-fixtures";

const minutesAgo = (count: number) => new Date(NOW.getTime() - count * 60_000).toISOString();

function outbid(orderId: number, typeName: string, since: string | null) {
  const base = outbidIshtar();
  return { ...base, orderId, typeName, standing: { ...base.standing, outbidSince: since } };
}

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
        <MarketPanel />
      </OverviewDataProvider>
    </MemoryRouter>,
  );
}

describe("outbidRows", () => {
  test("flattens every location's outbid orders, longest outbid first, capped", () => {
    const market = overview({
      locations: [
        location({ orders: [outbid(1, "Ishtar", minutesAgo(5)), row({ orderId: 2 })] }),
        location({ locationId: 2, orders: [outbid(3, "PLEX", minutesAgo(40)), outbid(4, "Megacyte", null), outbid(5, "Tritanium", minutesAgo(20))] }),
      ],
    });

    expect(outbidRows(market, 3).map((order) => order.typeName)).toEqual(["PLEX", "Tritanium", "Ishtar"]);
    expect(outbidRows(market, 10).map((order) => order.typeName)).toEqual(["PLEX", "Tritanium", "Ishtar", "Megacyte"]);
  });
});

describe("MarketPanel", () => {
  test("sell value, escrow, the standing bar and the outbid orders", async () => {
    fetchByUrl({ market: overview() });
    renderPanel();

    const panel = await screen.findByRole("region", { name: "Market" });
    const totals = await within(panel).findByRole("group", { name: "Market totals" });
    expect(within(totals).getByText("7.6B")).toHaveAttribute("title", expect.stringContaining("7,639,600,000"));
    expect(within(totals).getByText("1.4B")).toBeInTheDocument();
    expect(within(panel).getByRole("img", { name: "Order standing: 2 best, 0 tied, 1 outbid, 1 unknown, 0 likely filled" })).toBeInTheDocument();

    const ishtar = within(panel).getByText("Ishtar").closest("a")!;
    expect(ishtar).toHaveAttribute("href", "/market-orders?outbid=1");
    expect(within(ishtar).getByText("Sell")).toBeInTheDocument();
    expect(within(ishtar).getByText("Kael Varon")).toBeInTheDocument();
    expect(within(ishtar).getByText("0.16% · 12 min ago")).toBeInTheDocument();
    expect(within(panel).getByText("Books 3 min old")).toBeInTheDocument();
  });

  test("no outbid orders says so", async () => {
    fetchByUrl({
      market: overview({ locations: [location({ orders: [row()] })], summary: { ...overview().summary, outbid: 0 } }),
    });
    renderPanel();

    expect(await screen.findByText("No orders outbid.")).toBeInTheDocument();
  });

  test("no orders at all is the empty state", async () => {
    fetchByUrl({
      market: overview({
        locations: [],
        summary: { outbid: 0, tied: 0, best: 0, unknown: 0, likelyFilled: 0, activeSellValue: "0", buyEscrow: "0", oldestBookAt: null },
      }),
    });
    renderPanel();

    expect(await screen.findByText("No active market orders.")).toBeInTheDocument();
  });
});
