import { describe, expect, test } from "vitest";

import type { IndustryJob } from "../../../api/characters";
import { summarizeTicketJobs } from "../ticket-job-progress";

const NOW = Date.parse("2026-10-13T12:00:00Z");
const HOUR = 3_600_000;

function job(overrides: Partial<IndustryJob> = {}): IndustryJob {
  return {
    jobId: 1,
    activity: "reaction",
    activityId: 9,
    status: "active",
    blueprintTypeId: 46_166,
    blueprintName: null,
    productTypeId: 16_670,
    productName: null,
    runs: 10,
    facilityId: 1,
    facilityName: null,
    solarSystemName: null,
    startDate: new Date(NOW - 5 * HOUR).toISOString(),
    endDate: new Date(NOW + 5 * HOUR).toISOString(),
    ...overrides,
  };
}

describe("summarizeTicketJobs", () => {
  test("no jobs -> no summary", () => {
    expect(summarizeTicketJobs([], NOW)).toBeNull();
  });

  test("one running job reports its progress and remaining time", () => {
    expect(summarizeTicketJobs([job()], NOW)).toEqual({
      percent: 50,
      label: "5h 0m",
      state: "running",
      jobCount: 1,
      jobRuns: 10,
    });
  });

  test("several jobs weight progress by runs and wait for the last to finish", () => {
    const done = job({ jobId: 1, runs: 30, endDate: new Date(NOW - HOUR).toISOString() });
    const fresh = job({
      jobId: 2,
      runs: 10,
      startDate: new Date(NOW).toISOString(),
      endDate: new Date(NOW + 8 * HOUR).toISOString(),
    });
    const summary = summarizeTicketJobs([done, fresh], NOW);
    expect(summary?.percent).toBe(75);
    expect(summary?.label).toBe("8h 0m");
    expect(summary?.state).toBe("running");
    expect(summary?.jobRuns).toBe(40);
  });

  test("ready only once every job is done", () => {
    const ended = { endDate: new Date(NOW - HOUR).toISOString() };
    const summary = summarizeTicketJobs([job({ jobId: 1, ...ended }), job({ jobId: 2, ...ended })], NOW);
    expect(summary?.state).toBe("ready");
    expect(summary?.label).toBe("Ready");
    expect(summary?.percent).toBe(100);
  });

  test("any paused job marks the ticket paused", () => {
    const summary = summarizeTicketJobs([job({ jobId: 1 }), job({ jobId: 2, status: "paused" })], NOW);
    expect(summary?.state).toBe("paused");
    expect(summary?.label).toBe("Paused");
  });
});

describe("summarizeTicketJobs with delivered jobs", () => {
  test("delivered jobs count as done; all delivered reads Delivered", () => {
    const delivered = job({ jobId: 1, status: "delivered", endDate: new Date(NOW - HOUR).toISOString() });
    const ready = job({ jobId: 2, endDate: new Date(NOW - HOUR).toISOString() });
    expect(summarizeTicketJobs([delivered, ready], NOW)?.label).toBe("Ready");
    const all = summarizeTicketJobs([delivered, { ...delivered, jobId: 3 }], NOW);
    expect(all?.label).toBe("Delivered");
    expect(all?.percent).toBe(100);
    expect(all?.state).toBe("ready");
  });
});
