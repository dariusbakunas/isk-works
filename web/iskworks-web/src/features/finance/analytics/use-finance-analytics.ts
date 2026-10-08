import { useCallback, useEffect, useState } from "react";

import { getFinanceAnalytics, type AnalyticsQuery, type FinanceAnalytics } from "../../../api/finance-analytics";

export interface UseFinanceAnalytics {
  /** The latest successful payload; kept while a newer query loads. */
  data: FinanceAnalytics | null;
  loading: boolean;
  error: Error | null;
  retry: () => void;
}

export function useFinanceAnalytics(query: AnalyticsQuery): UseFinanceAnalytics {
  const [data, setData] = useState<FinanceAnalytics | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);
  const [attempt, setAttempt] = useState(0);
  // The query object is rebuilt on every render; its content is the identity.
  const key = JSON.stringify(query);

  useEffect(() => {
    const controller = new AbortController();
    setLoading(true);
    setError(null);
    getFinanceAnalytics(JSON.parse(key) as AnalyticsQuery, controller.signal)
      .then((payload) => {
        if (controller.signal.aborted) return;
        setData(payload);
        setLoading(false);
      })
      .catch((reason: unknown) => {
        if (controller.signal.aborted) return;
        setError(reason instanceof Error ? reason : new Error("Analytics request failed."));
        setLoading(false);
      });
    return () => controller.abort();
  }, [key, attempt]);

  const retry = useCallback(() => setAttempt((current) => current + 1), []);
  return { data, loading, error, retry };
}
