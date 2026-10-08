import { render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import type { CreateBuildPlanPreview } from "../../../../../api/industry";
import { CandidateSummary } from "../candidate-summary";

function preview(): CreateBuildPlanPreview {
  return {
    candidate: {
      estimatedMaterialCost: "140540690.6700",
      expectedRevenue: "158600000.0000",
      estimatedMargin: "2567257.0900",
    },
    calculationEvidence: {
      profitMarginPercent: "1.6",
    },
    decision: {
      headline: "3 input types have shortages.",
      supportingText: "226 total units must be acquired before production.",
      tone: "warning",
    },
    worksheet: {
      summary: {
        installationCost: "15492052.2400",
        totalCost: "156032742.9100",
      },
    },
  } as unknown as CreateBuildPlanPreview;
}

describe("CandidateSummary", () => {
  test("renders candidate economics and decision once", () => {
    render(<CandidateSummary preview={preview()} />);

    for (const label of [
      "Material cost",
      "Installation",
      "Total cost",
      "Revenue",
      "Estimated profit",
      "Profit margin",
    ]) {
      expect(screen.getByText(label)).toBeInTheDocument();
    }
    expect(screen.getByText("140,540,690.67 ISK")).toBeInTheDocument();
    expect(screen.getByText("15,492,052.24 ISK")).toBeInTheDocument();
    expect(screen.getByText("156,032,742.91 ISK")).toBeInTheDocument();
    expect(screen.getAllByText("3 input types have shortages.")).toHaveLength(1);
    expect(screen.getByText("226 total units must be acquired before production.")).toBeInTheDocument();
  });

  test("a missing material price shows material and total cost as Incomplete, never 0", () => {
    const incomplete = preview();
    Object.assign(incomplete.candidate, { estimatedMaterialCost: "0", missingPriceCount: 2 });
    render(<CandidateSummary preview={incomplete} />);

    expect(screen.queryByText("0 ISK")).toBeNull();
    expect(screen.queryByText("156,032,742.91 ISK")).toBeNull();
    // Installation is still known.
    expect(screen.getByText("15,492,052.24 ISK")).toBeInTheDocument();
    expect(screen.getAllByText("Incomplete").length).toBeGreaterThanOrEqual(2);
  });

  test("offers a jump to Logistics from a shortage decision when asked", async () => {
    const onShowLogistics = vi.fn();
    render(<CandidateSummary onShowLogistics={onShowLogistics} preview={preview()} />);
    screen.getByRole("button", { name: "View logistics" }).click();
    expect(onShowLogistics).toHaveBeenCalledTimes(1);
  });

  test("keeps all values explicitly incomplete without a preview", () => {
    render(<CandidateSummary preview={null} />);

    expect(screen.getAllByText("Incomplete")).toHaveLength(6);
  });
});
