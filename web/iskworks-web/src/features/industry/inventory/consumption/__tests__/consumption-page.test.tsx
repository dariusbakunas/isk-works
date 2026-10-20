import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type { InventoryConsumption } from "../../../../../api/inventory-analytics";
import { ConsumptionPage } from "../consumption-page";

const NOW = new Date("2026-10-20T15:00:00Z");

function sampleConsumption(overrides: Partial<InventoryConsumption> = {}): InventoryConsumption {
  return {
    from: "2026-09-09",
    to: "2026-10-20",
    granularity: "week",
    earliestObservedAt: "2026-09-09",
    kpis: {
      consumedValue: "2298000000.0000",
      materialsUsed: 2,
      busiestBucket: { start: "2026-10-12", value: "1409000000.0000" },
      topCategory: "Reaction Intermediates",
    },
    buckets: [
      { start: "2026-10-12", partial: false, total: "1409000000.0000", byCategory: [{ category: "Reaction Intermediates", value: "1409000000.0000" }] },
      { start: "2026-10-19", partial: true, total: "340000000.0000", byCategory: [{ category: "Planetary", value: "340000000.0000" }] },
    ],
    categories: [
      { category: "Reaction Intermediates", value: "1700000000.0000", sharePercent: 74 },
      { category: "Planetary", value: "598000000.0000", sharePercent: 26 },
    ],
    materials: [
      {
        typeId: 33362,
        name: "Photonic Metamaterials",
        category: "Reaction Intermediates",
        quantity: 4597,
        value: "512000000.0000",
        sharePercent: 22.3,
        weeklyQuantity: [0, 320, 20, 580, 110, 1640, 1800, 127],
        onHand: 512,
        averagePerWeek: 766,
        daysOfCover: 3.3,
      },
      {
        typeId: 9848,
        name: "Robotics",
        category: "Planetary",
        quantity: 2844,
        value: "224000000.0000",
        sharePercent: 9.7,
        weeklyQuantity: [0, 0, 0, 0, 0, 980, 1200, 154],
        onHand: 0,
        averagePerWeek: null,
        daysOfCover: null,
      },
    ],
    ...overrides,
  };
}

function stubFetch(payload: InventoryConsumption = sampleConsumption()) {
  return vi.spyOn(globalThis, "fetch").mockImplementation(async () =>
    new Response(JSON.stringify(payload), { status: 200, headers: { "content-type": "application/json" } }),
  );
}

function consumptionCalls(fetchMock: ReturnType<typeof stubFetch>) {
  return fetchMock.mock.calls
    .map(([url]) => String(url))
    .filter((url) => url.includes("/api/inventory/consumption"))
    .map((url) => new URL(url, "http://x").searchParams);
}

function LocationProbe() {
  const location = useLocation();
  return <div data-testid="location">{location.search}</div>;
}

function renderPage(path = "/inventory/consumption") {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route element={<><ConsumptionPage /><LocationProbe /></>} path="/inventory/consumption" />
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

describe("ConsumptionPage", () => {
  test("loads everything since the first consumption by week, then shows KPIs, chart and materials", async () => {
    const fetchMock = stubFetch();
    renderPage();

    expect(screen.getByRole("status", { name: "Loading consumption" })).toBeInTheDocument();
    const kpis = await screen.findAllByTestId("inventory-kpi");
    expect(kpis[0]).toHaveTextContent("2.3B ISK");
    expect(kpis[2]).toHaveTextContent("1.41B ISK");
    expect(kpis[2]).toHaveTextContent("Oct 12 – Oct 18");
    expect(kpis[3]).toHaveTextContent("Reaction Intermediates");

    const query = consumptionCalls(fetchMock)[0];
    expect(query.get("from")).toBeNull();
    expect(query.get("to")).toBe("2026-10-20");
    expect(query.get("granularity")).toBe("week");

    expect(screen.getByRole("img", { name: /Consumption by week: 2 periods/ })).toBeInTheDocument();
    const table = screen.getByRole("table", { name: "Consumed materials" });
    expect(within(table).getByRole("link", { name: "Photonic Metamaterials" })).toHaveAttribute("href", "/inventory?item=33362");
    expect(within(table).getByText("3.3d")).toHaveClass("text-danger");
    expect(within(table).getByText("766/wk")).toBeInTheDocument();
  });

  test("calls out short history and hides burn figures without enough of it", async () => {
    stubFetch();
    renderPage();
    expect(await screen.findByText("Consumption history starts 9 Sept 2026 (5 weeks)")).toBeInTheDocument();
    const robotics = screen.getByRole("link", { name: "Robotics" }).closest("tr")!;
    expect(within(robotics).getAllByTitle("Needs 4 weeks of history")).toHaveLength(2);
  });

  test("range and granularity refetch and are kept in the URL", async () => {
    const fetchMock = stubFetch();
    renderPage();
    await screen.findAllByTestId("inventory-kpi");

    fireEvent.click(within(screen.getByRole("group", { name: "Range" })).getByRole("button", { name: "30d" }));
    await waitFor(() => expect(consumptionCalls(fetchMock)).toHaveLength(2));
    expect(consumptionCalls(fetchMock)[1].get("from")).toBe("2026-09-21");

    fireEvent.click(within(screen.getByRole("group", { name: "Granularity" })).getByRole("button", { name: "Day" }));
    await waitFor(() => expect(consumptionCalls(fetchMock)).toHaveLength(3));
    expect(consumptionCalls(fetchMock)[2].get("granularity")).toBe("day");
    expect(screen.getByTestId("location")).toHaveTextContent("?range=30d&granularity=day");
  });

  test("a category in the donut legend filters the tab, and the chip clears it", async () => {
    const fetchMock = stubFetch();
    renderPage();
    const legend = await screen.findByRole("list", { name: "Consumption by category" });

    fireEvent.click(within(legend).getByRole("button", { name: /Planetary/ }));
    await waitFor(() => expect(consumptionCalls(fetchMock)).toHaveLength(2));
    expect(consumptionCalls(fetchMock)[1].get("category")).toBe("Planetary");
    expect(within(legend).getByRole("button", { name: /Planetary/ })).toHaveAttribute("aria-pressed", "true");

    fireEvent.click(screen.getByRole("button", { name: "Clear Planetary filter" }));
    expect(screen.getByTestId("location")).toHaveTextContent("");
  });

  test("with no consumption recorded it explains where history comes from", async () => {
    stubFetch(sampleConsumption({ earliestObservedAt: null, materials: [], categories: [], buckets: [] }));
    renderPage();
    expect(await screen.findByText("No consumption recorded yet")).toBeInTheDocument();
  });

  test("an empty period suggests widening the range", async () => {
    stubFetch(sampleConsumption({ materials: [] }));
    renderPage("/inventory/consumption?range=7d");
    expect(await screen.findByText("Nothing consumed in this period")).toBeInTheDocument();
  });

  test("shows the API error with a retry", async () => {
    const fetchMock = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response(JSON.stringify({ error: { code: "persistence_unavailable", message: "Inventory consumption persistence is unavailable." } }), { status: 503 }),
    );
    renderPage();
    expect(await screen.findByText("Inventory consumption persistence is unavailable.")).toBeInTheDocument();

    fetchMock.mockResolvedValueOnce(new Response(JSON.stringify(sampleConsumption()), { status: 200 }));
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findAllByTestId("inventory-kpi")).toHaveLength(4);
  });
});
