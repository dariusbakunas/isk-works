import { afterEach, expect, test, vi } from "vitest";
import { getCalendarMilestones, type CalendarMilestone } from "../../../api/calendar";
import { ApiError } from "../../../api/workspace";

afterEach(() => {
  vi.unstubAllGlobals();
});

const RESPONSE: CalendarMilestone[] = [
  {
    kind: "industry",
    id: "industry:connection-1:42",
    connectionId: "connection-1",
    eveCharacterId: 90000001,
    characterName: "Alyx",
    title: "Hurricane",
    occursAt: "2026-10-02T14:30:00Z",
    jobId: 42,
    activity: "manufacturing",
    activityId: 1,
    status: "active",
    typeId: 24702,
    typeName: "Hurricane",
    blueprintTypeId: 24703,
    blueprintName: "Hurricane Blueprint",
    productTypeId: 24702,
    productName: "Hurricane",
    runs: 5,
    facilityId: 60003760,
    facilityName: "Jita IV - Moon 4",
    solarSystemName: "Jita",
  },
  {
    kind: "skill",
    id: "skill:connection-2:3436:5",
    connectionId: "connection-2",
    eveCharacterId: 90000002,
    characterName: "Bryn",
    title: "Drones V",
    occursAt: "2026-10-03T00:30:00Z",
    skillTypeId: 3436,
    skillName: "Drones",
    targetLevel: 5,
    queuePosition: 0,
    nextSkillTypeId: 3442,
    nextSkillName: "Drone Interfacing",
  },
];

test("requests the absolute range with credentials and preserves the milestone union", async () => {
  const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(RESPONSE), {
    status: 200,
    headers: { "content-type": "application/json" },
  }));
  vi.stubGlobal("fetch", fetchMock);

  const milestones = await getCalendarMilestones(
    new Date("2026-10-01T04:00:00.000Z"),
    new Date("2026-11-01T04:00:00.000Z"),
  );

  expect(fetchMock).toHaveBeenCalledWith(
    "/api/calendar?from=2026-10-01T04%3A00%3A00.000Z&to=2026-11-01T04%3A00%3A00.000Z",
    expect.objectContaining({
      credentials: "include",
      headers: expect.objectContaining({ Accept: "application/json" }),
    }),
  );
  const industry = milestones.find((milestone) => milestone.kind === "industry");
  const skill = milestones.find((milestone) => milestone.kind === "skill");
  expect(industry?.jobId).toBe(42);
  expect(industry?.activity).toBe("manufacturing");
  expect(skill?.skillTypeId).toBe(3436);
  expect(skill?.nextSkillName).toBe("Drone Interfacing");
});

test("surfaces a curated calendar API error", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(JSON.stringify({
    error: { code: "calendar_unavailable", message: "Calendar is temporarily unavailable." },
  }), { status: 503, headers: { "content-type": "application/json" } })));

  const request = getCalendarMilestones(
    new Date("2026-10-01T00:00:00Z"),
    new Date("2026-11-01T00:00:00Z"),
  );

  await expect(request).rejects.toBeInstanceOf(ApiError);
  await expect(request).rejects.toMatchObject({
    status: 503,
    body: { code: "calendar_unavailable", message: "Calendar is temporarily unavailable." },
  });
});
