import { describe, expect, it } from "vitest";

import {
  prerequisiteSourcePresentation,
  type PrerequisiteSourceInput,
} from "../prerequisite-source";

function prereq(overrides: Partial<PrerequisiteSourceInput> = {}): PrerequisiteSourceInput {
  return {
    kind: "buy",
    requiredQuantity: 100,
    freshQuantity: 100,
    fulfillmentScope: "missing",
    ...overrides,
  };
}

describe("prerequisiteSourcePresentation", () => {
  it("A. Buy, fully covered by frozen inventory reuse -> Use Inventory (positive)", () => {
    expect(
      prerequisiteSourcePresentation(prereq({ kind: "buy", freshQuantity: 0 })),
    ).toEqual({ label: "Use Inventory", tone: "positive" });
  });

  it("B. Buy, partially covered -> Buy · Missing (60) (warning)", () => {
    expect(
      prerequisiteSourcePresentation(prereq({ kind: "buy", freshQuantity: 60 })),
    ).toEqual({ label: "Buy · Missing (60)", tone: "warning" });
  });

  it("C. Buy, no inventory coverage -> Buy (Buy tone)", () => {
    expect(
      prerequisiteSourcePresentation(prereq({ kind: "buy", freshQuantity: 100 })),
    ).toEqual({ label: "Buy", tone: "primary" });
  });

  it("D. Buy, Full scope -> Buy even though nothing is left fresh-outstanding numerically", () => {
    expect(
      prerequisiteSourcePresentation(
        prereq({ kind: "buy", freshQuantity: 100, fulfillmentScope: "full" }),
      ),
    ).toEqual({ label: "Buy", tone: "primary" });
  });

  it("D2. Full scope forces fresh <Kind> even if a stale freshQuantity reads 0", () => {
    // Defensive: a Full-scoped row was frozen with no reuse, so it must
    // never present as "Use Inventory" regardless of the persisted number.
    expect(
      prerequisiteSourcePresentation(
        prereq({ kind: "buy", freshQuantity: 0, fulfillmentScope: "full" }),
      ),
    ).toEqual({ label: "Buy", tone: "primary" });
  });

  it("E. Build, fully covered -> Use Inventory (positive)", () => {
    expect(
      prerequisiteSourcePresentation(prereq({ kind: "build", freshQuantity: 0 })),
    ).toEqual({ label: "Use Inventory", tone: "positive" });
  });

  it("F. Build, partially covered -> Build · Missing (60) (warning)", () => {
    expect(
      prerequisiteSourcePresentation(prereq({ kind: "build", freshQuantity: 60 })),
    ).toEqual({ label: "Build · Missing (60)", tone: "warning" });
  });

  it("G. Reaction, fully covered -> Use Inventory (positive)", () => {
    expect(
      prerequisiteSourcePresentation(prereq({ kind: "react", freshQuantity: 0 })),
    ).toEqual({ label: "Use Inventory", tone: "positive" });
  });

  it("H. Reaction, partially covered -> React · Missing (60) (warning)", () => {
    expect(
      prerequisiteSourcePresentation(prereq({ kind: "react", freshQuantity: 60 })),
    ).toEqual({ label: "React · Missing (60)", tone: "warning" });
  });

  it("I. Reaction, no inventory coverage -> React (Reaction tone)", () => {
    expect(
      prerequisiteSourcePresentation(prereq({ kind: "react", freshQuantity: 100 })),
    ).toEqual({ label: "React", tone: "reaction" });
  });

  it("uses compact quantities for large outstanding amounts, matching the Worksheet", () => {
    expect(
      prerequisiteSourcePresentation(
        prereq({ kind: "buy", requiredQuantity: 12_000, freshQuantity: 11_500 }),
      ).label,
    ).toBe("Buy · Missing (11.5K)");
  });
});
