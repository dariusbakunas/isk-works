import { render, screen } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import { BuildPlannerResults } from "../build-planner-results";
import type { BuildPlannerPreviewView } from "../contracts";

function preview(): BuildPlannerPreviewView {
  return {
    mode: "create",
    candidateFingerprint: "candidate-1",
    decision: {
      headline: "1 input type has a shortage.",
      supportingText: "100 total units must be acquired before production.",
      tone: "warning",
    },
    candidate: {
      estimatedMaterialCost: "4000.0000",
      expectedRevenue: "10000.0000",
      estimatedMargin: "6000.0000",
      facility: null,
      blueprint: { plannedDurationSeconds: 600 },
    } as never,
    coverage: {
      completeQuantityCoverage: false,
      materialLines: [{
        typeId: 34,
        typeName: "Tritanium",
        requiredQuantity: 1000,
        coveredQuantity: 900,
        missingQuantity: 100,
        projectedHistoricalCost: "5000.0000",
      }],
    } as never,
    warnings: [{ code: "stale", message: "Market observations are stale." }],
    validation: { fields: [], blockers: [] },
    completeness: {
      materials: "complete",
      duration: "complete",
      pricing: "complete",
      installation: "notConfigured",
      inventoryCost: "complete",
      profitability: "qualified",
    },
    profitabilityBasis: {
      includedCosts: ["projectedInventoryCost"],
      excludedCosts: ["installationCost", "marketFees"],
    },
  };
}

describe("BuildPlannerResults", () => {
  test("renders nothing for advisory-only previews", () => {
    // Advisory `warnings` live in CandidateSummary's WarningBadge, not
    // here -- with no blockers and not updating, this section is empty.
    const { container } = render(<BuildPlannerResults preview={preview()} updating={false} />);

    expect(container).toBeEmptyDOMElement();
    expect(screen.queryByText("Market observations are stale.")).not.toBeInTheDocument();
  });

  test("keeps candidate blockers visible", () => {
    const blocked = preview();
    blocked.validation.blockers = [{
      code: "missing_price",
      message: "One or more required prices are unavailable.",
    }];

    render(<BuildPlannerResults preview={blocked} updating={false} />);

    expect(screen.getByText("Candidate cannot be planned")).toBeInTheDocument();
    expect(screen.getByText("One or more required prices are unavailable.")).toBeInTheDocument();
  });
});
