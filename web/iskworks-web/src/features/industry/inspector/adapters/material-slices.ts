// Shared slice builders for a *material* row -- coverage, pricing, value and
// provenance. Used by BOTH the Worksheet adapter (`buildWorksheetInspector`)
// and the Graph acquisition-node path of `buildGraphInspector`, so a raw BUY
// material selected from either view renders identical sections from the same
// logic.

import type { WorksheetItem } from "../../../../api/industry";
import { formatIskSummary } from "../../../../components/money";
import type {
  CoverageSlice,
  PricingSlice,
  ProvenanceSlice,
  ValueSlice,
} from "../inspector-model";

const NUMBER = new Intl.NumberFormat("en-US");

export function coverageSlice(item: WorksheetItem): CoverageSlice {
  const percentage = Number(item.coveragePercentage) || 0;
  return {
    percentage,
    hasShortage: item.missingQuantity > 0,
    summary:
      item.missingQuantity > 0
        ? `${item.coveragePercentage}% covered · ${NUMBER.format(item.missingQuantity)} short`
        : `${item.coveragePercentage}% covered`,
    metrics: [
      { label: "Required", value: NUMBER.format(item.requiredQuantity) },
      { label: "Available", value: NUMBER.format(item.availableQuantity) },
      { label: "Covered", value: NUMBER.format(item.coveredQuantity), tone: "positive" },
      {
        label: "Shortage",
        value: NUMBER.format(item.missingQuantity),
        tone: item.missingQuantity > 0 ? "blocking" : "neutral",
      },
    ],
  };
}

export function valueSlice(item: WorksheetItem, installationFacilityName?: string | null): ValueSlice {
  const metrics: ValueSlice["metrics"] = [
    {
      label: "Inventory cost",
      value: item.projectedInventoryCost ? formatIskSummary(item.projectedInventoryCost) : "Unavailable",
    },
    {
      label: "Unit price",
      value: item.pricing.unitPrice ? formatIskSummary(item.pricing.unitPrice) : "Missing",
      tone: item.pricing.unitPrice ? undefined : "blocking",
    },
    {
      label: "Total value",
      value: item.lineTotal ? formatIskSummary(item.lineTotal) : "Incomplete",
      tone: item.lineTotal ? undefined : "blocking",
    },
  ];
  if (item.isBuildResolved) {
    metrics.push({
      label: "Installation",
      value: item.installationCost
        ? [formatIskSummary(item.installationCost), installationFacilityName].filter(Boolean).join(" · ")
        : "Facility not selected",
    });
  }
  return { metrics };
}

export function pricingSlice(
  item: WorksheetItem,
  opts: { allowPolicyOverride: boolean; readOnly: boolean; sourceName?: string | null },
): PricingSlice {
  return {
    kind: "row",
    mode:
      item.pricing.selectionKind === "manual"
        ? "manual"
        : item.pricing.selectionKind === "market_policy"
          ? "policy"
          : "default",
    policy:
      item.pricing.effectivePolicy === "acquireQuantityFromSellOrders"
        ? "acquireQuantityFromSellOrders"
        : "highestBuy",
    manualUnitPrice: item.pricing.manualUnitPrice,
    unitPrice: item.pricing.unitPrice,
    allowPolicyOverride: opts.allowPolicyOverride && item.role === "material",
    role: item.role,
    typeId: item.typeId,
    readOnly: opts.readOnly,
    summary:
      item.pricing.selectionKind === "manual"
        ? "Manual price"
        : item.pricing.selectionKind === "market_policy"
          ? "Policy override"
          : opts.sourceName ?? "Price Source default",
  };
}

export function provenanceSlice(opts: {
  pricingContext?: { sourceName: string; sourceRevision: number; capturedAt: string } | null;
  sourceNote: string | null;
  buildId: string | null;
  recipeCurrency: ProvenanceSlice["recipeCurrency"];
}): ProvenanceSlice {
  return {
    summary: opts.pricingContext?.sourceName ?? "Source details unavailable",
    lines: opts.pricingContext
      ? [
          { label: "Revision", value: NUMBER.format(opts.pricingContext.sourceRevision) },
          { label: "Captured", value: new Date(opts.pricingContext.capturedAt).toLocaleString() },
        ]
      : [],
    note: opts.sourceNote || null,
    buildId: opts.buildId,
    recipeCurrency: opts.recipeCurrency,
  };
}
