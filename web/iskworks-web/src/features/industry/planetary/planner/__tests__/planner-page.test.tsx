import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { PlannerPlanets, PlannerRanking, RankedProduct } from "../../../../../api/pi-planner";
import { ApiError } from "../../../../../api/workspace";
import { PlannerPage } from "../planner-page";
import { SOLUTION } from "./plan-fixture";

const api = vi.hoisted(() => ({
  getPlannerPlanets: vi.fn(),
  getPlannerCharacters: vi.fn(),
  rankProducts: vi.fn(),
  saveDensities: vi.fn(),
  savePlanetSettings: vi.fn(),
  solvePlan: vi.fn(),
}));
vi.mock("../../../../../api/pi-planner", async () => ({
  ...(await vi.importActual("../../../../../api/pi-planner")),
  ...api,
}));

const HOME = 30_000_797;

function planets(densityPct: string | null): PlannerPlanets {
  return {
    systems: [{ solarSystemId: HOME, name: "Q-3HS5", securityStatus: "-0.58", regionName: "Insmother", band: "null", jumps: 0 }],
    planets: [
      {
        planetId: 1,
        name: "Q-3HS5 I",
        solarSystemId: HOME,
        jumps: 0,
        typeId: 2015,
        typeName: "Planet (Lava)",
        customsRate: null,
        contested: false,
        settingsUpdatedAt: null,
        resources: ["Base Metals", "Felsic Magma", "Heavy Metals", "Non-CS Crystals", "Suspended Plasma"].map(
          (name, index) => ({
            typeId: 2300 + index,
            name,
            densityPct: index === 0 ? densityPct : null,
            headValue: null,
            updatedAt: index === 0 && densityPct ? "2026-10-31T00:00:00Z" : null,
            defaultDensityPct: "25",
          }),
        ),
      },
    ],
  };
}

function product(overrides: Partial<RankedProduct> = {}): RankedProduct {
  return {
    typeId: 9848,
    name: "Robotics",
    tier: 3,
    iskPerMonth: "1420000000",
    unitsPerHour: "6.5",
    colonies: 5,
    chains: 1,
    inputs: [
      { resourceTypeId: 2300, resourceName: "Base Metals", planetId: 1, planetName: "Q-3HS5 I", source: "scanned", contested: false },
      { resourceTypeId: 2302, resourceName: "Heavy Metals", planetId: 1, planetName: "Q-3HS5 I", source: "default", contested: false },
    ],
    bought: [],
    confidence: "0.5",
    effort: { loginsPerWeekByCharacter: ["2.33"], haulRunsPerWeek: "2.33", stopsPerRun: 5, m3PerRun: "7800" },
    haulConstrainedDays: null,
    ...overrides,
  };
}

const RANKING: PlannerRanking = {
  priceObservedAt: "2026-10-31T00:00:00Z",
  availableColonies: 5,
  ranked: [product()],
  outOfReach: [
    { typeId: 2867, name: "Broadcast Node", tier: 4, reason: { kind: "needsColonies", needed: 14, available: 5 } },
    { typeId: 2876, name: "Organic Mortar Applicators", tier: 3, reason: { kind: "missingResource", typeId: 2287, name: "Planktic Colonies" } },
    { typeId: 9834, name: "Guidance Systems", tier: 3, reason: { kind: "noPrice" } },
  ],
};

let location = "";
function Spy() {
  location = useLocation().search;
  return null;
}

function renderPage(path = `/planetary/planner?system=${HOME}`) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route
          element={
            <>
              <PlannerPage />
              <Spy />
            </>
          }
          path="/planetary/planner"
        />
      </Routes>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  window.localStorage.clear();
  Object.values(api).forEach((mock) => mock.mockReset());
  api.getPlannerPlanets.mockResolvedValue(planets("40"));
  api.getPlannerCharacters.mockResolvedValue([]);
  api.rankProducts.mockResolvedValue(RANKING);
  api.saveDensities.mockResolvedValue(undefined);
});

describe("PlannerPage", () => {
  test("without a home system it shows the three steps", () => {
    renderPage("/planetary/planner");
    expect(screen.getByText("Plan planetary production")).toBeInTheDocument();
    expect(screen.getByText("Pick your home system.")).toBeInTheDocument();
    expect(api.getPlannerPlanets).not.toHaveBeenCalled();
  });

  test("ranks the products with effort, inputs and confidence", async () => {
    renderPage();
    const products = await screen.findByRole("table", { name: "Products" });
    expect(within(products).getByText("Robotics")).toBeInTheDocument();
    expect(within(products).getByText("1.4B")).toHaveAttribute("title", expect.stringContaining("1,420,000,000"));
    expect(within(products).getByText(/2\.3 logins/)).toBeInTheDocument();
    expect(within(products).getByText("Heavy Metals")).toHaveAttribute("data-unscanned", "true");
    expect(within(products).getByText("Base Metals")).not.toHaveAttribute("data-unscanned");
    expect(within(products).getByLabelText("Confidence 50 %")).toHaveAttribute("data-filled", "2");
    expect(api.rankProducts).toHaveBeenCalledWith(
      expect.objectContaining({ homeSystemId: HOME, radius: 1, resetDays: 3, haulDays: 3, upliftAtLogin: true }),
    );
    expect(screen.getByText("1 system · 1 planet")).toBeInTheDocument();
  });

  test("out of reach is collapsed and offers each reason's action", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByText("Robotics");
    expect(screen.queryByText("Broadcast Node")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Expand Out of reach" }));
    expect(screen.getByText("Needs 14 colonies — your characters have 5")).toBeInTheDocument();
    expect(screen.getByText("No market price yet")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Try 2 jumps" }));
    expect(location).toContain("radius=2");
    await user.click(screen.getByRole("button", { name: "Add a character" }));
    expect(screen.getAllByTestId("scenario-character")).toHaveLength(2);
  });

  test("scenario changes go to the URL and characters to storage", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByText("Robotics");
    await user.click(within(screen.getByRole("radiogroup", { name: "Haul every" })).getByRole("radio", { name: "7d" }));
    expect(location).toContain("haul=7");
    await user.click(screen.getByLabelText("Uplift to skyhook at each login"));
    expect(location).toContain("uplift=0");

    const raise = screen.getByRole("button", { name: "Raise Command Center Upgrades" });
    await user.click(raise);
    expect(raise).toBeDisabled();
    expect(JSON.parse(window.localStorage.getItem("iskworks.piPlanner.characters") ?? "[]")[0].commandCenterUpgrades).toBe(5);
    await waitFor(() =>
      expect(api.rankProducts).toHaveBeenLastCalledWith(
        expect.objectContaining({ haulDays: 7, upliftAtLogin: false, characters: [{ interplanetaryConsolidation: 4, commandCenterUpgrades: 5 }] }),
      ),
    );
  });

  test("add character picks one connected character at a time", async () => {
    const user = userEvent.setup();
    api.getPlannerCharacters.mockResolvedValue([
      { eveCharacterId: 1, name: "Valka", interplanetaryConsolidation: 5, commandCenterUpgrades: 3 },
      { eveCharacterId: 2, name: "Alt", interplanetaryConsolidation: null, commandCenterUpgrades: null },
      { eveCharacterId: 3, name: "Hauler", interplanetaryConsolidation: 2, commandCenterUpgrades: 2 },
    ]);
    renderPage();
    await screen.findByText("Robotics");
    await user.click(screen.getByRole("button", { name: "Add character" }));
    await user.click(screen.getByRole("menuitem", { name: "Valka · IC 5 · CCU 3" }));
    // Picking a real character replaces the starting placeholder.
    let rows = screen.getAllByTestId("scenario-character");
    expect(rows).toHaveLength(1);
    expect(within(rows[0]).getByText("Valka")).toBeInTheDocument();
    expect(within(rows[0]).getByText("→ 6 planets")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Add character" }));
    expect(screen.queryByRole("menuitem", { name: /Valka/ })).toBeNull();
    await user.click(screen.getByRole("menuitem", { name: "Alt · skills not synced" }));
    rows = screen.getAllByTestId("scenario-character");
    expect(rows).toHaveLength(2);
    expect(within(rows[1]).getByText("→ 1 planet")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Add character" }));
    expect(screen.queryByRole("menuitem", { name: /^All/ })).toBeNull();
    await user.click(screen.getByRole("menuitem", { name: "Custom character" }));
    expect(screen.getAllByTestId("scenario-character")).toHaveLength(3);
  });

  test("add character can add every connected character at once", async () => {
    const user = userEvent.setup();
    api.getPlannerCharacters.mockResolvedValue([
      { eveCharacterId: 1, name: "Valka", interplanetaryConsolidation: 5, commandCenterUpgrades: 3 },
      { eveCharacterId: 2, name: "Alt", interplanetaryConsolidation: 4, commandCenterUpgrades: 4 },
    ]);
    renderPage();
    await screen.findByText("Robotics");
    await user.click(screen.getByRole("button", { name: "Add character" }));
    await user.click(screen.getByRole("menuitem", { name: "All 2 not yet added" }));
    const rows = screen.getAllByTestId("scenario-character");
    expect(rows.map((row) => within(row).getByText(/Valka|Alt/).textContent)).toEqual(["Valka", "Alt"]);
  });

  test("with nothing scanned a banner links to the unscanned planets", async () => {
    const user = userEvent.setup();
    api.getPlannerPlanets.mockResolvedValue(planets(null));
    renderPage();
    await screen.findByText("No planets scanned yet");
    await user.click(screen.getByRole("button", { name: "Show not scanned" }));
    expect(screen.getByRole("tab", { name: "Not scanned" })).toHaveAttribute("aria-selected", "true");

    await user.click(screen.getByLabelText("Heavy Metals density percent"));
    await user.keyboard("50{Enter}");
    expect(screen.queryByText("No planets scanned yet")).toBeNull();
  });

  test("unscanned planets stay out of the plan until estimates are on", async () => {
    const user = userEvent.setup();
    api.getPlannerPlanets.mockResolvedValue(planets(null));
    api.rankProducts.mockResolvedValue({
      ...RANKING,
      ranked: [],
      outOfReach: [
        { typeId: 9848, name: "Robotics", tier: 3, reason: { kind: "notScanned", typeId: 2302, name: "Heavy Metals" } },
      ],
    });
    renderPage();
    await screen.findByText("No planets scanned yet");
    await waitFor(() =>
      expect(api.rankProducts).toHaveBeenLastCalledWith(expect.objectContaining({ estimateUnscanned: false })),
    );
    // Nothing is buildable yet, but that's down to scanning, not the scenario.
    expect(screen.queryByText(/Nothing can be built/)).toBeNull();
    await user.click(screen.getByRole("button", { name: "Expand Out of reach" }));
    expect(screen.getByText("Not scanned: Heavy Metals — no planet within 1 jump has a reading")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Estimate unscanned planets" }));
    expect(location).toContain("estimate=1");
    expect(screen.getByLabelText("Estimate unscanned planets")).toBeChecked();
    await waitFor(() =>
      expect(api.rankProducts).toHaveBeenLastCalledWith(expect.objectContaining({ estimateUnscanned: true })),
    );
  });

  test("nothing feasible offers a wider radius and buying", async () => {
    const user = userEvent.setup();
    api.rankProducts.mockResolvedValue({ ...RANKING, ranked: [] });
    renderPage();
    expect(await screen.findByText("Nothing can be built within 1 jump with these characters.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Allow buying intermediates" }));
    expect(location).toContain("buy=1");
  });

  test("an SDE without planner data asks for a re-import", async () => {
    api.getPlannerPlanets.mockRejectedValue(
      new ApiError(409, { code: "sde_missing_planner_data", message: "Re-import the SDE." }),
    );
    renderPage();
    expect(await screen.findByText("Planet data needs an SDE re-import")).toBeInTheDocument();
    expect(screen.getByText("make sde-import SDE_PATH=…")).toBeInTheDocument();
  });
});

describe("PlannerPage plan tab", () => {
  const solution = {
    ...SOLUTION,
    plan: {
      ...SOLUTION.plan,
      colonies: SOLUTION.plan.colonies.map((colony) => ({ ...colony, planetId: 1, planetName: "Q-3HS5 I" })),
      sharedHotspots: [],
    },
    whatIfs: {
      ...SOLUTION.whatIfs,
      scanFirst: [
        {
          planetId: 1,
          planetName: "Q-3HS5 I",
          resourceTypeId: 2302,
          resourceName: "Heavy Metals",
          source: "default" as const,
          iskPerMonth: "90000000",
        },
      ],
    },
  };

  test("solves on opening and scan-first rings the reading", async () => {
    const user = userEvent.setup();
    api.solvePlan.mockResolvedValue(solution);
    renderPage();
    await screen.findByText("Robotics");
    expect(api.solvePlan).not.toHaveBeenCalled();

    await user.click(within(screen.getByRole("tablist", { name: "Results" })).getByRole("tab", { name: "Plan" }));
    expect(await screen.findByRole("table", { name: "Colonies" })).toBeInTheDocument();
    expect(api.solvePlan).toHaveBeenCalledWith(expect.objectContaining({ homeSystemId: HOME }));

    await user.click(screen.getByRole("button", { name: /Q-3HS5 I · Heavy Metals/ }));
    const cell = screen.getByLabelText("Heavy Metals density percent").closest("[data-testid='density-cell']");
    expect(cell).toHaveAttribute("data-highlighted", "true");
    expect(document.activeElement).toBe(screen.getByLabelText("Heavy Metals density percent"));
  });

  test("plan this solves for one product until the best mix is back", async () => {
    const user = userEvent.setup();
    api.solvePlan.mockResolvedValue(solution);
    renderPage();
    await user.click(await screen.findByRole("button", { name: "Plan Robotics" }));
    expect(within(screen.getByRole("tablist", { name: "Results" })).getByRole("tab", { name: "Plan" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(location).toContain("plan=9848");
    await waitFor(() => expect(api.solvePlan).toHaveBeenLastCalledWith(expect.objectContaining({ targetTypeId: 9848 })));
    expect(screen.getByText("Robotics", { selector: "b" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Show the best mix" }));
    expect(location).not.toContain("plan=");
    await waitFor(() => expect(api.solvePlan).toHaveBeenLastCalledWith(expect.objectContaining({ targetTypeId: null })));
    expect(screen.queryByRole("button", { name: "Show the best mix" })).toBeNull();
  });

  test("a resource extracted on two planets is listed once", async () => {
    const base = { resourceTypeId: 2300, resourceName: "Base Metals", planetName: "Q-3HS5 I", contested: false };
    api.rankProducts.mockResolvedValue({
      ...RANKING,
      ranked: [
        product({
          typeId: 2398,
          name: "Reactive Metals",
          tier: 1,
          inputs: [
            { ...base, planetId: 1, source: "scanned" },
            { ...base, planetId: 2, planetName: "Q-3HS5 II", source: "default" },
          ],
        }),
      ],
    });
    renderPage();
    const products = await screen.findByRole("table", { name: "Products" });
    const inputs = within(products).getAllByText("Base Metals");
    expect(inputs).toHaveLength(1);
    expect(inputs[0]).toHaveAttribute("data-unscanned", "true");
  });

  test("a solve past its deadline says so", async () => {
    const user = userEvent.setup();
    api.solvePlan.mockRejectedValue(
      new ApiError(408, { code: "solve_timeout", message: "The plan took too long to solve." }),
    );
    renderPage();
    await screen.findByText("Robotics");
    await user.click(within(screen.getByRole("tablist", { name: "Results" })).getByRole("tab", { name: "Plan" }));
    expect(await screen.findByText("Solve timed out")).toBeInTheDocument();
    expect(screen.getByText("The plan took too long to solve.")).toBeInTheDocument();
  });
});
