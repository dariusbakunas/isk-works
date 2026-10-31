import type { PlanColony, PlannerSolution } from "../../../../../api/pi-planner";

function colony(overrides: Partial<PlanColony> & Pick<PlanColony, "index" | "character" | "planetId" | "planetName">): PlanColony {
  return {
    jumps: 0,
    role: "extraction",
    setup: { ecus: 1, heads: 10, launchpads: 1, extraStorage: 0, processors: [{ typeId: 2473, name: "Basic Industry Facility", count: 4 }] },
    extracts: [{ typeId: 2267, name: "Base Metals", tier: 0, unitsPerHour: "9600" }],
    imports: [],
    exports: [{ typeId: 2398, name: "Reactive Metals", tier: 1, unitsPerHour: "160" }],
    recipes: [{ typeId: 2398, name: "Reactive Metals", tier: 1, factories: "4" }],
    taxPerMonth: "4100000",
    cpuUsed: "0.92",
    powerUsed: "0.81",
    onPlanetM3: "4378",
    skyhookM3: "10214",
    ...overrides,
  };
}

/** Two characters: Valka extracts on Q-3HS5 IV and runs a factory on V;
 * Alt extracts Base Metals on IV too (a shared hotspot) and ships across. */
export const SOLUTION: PlannerSolution = {
  priceObservedAt: "2026-11-01T00:00:00Z",
  availableColonies: 4,
  plan: {
    iskPerMonth: "1420000000",
    salesPerMonth: "1600000000",
    purchasesPerMonth: "0",
    customsTaxPerMonth: "184000000",
    effort: { loginsPerWeekByCharacter: ["2.33", "2.33"], haulRunsPerWeek: "2.33", stopsPerRun: 3, m3PerRun: "7800" },
    colonies: [
      colony({ index: 0, character: 0, planetId: 4, planetName: "Q-3HS5 IV", setup: { ecus: 1, heads: 10, launchpads: 1, extraStorage: 1, processors: [] } }),
      colony({
        index: 1,
        character: 0,
        planetId: 5,
        planetName: "Q-3HS5 V",
        role: "factory",
        extracts: [],
        imports: [{ typeId: 2398, name: "Reactive Metals", tier: 1, unitsPerHour: "320" }],
        exports: [{ typeId: 3689, name: "Mechanical Parts", tier: 2, unitsPerHour: "40" }],
        recipes: [{ typeId: 3689, name: "Mechanical Parts", tier: 2, factories: "8" }],
      }),
      colony({ index: 2, character: 1, planetId: 4, planetName: "Q-3HS5 IV" }),
    ],
    flows: [
      { from: 0, to: 1, typeId: 2398, name: "Reactive Metals", tier: 1, unitsPerHour: "160", m3PerHour: "30.4", taxPerMonth: "4100000", crossCharacter: false },
      { from: 2, to: 1, typeId: 2398, name: "Reactive Metals", tier: 1, unitsPerHour: "160", m3PerHour: "30.4", taxPerMonth: "4100000", crossCharacter: true },
      { from: 1, to: null, typeId: 3689, name: "Mechanical Parts", tier: 2, unitsPerHour: "40", m3PerHour: "30", taxPerMonth: "9000000", crossCharacter: false },
    ],
    inputs: [
      { resourceTypeId: 2267, resourceName: "Base Metals", planetId: 4, planetName: "Q-3HS5 IV", source: "default", contested: false },
    ],
    sharedHotspots: [{ planetId: 4, planetName: "Q-3HS5 IV", resourceTypeId: 2267, resourceName: "Base Metals", characters: 2 }],
  },
  whatIfs: {
    perCharacter: ["800000000", "1420000000"],
    perRadius: [
      { jumps: 0, iskPerMonth: "1000000000" },
      { jumps: 1, iskPerMonth: "1420000000" },
    ],
    grid: [1, 3, 7].flatMap((resetDays) =>
      [1, 3, 7, 14].map((haulDays) => ({
        resetDays,
        haulDays,
        iskPerMonth: String(2_000_000_000 - resetDays * 50_000_000 - haulDays * 10_000_000),
        loginsPerWeek: String(7 / Math.min(resetDays, haulDays)),
      })),
    ),
    scanFirst: [
      { planetId: 4, planetName: "Q-3HS5 IV", resourceTypeId: 2267, resourceName: "Base Metals", source: "default", iskLostPerMonth: "120000000" },
    ],
  },
};
