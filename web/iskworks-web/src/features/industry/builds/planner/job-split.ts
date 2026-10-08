/**
 * How many industry jobs `requiredRuns` splits into when one blueprint copy
 * licenses at most `licensedRuns` -- mirrors the planner's `JobSplit`
 * (`crates/iskworks-core/src/job_split.rs`).
 */
export function jobCount(requiredRuns: number, licensedRuns: number) {
  return Math.ceil(requiredRuns / licensedRuns);
}

/** "4 jobs" when a copy's licensed runs split the runs, otherwise `null`
 * (an original, unknown runs, or one job). */
export function jobSplitLabel(requiredRuns: number, licensedRuns: number | null | undefined) {
  if (licensedRuns == null || licensedRuns <= 0 || requiredRuns <= licensedRuns) return null;
  return `${jobCount(requiredRuns, licensedRuns)} jobs`;
}

/** `summary` plus " · 4 jobs" when the copy splits the runs. */
export function withJobSplit(summary: string, requiredRuns: number, licensedRuns: number | null | undefined) {
  const label = jobSplitLabel(requiredRuns, licensedRuns);
  return label ? `${summary} · ${label}` : summary;
}
