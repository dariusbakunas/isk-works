import { afterEach, expect, test, vi } from "vitest";

import { getSdeStatus } from "../sde";
import { ApiError } from "../workspace";

afterEach(() => vi.unstubAllGlobals());

function stubFetch(response: Response) {
  vi.stubGlobal("fetch", vi.fn(async () => response));
}

test("a non-JSON error response becomes an ApiError with the response status", async () => {
  stubFetch(new Response("<html>Bad Gateway</html>", { status: 502 }));

  const error = await getSdeStatus().catch((caught: unknown) => caught);

  expect(error).toBeInstanceOf(ApiError);
  expect((error as ApiError).status).toBe(502);
  expect((error as ApiError).body.code).toBe("api_error");
});

test("a JSON error response without an error body becomes an ApiError", async () => {
  stubFetch(
    new Response(JSON.stringify({ unexpected: true }), {
      status: 500,
      headers: { "content-type": "application/json" },
    }),
  );

  const error = await getSdeStatus().catch((caught: unknown) => caught);

  expect(error).toBeInstanceOf(ApiError);
  expect((error as ApiError).status).toBe(500);
  expect((error as ApiError).body.code).toBe("api_error");
});

test("a JSON error body is passed through unchanged", async () => {
  stubFetch(
    new Response(
      JSON.stringify({ error: { code: "no_active_sde", message: "No SDE import is active." } }),
      { status: 409, headers: { "content-type": "application/json" } },
    ),
  );

  const error = await getSdeStatus().catch((caught: unknown) => caught);

  expect(error).toBeInstanceOf(ApiError);
  expect((error as ApiError).body).toEqual({
    code: "no_active_sde",
    message: "No SDE import is active.",
  });
});
