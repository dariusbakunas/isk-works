import { useMarketScopeLabel } from "../../hooks/use-market-scope-label";
import {
  acquisitionPricingScope,
  describeAcquisitionPricing,
  type AcquisitionPricingDescriptor,
  type AcquisitionPricingIdentity,
} from "./acquisition-pricing";

/**
 * Resolves an {@link AcquisitionPricingIdentity} to a display descriptor,
 * looking up the market region/location name via {@link useMarketScopeLabel}
 * when the identity is a market scope. Call it once per card/drawer -- it is
 * a hook, so it must not run inside a loop.
 */
export function useAcquisitionPricingDescriptor(
  identity: AcquisitionPricingIdentity,
  priceSourceName?: string | null,
): AcquisitionPricingDescriptor {
  const scope = acquisitionPricingScope(identity);
  const { regionName, locationName } = useMarketScopeLabel(scope);
  return describeAcquisitionPricing(identity, {
    regionName: scope ? regionName : null,
    locationName: scope ? locationName : null,
    priceSourceName,
  });
}
