import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  postBuildWorksheet,
  type BuildWorksheetProjection,
  type PreviewBuildPlanCommand,
} from "../../../../api/industry";
import { apiMessage } from "../../shared/api-error";

const REFRESH_DEBOUNCE_MS = 250;

export interface UseBuildWorksheetArgs {
  rootBuildId: string;
  focusedProducerId: string | null;
  includeDownstream: boolean;
  previewKey: string;
  active: boolean;
  linkedBuildsByTypeId: Record<number, { id: string }>;
  /** A linked-build create/reuse is in flight. The server-side plan is about
   * to change, so hold the fetch until it settles rather than sending one
   * request per linked build and aborting all but the last. */
  linkedBuildsSettling?: boolean;
}

export interface UseBuildWorksheetResult {
  worksheet: BuildWorksheetProjection | null;
  loading: boolean;
  refreshError: string | null;
  hardError: string | null;
  refetch: () => void;
}

export function useBuildWorksheet({
  rootBuildId,
  focusedProducerId,
  includeDownstream,
  previewKey,
  active,
  linkedBuildsByTypeId,
  linkedBuildsSettling = false,
}: UseBuildWorksheetArgs): UseBuildWorksheetResult {
  const [worksheet, setWorksheet] = useState<BuildWorksheetProjection | null>(null);
  const [loading, setLoading] = useState(false);
  const [refreshError, setRefreshError] = useState<string | null>(null);
  const [hardError, setHardError] = useState<string | null>(null);
  const [refetchToken, setRefetchToken] = useState(0);
  const worksheetRef = useRef(worksheet);
  const generationRef = useRef(0);
  worksheetRef.current = worksheet;

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
    if (!active || !rootBuildId || !previewKey) return;
    if (linkedBuildsSettling) {
      setLoading(true);
      return;
    }
    let command: PreviewBuildPlanCommand;
    try {
      command = JSON.parse(previewKey) as PreviewBuildPlanCommand;
    } catch {
      return;
    }

    const controller = new AbortController();
    const generation = ++generationRef.current;
    setLoading(true);
    const timer = window.setTimeout(() => {
      void postBuildWorksheet(rootBuildId, command, focusedProducerId, includeDownstream, controller.signal)
        .then((next) => {
          if (controller.signal.aborted || generation !== generationRef.current) return;
          setWorksheet(next);
          setRefreshError(null);
          setHardError(null);
        })
        .catch((error: unknown) => {
          if (controller.signal.aborted || generation !== generationRef.current) return;
          const message = apiMessage(error);
          if (worksheetRef.current) setRefreshError(message);
          else setHardError(message);
        })
        .finally(() => {
          if (!controller.signal.aborted && generation === generationRef.current) setLoading(false);
        });
    }, REFRESH_DEBOUNCE_MS);

    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [rootBuildId, focusedProducerId, includeDownstream, previewKey, active, linkedSignal, linkedBuildsSettling, refetchToken]);

  return { worksheet, loading, refreshError, hardError, refetch };
}
