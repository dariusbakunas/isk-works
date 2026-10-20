import { useCallback, useEffect, useState } from "react";

import { getInventoryRunway, type InventoryRunway, type MaterialCategory } from "../../../../api/inventory-analytics";

export interface UseInventoryRunway {
  /** The latest successful payload; kept while a newer filter loads. */
  data: InventoryRunway | null;
  loading: boolean;
  error: Error | null;
  retry: () => void;
}

export function useInventoryRunway(category: MaterialCategory | null): UseInventoryRunway {
  const [data, setData] = useState<InventoryRunway | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    const controller = new AbortController();
    setLoading(true);
    setError(null);
    getInventoryRunway(category, controller.signal)
      .then((payload) => {
        if (controller.signal.aborted) return;
        setData(payload);
        setLoading(false);
      })
      .catch((reason: unknown) => {
        if (controller.signal.aborted) return;
        setError(reason instanceof Error ? reason : new Error("Inventory runway request failed."));
        setLoading(false);
      });
    return () => controller.abort();
  }, [category, attempt]);

  const retry = useCallback(() => setAttempt((current) => current + 1), []);
  return { data, loading, error, retry };
}
