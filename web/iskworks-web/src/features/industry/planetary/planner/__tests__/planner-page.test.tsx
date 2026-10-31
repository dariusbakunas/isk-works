import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { PlannerPlanets, PlannerRanking, RankedProduct } from "../../../../../api/pi-planner";
import { ApiError } from "../../../../../api/workspace";
import { PlannerPage } from "../planner-page";

const api = vi.hoisted(() => ({
  getPlannerPlanets: vi.fn(),
  getPlannerCharacters: vi.fn(),
  rankProducts: vi.fn(),
  saveDensities: vi.fn(),
  savePlanetSettings: vi.fn(),
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

  test("prefill replaces the characters with connected ones", async () => {
    const user = userEvent.setup();
    api.getPlannerCharacters.mockResolvedValue([
      { eveCharacterId: 1, name: "Valka", interplanetaryConsolidation: 5, commandCenterUpgrades: 3 },
      { eveCharacterId: 2, name: "Alt", interplanetaryConsolidation: null, commandCenterUpgrades: null },
    ]);
    renderPage();
    await user.click(screen.getByRole("button", { name: "Prefill from connected characters" }));
    const rows = await screen.findAllByTestId("scenario-character");
    expect(rows).toHaveLength(2);
    expect(within(rows[0]).getByText("Valka")).toBeInTheDocument();
    expect(within(rows[0]).getByText("→ 6 planets")).toBeInTheDocument();
    expect(within(rows[1]).getByText("→ 1 planet")).toBeInTheDocument();
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
