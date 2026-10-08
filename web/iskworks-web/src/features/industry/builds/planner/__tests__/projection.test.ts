import { describe, expect, test } from "vitest";

import { projectCreatePreview } from "../projection";

describe("Build Planner preview projection", () => {
  test("Create projection has no current-plan comparison", () => {
    const preview = {
      candidateFingerprint: "create-1",
      decision: { headline: "Covered", supportingText: "Ready", tone: "positive" },
      candidate: { estimatedMaterialCost: "100.0000", expectedRevenue: "150.0000", estimatedMargin: "50.0000" },
      coverage: { materialLines: [] },
      warnings: [],
      validation: { fields: [], blockers: [] },
      completeness: { profitability: "qualified" },
      profitabilityBasis: { includedCosts: [], excludedCosts: ["marketFees"] },
    } as never;

    const projected = projectCreatePreview(preview);

    expect(projected.mode).toBe("create");
    expect(projected.candidateFingerprint).toBe("create-1");
  });
});
