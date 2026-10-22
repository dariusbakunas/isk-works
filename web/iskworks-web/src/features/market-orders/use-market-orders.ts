import { useCallback, useEffect, useState } from "react";

import { getMarketOrders, refreshMarketOrderBooks, type MarketOrdersOverview } from "../../api/market-orders";

export interface MarketOrdersState {
  /** The latest successful payload; kept while a refetch runs. */
  data: MarketOrdersOverview | null;
  loading: boolean;
  error: Error | null;
  /** Re-reads the page. */
  reload: () => void;
  /** Asks the worker to refresh every book first, then re-reads. */
  refresh: () => Promise<void>;
  refreshing: boolean;
}

export function useMarketOrders(): MarketOrdersState {
  const [data, setData] = useState<MarketOrdersOverview | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);
  const [attempt, setAttempt] = useState(0);
  const [refreshing, setRefreshing] = useState(false);

  useEffect(() => {
    const controller = new AbortController();
    setLoading(true);
    setError(null);
    getMarketOrders(controller.signal)
      .then((payload) => {
        if (controller.signal.aborted) return;
        setData(payload);
        setLoading(false);
      })
      .catch((reason: unknown) => {
        if (controller.signal.aborted) return;
        setError(reason instanceof Error ? reason : new Error("Market orders request failed."));
        setLoading(false);
      });
    return () => controller.abort();
  }, [attempt]);

  const reload = useCallback(() => setAttempt((current) => current + 1), []);
  const refresh = useCallback(async () => {
    setRefreshing(true);
    try {
      await refreshMarketOrderBooks();
    } catch (reason) {
      setError(reason instanceof Error ? reason : new Error("Refresh failed."));
    } finally {
      setRefreshing(false);
      setAttempt((current) => current + 1);
    }
  }, []);
  return { data, loading, error, reload, refresh, refreshing };
}

/** A clock that ticks every `intervalMs`, for countdowns. */
export function useNow(intervalMs = 1_000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), intervalMs);
    return () => window.clearInterval(timer);
  }, [intervalMs]);
  return now;
}
