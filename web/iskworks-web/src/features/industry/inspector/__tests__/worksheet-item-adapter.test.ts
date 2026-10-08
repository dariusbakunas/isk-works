import { describe, expect, it } from "vitest";

import type { WorksheetItem } from "../../../../api/industry";
import { formatIskSummary } from "../../../../components/money";
import { worksheetItemToInspectorModel } from "../adapters/worksheet-item";

function item(overrides: Partial<WorksheetItem> = {}): WorksheetItem {
  return {
    typeId: 34,
    typeName: "Tritanium",
    role: "material",
    requiredQuantity: 4800,
    availableQuantity: 3600,
    coveredQuantity: 3600,
    missingQuantity: 1200,
    coveragePercentage: "75.00",
    projectedInventoryCost: "1000.0000",
    pricing: {
      selectionKind: "default",
      effectivePolicy: "highestBuy",
      unitPrice: "1840.0000",
      manualUnitPrice: null,
      missing: false,
      sourceNote: "",
    },
    lineTotal: "8832000.0000",
    contributions: [],
    isBuildResolved: false,
    installationCost: null,
    ...overrides,
  };
}

describe("worksheetItemToInspectorModel", () => {
  it("keeps coverage / value / provenance for a raw BUY material", () => {
    const model = worksheetItemToInspectorModel(item(), {
      pricingContext: { sourceName: "Jita 4-4 · Buy", sourceRevision: 12, capturedAt: "2026-09-01T00:00:00Z" },
    });

    expect(model.identity.kind).toBe("buyMaterial");
    expect(model.identity.kindLabel).toBe("BUY MATERIAL");
    expect(model.identity.summary).toBe("Required 4,800 · 1,200 short");

    expect(model.coverage?.percentage).toBe(75);
    expect(model.coverage?.hasShortage).toBe(true);
    expect(model.coverage?.summary).toBe("75.00% covered · 1,200 short");
    expect(model.coverage?.metrics.map((metric) => metric.label)).toEqual([
      "Required",
      "Available",
      "Covered",
      "Shortage",
    ]);

    expect(model.value?.metrics.find((metric) => metric.label === "Unit price")?.value).toBe(formatIskSummary("1840.0000"));
    expect(model.value?.metrics.some((metric) => metric.label === "Installation")).toBe(false);
    expect(model.provenance?.summary).toBe("Jita 4-4 · Buy");
    expect(model.provenance?.lines.map((line) => line.label)).toEqual(["Revision", "Captured"]);
    expect(model.warnings).toHaveLength(0);
  });

  it("marks a build-resolved row as a linked build and adds an installation metric", () => {
    const model = worksheetItemToInspectorModel(
      item({ isBuildResolved: true, installationCost: "4200000.0000" }),
    );
    expect(model.identity.kind).toBe("linkedBuild");
    expect(model.identity.kindLabel).toBe("LINKED BUILD");
    expect(model.value?.metrics.find((metric) => metric.label === "Installation")?.value).toBe(formatIskSummary("4200000.0000"));
  });

  it("classifies an output row and summarises it as producing", () => {
    const model = worksheetItemToInspectorModel(item({ role: "output", missingQuantity: 0 }));
    expect(model.identity.kind).toBe("outputItem");
    expect(model.identity.summary).toBe("Producing 4,800");
  });

  it("raises blocking warnings for a row with no price / no value", () => {
    const model = worksheetItemToInspectorModel(
      item({
        pricing: { ...item().pricing, unitPrice: null, missing: true },
        lineTotal: null,
      }),
    );
    expect(model.warnings.map((warning) => warning.label)).toEqual([
      "Pricing incomplete",
      "Value incomplete",
    ]);
    expect(model.warnings.every((warning) => warning.tone === "blocking")).toBe(true);
  });

  it("surfaces parent contributions as the Used By slice", () => {
    const model = worksheetItemToInspectorModel(
      item({
        contributions: [
          { parentTypeId: null, parentTypeName: "Thanatos", quantity: 60 },
          { parentTypeId: 999, parentTypeName: "Rifter Hull Section", quantity: 40 },
        ],
      }),
    );
    expect(model.usedBy?.entries).toEqual([
      { typeId: null, name: "Thanatos", quantity: 60 },
      { typeId: 999, name: "Rifter Hull Section", quantity: 40 },
    ]);
  });
});
