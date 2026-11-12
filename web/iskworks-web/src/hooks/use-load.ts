import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";

import { apiMessage } from "../api/api-error";

export interface Loaded<T> {
  /** The latest successful payload; kept while a refetch runs. */
  data: T | null;
  /** Fetching for a new key, or a retry: there's nothing current to show. */
  loading: boolean;
  /** A `reload` is fetching while `data` stays shown. */
  refreshing: boolean;
  error: Error | null;
  /** `error` as the user-facing text (`apiMessage`): the backend's message,
   * with the Error ID of a redacted failure. */
  errorMessage: string | null;
  /** Fetches again, as a fresh attempt (after a failure). */
  retry: () => void;
  /** Fetches again behind the current data, without `loading`: after a
   * change the caller made. With nothing loaded yet it is a load. */
  reload: () => void;
}

interface Attempt {
  count: number;
  /** The key a `reload` was asked for; a different key is a fresh load. */
  reloadKey: string | null;
}

interface Settled {
  /** The request (key, attempt, waiting or not) this outcome belongs to. */
  request: string;
  error: Error | null;
}

/** Loads `load` whenever `key` changes (or on `retry` / `reload`), aborting
 * the previous request. A `null` load waits: nothing is fetched.
 *
 * `loading` and `error` describe the current request from the very render
 * it changes in, not one render later once the effect has run. */
export function useLoad<T>(load: ((signal: AbortSignal) => Promise<T>) | null, key: string): Loaded<T> {
  const [data, setData] = useState<T | null>(null);
  const [settled, setSettled] = useState<Settled | null>(null);
  const [attempt, setAttempt] = useState<Attempt>({ count: 0, reloadKey: null });
  const waiting = load === null;
  // Waiting is part of the identity, so a load that arrives under an
  // unchanged key still runs.
  const request = `${waiting ? "waiting" : "load"}:${attempt.count}:${key}`;
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

  const retry = useCallback(() => setAttempt((current) => ({ count: current.count + 1, reloadKey: null })), []);
  const reload = useCallback(() => setAttempt((current) => ({ count: current.count + 1, reloadKey: key })), [key]);
  const current = settled?.request === request ? settled : null;
  const pending = !waiting && current === null;
  const behindData = attempt.reloadKey === key && data !== null;
  const error = current?.error ?? null;
  return {
    data,
    loading: pending && !behindData,
    refreshing: pending && behindData,
    error,
    errorMessage: error ? apiMessage(error) : null,
    retry,
    reload,
  };
}
