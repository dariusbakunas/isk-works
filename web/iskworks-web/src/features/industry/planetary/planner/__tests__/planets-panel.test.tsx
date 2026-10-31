import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState, type ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type { PlannerPlanet, PlannerPlanets } from "../../../../../api/pi-planner";
import { PlanetsPanel, SAVE_DEBOUNCE_MS, type PlanetFilter } from "../planets-panel";

const api = vi.hoisted(() => ({ saveDensities: vi.fn(), savePlanetSettings: vi.fn() }));
vi.mock("../../../../../api/pi-planner", async () => ({
  ...(await vi.importActual("../../../../../api/pi-planner")),
  saveDensities: api.saveDensities,
  savePlanetSettings: api.savePlanetSettings,
}));

const BARREN = ["Aqueous Liquids", "Base Metals", "Carbon Compounds", "Microorganisms", "Noble Metals"];

function planet(planetId: number, name: string, densities: Array<string | null> = []): PlannerPlanet {
  return {
    planetId,
    name,
    solarSystemId: 30_000_797,
    jumps: 0,
    typeId: 2016,
    typeName: "Planet (Barren)",
    customsRate: null,
    contested: false,
    settingsUpdatedAt: null,
    resources: BARREN.map((resourceName, index) => ({
      typeId: 2000 + index,
      name: resourceName,
      densityPct: densities[index] ?? null,
      headValue: null,
      updatedAt: densities[index] ? "2026-10-31T00:00:00Z" : null,
      defaultDensityPct: "25",
    })),
  };
}

const DATA: PlannerPlanets = {
  systems: [
    {
      solarSystemId: 30_000_797,
      name: "Q-3HS5",
      securityStatus: "-0.58",
      regionName: "Insmother",
      band: "null",
      jumps: 0,
    },
  ],
  planets: [planet(1, "Q-3HS5 I", ["40"]), planet(2, "Q-3HS5 II")],
};

function Harness({ initial = "all" as PlanetFilter, onSaved = () => {} }) {
  return (
    <FilterState initial={initial}>
      {(filter, setFilter) => (
        <PlanetsPanel
          data={DATA}
          defaultCustomsRate="0.1"
          filter={filter}
          nowMs={Date.parse("2026-11-01T00:00:00Z")}
          onFilterChange={setFilter}
          onSaved={onSaved}
        />
      )}
    </FilterState>
  );
}

function FilterState({
  children,
  initial,
}: {
  children: (filter: PlanetFilter, setFilter: (filter: PlanetFilter) => void) => ReactNode;
  initial: PlanetFilter;
}) {
  const [filter, setFilter] = useState(initial);
  return <>{children(filter, setFilter)}</>;
}

beforeEach(() => {
  api.saveDensities.mockReset().mockResolvedValue(undefined);
  api.savePlanetSettings.mockReset().mockResolvedValue(undefined);
});
afterEach(() => vi.useRealTimers());

describe("PlanetsPanel", () => {
  test("Enter walks every resource and moves on to the next planet", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const first = screen.getAllByLabelText("Aqueous Liquids density percent")[0];
    await user.click(first);
    for (let step = 0; step < 5; step += 1) await user.keyboard("{Enter}");
    expect(document.activeElement).toBe(screen.getAllByLabelText("Aqueous Liquids density percent")[1]);
  });

  test("a row's readings are saved together after the debounce", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    const onSaved = vi.fn();
    render(<Harness onSaved={onSaved} />);

    const row = screen.getAllByLabelText("Base Metals density percent")[1];
    await user.click(row);
    await user.keyboard("58{Enter}");
    await user.keyboard("{Enter}{Enter}47{Enter}");
    expect(api.saveDensities).not.toHaveBeenCalled();

    await act(async () => {
      vi.advanceTimersByTime(SAVE_DEBOUNCE_MS + 10);
    });
    expect(api.saveDensities).toHaveBeenCalledTimes(1);
    expect(api.saveDensities).toHaveBeenCalledWith([
      { planetId: 2, resourceTypeId: 2001, densityPct: "58", headValue: null },
      { planetId: 2, resourceTypeId: 2004, densityPct: "47", headValue: null },
    ]);
    expect(onSaved).toHaveBeenCalled();
    expect(screen.getAllByLabelText("Saved").length).toBe(2);
  });

  test("filters by scan state and counts scanned planets", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    expect(screen.getByText("1 of 2 planets scanned")).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Not scanned" }));
    expect(screen.queryByText("Q-3HS5 I")).toBeNull();
    expect(screen.getByText("Q-3HS5 II")).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Scanned" }));
    expect(screen.getByText("Q-3HS5 I")).toBeInTheDocument();
    expect(screen.queryByText("Q-3HS5 II")).toBeNull();

    await user.click(screen.getByRole("tab", { name: "All" }));
    fireEvent.change(screen.getByRole("searchbox", { name: "Search planets" }), { target: { value: "II" } });
    expect(screen.queryByText("Q-3HS5 I")).toBeNull();
  });

  test("head values and contested are saved from the row menu", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByRole("button", { name: "Q-3HS5 I actions" }));
    await user.click(screen.getByRole("menuitem", { name: "Head values…" }));
    const dialog = screen.getByRole("dialog");
    await user.type(within(dialog).getByLabelText("Noble Metals head value"), "160");
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    expect(api.saveDensities).toHaveBeenCalledWith([
      { planetId: 1, resourceTypeId: 2004, densityPct: null, headValue: "160" },
    ]);
    expect(screen.getByText(/HV 160/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Q-3HS5 I actions" }));
    await user.click(screen.getByRole("menuitem", { name: /Contested/ }));
    expect(api.savePlanetSettings).toHaveBeenCalledWith([{ planetId: 1, customsRate: null, contested: true }]);
    expect(screen.getByLabelText("Contested: other players extract here")).toBeInTheDocument();
  });
});
