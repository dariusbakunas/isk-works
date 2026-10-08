import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  postBuildExecutionPlan,
  type ExecutionPlanProjection,
  type PreviewBuildPlanCommand,
} from "../../../../api/industry";
import { apiMessage } from "../../shared/api-error";

// Mirrors `useBuildMaterials` exactly (same debounce/abort/error shape) --
// Stages is another live projection of the same unsaved editor overlay, not
// a different fetching architecture.
const REFRESH_DEBOUNCE_MS = 250;

export interface UseBuildExecutionPlanArgs {
  buildId: string;
  /** The editor's `previewKey` -- a `JSON.stringify` of the exact
   * `PreviewBuildPlanCommand` a preview / Materials / Graph sends. */
  previewKey: string;
  /** Only fetch while the Stages tab is the active view. */
  active: boolean;
  /** The editor-owned linked-build lifecycle map (only `.id` is read). A
   * new/changed entry refetches so a newly-linked child's occurrence
   * appears without waiting for an unrelated overlay edit. */
  linkedBuildsByTypeId: Record<number, { id: string }>;
}

export interface UseBuildExecutionPlanResult {
  plan: ExecutionPlanProjection | null;
  /** A request is in flight (debounced). The table stays visible meanwhile. */
  loading: boolean;
  /** Non-fatal: a refresh failed but a previous plan is still shown. */
  refreshError: string | null;
  /** Fatal: nothing usable to show (first load failed, or the whole-tree
   * projection is unavailable). */
  hardError: string | null;
  refetch: () => void;
}

export function useBuildExecutionPlan({
  buildId,
  previewKey,
  active,
  linkedBuildsByTypeId,
}: UseBuildExecutionPlanArgs): UseBuildExecutionPlanResult {
  const [plan, setPlan] = useState<ExecutionPlanProjection | null>(null);
  const [loading, setLoading] = useState(false);
  const [refreshError, setRefreshError] = useState<string | null>(null);
  const [hardError, setHardError] = useState<string | null>(null);
  const [refetchToken, setRefetchToken] = useState(0);

  const planRef = useRef<ExecutionPlanProjection | null>(null);
  planRef.current = plan;

  const linkedSignal = useMemo(
    () =>
      Object.entries(linkedBuildsByTypeId)
        .map(([typeId, build]) => `${typeId}:${build.id}`)
        .sort()
        .join(","),
    [linkedBuildsByTypeId],
  );

  const refetch = useCallback(() => setRefetchToken((token) => token + 1), []);

  useEffect(() => {
    if (!active || !buildId || !previewKey) return;

    let command: PreviewBuildPlanCommand;
    try {
      command = JSON.parse(previewKey) as PreviewBuildPlanCommand;
    } catch {
      return;
    }

    const controller = new AbortController();
    setLoading(true);
    const timer = window.setTimeout(() => {
      void postBuildExecutionPlan(buildId, command, controller.signal)
        .then((next) => {
          if (controller.signal.aborted) return;
          setPlan(next);
          setRefreshError(null);
          setHardError(null);
        })
        .catch((error: unknown) => {
          if (controller.signal.aborted) return;
          const message = apiMessage(error);
          if (planRef.current) {
            setRefreshError(message);
          } else {
            setHardError(message);
          }
        })
        .finally(() => {
          if (!controller.signal.aborted) setLoading(false);
        });
    }, REFRESH_DEBOUNCE_MS);

    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [buildId, previewKey, active, linkedSignal, refetchToken]);

  return { plan, loading, refreshError, hardError, refetch };
}
