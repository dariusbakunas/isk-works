import { expect, test } from "vitest";

import type { RigApplicability } from "../../../../api/sde";
import { describeRigApplicability, describeRigScience } from "../rig-type-selector";

const filter = (filterId: number, name: string): RigApplicability["material"] => ({
  filterId,
  name,
  categoryIds: [],
  groupIds: [],
});

test("unrestricted rig affects every job of its activity", () => {
  expect(describeRigApplicability({ material: null, time: null }, "manufacturing")).toBe(
    "Affects all manufacturing jobs",
  );
  expect(describeRigApplicability({ material: null, time: null }, "reaction")).toBe(
    "Affects all reaction jobs",
  );
});

test("a single shared filter collapses to one 'Affects: <name>' line", () => {
  expect(
    describeRigApplicability(
      { material: filter(3, "Ships"), time: filter(3, "Ships") },
      "manufacturing",
    ),
  ).toBe("Affects: Ships");
});

test("split material/time filters are reported separately, with 'all jobs' for a null side", () => {
  expect(
    describeRigApplicability(
      { material: filter(5, "Small T1 Ships"), time: null },
      "manufacturing",
    ),
  ).toBe("Material bonus: Small T1 Ships · Time bonus: all manufacturing jobs");
  expect(
    describeRigApplicability(
      { material: filter(18, "Composite Reactions"), time: filter(16, "Hybrid Reactions") },
      "reaction",
    ),
  ).toBe("Material bonus: Composite Reactions · Time bonus: Hybrid Reactions");
});

const science = (invention: [string, string], copying: [string, string]) => ({
  inventionCostReductionPercent: invention[0],
  inventionTimeReductionPercent: invention[1],
  copyingCostReductionPercent: copying[0],
  copyingTimeReductionPercent: copying[1],
});

test("a lab rig names the science activities it has a bonus for", () => {
  expect(describeRigScience(science(["25.2", "0"], ["0", "0"]))).toBe("invention");
  expect(describeRigScience(science(["0", "0"], ["0", "50.4"]))).toBe("copying");
  expect(describeRigScience(science(["25.2", "50.4"], ["25.2", "50.4"]))).toBe("invention and copying");
});

test("a rig with no science bonus has no science line", () => {
  expect(describeRigScience(science(["0", "0"], ["0", "0"]))).toBeNull();
});
