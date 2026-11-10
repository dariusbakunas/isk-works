import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { MarketLocation } from "../../api/industry";
import { useMarketScopeData, type MarketScopeDataRequest } from "../use-market-scope-data";

const api = vi.hoisted(() => ({
  listMarketRegions: vi.fn(),
  listMarketRegionLocations: vi.fn(),
  listMarketHubs: vi.fn(),
  listMarketStructures: vi.fn(),
}));

vi.mock("../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../api/industry")>("../../api/industry");
  return { ...actual, ...api };
});

const FORGE = 10_000_002;
const DOMAIN = 10_000_043;

function location(locationId: number, locationName: string): MarketLocation {
  return { locationId, locationName } as MarketLocation;
}

const closed: MarketScopeDataRequest = {
  regionId: FORGE,
  draftRegionId: FORGE,
  open: false,
  session: 0,
  structuresWanted: false,
};

beforeEach(() => {
  vi.resetAllMocks();
  api.listMarketRegions.mockResolvedValue([{ regionId: FORGE, regionName: "The Forge" }]);
  api.listMarketRegionLocations.mockImplementation((regionId: number) =>
    Promise.resolve(regionId === FORGE ? [location(1, "Jita")] : [location(2, "Amarr")]),
  );
  api.listMarketHubs.mockResolvedValue([]);
  api.listMarketStructures.mockResolvedValue([]);
});

describe("useMarketScopeData", () => {
  test("closed, it loads only what the trigger label needs", async () => {
    const { result } = renderHook(() => useMarketScopeData(closed));

    await waitFor(() => expect(result.current.activeLocations).toEqual([location(1, "Jita")]));
    expect(result.current.regions).toHaveLength(1);
    expect(api.listMarketHubs).not.toHaveBeenCalled();
    expect(api.listMarketStructures).not.toHaveBeenCalled();
    expect(api.listMarketRegionLocations).toHaveBeenCalledTimes(1);
    expect(result.current.structures).toEqual({ status: "idle" });
  });

  test("hubs load per open session, showing loading again on each", async () => {
    const { result, rerender } = renderHook((request: MarketScopeDataRequest) => useMarketScopeData(request), {
      initialProps: { ...closed, open: true, session: 1 },
    });
    expect(result.current.hubs).toBeNull();
    await waitFor(() => expect(result.current.hubs).toEqual([]));

    rerender({ ...closed, open: false, session: 1 });
    rerender({ ...closed, open: true, session: 2 });
    expect(result.current.hubs).toBeNull();
    await waitFor(() => expect(result.current.hubs).toEqual([]));
    expect(api.listMarketHubs).toHaveBeenCalledTimes(2);
  });

  test("structures stay idle until wanted, and a failure reads as an error", async () => {
    api.listMarketStructures.mockRejectedValue(new Error("403"));
    const open = { ...closed, open: true, session: 1 };
    const { result, rerender } = renderHook((request: MarketScopeDataRequest) => useMarketScopeData(request), {
      initialProps: open,
    });
    expect(result.current.structures).toEqual({ status: "idle" });

    rerender({ ...open, structuresWanted: true });
    expect(result.current.structures).toEqual({ status: "loading" });
    await waitFor(() => expect(result.current.structures).toEqual({ status: "error" }));
  });

  test("draft locations reuse the committed region's, and fetch another region's", async () => {
    const open = { ...closed, open: true, session: 1 };
    const { result, rerender } = renderHook((request: MarketScopeDataRequest) => useMarketScopeData(request), {
      initialProps: open,
    });
    await waitFor(() => expect(result.current.draftLocations).toEqual([location(1, "Jita")]));
    expect(api.listMarketRegionLocations).toHaveBeenCalledTimes(1);

    rerender({ ...open, draftRegionId: DOMAIN });
    await waitFor(() => expect(result.current.draftLocations).toEqual([location(2, "Amarr")]));
    expect(result.current.activeLocations).toEqual([location(1, "Jita")]);
  });

  test("a failed list reads as empty", async () => {
    api.listMarketRegions.mockRejectedValue(new Error("down"));
    api.listMarketHubs.mockRejectedValue(new Error("down"));
    const { result } = renderHook(() => useMarketScopeData({ ...closed, open: true, session: 1 }));

    await waitFor(() => expect(result.current.hubs).toEqual([]));
    expect(result.current.regions).toEqual([]);
  });
});
