import { useCallback, useRef, useState } from "react";

import {
  clearComponentResolution,
  createLinkedBuild,
  getBuild,
  setComponentResolution,
  type RecipeSelection,
} from "../../../../api/industry";
import { sourcingErrorMessage } from "../sourcing-errors";

/**
 * Build-ID-addressable graph sourcing for **non-root** production nodes: a
 * nested BUY -> BUILD (or BUILD -> BUY) mutation that targets the Build that
 * *owns* the requirement (`parentBuildId` on the graph node), never the root
 * page editor.
 *
 * Not a second worksheet editor -- it holds no build state, only per-slot
 * in-flight / error UI state and drives two existing server calls:
 *   1. `POST /api/builds/:owningBuildId/component-resolutions`  (persist)
 *   2. `POST /api/builds/:owningBuildId/linked-builds`          (create-or-reuse)
 * then asks the graph to re-project. The root Build's own `useBuildWorksheetEditor`
 * path is unchanged and still handles root-level switching.
 */
export interface GraphSourcing {
  /** graphNodeId -> a sourcing call is in flight for that slot. */
  pendingByNodeId: Record<string, boolean>;
  /** graphNodeId -> last error message for that slot (cleared on retry). */
  errorByNodeId: Record<string, string>;
  /** BUY -> BUILD on `owningBuildId`'s `componentTypeId`. Resolves once
   * written (or failed -- the error lands in `errorByNodeId`). */
  switchToBuild: (
    graphNodeId: string,
    owningBuildId: string,
    componentTypeId: number,
    recipe: RecipeSelection,
  ) => Promise<void>;
  /** BUILD -> BUY on `owningBuildId`'s `componentTypeId` (retains the linked Build). */
  switchToBuy: (graphNodeId: string, owningBuildId: string, componentTypeId: number) => Promise<void>;
}

export function useGraphSourcing(onChanged: () => void): GraphSourcing {
  const [pendingByNodeId, setPending] = useState<Record<string, boolean>>({});
  const [errorByNodeId, setError] = useState<Record<string, string>>({});
  const inFlight = useRef<Set<string>>(new Set());

  const run = useCallback(
    async (graphNodeId: string, work: () => Promise<unknown>): Promise<void> => {
      if (inFlight.current.has(graphNodeId)) return;
      inFlight.current.add(graphNodeId);
      setPending((prev) => ({ ...prev, [graphNodeId]: true }));
      setError((prev) => {
        if (!(graphNodeId in prev)) return prev;
        const next = { ...prev };
        delete next[graphNodeId];
        return next;
      });
      try {
        await work();
        onChanged();
      } catch (error) {
        // A stale-plan error still re-projects, so the user retries on
        // what is actually saved.
        onChanged();
        setError((prev) => ({ ...prev, [graphNodeId]: sourcingErrorMessage(error) }));
      } finally {
        inFlight.current.delete(graphNodeId);
        setPending((prev) => {
          const next = { ...prev };
          delete next[graphNodeId];
          return next;
        });
      }
    },
    [onChanged],
  );

  const switchToBuild = useCallback(
    (
      graphNodeId: string,
      owningBuildId: string,
      componentTypeId: number,
      recipe: RecipeSelection,
    ) =>
      run(graphNodeId, async () => {
        const owner = await getBuild(owningBuildId);
        await setComponentResolution(owningBuildId, {
          componentTypeId,
          recipe,
          expectedRevision: owner.revision,
        });
        await createLinkedBuild(owningBuildId, { componentTypeId });
      }),
    [run],
  );

  const switchToBuy = useCallback(
    (graphNodeId: string, owningBuildId: string, componentTypeId: number) =>
      run(graphNodeId, async () => {
        const owner = await getBuild(owningBuildId);
        await clearComponentResolution(owningBuildId, componentTypeId, owner.revision);
      }),
    [run],
  );

  return { pendingByNodeId, errorByNodeId, switchToBuild, switchToBuy };
}
