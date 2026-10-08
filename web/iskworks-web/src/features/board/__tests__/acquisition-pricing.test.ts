import { describe, expect, it } from "vitest";

import {
  acquisitionPricingIdentity,
  acquisitionPricingKey,
  acquisitionPricingScope,
  describeAcquisitionPricing,
  isBatchablePricingIdentity,
} from "../acquisition-pricing";

describe("acquisitionPricingIdentity", () => {
  it("prefers a market scope over a manual price list (matches backend BatchKey)", () => {
    expect(
      acquisitionPricingIdentity({ marketRegionId: 10_000_002, marketLocationId: 60_003_760, priceSourceId: "s1" }),
    ).toEqual({ kind: "scope", marketRegionId: 10_000_002, marketLocationId: 60_003_760 });
  });

  it("falls back to a manual price list only when there is no market scope", () => {
    expect(
      acquisitionPricingIdentity({ marketRegionId: null, marketLocationId: null, priceSourceId: "s1" }),
    ).toEqual({ kind: "list", priceSourceId: "s1" });
  });

  it("is 'none' when there is neither a scope nor a price list", () => {
    expect(
      acquisitionPricingIdentity({ marketRegionId: null, marketLocationId: null, priceSourceId: null }),
    ).toEqual({ kind: "none" });
  });

  it("keeps a region-wide scope (null location) distinct from a station scope", () => {
    const regionWide = acquisitionPricingIdentity({ marketRegionId: 10_000_002, marketLocationId: null, priceSourceId: null });
    const station = acquisitionPricingIdentity({ marketRegionId: 10_000_002, marketLocationId: 60_003_760, priceSourceId: null });
    expect(acquisitionPricingKey(regionWide)).toBe("scope:10000002:");
    expect(acquisitionPricingKey(station)).toBe("scope:10000002:60003760");
  });
});

describe("acquisitionPricingScope", () => {
  it("returns a MarketScope for a scope identity, null otherwise", () => {
    expect(acquisitionPricingScope({ kind: "scope", marketRegionId: 1, marketLocationId: 2 })).toEqual({
      regionId: 1,
      locationId: 2,
    });
    expect(acquisitionPricingScope({ kind: "scope", marketRegionId: 1, marketLocationId: null })).toEqual({
      regionId: 1,
      locationId: undefined,
    });
    expect(acquisitionPricingScope({ kind: "list", priceSourceId: "s1" })).toBeNull();
    expect(acquisitionPricingScope({ kind: "none" })).toBeNull();
  });
});

describe("isBatchablePricingIdentity", () => {
  it("is false only for 'none'", () => {
    expect(isBatchablePricingIdentity({ kind: "scope", marketRegionId: 1, marketLocationId: 2 })).toBe(true);
    expect(isBatchablePricingIdentity({ kind: "list", priceSourceId: "s1" })).toBe(true);
    expect(isBatchablePricingIdentity({ kind: "none" })).toBe(false);
  });
});

describe("describeAcquisitionPricing", () => {
  it("names a station scope as 'Priced at <location>'", () => {
    expect(
      describeAcquisitionPricing(
        { kind: "scope", marketRegionId: 10_000_002, marketLocationId: 60_003_760 },
        { regionName: "The Forge", locationName: "Jita IV - Moon 4 - Caldari Navy Assembly Plant" },
      ),
    ).toEqual({ label: "Priced at Jita IV - Moon 4 - Caldari Navy Assembly Plant", icon: "location" });
  });

  it("names a region-wide scope as 'Priced in <region>'", () => {
    expect(
      describeAcquisitionPricing(
        { kind: "scope", marketRegionId: 10_000_002, marketLocationId: null },
        { regionName: "The Forge", locationName: "All locations" },
      ),
    ).toEqual({ label: "Priced in The Forge", icon: "location" });
  });

  it("degrades a scope to a generic 'Market pricing' while names are still loading", () => {
    expect(
      describeAcquisitionPricing(
        { kind: "scope", marketRegionId: 10_000_002, marketLocationId: 60_003_760 },
        { regionName: "…", locationName: "…" },
      ),
    ).toEqual({ label: "Market pricing", icon: "location" });
  });

  it("never presents a scope as 'Unknown source'", () => {
    const label = describeAcquisitionPricing(
      { kind: "scope", marketRegionId: 10_000_002, marketLocationId: 60_003_760 },
      { regionName: "Unknown region", locationName: "Unknown location" },
    ).label;
    expect(label).toBe("Market pricing");
  });

  it("names a manual price list as a list, not a place", () => {
    expect(
      describeAcquisitionPricing({ kind: "list", priceSourceId: "s1" }, { priceSourceName: "Weekend buy list" }),
    ).toEqual({ label: "Price list: Weekend buy list", icon: "list" });
  });

  it("says 'No pricing source' for an unspecified identity", () => {
    expect(describeAcquisitionPricing({ kind: "none" }, {})).toEqual({
      label: "No pricing source",
      icon: "none",
    });
  });
});
