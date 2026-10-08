import type { MarketScope } from "../../api/industry";

/**
 * The acquisition batching-compatibility identity, mirroring the backend
 * `BatchKey` in `crates/iskworks-storage/src/order/acquisition.rs`:
 *
 *   - a resolved **market scope** (`market_region_id` [+ `market_location_id`])
 *     takes precedence -- it is the real "where this was priced / where it
 *     will be acquired" key;
 *   - a **manual price list** (`price_source_id`) is the fallback, used only
 *     for a ticket that has no market scope at all;
 *   - a ticket with **neither** is not batch-compatible with anything
 *     (`TicketNotBatchable` server-side).
 *
 * This is deliberately independent of Epic/`orderId`: two tickets from
 * different Epics that share one market scope are intentionally compatible.
 */
export type AcquisitionPricingIdentity =
  | { kind: "scope"; marketRegionId: number; marketLocationId: number | null }
  | { kind: "list"; priceSourceId: string }
  | { kind: "none" };

export interface AcquisitionPricingFields {
  marketRegionId: number | null;
  marketLocationId: number | null;
  priceSourceId: string | null;
}

export function acquisitionPricingIdentity(
  fields: AcquisitionPricingFields,
): AcquisitionPricingIdentity {
  if (fields.marketRegionId !== null) {
    return {
      kind: "scope",
      marketRegionId: fields.marketRegionId,
      marketLocationId: fields.marketLocationId,
    };
  }
  if (fields.priceSourceId !== null) {
    return { kind: "list", priceSourceId: fields.priceSourceId };
  }
  return { kind: "none" };
}

/**
 * Stable grouping key for an identity. Market scope wins over a manual price
 * list exactly as it does in the backend `BatchKey`, so tickets that the
 * server would refuse to batch together never share a Board group.
 */
export function acquisitionPricingKey(identity: AcquisitionPricingIdentity): string {
  switch (identity.kind) {
    case "scope":
      return `scope:${identity.marketRegionId}:${identity.marketLocationId ?? ""}`;
    case "list":
      return `list:${identity.priceSourceId}`;
    case "none":
      return "none";
  }
}

/** The `MarketScope` value to hand to `useMarketScopeLabel`, or `null` when
 * this identity has no market scope to resolve. */
export function acquisitionPricingScope(identity: AcquisitionPricingIdentity): MarketScope | null {
  return identity.kind === "scope"
    ? { regionId: identity.marketRegionId, locationId: identity.marketLocationId ?? undefined }
    : null;
}

/** Only a market scope or a manual price list can be batched into an
 * Acquisition Run -- a `none` identity cannot. */
export function isBatchablePricingIdentity(identity: AcquisitionPricingIdentity): boolean {
  return identity.kind !== "none";
}

export type AcquisitionPricingIcon = "location" | "list" | "none";

export interface AcquisitionPricingDescriptor {
  label: string;
  icon: AcquisitionPricingIcon;
}

const UNRESOLVED = new Set(["", "…", "Unknown region", "Unknown location", "All locations"]);

function usableName(value: string | null | undefined): string | undefined {
  const trimmed = value?.trim();
  return trimmed && !UNRESOLVED.has(trimmed) ? trimmed : undefined;
}

/**
 * Turns an acquisition pricing identity plus whatever names have resolved
 * so far into an explicit, non-misleading descriptor. A market scope is
 * presented as *where the price came from* ("Priced at …" / "Priced in …"),
 * never as a mandated hauling destination; a manual price list is presented
 * as a list, never as a place; an unspecified identity says so plainly.
 * Degrades to a generic-but-accurate "Market pricing" while names load.
 */
export function describeAcquisitionPricing(
  identity: AcquisitionPricingIdentity,
  names: { regionName?: string | null; locationName?: string | null; priceSourceName?: string | null },
): AcquisitionPricingDescriptor {
  switch (identity.kind) {
    case "scope": {
      const region = usableName(names.regionName);
      const location = usableName(names.locationName);
      if (identity.marketLocationId !== null && location) {
        return { label: `Priced at ${location}`, icon: "location" };
      }
      if (region) {
        return { label: `Priced in ${region}`, icon: "location" };
      }
      return { label: "Market pricing", icon: "location" };
    }
    case "list": {
      const name = usableName(names.priceSourceName);
      return { label: `Price list: ${name ?? "Manual"}`, icon: "list" };
    }
    case "none":
      return { label: "No pricing source", icon: "none" };
  }
}
