import { useEffect, useState } from "react";

import { listMarketRegionLocations, listMarketRegions, type MarketScope } from "../api/industry";

/**
 * Resolves a `MarketScope` to human-readable region/location names for
 * read-only display (e.g. a collapsed settings summary) -- independent of
 * `MarketScopeSelector`, which resolves the same names internally for its
 * own trigger button but only while mounted (i.e. only while its owning
 * editor UI is open).
 *
 * Pass `null` when the caller has no scope to resolve (e.g. a group priced
 * from a manual list rather than a market scope) -- the hook then performs
 * no fetches and returns empty strings, so it can still be called
 * unconditionally to satisfy the rules of hooks.
 */
export function useMarketScopeLabel(scope: MarketScope | null): { regionName: string; locationName: string } {
  const regionId = scope?.regionId ?? null;
  const locationId = scope?.locationId;
  const [regionName, setRegionName] = useState(scope === null ? "" : "…");
  const [locationName, setLocationName] = useState(
    scope === null ? "" : scope.locationId === undefined ? "All locations" : "…",
  );

  useEffect(() => {
    if (regionId === null) {
      setRegionName("");
      return;
    }
    let cancelled = false;
    listMarketRegions()
      .then((regions) => {
        if (cancelled) return;
        setRegionName(regions.find((region) => region.regionId === regionId)?.regionName ?? "Unknown region");
      })
      .catch(() => {
        if (!cancelled) setRegionName("Unknown region");
      });
    return () => {
      cancelled = true;
    };
  }, [regionId]);

  useEffect(() => {
    if (regionId === null) {
      setLocationName("");
      return;
    }
    if (locationId === undefined) {
      setLocationName("All locations");
      return;
    }
    let cancelled = false;
    listMarketRegionLocations(regionId)
      .then((locations) => {
        if (cancelled) return;
        setLocationName(locations.find((location) => location.locationId === locationId)?.locationName ?? "Unknown location");
      })
      .catch(() => {
        if (!cancelled) setLocationName("Unknown location");
      });
    return () => {
      cancelled = true;
    };
  }, [regionId, locationId]);

  return { regionName, locationName };
}
