import { describe, expect, test } from "vitest";

import type { PlanFlow } from "../../../../../api/pi-planner";
import { defaultPlanName } from "../save-plan-button";
import { SOLUTION } from "./plan-fixture";

const NOV_14 = new Date("2026-11-14T12:00:00Z");

function sale(name: string, tier: number, unitsPerHour: string): PlanFlow {
  return { from: 0, to: null, typeId: tier * 1000 + name.length, name, tier, unitsPerHour, m3PerHour: "1", taxPerMonth: "0", crossCharacter: false };
}

function selling(flows: PlanFlow[]) {
  return { ...SOLUTION, plan: { ...SOLUTION.plan, flows } };
}

describe("defaultPlanName", () => {
  test("one product sold names it", () => {
    expect(defaultPlanName("Q-3HS5", SOLUTION, NOV_14)).toBe("Q-3HS5 · Mechanical Parts · Nov 14");
  });

  test("a mix names its top tier and counts the rest, skipping trickles", () => {
    const mixed = selling([
      sale("Precious Metals", 1, "200"),
      sale("Self-Harmonizing Power Core", 4, "0.3"),
      sale("High-Tech Transmitters", 4, "2"),
      sale("Enriched Uranium", 2, "19"),
    ]);
    expect(defaultPlanName("Q-3HS5", mixed, NOV_14)).toBe("Q-3HS5 · High-Tech Transmitters + 2 more · Nov 14");
  });

  test("without a home or sales it still has a date", () => {
    expect(defaultPlanName(null, selling([]), NOV_14)).toBe("Nov 14");
  });
});
