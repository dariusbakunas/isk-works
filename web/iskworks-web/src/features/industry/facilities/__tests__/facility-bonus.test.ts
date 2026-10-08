import { describe, expect, test } from "vitest";

import type { FacilityProfile } from "../../../../api/industry";
import { effectiveFacilityReductionPercent, formatBonusPercent } from "../facility-bonus";

type Rig = FacilityProfile["rigs"][number];

function rig(overrides: Partial<Rig> = {}): Rig {
  return {
    slotNumber: 1,
    typeId: 1,
    typeName: "Rig",
    materialReductionPercent: "0",
    timeReductionPercent: "0",
    ...overrides,
  };
}

function profile(overrides: Partial<FacilityProfile> = {}): FacilityProfile {
  return {
    id: "f1",
    workspaceId: "w1",
    name: "Facility",
    kind: "upwellStructure",
    role: "manufacturing",
    structureId: 1,
    structureTypeId: 35_827,
    structureTypeName: "Sotiyo",
    solarSystemId: 1,
    solarSystemName: "Jita",
    securityClass: "highSec",
    materialReductionPercent: "0",
    timeReductionPercent: "0",
    jobCostReductionPercent: "0",
    facilityTaxPercent: "0",
    sccSurchargePercent: "4",
    allianceSurchargePercent: "0",
    fixedSupplementalCost: "0",
    manualSystemCostIndex: null,
    notes: "",
    rigs: [],
    archivedAt: null,
    revision: 1,
    createdAt: "2026-09-01T00:00:00Z",
    updatedAt: "2026-09-01T00:00:00Z",
    ...overrides,
  };
}

describe("effectiveFacilityReductionPercent", () => {
  test("folds the structure role bonus together with every captured rig, not just the structure field", () => {
    // Regression guard: the card must not show only
    // `materialReductionPercent` (the 1% structure role bonus) and silently
    // drop the rigs.
    const rigged = profile({
      materialReductionPercent: "1",
      rigs: [rig({ materialReductionPercent: "4.04" })],
    });

    // 1 - (1 - 0.01)(1 - 0.0404) = 0.049996 -> ~5%, and crucially > the
    // bare 1% structure field alone.
    expect(effectiveFacilityReductionPercent(rigged, "material")).toBeCloseTo(4.9996, 3);
    expect(effectiveFacilityReductionPercent(rigged, "material")).toBeGreaterThan(1);
  });

  test("a single rig with no structure bonus reports that rig's value", () => {
    const rigged = profile({ rigs: [rig({ materialReductionPercent: "5.04" })] });
    expect(effectiveFacilityReductionPercent(rigged, "material")).toBeCloseTo(5.04, 6);
  });

  test("stacks multiple rigs multiplicatively", () => {
    const rigged = profile({
      timeReductionPercent: "20",
      rigs: [rig({ timeReductionPercent: "20" }), rig({ slotNumber: 2, timeReductionPercent: "24.24" })],
    });
    // 1 - 0.8 * 0.8 * 0.7576 = 0.515136
    expect(effectiveFacilityReductionPercent(rigged, "time")).toBeCloseTo(51.5136, 4);
  });

  test("reaction profiles ignore the structure bonus fields and stack rigs only", () => {
    const reaction = profile({
      role: "reaction",
      materialReductionPercent: "5",
      timeReductionPercent: "5",
      rigs: [rig({ materialReductionPercent: "2.2" })],
    });
    expect(effectiveFacilityReductionPercent(reaction, "material")).toBeCloseTo(2.2, 6);
    expect(effectiveFacilityReductionPercent(reaction, "time")).toBe(0);
  });

  test("no bonuses at all is exactly zero", () => {
    expect(effectiveFacilityReductionPercent(profile(), "material")).toBe(0);
    expect(effectiveFacilityReductionPercent(profile(), "time")).toBe(0);
  });
});

describe("formatBonusPercent", () => {
  test("keeps meaningful precision without trailing zeros", () => {
    expect(formatBonusPercent(5)).toBe("5%");
    expect(formatBonusPercent(5.04)).toBe("5.04%");
    expect(formatBonusPercent(50.4)).toBe("50.4%");
    expect(formatBonusPercent(5.040000000001)).toBe("5.04%");
  });

  test("does not print spurious floating-point tails", () => {
    // 1 - 0.99 * 0.9496 computed in float
    expect(formatBonusPercent((1 - 0.99 * 0.9496) * 100)).toBe("5.99%");
  });
});
