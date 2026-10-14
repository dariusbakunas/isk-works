import type { IndustryJob } from "../../api/characters";
import { formatJobRemaining, industryJobProgress, type JobProgressState } from "../characters/industry-jobs";

export interface TicketJobSummary {
  /** 0-100, runs-weighted across the matched jobs. */
  percent: number;
  /** "Delivered" once every job is, "Ready" once every job is done, "Paused" if any is, else time until the last one finishes. */
  label: string;
  state: JobProgressState;
  jobCount: number;
  /** Runs covered by the matched jobs -- may be below the ticket's runs while it's only partly started. */
  jobRuns: number;
}

// One bar for a ticket that may run as several jobs (a multi-BPC split, or
// runs spread across slots by hand): overall completion weighted by runs,
// and the remaining time of whichever job finishes last.
export function summarizeTicketJobs(jobs: IndustryJob[], nowMs: number): TicketJobSummary | null {
  if (jobs.length === 0) return null;
  const progresses = jobs.map((job) => ({ job, progress: industryJobProgress(job, nowMs) }));
  const jobRuns = jobs.reduce((total, job) => total + Math.max(job.runs, 0), 0);
  const weighted = progresses.reduce(
    (total, { job, progress }) => total + progress.fraction * Math.max(job.runs, 0),
    0,
  );
  const fraction = jobRuns > 0 ? weighted / jobRuns : 0;
  const state: JobProgressState = progresses.some(({ progress }) => progress.state === "paused")
    ? "paused"
    : progresses.every(({ progress }) => progress.state === "ready")
      ? "ready"
      : "running";
  const remainingMs = Math.max(...progresses.map(({ progress }) => progress.remainingMs));
  const allDelivered = jobs.every((job) => job.status === "delivered");
  const label =
    state === "paused"
      ? "Paused"
      : allDelivered
        ? "Delivered"
        : state === "ready"
          ? "Ready"
          : formatJobRemaining(remainingMs);
  return { percent: Math.round(fraction * 100), label, state, jobCount: jobs.length, jobRuns };
}
