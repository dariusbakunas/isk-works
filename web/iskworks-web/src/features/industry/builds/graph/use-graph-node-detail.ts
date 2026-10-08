import { useCallback, useEffect, useRef, useState } from "react";

import { getBuild, type Build } from "../../../../api/industry";
import { apiMessage } from "../../shared/api-error";

import type { BuildGraphNode } from "./to-react-flow";

/**
 * Lazily loads the persisted `Build` behind a *selected linked* Production
 * node so the Graph inspector can show Blueprint/Formula + Facility detail
 * that the topology response (`POST /api/builds/:id/graph`) deliberately
 * omits.
 *
 * Never fires a request for:
 *   - the root node       -- its `Build` is already resident in the editor
 *   - actionable-Buy / unresolved-Build nodes -- no persisted Build exists
 *   - no selection
 *   - an inactive Graph
 *
 * One `GET /api/builds/:buildId` per distinct `buildId`. Results are cached
 * for the hook's mount lifetime, so a Graph -> Worksheet -> Graph round trip
 * (the view stays mounted-but-hidden) re-serves from cache with no second
 * request. Race-safe: a stale response for a since-changed selection is
 * dropped (generation guard), and flipping the Graph inactive aborts an
 * in-flight request. A cache entry is always keyed by the `buildId` that was
 * requested, never by whatever is selected when the promise resolves.
 */
export interface GraphNodeDetail {
  detail: Build | null;
  loading: boolean;
  error: string | null;
  retry: () => void;
}

/** The linked-Build id to fetch for this selection, or `null` for anything
 * that must not trigger a request (root, buy/unresolved, nothing). */
function targetBuildId(node: BuildGraphNode | undefined): string | null {
  return node?.data.nodeType === "production" ? node.data.node.buildId : null;
}

export function useGraphNodeDetail({
  active,
  selectedNode,
}: {
  active: boolean;
  selectedNode: BuildGraphNode | undefined;
}): GraphNodeDetail {
  const buildId = active ? targetBuildId(selectedNode) : null;

  const cacheRef = useRef<Map<string, Build>>(new Map());
  const generationRef = useRef(0);
  const [detail, setDetail] = useState<Build | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [retryToken, setRetryToken] = useState(0);

  const retry = useCallback(() => setRetryToken((token) => token + 1), []);

  useEffect(() => {
    // Nothing to enrich: clear, and invalidate any response still in flight.
    if (!buildId) {
      generationRef.current += 1;
      setDetail(null);
      setLoading(false);
      setError(null);
      return;
    }

    const cached = cacheRef.current.get(buildId);
    if (cached) {
      generationRef.current += 1;
      setDetail(cached);
      setLoading(false);
      setError(null);
      return;
    }

    const generation = ++generationRef.current;
    const controller = new AbortController();
    setDetail(null);
    setError(null);
    setLoading(true);

    void getBuild(buildId, controller.signal)
      .then((build) => {
        if (controller.signal.aborted || generation !== generationRef.current) return;
        cacheRef.current.set(buildId, build);
        setDetail(build);
        setLoading(false);
      })
      .catch((requestError) => {
        if (controller.signal.aborted || generation !== generationRef.current) return;
        setError(apiMessage(requestError));
        setLoading(false);
      });

    return () => controller.abort();
  }, [buildId, retryToken]);

  return { detail, loading, error, retry };
}
