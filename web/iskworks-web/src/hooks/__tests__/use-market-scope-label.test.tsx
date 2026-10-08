import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { MarketLocation, MarketRegion } from "../../api/industry";
import { useMarketScopeLabel } from "../use-market-scope-label";

const api = vi.hoisted(() => ({
  listMarketRegions: vi.fn(),
  listMarketRegionLocations: vi.fn(),
}));

vi.mock("../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../api/industry")>("../../api/industry");
  return {
    ...actual,
    listMarketRegions: api.listMarketRegions,
    listMarketRegionLocations: api.listMarketRegionLocations,
  };
});

const regions: MarketRegion[] = [
  { regionId: 10_000_002, regionName: "The Forge" },
  { regionId: 10_000_043, regionName: "Domain" },
];

const forgeLocations: MarketLocation[] = [
  {
    locationId: 60_003_760,
    locationName: "Jita IV - Moon 4 - Caldari Navy Assembly Plant",
    kind: "npcStation",
    solarSystemId: 30_000_142,
    solarSystemName: "Jita",
    stationTypeId: 1_529,
    stationTypeName: "Caldari Navy Assembly Plant",
    securityClass: "highSec",
    structureTypeId: null,
    freshness: { trackedTypeCount: 0, observedTypeCount: 0, mostRecentObservedAt: null },
  },
];

describe("useMarketScopeLabel", () => {
  beforeEach(() => {
    api.listMarketRegions.mockReset();
    api.listMarketRegionLocations.mockReset();
  });

  test("resolves region and location names for a specific-location scope", async () => {
    api.listMarketRegions.mockResolvedValue(regions);
    api.listMarketRegionLocations.mockResolvedValue(forgeLocations);

    const { result } = renderHook(() => useMarketScopeLabel({ regionId: 10_000_002, locationId: 60_003_760 }));

    await waitFor(() => {
      expect(result.current.regionName).toBe("The Forge");
      expect(result.current.locationName).toBe("Jita IV - Moon 4 - Caldari Navy Assembly Plant");
    });
  });

  test("reports \"All locations\" for a region-wide scope without fetching locations", async () => {
    api.listMarketRegions.mockResolvedValue(regions);
    api.listMarketRegionLocations.mockResolvedValue(forgeLocations);

    const { result } = renderHook(() => useMarketScopeLabel({ regionId: 10_000_002, locationId: undefined }));

    await waitFor(() => expect(result.current.regionName).toBe("The Forge"));
    expect(result.current.locationName).toBe("All locations");
    expect(api.listMarketRegionLocations).not.toHaveBeenCalled();
  });

  test("falls back to a readable placeholder when the region lookup fails", async () => {
    api.listMarketRegions.mockRejectedValue(new Error("boom"));
    api.listMarketRegionLocations.mockResolvedValue([]);

    const { result } = renderHook(() => useMarketScopeLabel({ regionId: 10_000_002, locationId: 60_003_760 }));

    await waitFor(() => expect(result.current.regionName).toBe("Unknown region"));
  });

  test("falls back to a readable placeholder when the location lookup fails", async () => {
    api.listMarketRegions.mockResolvedValue(regions);
    api.listMarketRegionLocations.mockRejectedValue(new Error("boom"));

    const { result } = renderHook(() => useMarketScopeLabel({ regionId: 10_000_002, locationId: 60_003_760 }));

    await waitFor(() => expect(result.current.locationName).toBe("Unknown location"));
  });

  test("resolves to empty strings and performs no fetches for a null scope", () => {
    api.listMarketRegions.mockResolvedValue(regions);
    api.listMarketRegionLocations.mockResolvedValue(forgeLocations);

    const { result } = renderHook(() => useMarketScopeLabel(null));

    expect(result.current).toEqual({ regionName: "", locationName: "" });
    expect(api.listMarketRegions).not.toHaveBeenCalled();
    expect(api.listMarketRegionLocations).not.toHaveBeenCalled();
  });
});
