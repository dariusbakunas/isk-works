import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { Planet, PlanetaryCharacter, PlanetaryOverview } from "../../../../api/planetary";
import { PlanetaryPage } from "../planetary-page";

const api = vi.hoisted(() => ({
  getPlanetary: vi.fn(),
  getPlanetaryPreferences: vi.fn(),
  savePlanetaryPreferences: vi.fn(),
  syncCharacter: vi.fn(),
  beginAuthorization: vi.fn(),
}));

vi.mock("../../../../api/planetary", async () => ({
  ...(await vi.importActual("../../../../api/planetary")),
  getPlanetary: api.getPlanetary,
  getPlanetaryPreferences: api.getPlanetaryPreferences,
  savePlanetaryPreferences: api.savePlanetaryPreferences,
}));
vi.mock("../../../../api/characters", async () => ({
  ...(await vi.importActual("../../../../api/characters")),
  syncCharacter: api.syncCharacter,
}));
vi.mock("../../../../api/esi", async () => ({
  ...(await vi.importActual("../../../../api/esi")),
  beginAuthorization: api.beginAuthorization,
}));

const inHours = (hours: number) => new Date(Date.now() + hours * 3_600_000).toISOString();

function planet(overrides: Partial<Planet> & Pick<Planet, "planetId" | "name">): Planet {
  return {
    planetType: "barren",
    solarSystemId: 30_000_797,
    solarSystemName: "Q-3HS5",
    security: "-0.14",
    upgradeLevel: 5,
    lastUpdate: inHours(-1),
    attention: null,
    iskPerMonth: "82400000",
    extractors: [],
    production: [{ schematicId: 126, name: "Reactive Metals", factoryCount: 3, outputTypeId: 2398, outputPerHour: "80" }],
    imports: [],
    exports: [{ typeId: 2398, name: "Reactive Metals", unitsPerHour: "80", iskPerMonth: "82400000", excluded: false }],
    storage: [
      {
        pinId: 6,
        kind: "L",
        capacityM3: "10000",
        usedM3: "4500",
        fillPercent: "45",
        value: "12100000",
        contents: [{ typeId: 2398, name: "Reactive Metals", quantity: 23_684, volumeM3: "4500", value: "12100000" }],
      },
    ],
    ...overrides,
  };
}

function character(overrides: Partial<PlanetaryCharacter> & Pick<PlanetaryCharacter, "eveCharacterId" | "name">): PlanetaryCharacter {
  return {
    connectionId: `conn-${overrides.eveCharacterId}`,
    piSkillLevel: 5,
    scopeGranted: true,
    sync: { observedAt: new Date().toISOString(), refreshState: "current", lastError: null },
    iskPerMonth: "342460000",
    nextExpiryAt: inHours(100),
    alertCount: 0,
    planets: [],
    ...overrides,
  };
}

function overview(characters: PlanetaryCharacter[]): PlanetaryOverview {
  return {
    priceObservedAt: new Date().toISOString(),
    summary: {
      iskPerMonth: "1110000000",
      planetCount: characters.reduce((total, entry) => total + entry.planets.length, 0),
      characterCount: characters.length,
      nextAction: { characterName: "Valka", planetName: "EUU-4N II", at: inHours(2.2) },
      alerts: { expired: 1, storageFull: 1, starved: 1 },
    },
    characters,
  };
}

const corvin = character({
  eveCharacterId: 1,
  name: "Corvin Vale",
  alertCount: 2,
  planets: [
    planet({
      planetId: 10,
      name: "Q-3HS5 III",
      planetType: "lava",
      attention: "red",
      extractors: [{ pinId: 2, productTypeId: 2306, productName: "Felsic Magma", expiresAt: inHours(-3), unitsPerHour: "12000" }],
    }),
    planet({
      planetId: 11,
      name: "Q-3HS5 VI",
      attention: "red",
      production: [{ schematicId: 73, name: "Biocells", factoryCount: 2, outputTypeId: 2329, outputPerHour: "25" }],
      imports: [
        { typeId: 2401, name: "Chiral Structures", qtyPerHour: "400", lastsHours: "46" },
        { typeId: 2398, name: "Reactive Metals", qtyPerHour: "500", lastsHours: "0" },
      ],
      exports: [{ typeId: 2329, name: "Biocells", unitsPerHour: "25", iskPerMonth: "176720000", excluded: false }],
    }),
    planet({
      planetId: 12,
      name: "Q-3HS5 IV",
      extractors: [{ pinId: 2, productTypeId: 2267, productName: "Noble Gas", expiresAt: inHours(100), unitsPerHour: "12000" }],
    }),
  ],
});

const valka = character({
  eveCharacterId: 2,
  name: "Valka",
  planets: [planet({ planetId: 20, name: "EUU-4N II", attention: "amber" })],
});

function LocationProbe() {
  const location = useLocation();
  return <output data-testid="location">{location.search}</output>;
}

function renderPage(entry = "/planetary") {
  return render(
    <MemoryRouter initialEntries={[entry]}>
      <Routes>
        <Route
          element={
            <>
              <PlanetaryPage />
              <LocationProbe />
            </>
          }
          path="/planetary"
        />
      </Routes>
    </MemoryRouter>,
  );
}

describe("PlanetaryPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.getPlanetary.mockResolvedValue(overview([corvin, valka]));
    api.getPlanetaryPreferences.mockResolvedValue({ excludedExports: [], characterOrder: [] });
    api.savePlanetaryPreferences.mockImplementation(async (preferences) => preferences);
    api.syncCharacter.mockResolvedValue([]);
  });

  test("renders the summary strip and one group per character with timers and alerts", async () => {
    renderPage();

    const table = await screen.findByRole("table", { name: "Planetary colonies" });
    const summary = screen.getByRole("region", { name: "Planetary summary" });
    expect(within(summary).getByText("1.1B")).toBeInTheDocument();
    expect(within(summary).getByText("Valka · EUU-4N II")).toBeInTheDocument();
    expect(within(summary).getByRole("button", { name: /1 starved/ })).toBeInTheDocument();

    expect(within(table).getByRole("button", { name: "Collapse Corvin Vale" })).toBeInTheDocument();
    const expired = within(table).getByRole("row", { name: /Q-3HS5 III/ });
    expect(expired).toHaveAttribute("data-accent", "blocking");
    expect(within(expired).getByText("Expired")).toBeInTheDocument();
    expect(within(table).getByRole("row", { name: /Q-3HS5 IV/ })).toHaveTextContent("4d 4h");
    const starved = within(table).getByRole("row", { name: /Q-3HS5 VI/ });
    expect(within(starved).getByTitle("Lasts 0.0h")).toHaveClass("text-destructive");
    expect(within(table).getByRole("row", { name: /EUU-4N II/ })).toHaveAttribute("data-accent", "warning");
  });

  test("needs-attention filter hides healthy planets and is kept in the URL", async () => {
    const user = userEvent.setup();
    renderPage();
    const table = await screen.findByRole("table", { name: "Planetary colonies" });
    expect(within(table).getByRole("row", { name: /Q-3HS5 IV/ })).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "Needs attention" }));

    expect(screen.getByTestId("location")).toHaveTextContent("filter=attention");
    expect(within(table).queryByRole("row", { name: /Q-3HS5 IV/ })).not.toBeInTheDocument();
    expect(within(table).getByRole("row", { name: /Q-3HS5 III/ })).toBeInTheDocument();
  });

  test("search narrows planets by product", async () => {
    const user = userEvent.setup();
    renderPage();
    const table = await screen.findByRole("table", { name: "Planetary colonies" });

    await user.type(screen.getByRole("searchbox", { name: "Search planets" }), "biocells");

    expect(within(table).getByRole("row", { name: /Q-3HS5 VI/ })).toBeInTheDocument();
    expect(within(table).queryByRole("row", { name: /Q-3HS5 III/ })).not.toBeInTheDocument();
    expect(within(table).queryByRole("button", { name: /Valka/ })).not.toBeInTheDocument();
  });

  test("excluding an export saves the preference and reloads", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("table", { name: "Planetary colonies" });

    await user.click(screen.getByRole("button", { name: "Exclude Biocells on Q-3HS5 VI from totals" }));

    expect(api.savePlanetaryPreferences).toHaveBeenCalledWith({
      excludedExports: [{ characterId: 1, planetId: 11, typeId: 2329 }],
      characterOrder: [],
    });
    expect(api.getPlanetary).toHaveBeenCalledTimes(2);
  });

  test("moving a character down saves the new order", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("table", { name: "Planetary colonies" });

    await user.click(screen.getByRole("button", { name: "Corvin Vale options" }));
    await user.click(screen.getByRole("menuitem", { name: "Move down" }));

    expect(api.savePlanetaryPreferences).toHaveBeenCalledWith({ excludedExports: [], characterOrder: [2, 1] });
  });

  test("a planet's row menu excludes all of its exports", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("table", { name: "Planetary colonies" });

    await user.click(screen.getByRole("button", { name: "Q-3HS5 VI options" }));
    await user.click(screen.getByRole("menuitem", { name: "Exclude all exports" }));

    expect(api.savePlanetaryPreferences).toHaveBeenCalledWith({
      excludedExports: [{ characterId: 1, planetId: 11, typeId: 2329 }],
      characterOrder: [],
    });
  });

  test("storage bars reveal their contents on hover", async () => {
    const user = userEvent.setup();
    renderPage();
    const table = await screen.findByRole("table", { name: "Planetary colonies" });

    const bar = within(within(table).getByRole("row", { name: /EUU-4N II/ })).getByRole("group", { name: "Launchpad 45.0% full" });
    await user.hover(bar);

    const tooltip = screen.getByRole("tooltip");
    expect(tooltip).toHaveTextContent("Launchpad contents");
    expect(tooltip).toHaveTextContent("23,684");
  });

  test("refresh syncs each character with the scope then reloads", async () => {
    const user = userEvent.setup();
    api.getPlanetary.mockResolvedValue(
      overview([corvin, character({ eveCharacterId: 3, name: "No PI", scopeGranted: false })]),
    );
    renderPage();
    await screen.findByRole("table", { name: "Planetary colonies" });

    await user.click(screen.getByRole("button", { name: "Refresh" }));

    expect(api.syncCharacter).toHaveBeenCalledTimes(1);
    expect(api.syncCharacter).toHaveBeenCalledWith("conn-1");
    expect(api.getPlanetary).toHaveBeenCalledTimes(2);
  });

  test("hides characters without colonies by default and can show them", async () => {
    const user = userEvent.setup();
    api.getPlanetary.mockResolvedValue(
      overview([corvin, character({ eveCharacterId: 3, name: "No PI", scopeGranted: false }), character({ eveCharacterId: 4, name: "Empty", planets: [] })]),
    );
    renderPage();
    const table = await screen.findByRole("table", { name: "Planetary colonies" });

    const toggle = screen.getByRole("checkbox", { name: /Hide characters without colonies/ });
    expect(toggle).toBeChecked();
    expect(screen.getByText("(2 hidden)")).toBeInTheDocument();
    expect(within(table).queryByRole("button", { name: /No PI/ })).not.toBeInTheDocument();
    expect(within(table).queryByRole("button", { name: /Expand Empty|Collapse Empty/ })).not.toBeInTheDocument();

    await user.click(toggle);

    expect(screen.getByTestId("location")).toHaveTextContent("empty=show");
    expect(within(table).getByRole("button", { name: "Collapse No PI" })).toBeInTheDocument();
    expect(screen.getByText(/This character has not granted planetary access/)).toBeInTheDocument();
    expect(within(table).getByText("No colonies on this character.")).toBeInTheDocument();
  });

  test("moving a character skips over hidden ones in the saved order", async () => {
    const user = userEvent.setup();
    api.getPlanetary.mockResolvedValue(
      overview([corvin, character({ eveCharacterId: 3, name: "No PI", scopeGranted: false }), valka]),
    );
    renderPage();
    await screen.findByRole("table", { name: "Planetary colonies" });

    await user.click(screen.getByRole("button", { name: "Valka options" }));
    await user.click(screen.getByRole("menuitem", { name: "Move up" }));

    expect(api.savePlanetaryPreferences).toHaveBeenCalledWith({ excludedExports: [], characterOrder: [2, 1, 3] });
  });

  test("explains when every character is hidden for lack of colonies", async () => {
    api.getPlanetary.mockResolvedValue(overview([character({ eveCharacterId: 4, name: "Empty", planets: [] })]));
    renderPage();

    expect(await screen.findByText("No colonies yet")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Show all characters" })).toBeInTheDocument();
  });

  test("shows a grant-access empty state when no character has the scope", async () => {
    const user = userEvent.setup();
    api.getPlanetary.mockResolvedValue(overview([character({ eveCharacterId: 3, name: "No PI", scopeGranted: false })]));
    api.beginAuthorization.mockResolvedValue({ authorizationUrl: "", fixtureMode: true, connection: null, requestedScopes: [] });
    renderPage();

    expect(await screen.findByText("No planetary access")).toBeInTheDocument();
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Grant planetary access" }));
    expect(api.beginAuthorization).toHaveBeenCalled();
  });

  test("flags a character whose colonies have not synced in over an hour as stale", async () => {
    api.getPlanetary.mockResolvedValue(
      overview([
        character({
          ...valka,
          sync: { observedAt: new Date(Date.now() - 2 * 3_600_000).toISOString(), refreshState: "current", lastError: null },
        }),
      ]),
    );
    renderPage();

    expect(await screen.findByText("Stale 2h ago")).toBeInTheDocument();
  });
});
