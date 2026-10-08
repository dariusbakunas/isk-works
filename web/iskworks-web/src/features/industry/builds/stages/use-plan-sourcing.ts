import { useCallback, type Dispatch, type SetStateAction } from "react";

import type { RecipeSelection } from "../../../../api/industry";
import { applyComponentResolution, type ComponentResolutionState } from "../components/planner-panels";
import { useGraphSourcing } from "../graph/use-graph-sourcing";

/** A demand edge's sourcing target: Buy (`null`) or one production method. */
export type SourcingChoice = RecipeSelection | null;

/** The stable key of one demand edge (consumer Build + component). */
export function edgeKey(consumerBuildId: string, componentTypeId: number): string {
  return `${consumerBuildId}:${componentTypeId}`;
}

export interface PlanSourcing {
  pendingByEdge: Record<string, boolean>;
  errorByEdge: Record<string, string>;
  /** Change one demand edge's sourcing (Buy <-> Build <-> Reaction). */
  change: (consumerBuildId: string, componentTypeId: number, choice: SourcingChoice) => void;
  /** Explicitly multi-consumer: set every listed consumer's edge for
   * `componentTypeId` to `choice`, one after another (the first creates the
   * shared producer, the rest reuse it). */
  changeAll: (consumerBuildIds: string[], componentTypeId: number, choice: SourcingChoice) => Promise<void>;
}

/**
 * The Plan inspector's sourcing changes, through the existing write paths
 * only -- never a second planner in the browser.
 *
 * - An edge the **root** consumes is the root editor's own overlay
 *   (`componentResolutions`, autosaved like any other root edit); the plan
 *   re-projects from the changed overlay.
 * - An edge a **descendant** consumes targets that Build's component
 *   resolutions (`useGraphSourcing`: persist, then create-or-reuse): the
 *   dependency write that references (or creates once) the plan's shared
 *   producer.
 */
export function usePlanSourcing({
  rootBuildId,
  setComponentResolutions,
  onChanged,
}: {
  rootBuildId: string;
  setComponentResolutions: Dispatch<SetStateAction<Record<number, ComponentResolutionState>>>;
  onChanged: () => void;
}): PlanSourcing {
  const nested = useGraphSourcing(onChanged);

  const write = useCallback(
    async (consumerBuildId: string, componentTypeId: number, choice: SourcingChoice) => {
      if (consumerBuildId === rootBuildId) {
        applyComponentResolution(componentTypeId, choice, setComponentResolutions);
        return;
      }
      const key = edgeKey(consumerBuildId, componentTypeId);
      if (choice) {
        await nested.switchToBuild(key, consumerBuildId, componentTypeId, choice);
      } else {
        await nested.switchToBuy(key, consumerBuildId, componentTypeId);
      }
    },
    [rootBuildId, setComponentResolutions, nested],
  );

  const change = useCallback(
    (consumerBuildId: string, componentTypeId: number, choice: SourcingChoice) => {
      void write(consumerBuildId, componentTypeId, choice);
    },
    [write],
  );

  const changeAll = useCallback(
    async (consumerBuildIds: string[], componentTypeId: number, choice: SourcingChoice) => {
      // Nested edges first, in order, so the shared producer exists before
      // the root's own (debounced) overlay save references it.
      const ordered = [
        ...consumerBuildIds.filter((id) => id !== rootBuildId),
        ...consumerBuildIds.filter((id) => id === rootBuildId),
      ];
      for (const consumerBuildId of ordered) {
        await write(consumerBuildId, componentTypeId, choice);
      }
    },
    [rootBuildId, write],
  );

  return {
    pendingByEdge: nested.pendingByNodeId,
    errorByEdge: nested.errorByNodeId,
    change,
    changeAll,
  };
}
