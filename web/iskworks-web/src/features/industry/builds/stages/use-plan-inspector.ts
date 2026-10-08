// The write-side wiring shared by every view that hosts the Plan inspector
// (Plan and Worksheet): the live overlay command, sourcing writes through the
// existing paths, root row-level exceptions, and the
// "configure the new producer" follow-up after Buy -> Build/Reaction. Selection
// itself stays with the view -- each view identifies rows differently.

import { useEffect, useMemo, useState } from "react";

import type { ExecutionPlanProjection, PreviewBuildPlanCommand } from "../../../../api/industry";
import type { BuildWorksheetEditorModel } from "../use-build-worksheet-editor";
import { rootRowEditing } from "./root-row-editing";
import { usePlanSourcing, type PlanSourcing, type SourcingChoice } from "./use-plan-sourcing";

export function usePlanInspector({
  editor,
  buildId,
  plan,
  refetch,
  onProducerCreated,
}: {
  editor: BuildWorksheetEditorModel;
  buildId: string;
  plan: ExecutionPlanProjection | null;
  refetch: () => void;
  /** Called with the new producer's `ExecutionNode.id` once the re-projected
   * plan contains the producer serving an edge the user just switched to
   * Build/Reaction. */
  onProducerCreated: (nodeId: string) => void;
}) {
  // The exact same live overlay driving `plan`, parsed once for the
  // descendant-configuration mutation to echo back -- never a second,
  // independently-derived command.
  const command = useMemo<PreviewBuildPlanCommand | null>(() => {
    try {
      return JSON.parse(editor.previewKey) as PreviewBuildPlanCommand;
    } catch {
      return null;
    }
  }, [editor.previewKey]);

  // Sourcing changes go through the existing write paths only -- the root's
  // own requirements through the editor overlay (autosaved like any
  // Economics edit), a descendant's through its Build's component
  // resolutions (the canonical dependency write on a canonical root). The
  // plan is then re-projected; nothing is re-planned client-side.
  // A write re-projects everything keyed on the editor's preview key -- the
  // persistent Build economics, Plan, Worksheet, Logistics and Graph.
  const replan = editor.bumpPreview ?? refetch;
  const edgeSourcing = usePlanSourcing({
    rootBuildId: buildId,
    setComponentResolutions: editor.setComponentResolutions,
    onChanged: replan,
  });

  // After Buy -> Build/Reaction the next thing to do is configure the new
  // producer (blueprint/formula, ME/TE, facility), so once the re-projected
  // plan contains the producer serving that edge it is selected.
  const [focusAfterReplan, setFocusAfterReplan] = useState<{
    typeId: number;
    consumerBuildId: string;
  } | null>(null);
  const sourcing = useMemo<PlanSourcing>(
    () => ({
      ...edgeSourcing,
      change: (consumerBuildId: string, typeId: number, choice: SourcingChoice) => {
        setFocusAfterReplan(choice ? { typeId, consumerBuildId } : null);
        edgeSourcing.change(consumerBuildId, typeId, choice);
      },
      changeAll: (consumerBuildIds: string[], typeId: number, choice: SourcingChoice) => {
        setFocusAfterReplan(choice && consumerBuildIds.length > 0 ? { typeId, consumerBuildId: consumerBuildIds[0] } : null);
        return edgeSourcing.changeAll(consumerBuildIds, typeId, choice);
      },
    }),
    [edgeSourcing],
  );

  // The root's row-level exceptions (per-row price override, fulfillment
  // scope) -- the same editor overlay the Worksheet edited.
  const rootEditing = rootRowEditing(editor);

  useEffect(() => {
    if (!plan || !focusAfterReplan) return;
    const producer = plan.nodes.find(
      (node) =>
        node.outputTypeId === focusAfterReplan.typeId &&
        node.consumers.some((consumer) => consumer.buildId === focusAfterReplan.consumerBuildId),
    );
    if (!producer) return;
    setFocusAfterReplan(null);
    onProducerCreated(producer.id);
  }, [plan, focusAfterReplan, onProducerCreated]);

  return { command, replan, sourcing, rootEditing };
}
