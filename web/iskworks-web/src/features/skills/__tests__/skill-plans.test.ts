import { describe, expect, it } from "vitest";

import type { PlanEntry } from "../../../api/skills";
import { dependentsOf, planProgressLabel } from "../skill-plans";
import { catalogue, DRONE_INTERFACING, DRONES, EW_DRONES, SCOUT_DRONES } from "./fixtures";

const entry = (skillTypeId: number, level: number): PlanEntry => ({ skillTypeId, level });

const PLAN: PlanEntry[] = [
  entry(DRONES, 1),
  entry(SCOUT_DRONES, 1),
  entry(DRONES, 2),
  entry(DRONES, 3),
  entry(DRONES, 4),
  entry(DRONES, 5),
  entry(DRONE_INTERFACING, 1),
  entry(DRONE_INTERFACING, 2),
  entry(DRONE_INTERFACING, 3),
  entry(DRONE_INTERFACING, 4),
  entry(DRONE_INTERFACING, 5),
  entry(EW_DRONES, 1),
];

describe("dependentsOf", () => {
  const skills = catalogue().skills;

  it("finds every later entry that needs the removed one, transitively", () => {
    expect(dependentsOf(PLAN, 0, skills)).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
    expect(dependentsOf(PLAN, 5, skills)).toEqual([6, 7, 8, 9, 10, 11]);
    expect(dependentsOf(PLAN, 10, skills)).toEqual([11]);
  });

  it("is empty for an entry nothing needs", () => {
    expect(dependentsOf(PLAN, 1, skills)).toEqual([]);
    expect(dependentsOf(PLAN, 11, skills)).toEqual([]);
  });
});

describe("planProgressLabel", () => {
  it("reads like the client's plan cards", () => {
    expect(planProgressLabel({ percentComplete: 100, remainingSeconds: 0, estimated: false })).toBe("Completed");
    expect(planProgressLabel({ percentComplete: 45, remainingSeconds: 76 * 86_400, estimated: false })).toBe("76d 0h");
    expect(planProgressLabel({ percentComplete: 45, remainingSeconds: 3_600, estimated: true })).toBe("≈ 1h 0m");
    expect(planProgressLabel({ percentComplete: 10, remainingSeconds: null, estimated: false })).toBe("—");
  });
});
