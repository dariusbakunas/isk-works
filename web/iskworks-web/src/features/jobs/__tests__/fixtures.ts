import type { ActiveJobs, JobCharacter, JobHistory, JobView } from "../../../api/jobs";

export const NOW = new Date("2026-10-20T12:00:00Z");
export const KIRA = "11111111-1111-1111-1111-111111111111";
export const MOX = "22222222-2222-2222-2222-222222222222";
export const TICKET = "99999999-9999-9999-9999-999999999999";

const hours = (count: number) => new Date(NOW.getTime() + count * 3_600_000).toISOString();

export function character(overrides: Partial<JobCharacter> = {}): JobCharacter {
  return {
    connectionId: KIRA,
    eveCharacterId: 2_119_000_001,
    characterName: "Kira Vantor",
    slots: {
      manufacturing: { used: 1, max: 12 },
      reaction: { used: 1, max: 10 },
      research: { used: 0, max: 20 },
    },
    syncState: "ok",
    syncedAt: hours(-0.2),
    ...overrides,
  };
}

export function job(overrides: Partial<JobView> = {}): JobView {
  return {
    connectionId: KIRA,
    jobId: 1,
    activity: "manufacturing",
    status: "active",
    blueprintTypeId: 12_006,
    blueprintName: "Ishtar Blueprint",
    productTypeId: 12_005,
    productName: "Ishtar",
    runs: 2,
    successfulRuns: null,
    facilityId: 1_050_474_463_169,
    facilityName: "Tatara \"Forge Two\"",
    solarSystemName: "Perimeter",
    startDate: hours(-7),
    endDate: hours(3),
    pauseDate: null,
    completedDate: null,
    cost: "12400000.0000",
    ticket: { ticketId: TICKET, displayId: "TK-142" },
    ...overrides,
  };
}

export function activeJobs(overrides: Partial<ActiveJobs> = {}): ActiveJobs {
  return {
    characters: [
      character(),
      character({
        connectionId: MOX,
        eveCharacterId: 2_119_000_002,
        characterName: "Mox Halden",
        slots: { manufacturing: { used: 0, max: 12 }, reaction: { used: 0, max: 10 }, research: { used: 1, max: 25 } },
        syncState: "missingScope",
      }),
    ],
    jobs: [
      job({
        jobId: 2,
        activity: "reaction",
        status: "ready",
        blueprintName: "Fernite Carbide Reaction Formula",
        productName: "Fernite Carbide",
        runs: 40,
        endDate: hours(-1),
        cost: "1800000.0000",
        ticket: null,
      }),
      job(),
      job({
        connectionId: MOX,
        jobId: 3,
        activity: "materialEfficiencyResearch",
        status: "paused",
        blueprintName: "Revelation Blueprint",
        productName: "Revelation Blueprint",
        runs: 5,
        facilityName: "Jita IV - Moon 4 - Caldari Navy Assembly Plant",
        solarSystemName: "Jita",
        startDate: hours(-20),
        endDate: hours(52),
        pauseDate: hours(-2),
        cost: "3100000.0000",
        ticket: null,
      }),
    ],
    summary: { running: 3, ready: 1, endingWithinDay: 1 },
    slots: {
      manufacturing: { used: 1, max: 24 },
      reaction: { used: 1, max: 20 },
      research: { used: 1, max: 45 },
    },
    syncedAt: hours(-0.2),
    ...overrides,
  };
}

export function jobHistory(overrides: Partial<JobHistory> = {}): JobHistory {
  return {
    from: "2026-09-21",
    to: "2026-10-20",
    earliestObservedAt: "2026-10-20T11:00:00Z",
    kpis: {
      jobsCompleted: 3,
      runsDelivered: 6,
      installCost: "14640000.0000",
      inventionAttemptedRuns: 10,
      inventionSuccessfulRuns: 4,
    },
    weekly: [
      { weekStart: "2026-10-05", byActivity: [{ activity: "manufacturing", count: 1 }] },
      { weekStart: "2026-10-12", byActivity: [{ activity: "manufacturing", count: 1 }, { activity: "invention", count: 1 }] },
    ],
    jobs: [
      job({ jobId: 10, status: "delivered", completedDate: "2026-10-18T09:00:00Z", successfulRuns: 2 }),
      job({
        jobId: 11,
        connectionId: MOX,
        activity: "invention",
        status: "delivered",
        blueprintName: "Cerberus Blueprint",
        productName: "Cerberus Blueprint",
        runs: 10,
        successfulRuns: 4,
        completedDate: "2026-10-17T09:00:00Z",
        cost: "840000.0000",
        ticket: null,
      }),
      job({ jobId: 12, status: "cancelled", completedDate: "2026-10-12T09:00:00Z", cost: "1400000.0000", ticket: null }),
    ],
    characters: activeJobs().characters,
    ...overrides,
  };
}
