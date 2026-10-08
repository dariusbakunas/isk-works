import { describe, expect, it } from "vitest";

import { automaticPricingRetryDelay } from "../preview-retry";

const incompletePreview = {
  canPlan: false,
  validation: {
    blockers: [{ code: "pricingIncomplete", message: "Missing market prices: Ishtar." }],
  },
};

describe("automatic pricing preview retry", () => {
  it("retries incomplete ESI pricing with a bounded delay", () => {
    expect(automaticPricingRetryDelay("esiMarketOrders", incompletePreview, 0)).toBe(1500);
    expect(automaticPricingRetryDelay("esiMarketOrders", incompletePreview, 3)).toBe(1500);
    expect(automaticPricingRetryDelay("esiMarketOrders", incompletePreview, 4)).toBeNull();
  });

  it("does not retry manual, imported, or complete previews", () => {
    expect(automaticPricingRetryDelay("manual", incompletePreview, 0)).toBeNull();
    expect(automaticPricingRetryDelay("eveClientMarketExport", incompletePreview, 0)).toBeNull();
    expect(automaticPricingRetryDelay("esiMarketOrders", {
      canPlan: true,
      validation: { blockers: [] },
    }, 0)).toBeNull();
  });
});
