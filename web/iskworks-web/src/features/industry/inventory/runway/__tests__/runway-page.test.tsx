import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, describe, expect, test, vi } from "vitest";

import type { InventoryRunway, RunwayMaterial } from "../../../../../api/inventory-analytics";
import { RunwayPage } from "../runway-page";

const REVELATION = "11111111-1111-1111-1111-111111111111";
const ISHTAR = "22222222-2222-2222-2222-222222222222";

function material(overrides: Partial<RunwayMaterial>): RunwayMaterial {
  return {
    typeId: 11399,
    name: "Morphite",
    category: "Minerals",
    needed: 13767,
    onHand: 0,
    reserved: 0,
    short: 13767,
    unitCost: "16690.0000",
    shortValue: "229771230.0000",
    coveragePercent: 0,
    epics: [{ orderId: REVELATION, required: 13767, consumed: 0, reserved: 0, outstanding: 13767, short: 13767 }],
    ...overrides,
  };
}

function sampleRunway(overrides: Partial<InventoryRunway> = {}): InventoryRunway {
  return {
    kpis: {
      shortfallValue: "229771230.0000",
      materialsShort: 1,
      materialsTotal: 2,
      epicsBlocked: 1,
      epicsTotal: 2,
      demandCoveredPercent: 38.4,
    },
    epics: [
      {
        orderId: REVELATION,
        displayName: "Revelation x1",
        createdAt: "2026-10-05T00:00:00Z",
        coveredPercent: 12,
        shortValue: "229771230.0000",
        blocked: true,
      },
      {
        orderId: ISHTAR,
        displayName: "Ishtar x2",
        createdAt: "2026-10-14T00:00:00Z",
        coveredPercent: 100,
        shortValue: "0.0000",
        blocked: false,
      },
    ],
    materials: [
      material({}),
      material({
        typeId: 34,
        name: "Tritanium",
        needed: 2100,
        onHand: 6800,
        short: 0,
        shortValue: "0.0000",
        coveragePercent: 100,
        epics: [{ orderId: ISHTAR, required: 3000, consumed: 900, reserved: 0, outstanding: 2100, short: 0 }],
      }),
    ],
    missingPriceCount: 0,
    ...overrides,
  };
}

function stubFetch(payload: InventoryRunway = sampleRunway()) {
  return vi.spyOn(globalThis, "fetch").mockImplementation(async () =>
    new Response(JSON.stringify(payload), { status: 200, headers: { "content-type": "application/json" } }),
  );
}

function runwayCalls(fetchMock: ReturnType<typeof stubFetch>) {
  return fetchMock.mock.calls
    .map(([url]) => String(url))
    .filter((url) => url.includes("/api/inventory/runway"));
}

function LocationProbe() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}{location.search}</div>;
}

function renderPage(path = "/inventory/runway") {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route element={<><RunwayPage /><LocationProbe /></>} path="/inventory/runway" />
        <Route element={<LocationProbe />} path="*" />
      </Routes>
    </MemoryRouter>,
  );
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("RunwayPage", () => {
  test("shows a skeleton, then KPIs, Epic readiness and the grouped table", async () => {
    stubFetch();
    renderPage();

    expect(screen.getByRole("status", { name: "Loading runway" })).toBeInTheDocument();
    const kpis = await screen.findAllByTestId("runway-kpi");
    expect(kpis).toHaveLength(4);
    expect(kpis[0]).toHaveTextContent("229.8M ISK");
    expect(kpis[1]).toHaveTextContent("1 of 2");
    expect(kpis[3]).toHaveTextContent("38%");

    const readiness = screen.getByRole("region", { name: "Epic readiness" });
    expect(within(readiness).getByText("Revelation x1")).toBeInTheDocument();
    expect(within(readiness).getByText("−229.8M short")).toBeInTheDocument();
    expect(within(readiness).getByText("inputs covered")).toBeInTheDocument();

    const table = screen.getByRole("table", { name: "Runway materials" });
    expect(within(table).getByText("Minerals")).toBeInTheDocument();
    const morphite = within(table).getByRole("link", { name: "Morphite" });
    expect(morphite).toHaveAttribute("href", "/inventory?item=11399");
    expect(within(table).getByTitle("229,771,230 ISK")).toHaveTextContent("−229.8M");
  });

  test("the Stock and Runway tabs link to each other", async () => {
    stubFetch();
    renderPage();
    const tabs = screen.getByRole("navigation", { name: "Inventory views" });
    expect(within(tabs).getByRole("link", { name: "Stock" })).toHaveAttribute("href", "/inventory");
    expect(within(tabs).getByRole("link", { name: "Runway" })).toHaveAttribute("aria-current", "page");
    await screen.findAllByTestId("runway-kpi");
  });

  test("expanding a material shows its per-Epic breakdown", async () => {
    stubFetch();
    renderPage();
    const table = await screen.findByRole("table", { name: "Runway materials" });

    expect(within(table).queryByText(/required 3,000/)).not.toBeInTheDocument();
    fireEvent.click(within(table).getByText("2,100").closest("tr")!);
    expect(within(table).getByText("required 3,000 · consumed 900")).toBeInTheDocument();
  });

  test("an Epic card filters the table and is kept in the URL", async () => {
    stubFetch();
    renderPage();
    const readiness = await screen.findByRole("region", { name: "Epic readiness" });

    fireEvent.click(within(readiness).getByRole("button", { name: "Ishtar x2" }));

    expect(screen.getByTestId("location")).toHaveTextContent(`?epic=${ISHTAR}`);
    const table = screen.getByRole("table", { name: "Runway materials" });
    expect(within(table).queryByRole("link", { name: "Morphite" })).not.toBeInTheDocument();
    expect(within(table).getByRole("link", { name: "Tritanium" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Show all" }));
    expect(within(table).getByRole("link", { name: "Morphite" })).toBeInTheDocument();
  });

  test("the category filter refetches with ?category=", async () => {
    const fetchMock = stubFetch();
    renderPage();
    await screen.findAllByTestId("runway-kpi");

    fireEvent.change(screen.getByLabelText("Category"), { target: { value: "Moon Materials" } });

    await waitFor(() => expect(runwayCalls(fetchMock)).toHaveLength(2));
    expect(runwayCalls(fetchMock)[1]).toContain("category=Moon+Materials");
    expect(screen.getByTestId("location")).toHaveTextContent("?category=Moon+Materials");
  });

  test("copies the shortfall as EVE multibuy text", async () => {
    stubFetch();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    renderPage();
    await screen.findAllByTestId("runway-kpi");

    fireEvent.click(screen.getByRole("button", { name: "Shopping list" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Copy as EVE multibuy" }));

    await waitFor(() => expect(writeText).toHaveBeenCalledWith("Morphite\t13767"));
    expect(await screen.findByText(/Paste it into EVE's Multibuy window/)).toBeInTheDocument();
  });

  test("with no open Epics it points to Builds", async () => {
    stubFetch(sampleRunway({ epics: [], materials: [] }));
    renderPage();

    expect(await screen.findByText("No committed Epics")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open Builds" })).toHaveAttribute("href", "/builds");
  });

  test("says so when everything is covered, and warns about unpriced shortfalls", async () => {
    stubFetch(sampleRunway({
      kpis: { shortfallValue: "0.0000", materialsShort: 0, materialsTotal: 2, epicsBlocked: 0, epicsTotal: 2, demandCoveredPercent: 100 },
    }));
    renderPage();
    expect(await screen.findByText("All 2 inputs covered by stock")).toBeInTheDocument();
    expect(screen.queryByText("Some shortfalls have no price")).not.toBeInTheDocument();
  });

  test("group headers total the short value exactly, and flag an unpriced shortfall", async () => {
    stubFetch(sampleRunway({
      materials: [
        material({}),
        material({ typeId: 11400, name: "Isogen", shortValue: "0.1000", short: 1 }),
        material({ typeId: 16274, name: "Helium Isotopes", category: "Ice Products", unitCost: null, shortValue: null }),
      ],
    }));
    renderPage();
    const table = await screen.findByRole("table", { name: "Runway materials" });
    const headers = within(table).getAllByRole("rowheader").map((header) => header.textContent);
    expect(headers).toEqual([
      expect.stringContaining("2 short · −229.8M"),
      expect.stringContaining("1 short · no price"),
    ]);
  });

  test("warns when shortfalls have no price", async () => {
    stubFetch(sampleRunway({ missingPriceCount: 3 }));
    renderPage();
    expect(await screen.findByText("Some shortfalls have no price")).toBeInTheDocument();
  });

  test("shows the API error with a retry", async () => {
    const fetchMock = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response(JSON.stringify({ error: { code: "persistence_unavailable", message: "Inventory runway persistence is unavailable." } }), { status: 503 }),
    );
    renderPage();
    expect(await screen.findByText("Inventory runway persistence is unavailable.")).toBeInTheDocument();

    fetchMock.mockResolvedValueOnce(new Response(JSON.stringify(sampleRunway()), { status: 200 }));
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findAllByTestId("runway-kpi")).toHaveLength(4);
  });
});
