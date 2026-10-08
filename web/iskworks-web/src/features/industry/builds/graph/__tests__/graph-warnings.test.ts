import { describe, expect, it } from "vitest";

import type { GraphWarning } from "../../../../../api/industry";
import { isCardWarning, warningChipLabel, warningsByNodeId } from "../graph-warnings";

const w = (graphNodeId: string, code: GraphWarning["code"], message = "m"): GraphWarning => ({
  graphNodeId,
  code,
  message,
});

describe("warningsByNodeId", () => {
  it("groups warnings by their target graphNodeId", () => {
    const map = warningsByNodeId([
      w("build:a", "runsDiverged"),
      w("build:a", "linkedBuildUnresolved"),
      w("buy:root:1", "staleRecipe"),
    ]);
    expect(map.get("build:a")?.map((x) => x.code)).toEqual([
      "runsDiverged",
      "linkedBuildUnresolved",
    ]);
    expect(map.get("buy:root:1")).toHaveLength(1);
    expect(map.has("build:missing")).toBe(false);
  });

  it("is empty for no warnings", () => {
    expect(warningsByNodeId([]).size).toBe(0);
  });
});

describe("isCardWarning", () => {
  it("promotes topology warnings to the card, keeps pricing ones off it", () => {
    expect(isCardWarning("runsDiverged")).toBe(true);
    expect(isCardWarning("linkedBuildUnresolved")).toBe(true);
    expect(isCardWarning("marketPriceUnavailable")).toBe(false);
    expect(isCardWarning("staleRecipe")).toBe(false);
  });
});

describe("warningChipLabel", () => {
  it("names a single card warning and counts multiple", () => {
    expect(warningChipLabel([w("n", "runsDiverged")])).toBe("Runs diverged");
    expect(
      warningChipLabel([w("n", "runsDiverged"), w("n", "linkedBuildUnresolved")]),
    ).toBe("2 warnings");
  });

  it("returns null when nothing is card-worthy", () => {
    expect(warningChipLabel([w("n", "marketPriceUnavailable")])).toBeNull();
    expect(warningChipLabel([])).toBeNull();
  });
});
