import { useCallback, useEffect, useState } from "react";

export interface Loaded<T> {
  /** The latest successful payload; kept while a refetch runs. */
  data: T | null;
  loading: boolean;
  error: Error | null;
  retry: () => void;
}

/** Loads `load` whenever `key` changes (or on `retry`), aborting the
 * previous request. A `null` load waits: nothing is fetched. */
export function useLoad<T>(load: ((signal: AbortSignal) => Promise<T>) | null, key: string): Loaded<T> {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(load !== null);
  const [error, setError] = useState<Error | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    if (load === null) {
      setLoading(false);
      return;
    }
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
        setError(reason instanceof Error ? reason : new Error("Request failed."));
        setLoading(false);
      });
    return () => controller.abort();
    // `load` is rebuilt every render; `key` is its identity.
  }, [key, attempt]);

  const retry = useCallback(() => setAttempt((current) => current + 1), []);
  return { data, loading, error, retry };
}
