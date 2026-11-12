import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";

export interface Loaded<T> {
  /** The latest successful payload; kept while a refetch runs. */
  data: T | null;
  loading: boolean;
  error: Error | null;
  retry: () => void;
}

interface Settled {
  /** The request (key, attempt, waiting or not) this outcome belongs to. */
  request: string;
  error: Error | null;
}

/** Loads `load` whenever `key` changes (or on `retry`), aborting the
 * previous request. A `null` load waits: nothing is fetched.
 *
 * `loading` and `error` describe the current request from the very render
 * it changes in, not one render later once the effect has run. */
export function useLoad<T>(load: ((signal: AbortSignal) => Promise<T>) | null, key: string): Loaded<T> {
  const [data, setData] = useState<T | null>(null);
  const [settled, setSettled] = useState<Settled | null>(null);
  const [attempt, setAttempt] = useState(0);
  const waiting = load === null;
  // Waiting is part of the identity, so a load that arrives under an
  // unchanged key still runs.
  const request = `${waiting ? "waiting" : "load"}:${attempt}:${key}`;
  // `load` is rebuilt every render; `request` is its identity. The effect
  // reads the latest one (updated before effects run) rather than
  // re-running for every new function.
  const latestLoad = useRef(load);
  useLayoutEffect(() => {
    latestLoad.current = load;
  });

  useEffect(() => {
    const load = latestLoad.current;
    if (load === null) return;
    const controller = new AbortController();
    load(controller.signal)
      .then((payload) => {
        if (controller.signal.aborted) return;
        setData(payload);
        setSettled({ request, error: null });
      })
      .catch((reason: unknown) => {
        if (controller.signal.aborted) return;
        setSettled({ request, error: reason instanceof Error ? reason : new Error("Request failed.") });
      });
    return () => controller.abort();
  }, [request]);

  const retry = useCallback(() => setAttempt((current) => current + 1), []);
  const current = settled?.request === request ? settled : null;
  return {
    data,
    loading: !waiting && current === null,
    error: current?.error ?? null,
    retry,
  };
}
