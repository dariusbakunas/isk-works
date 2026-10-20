import { useCallback, useEffect, useState } from "react";

import {
  getInventoryConsumption,
  type ConsumptionQuery,
  type InventoryConsumption,
} from "../../../../api/inventory-analytics";

export interface UseInventoryConsumption {
  /** The latest successful payload; kept while a newer query loads. */
  data: InventoryConsumption | null;
  loading: boolean;
  error: Error | null;
  retry: () => void;
}

export function useInventoryConsumption(query: ConsumptionQuery): UseInventoryConsumption {
  const [data, setData] = useState<InventoryConsumption | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);
  const [attempt, setAttempt] = useState(0);
  // The query object is rebuilt on every render; its content is the identity.
  const key = JSON.stringify(query);

  useEffect(() => {
    const controller = new AbortController();
    setLoading(true);
    setError(null);
    getInventoryConsumption(JSON.parse(key) as ConsumptionQuery, controller.signal)
      .then((payload) => {
        if (controller.signal.aborted) return;
        setData(payload);
        setLoading(false);
      })
      .catch((reason: unknown) => {
        if (controller.signal.aborted) return;
        setError(reason instanceof Error ? reason : new Error("Inventory consumption request failed."));
        setLoading(false);
      });
    return () => controller.abort();
  }, [key, attempt]);

  const retry = useCallback(() => setAttempt((current) => current + 1), []);
  return { data, loading, error, retry };
}
