import { useCallback, useEffect, useState } from "react";
import {
  getOrder,
  getOrderExecutionPlan,
  type EpicExecutionPlan,
  type OrderRequirement,
} from "../../../../api/industry";
import { apiMessage } from "../../shared/api-error";

export interface EpicExecutionPlanState {
  data: EpicExecutionPlan | null;
  /** Buy requirements nothing tracks yet, by type -- what "Create ticket"
   * on an Inputs to Source line creates tickets for. */
  untrackedBuyRequirementIds: Map<number, string[]>;
  error: string;
  reload: () => void;
}

function untrackedBuys(requirements: OrderRequirement[]): Map<number, string[]> {
  const byType = new Map<number, string[]>();
  for (const requirement of requirements) {
    if (requirement.kind !== "buy" || requirement.state !== "needsAction") continue;
    byType.set(requirement.typeId, [...(byType.get(requirement.typeId) ?? []), requirement.id]);
  }
  return byType;
}

/** An Epic's frozen plan for the Plan tab, loaded while the tab is active. */
export function useEpicExecutionPlan(epicId: string | null, active: boolean): EpicExecutionPlanState {
  const [data, setData] = useState<EpicExecutionPlan | null>(null);
  const [untracked, setUntracked] = useState<Map<number, string[]>>(new Map());
  const [error, setError] = useState("");
  const [reloadKey, setReloadKey] = useState(0);

  useEffect(() => {
    setData(null);
    setError("");
  }, [epicId]);

  useEffect(() => {
    if (!epicId || !active) return;
    let cancelled = false;
    Promise.all([getOrderExecutionPlan(epicId), getOrder(epicId)])
      .then(([plan, detail]) => {
        if (cancelled) return;
        setData(plan);
        setUntracked(untrackedBuys(detail.requirements));
        setError("");
      })
      .catch((requestError) => {
        if (!cancelled) setError(apiMessage(requestError));
      });
    return () => {
      cancelled = true;
    };
  }, [epicId, active, reloadKey]);

  const reload = useCallback(() => setReloadKey((key) => key + 1), []);
  return { data: data?.epic.orderId === epicId ? data : null, untrackedBuyRequirementIds: untracked, error, reload };
}
