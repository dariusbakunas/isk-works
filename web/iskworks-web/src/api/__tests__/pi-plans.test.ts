import { afterEach, expect, test, vi } from "vitest";

import { deleteSavedPlan, getSavedPlan, listSavedPlans, savePlan, updateSavedPlan } from "../pi-plans";
import { SOLUTION } from "../../features/industry/planetary/planner/__tests__/plan-fixture";

afterEach(() => vi.unstubAllGlobals());

function respond(body: unknown, status = 200) {
  const fetch = vi.fn(async () => new Response(status === 204 ? null : JSON.stringify(body), { status }));
  vi.stubGlobal("fetch", fetch);
  return fetch;
}

function call(fetch: ReturnType<typeof respond>): [string, RequestInit | undefined] {
  return fetch.mock.calls[0] as unknown as [string, RequestInit | undefined];
}

const SCENARIO = {
  homeSystemId: 30_000_797,
  radius: 1,
  characters: [{ interplanetaryConsolidation: 4, commandCenterUpgrades: 4 }],
  characterNames: ["Valka"],
  defaultCustomsRate: "0.1",
  resetDays: 3,
  haulDays: 3,
  upliftAtLogin: true,
  buyIntermediates: false,
  maxColonies: null,
  maxSystemsPerCharacter: null,
  estimateUnscanned: false,
};

test("saved plans are listed, opened and deleted by id", async () => {
  let fetch = respond([]);
  await listSavedPlans();
  expect(call(fetch)[0]).toBe("/api/planetary/plans");

  fetch = respond({});
  await getSavedPlan("abc");
  expect(call(fetch)[0]).toBe("/api/planetary/plans/abc");

  fetch = respond(null, 204);
  await deleteSavedPlan("abc");
  expect(call(fetch)).toEqual(["/api/planetary/plans/abc", expect.objectContaining({ method: "DELETE" })]);
});

test("saving posts the scenario and the solution shown", async () => {
  const fetch = respond({}, 201);
  await savePlan({ name: "Q-3HS5", scenario: SCENARIO, solution: SOLUTION });
  const [path, init] = call(fetch);
  expect(path).toBe("/api/planetary/plans");
  expect(init?.method).toBe("POST");
  expect(JSON.parse(String(init?.body))).toEqual({ name: "Q-3HS5", scenario: SCENARIO, solution: SOLUTION });
});

test("updates PATCH only what changes", async () => {
  const fetch = respond({});
  await updateSavedPlan("abc", { setUpColonies: [0, 2] });
  const [path, init] = call(fetch);
  expect(path).toBe("/api/planetary/plans/abc");
  expect(init?.method).toBe("PATCH");
  expect(JSON.parse(String(init?.body))).toEqual({ setUpColonies: [0, 2] });
});
