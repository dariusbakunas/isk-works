import { afterEach, expect, test, vi } from "vitest";

import { exportFitText, getFittingsMeta, importFitText, listFits, updateFit } from "../fittings";
import { ApiError } from "../workspace";

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

test("fits are listed from the fittings route", async () => {
  const fetch = vi.fn(async () => new Response("[]", { status: 200 }));
  vi.stubGlobal("fetch", fetch);

  await expect(listFits()).resolves.toEqual([]);
  expect(fetch).toHaveBeenCalledWith("/api/fittings", expect.anything());
});

test("an import posts the text and the optional name as JSON", async () => {
  const fetch = vi.fn(async () => new Response(JSON.stringify({ id: "f1" }), { status: 201 }));
  vi.stubGlobal("fetch", fetch);

  await importFitText({ text: "[Bustard, .]", name: "Hauler" });
  const [path, init] = fetch.mock.calls[0] as unknown as [string, RequestInit];
  expect(path).toBe("/api/fittings/import");
  expect(init.method).toBe("POST");
  expect(JSON.parse(init.body as string)).toEqual({ text: "[Bustard, .]", name: "Hauler" });
  expect((init.headers as Record<string, string>)["Content-Type"]).toBe("application/json");
});

test("an update puts the whole fit at its id", async () => {
  const fetch = vi.fn(async () => new Response(JSON.stringify({ id: "a b" }), { status: 200 }));
  vi.stubGlobal("fetch", fetch);

  await updateFit("a b", { name: "Fit", description: "", shipTypeId: 12731, items: [] });
  const [path, init] = fetch.mock.calls[0] as unknown as [string, RequestInit];
  expect(path).toBe("/api/fittings/a%20b");
  expect(init.method).toBe("PUT");
});

test("an export reads the fit as text", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => new Response("[Bustard, Hauler]\n", { status: 200 })));

  await expect(exportFitText("f1")).resolves.toBe("[Bustard, Hauler]\n");
});

test("an export without the engine fails with the API's error", async () => {
  const body = { error: { code: "fitting_engine_unavailable", message: "No engine data." } };
  vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify(body), { status: 503 })));

  const error = await exportFitText("f1").catch((caught: unknown) => caught);
  expect(error).toBeInstanceOf(ApiError);
  expect((error as ApiError).status).toBe(503);
  expect((error as ApiError).body.code).toBe("fitting_engine_unavailable");
});
