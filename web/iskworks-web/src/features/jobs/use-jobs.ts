import { useEffect, useState } from "react";

import { getActiveJobs, getJobHistory, type ActiveJobs, type JobHistory } from "../../api/jobs";
import { useLoad, type Loaded } from "../../hooks/use-load";

export function useActiveJobs(): Loaded<ActiveJobs> {
  return useLoad((signal) => getActiveJobs(signal), "active");
}

export function useJobHistory(from: string, to: string): Loaded<JobHistory> {
  return useLoad((signal) => getJobHistory(from, to, signal), `${from}:${to}`);
}

/** A clock that ticks every 30 s, for countdowns and progress bars. */
export function useNow(intervalMs = 30_000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), intervalMs);
    return () => window.clearInterval(timer);
  }, [intervalMs]);
  return now;
}
