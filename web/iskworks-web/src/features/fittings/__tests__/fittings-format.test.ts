import { describe, expect, test } from "vitest";

import type { FitSummary } from "../../../api/fittings";
import {
  capacitorLabel,
  fitTree,
  formatDistance,
  formatHeadline,
  formatPercent,
  formatStat,
  pilotsByStatus,
  rackLabel,
  stateLabel,
} from "../fittings-format";

const summary = (id: string, ship: [number, string, string], pilots: FitSummary["pilots"] = []): FitSummary => ({
  id,
  name: `Fit ${id}`,
  ship: { typeId: ship[0], name: ship[1], groupName: ship[2] },
  itemCount: 1,
  updatedAt: "2026-10-26T00:00:00Z",
  pilots,
});

describe("formatting", () => {
  test("stats read compactly with their unit", () => {
    expect(formatStat(45310.4)).toBe("45,310");
    expect(formatStat(136.94, 1)).toBe("136.9");
    expect(formatDistance(66100)).toBe("66.1 km");
    expect(formatDistance(950)).toBe("950 m");
    expect(formatPercent(0.735)).toBe("73.5%");
    expect(formatPercent(0.5)).toBe("50%");
  });

  test("the capacitor is stable at a level or depletes after a time", () => {
    expect(capacitorLabel({ kind: "stable", level: 0.735 })).toBe("Stable 73.5%");
    expect(capacitorLabel({ kind: "depletes", seconds: 160 })).toBe("Depletes in 2:40");
    expect(capacitorLabel({ kind: "depletes", seconds: 3725 })).toBe("Depletes in 1:02:05");
  });

  test("headlines say what a module does, and nothing when it has none", () => {
    expect(formatHeadline({ kind: "dps", value: 116.9 })).toBe("116.9 DPS");
    expect(formatHeadline({ kind: "shieldRepair", value: 42.5 })).toBe("42.5 HP/s");
    expect(formatHeadline({ kind: "resist", value: 32.5 })).toBe("+32.5% resists");
    expect(formatHeadline({ kind: "velocity", value: 505 })).toBe("+505% velocity");
    expect(formatHeadline({ kind: "shieldHp", value: 1875 })).toBe("+1,875 shield HP");
    expect(formatHeadline({ kind: "armorHp", value: 6000 })).toBe("+6,000 armor HP");
    expect(formatHeadline({ kind: "capacitor", value: 3200 })).toBe("+3,200 GJ");
    expect(formatHeadline(null)).toBe("—");
  });

  test("racks and states have readable names", () => {
    expect(rackLabel("high")).toBe("High slots");
    expect(rackLabel("drone_bay")).toBe("Drones");
    expect(stateLabel("overload")).toBe("Overloaded");
  });
});

describe("the hull tree", () => {
  const fits = [
    summary("b", [12731, "Bustard", "Deep Space Transport"]),
    summary("a", [12731, "Bustard", "Deep Space Transport"]),
    summary("r", [641, "Megathron", "Battleship"]),
    summary("x", [999, "", ""]),
  ];

  test("groups fits by ship group and ship, alphabetically, with counts", () => {
    const tree = fitTree(fits);
    expect(tree.map((group) => [group.name, group.fitCount])).toEqual([
      ["Battleship", 1],
      ["Deep Space Transport", 2],
      ["Unknown group", 1],
    ]);
    const bustard = tree[1].ships[0];
    expect(bustard.name).toBe("Bustard");
    expect(bustard.fits.map((fit) => fit.id)).toEqual(["a", "b"]);
  });

  test("fits whose ship the SDE lacks are named by type id", () => {
    expect(fitTree(fits)[2].ships[0].name).toBe("Type 999");
  });
});

describe("flyability chips", () => {
  test("characters are split by what they can do, unknown ones left out", () => {
    const fit = summary("a", [12731, "Bustard", "Deep Space Transport"], [
      { connectionId: "1", name: "Mira", status: "canUseAll" },
      { connectionId: "2", name: "Aura", status: "canFlyHull" },
      { connectionId: "3", name: "Fresh", status: "cannot" },
      { connectionId: "4", name: "Scout", status: null },
    ]);
    const split = pilotsByStatus(fit.pilots);
    expect(split.canUseAll.map((pilot) => pilot.name)).toEqual(["Mira"]);
    expect(split.hullOnly.map((pilot) => pilot.name)).toEqual(["Aura"]);
    expect(split.anyone).toBe(true);
    expect(pilotsByStatus([{ connectionId: "3", name: "Fresh", status: "cannot" }]).anyone).toBe(false);
  });
});

