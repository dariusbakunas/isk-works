import type { CalendarMilestone } from "../../../api/calendar";

const industry = (
  jobId: number,
  occursAt: string,
  title: string,
  overrides: Partial<Extract<CalendarMilestone, { kind: "industry" }>> = {},
): Extract<CalendarMilestone, { kind: "industry" }> => ({
  kind: "industry",
  id: `industry:0aa719e8-f94f-4ef1-8f47-cf649d3b2d95:${jobId}`,
  connectionId: "0aa719e8-f94f-4ef1-8f47-cf649d3b2d95",
  eveCharacterId: 90000001,
  characterName: "Alix Morgan",
  title,
  occursAt,
  jobId,
  activity: "manufacturing",
  activityId: 1,
  status: "active",
  typeId: 24702,
  typeName: title,
  blueprintTypeId: 24703,
  blueprintName: `${title} Blueprint`,
  productTypeId: 24702,
  productName: title,
  runs: 5,
  facilityId: 60003760,
  facilityName: "Jita IV - Moon 4 - Caldari Navy Assembly Plant",
  solarSystemName: "Jita",
  ...overrides,
});

const skill = (
  skillTypeId: number,
  occursAt: string,
  skillName: string,
  targetLevel: number,
  overrides: Partial<Extract<CalendarMilestone, { kind: "skill" }>> = {},
): Extract<CalendarMilestone, { kind: "skill" }> => ({
  kind: "skill",
  id: `skill:874c445a-67f4-4368-9028-b785770a2e52:${skillTypeId}:${targetLevel}`,
  connectionId: "874c445a-67f4-4368-9028-b785770a2e52",
  eveCharacterId: 90000002,
  characterName: "Bryn of an Unnecessarily Long Character Name",
  title: `${skillName} ${["0", "I", "II", "III", "IV", "V"][targetLevel]}`,
  occursAt,
  skillTypeId,
  skillName,
  targetLevel,
  queuePosition: 0,
  nextSkillTypeId: 3442,
  nextSkillName: "Drone Interfacing",
  ...overrides,
});

/** Deterministic October 2026 data for unit, component, and browser tests. */
export const CALENDAR_MILESTONE_FIXTURES: CalendarMilestone[] = [
  industry(101, "2026-09-20T18:00:00Z", "Past Hurricane batch"),
  industry(102, "2026-10-01T00:30:00Z", "UTC midnight-crossing Muninn batch"),
  industry(103, "2026-10-02T12:00:00Z", "Scimitar"),
  skill(3436, "2026-10-02T13:15:00Z", "Drones", 5),
  industry(104, "2026-10-02T14:30:00Z", "An extremely long production milestone name for overflow coverage"),
  skill(3442, "2026-10-02T17:45:00Z", "Drone Interfacing", 4, { queuePosition: 1 }),
  industry(105, "2026-10-02T19:00:00Z", "Vagabond"),
  skill(22536, "2026-10-15T16:00:00Z", "Mining Director", 3, {
    connectionId: "3ad88831-c85d-4c97-9a7c-3f13d75f2a6b",
    eveCharacterId: 90000003,
    characterName: "Cass",
    nextSkillTypeId: null,
    nextSkillName: null,
  }),
  industry(106, "2026-10-31T23:30:00Z", "Future Sleipnir batch", {
    activity: "reaction",
    activityId: 11,
  }),
];

