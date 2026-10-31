import { afterEach, expect, test, vi } from "vitest";

import {
  getPlannerCharacters,
  getPlannerPlanets,
  rankProducts,
  saveDensities,
  savePlanetSettings,
  SDE_MISSING_PLANNER_DATA,
  type PlannerScenario,
} from "../pi-planner";
import { ApiError } from "../workspace";

afterEach(() => vi.unstubAllGlobals());

function respond(body: unknown, status = 200) {
  const fetch = vi.fn(
    async () => new Response(status === 204 ? null : JSON.stringify(body), { status }),
  );
  vi.stubGlobal("fetch", fetch);
  return fetch;
}

function call(fetch: ReturnType<typeof respond>): [string, RequestInit | undefined] {
  return fetch.mock.calls[0] as unknown as [string, RequestInit | undefined];
}

test("planets and characters are read from their routes", async () => {
  let fetch = respond({ systems: [], planets: [] });
  await getPlannerPlanets(30_000_797, 2);
  expect(call(fetch)[0]).toBe("/api/planetary/planner/planets?systemId=30000797&radius=2");

  fetch = respond([]);
  await getPlannerCharacters();
  expect(call(fetch)[0]).toBe("/api/planetary/planner/characters");
});

test("densities and planet settings are PUT as arrays", async () => {
  let fetch = respond(null, 204);
  await saveDensities([{ planetId: 1, resourceTypeId: 2267, densityPct: "58", headValue: null }]);
  let [path, init] = call(fetch);
  expect(path).toBe("/api/planetary/planner/densities");
  expect(init?.method).toBe("PUT");
  expect(JSON.parse(String(init?.body))).toEqual([
    { planetId: 1, resourceTypeId: 2267, densityPct: "58", headValue: null },
  ]);

  fetch = respond(null, 204);
  await savePlanetSettings([{ planetId: 1, customsRate: "0.05", contested: true }]);
  [path, init] = call(fetch);
  expect(path).toBe("/api/planetary/planner/planet-settings");
  expect(JSON.parse(String(init?.body))).toEqual([{ planetId: 1, customsRate: "0.05", contested: true }]);
});

test("rank posts the scenario and surfaces a missing-SDE 409", async () => {
  const scenario: PlannerScenario = {
    homeSystemId: 30_000_797,
    radius: 1,
    characters: [{ interplanetaryConsolidation: 4, commandCenterUpgrades: 4 }],
    defaultCustomsRate: "0.1",
    resetDays: 3,
    haulDays: 3,
    upliftAtLogin: true,
    buyIntermediates: false,
    maxColonies: null,
  };
  let fetch = respond({ priceObservedAt: null, availableColonies: 5, ranked: [], outOfReach: [] });
  await rankProducts(scenario);
  const [path, init] = call(fetch);
  expect(path).toBe("/api/planetary/planner/rank");
  expect(init?.method).toBe("POST");
  expect(JSON.parse(String(init?.body))).toEqual(scenario);

  fetch = respond({ error: { code: SDE_MISSING_PLANNER_DATA, message: "Re-import the SDE." } }, 409);
  const error = await rankProducts(scenario).catch((reason: unknown) => reason);
  expect(error).toBeInstanceOf(ApiError);
  expect((error as ApiError).body.code).toBe(SDE_MISSING_PLANNER_DATA);
});
