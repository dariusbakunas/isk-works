import { afterEach, expect, test, vi } from "vitest";

import { getFittingsMeta } from "../fittings";

afterEach(() => vi.unstubAllGlobals());

test("fittings meta is read from the meta route", async () => {
  const meta = {
    available: true,
    engine: { dataVersion: "13.3650286.0", major: 13, buildNumber: 3650286 },
    sde: { buildNumber: "3482594" },
    damageProfiles: [{ id: "uniform", name: "Uniform", em: 25, thermal: 25, kinetic: 25, explosive: 25 }],
  };
  const fetch = vi.fn(async () => new Response(JSON.stringify(meta), { status: 200 }));
  vi.stubGlobal("fetch", fetch);

  await expect(getFittingsMeta()).resolves.toEqual(meta);
  expect(fetch).toHaveBeenCalledWith("/api/fittings/meta", expect.objectContaining({ credentials: "include" }));
});
