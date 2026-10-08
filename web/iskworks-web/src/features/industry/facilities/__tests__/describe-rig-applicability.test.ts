import { expect, test } from "vitest";

import type { RigApplicability } from "../../../../api/sde";
import { describeRigApplicability } from "../rig-type-selector";

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
