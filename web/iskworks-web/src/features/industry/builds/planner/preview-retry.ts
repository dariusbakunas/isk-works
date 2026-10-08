import type { PriceSource } from "../../../../api/industry";

const RETRY_DELAY_MS = 1500;
export const MAX_MARKET_SYNC_RETRIES = 4;
const MAX_RETRIES = MAX_MARKET_SYNC_RETRIES;

interface PricingPreview {
  canPlan: boolean;
  validation: {
    blockers: Array<{ code: string }>;
  };
}

export function automaticPricingRetryDelay(
  sourceKind: PriceSource["kind"],
  preview: PricingPreview,
  attempts: number,
): number | null {
  if (
    sourceKind !== "esiMarketOrders"
    || preview.canPlan
    || attempts >= MAX_RETRIES
    || !preview.validation.blockers.some((blocker) => blocker.code === "pricingIncomplete")
  ) {
    return null;
  }
  return RETRY_DELAY_MS;
}
