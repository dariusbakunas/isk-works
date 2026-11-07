import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { installMaintenanceReload } from "../maintenance";
import { ApiError, requestJson } from "../workspace";

function maintenanceResponse() {
  return new Response(JSON.stringify({ maintenance: true, message: "Back soon." }), {
    status: 503,
    headers: { "Content-Type": "application/json", "X-Maintenance": "1" },
  });
}

let reload: ReturnType<typeof vi.fn>;

function stubFetch(implementation: () => Promise<Response>) {
  vi.stubGlobal("fetch", vi.fn().mockImplementation(implementation));
  installMaintenanceReload();
}

beforeEach(() => {
  sessionStorage.clear();
  reload = vi.fn();
  vi.stubGlobal("location", { ...window.location, reload });
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

test("a maintenance response reloads the page so the Worker can serve its page", async () => {
  stubFetch(async () => maintenanceResponse());

  await expect(requestJson("/api/auth/session")).rejects.toBeInstanceOf(ApiError);

  expect(reload).toHaveBeenCalledOnce();
});

test("a second maintenance response right after a reload does not reload again", async () => {
  stubFetch(async () => maintenanceResponse());

  await expect(requestJson("/api/auth/session")).rejects.toBeInstanceOf(ApiError);
  await expect(requestJson("/api/auth/session")).rejects.toBeInstanceOf(ApiError);

  expect(reload).toHaveBeenCalledOnce();
});

test("reloads again once the guard window has passed", async () => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date("2026-11-06T12:00:00Z"));
  stubFetch(async () => maintenanceResponse());

  await expect(requestJson("/api/x")).rejects.toBeInstanceOf(ApiError);
  vi.setSystemTime(new Date("2026-11-06T12:01:01Z"));
  await expect(requestJson("/api/x")).rejects.toBeInstanceOf(ApiError);

  expect(reload).toHaveBeenCalledTimes(2);
});

test("without sessionStorage it does not reload, because nothing could stop a loop", async () => {
  vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
    throw new Error("storage disabled");
  });
  stubFetch(async () => maintenanceResponse());

  await expect(requestJson("/api/x")).rejects.toBeInstanceOf(ApiError);

  expect(reload).not.toHaveBeenCalled();
});

test("an ordinary 503 from the API does not reload", async () => {
  stubFetch(
    async () =>
      new Response(JSON.stringify({ error: { code: "persistence_unavailable", message: "db down" } }), {
        status: 503,
      }),
  );

  await expect(requestJson("/api/x")).rejects.toMatchObject({
    status: 503,
    body: { code: "persistence_unavailable" },
  });

  expect(reload).not.toHaveBeenCalled();
  expect(sessionStorage.length).toBe(0);
});

test("a successful response passes through untouched", async () => {
  stubFetch(async () => new Response(JSON.stringify({ ok: true }), { status: 200 }));

  await expect(requestJson("/api/x")).resolves.toEqual({ ok: true });

  expect(reload).not.toHaveBeenCalled();
});
