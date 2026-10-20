import { useCallback, useEffect, useState } from "react";

import { getActiveJobs, getJobHistory, type ActiveJobs, type JobHistory } from "../../api/jobs";

interface Loaded<T> {
  /** The latest successful payload; kept while a refetch runs. */
  data: T | null;
  loading: boolean;
  error: Error | null;
  retry: () => void;
}

function useLoad<T>(load: (signal: AbortSignal) => Promise<T>, key: string): Loaded<T> {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    const controller = new AbortController();
    setLoading(true);
    setError(null);
    load(controller.signal)
      .then((payload) => {
        if (controller.signal.aborted) return;
        setData(payload);
        setLoading(false);
      })
      .catch((reason: unknown) => {
        if (controller.signal.aborted) return;
        setError(reason instanceof Error ? reason : new Error("Jobs request failed."));
        setLoading(false);
      });
    return () => controller.abort();
    // `load` is rebuilt every render; `key` is its identity.
  }, [key, attempt]);

  const retry = useCallback(() => setAttempt((current) => current + 1), []);
  return { data, loading, error, retry };
}

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
