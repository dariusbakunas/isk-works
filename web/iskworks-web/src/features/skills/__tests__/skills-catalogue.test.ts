import { describe, expect, it } from "vitest";

import {
  filterSkills,
  prerequisiteTree,
  requiredForIndex,
  stateLabel,
  timeLabel,
  type SkillFilter,
} from "../skills-catalogue";
import { catalogue, DRONE_INTERFACING, DRONES, EW_DRONES, LEADERSHIP, progress, SCOUT_DRONES } from "./fixtures";

const ids = (filter: SkillFilter, options: { groupId?: number | null; search?: string } = {}) =>
  filterSkills(catalogue().skills, { filter, groupId: options.groupId ?? null, search: options.search ?? "" }).map(
    (skill) => skill.typeId,
  );

describe("filterSkills", () => {
  it("keeps catalogue order for all skills", () => {
    expect(ids("all")).toEqual([DRONE_INTERFACING, DRONES, EW_DRONES, SCOUT_DRONES, LEADERSHIP]);
  });

  it("matches the client's filters", () => {
    expect(ids("my")).toEqual([DRONE_INTERFACING, DRONES, LEADERSHIP]);
    expect(ids("canTrain")).toEqual([DRONE_INTERFACING, LEADERSHIP]);
    expect(ids("havePrerequisites")).toEqual([SCOUT_DRONES]);
  });

  it("narrows by group and name", () => {
    expect(ids("all", { groupId: 258 })).toEqual([LEADERSHIP]);
    expect(ids("all", { search: "  drone INTER" })).toEqual([DRONE_INTERFACING, EW_DRONES]);
  });

  it("keeps every skill without a character overlay", () => {
    const skills = catalogue().skills.map((skill) => ({ ...skill, progress: null }));
    expect(filterSkills(skills, { filter: "all", groupId: null, search: "" })).toHaveLength(5);
    expect(filterSkills(skills, { filter: "my", groupId: null, search: "" })).toHaveLength(0);
  });
});

describe("prerequisiteTree", () => {
  it("nests prerequisites and marks each met at the active level", () => {
    const skills = catalogue().skills;
    const tree = prerequisiteTree(skills.find((skill) => skill.typeId === EW_DRONES)!, skills);

    expect(tree).toEqual([
      {
        skillTypeId: DRONE_INTERFACING,
        name: "Drone Interfacing",
        level: 5,
        met: false,
        children: [{ skillTypeId: DRONES, name: "Drones", level: 5, met: true, children: [] }],
      },
    ]);
  });
});

describe("requiredForIndex", () => {
  it("lists the skills that need each skill, by name", () => {
    const index = requiredForIndex(catalogue().skills);

    expect(index.get(DRONES)?.map((entry) => entry.name)).toEqual(["Drone Interfacing", "Scout Drone Operation"]);
    expect(index.get(DRONE_INTERFACING)?.map((entry) => [entry.name, entry.level])).toEqual([
      ["EW Drone Interfacing", 5],
    ]);
    expect(index.get(LEADERSHIP)).toBeUndefined();
  });
});

describe("labels", () => {
  it("names states the way the client does", () => {
    expect(stateLabel(progress({ state: "max" }))).toBe("Max");
    expect(stateLabel(progress({ state: "queued", queuedToLevel: 4 }))).toBe("Queued → IV");
    expect(stateLabel(progress({ state: "canInject" }))).toBe("Can inject");
    expect(stateLabel(progress({ state: "omegaOnly" }))).toBe("Omega only");
  });

  it("formats time to the next level", () => {
    expect(timeLabel(progress({ state: "max" }))).toBe("Max");
    expect(timeLabel(progress({ state: "canTrain", secondsToNextLevel: 13_277 }))).toBe("3h 41m");
    expect(timeLabel(progress({ state: "canTrain", secondsToNextLevel: 13_277, estimated: true }))).toBe("≈ 3h 41m");
    expect(timeLabel(progress({ state: "canInject" }))).toBe("—");
  });
});
