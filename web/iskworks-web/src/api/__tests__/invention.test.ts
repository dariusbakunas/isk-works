import { afterEach, describe, expect, test, vi } from "vitest";

import { quoteInvention, searchInventableProducts } from "../invention";

function respondWith(body: unknown) {
  const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(body), { status: 200 }));
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

describe("invention api", () => {
  afterEach(() => vi.unstubAllGlobals());

  test("searches products with an encoded query", async () => {
    const fetchMock = respondWith({ inventionDataAvailable: true, products: [] });

    const result = await searchInventableProducts("hammerhead ii");

    expect(fetchMock.mock.calls[0][0]).toBe("/api/invention/products?query=hammerhead%20ii");
    expect(result.inventionDataAvailable).toBe(true);
  });

  test("posts the quote command as JSON", async () => {
    const fetchMock = respondWith({ inventionDataAvailable: true, quote: null });
    const command = {
      productTypeId: 11377,
      characterId: "0b8f8d6e-2f43-4c1a-9a55-5d0f3c6f9a11",
      skillOverrides: { "23121": 5 },
      targetRuns: 10,
    };

    await quoteInvention(command);

    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe("/api/invention/quote");
    expect(init.method).toBe("POST");
    expect(JSON.parse(init.body)).toEqual(command);
  });
});
