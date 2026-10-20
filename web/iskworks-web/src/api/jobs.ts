import type { IndustryActivity } from "./characters";
import { jsonRequest } from "./json-request";

/** A job's status as shown: an `active` job past its end is already `ready`. */
export type JobStatus = "active" | "paused" | "ready" | "delivered" | "cancelled" | "reverted";
export type JobsSyncState = "ok" | "stale" | "missingScope" | "failed" | "neverSynced";

export interface SlotUsage {
  used: number;
  /** `null` until the character's skills have synced. */
  max: number | null;
}

export interface JobSlots {
  manufacturing: SlotUsage;
  reaction: SlotUsage;
  research: SlotUsage;
}

export interface JobCharacter {
  connectionId: string;
  eveCharacterId: number;
  characterName: string;
  slots: JobSlots;
  syncState: JobsSyncState;
  syncedAt: string | null;
}

export interface JobTicketLink {
  ticketId: string;
  displayId: string;
}

/** Money is a decimal string; never do arithmetic on it in floats. */
export interface JobView {
  connectionId: string;
  jobId: number;
  activity: IndustryActivity;
  status: JobStatus;
  blueprintTypeId: number;
  blueprintName: string | null;
  productTypeId: number | null;
  productName: string | null;
  runs: number;
  /** Finished jobs only: delivered runs (for invention, the successful ones). */
  successfulRuns: number | null;
  facilityId: number;
  facilityName: string | null;
  solarSystemName: string | null;
  startDate: string | null;
  endDate: string | null;
  pauseDate: string | null;
  completedDate: string | null;
  cost: string | null;
  ticket: JobTicketLink | null;
}

export interface ActiveJobs {
  characters: JobCharacter[];
  /** Soonest-ending first. */
  jobs: JobView[];
  summary: { running: number; ready: number; endingWithinDay: number };
  slots: JobSlots;
  syncedAt: string | null;
}

export interface JobHistory {
  from: string;
  to: string;
  earliestObservedAt: string | null;
  kpis: {
    jobsCompleted: number;
    runsDelivered: number;
    installCost: string;
    inventionAttemptedRuns: number;
    inventionSuccessfulRuns: number;
  };
  weekly: Array<{ weekStart: string; byActivity: Array<{ activity: IndustryActivity; count: number }> }>;
  /** Newest first. */
  jobs: JobView[];
  characters: JobCharacter[];
}

const request = jsonRequest({
  unavailable: "ISK Works API is unavailable.",
  failed: "Jobs request failed.",
});

export function getActiveJobs(signal?: AbortSignal): Promise<ActiveJobs> {
  return request("/api/jobs", { signal });
}

export function getJobHistory(from: string, to: string, signal?: AbortSignal): Promise<JobHistory> {
  return request(`/api/jobs/history?${new URLSearchParams({ from, to })}`, { signal });
}
