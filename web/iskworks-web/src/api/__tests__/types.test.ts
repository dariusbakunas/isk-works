import { afterEach, expect, test, vi } from "vitest";

import { getTypeInfo, getTypeUsedIn, getTypeUsedWith, getTypeVariations } from "../types";
import { ApiError } from "../workspace";

afterEach(() => vi.unstubAllGlobals());

function respond(body: unknown, status = 200) {
  const fetch = vi.fn(async () => new Response(JSON.stringify(body), { status }));
  vi.stubGlobal("fetch", fetch);
  return fetch;
}

function path(fetch: ReturnType<typeof respond>): string {
  return (fetch.mock.calls[0] as unknown as [string])[0];
}

test("info is read for All V unless a pilot is given", async () => {
  const info = { typeId: 11381, name: "Harpy", description: [{ kind: "break" }] };
  let fetch = respond(info);
  await expect(getTypeInfo(11381)).resolves.toEqual(info);
  expect(path(fetch)).toBe("/api/types/11381/info");

  fetch = respond(info);
  await getTypeInfo(11381, "allZero");
  expect(path(fetch)).toBe("/api/types/11381/info?pilot=allZero");
});

test("an unknown type surfaces the API's error", async () => {
  respond({ error: { code: "type_not_found", message: "That item isn't in the current static data." } }, 404);
  const error = await getTypeInfo(77777).catch((reason: unknown) => reason);
  expect(error).toBeInstanceOf(ApiError);
  expect((error as ApiError).status).toBe(404);
  expect((error as ApiError).body.code).toBe("type_not_found");
});

test("variations and used-with are read from their routes", async () => {
  let fetch = respond([]);
  await getTypeVariations(37546);
  expect(path(fetch)).toBe("/api/types/37546/variations");

  fetch = respond({ kind: null, groups: [] });
  await expect(getTypeUsedWith(40334)).resolves.toEqual({ kind: null, groups: [] });
  expect(path(fetch)).toBe("/api/types/40334/used-with");
});

test("used-in pages by limit and offset", async () => {
  let fetch = respond({ items: [], total: 0, offset: 0, limit: 50, nextOffset: null });
  await getTypeUsedIn(34);
  expect(path(fetch)).toBe("/api/types/34/used-in");

  fetch = respond({ items: [], total: 1687, offset: 50, limit: 50, nextOffset: 100 });
  await getTypeUsedIn(34, { limit: 50, offset: 50 });
  expect(path(fetch)).toBe("/api/types/34/used-in?limit=50&offset=50");
});
