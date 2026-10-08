import type { Build } from "../../../../api/industry";
import { InlineAlert, Panel } from "../../../../components/primitives";
import { focusExecutionPlan } from "../focused-producer-projection";
import { CostAmount, Quantity } from "../stages/execution-node-row";
import { useBuildExecutionPlan } from "../stages/use-build-execution-plan";
import type { BuildWorksheetEditorModel } from "../use-build-worksheet-editor";

export function FocusedProducerSummary({
  editor,
  producer,
}: {
  editor: BuildWorksheetEditorModel;
  producer: Build;
}) {
  const { plan: rootPlan, hardError } = useBuildExecutionPlan({
    buildId: editor.initialBuild?.id ?? "",
    previewKey: editor.previewKey,
    active: true,
    linkedBuildsByTypeId: editor.linkedBuildsByTypeId, linkedBuildsSettling: editor.linkedBuildsSettling,
  });
  const focusedPlan = rootPlan ? focusExecutionPlan(rootPlan, producer.id) : null;
  const operation = focusedPlan?.nodes.find((node) => node.id === focusedPlan.rootNodeId);

  if (hardError) return <InlineAlert title="Producer economics unavailable">{hardError}</InlineAlert>;
  if (rootPlan && !operation) {
    return (
      <InlineAlert title="Focused producer unavailable">
        This producer is not present in the authoritative root projection.
      </InlineAlert>
    );
  }
  if (!operation) return <Panel><div className="p-3 text-sm text-muted">Calculating producer economics...</div></Panel>;

  const values = [
    ["Required", <Quantity value={operation.requiredQuantity} />],
    ["Production demand", <Quantity value={operation.productionDemand} />],
    ["Runs", <Quantity value={operation.projectedRuns} />],
    ["Projected output", <Quantity value={operation.projectedOutput} />],
    ["Surplus", <Quantity value={operation.retainedSurplusQuantity} />],
    ["Material", money(operation.materialComponentCost)],
    ["Installation", money(operation.ownInstallationCost)],
    ["Total", money(operation.totalProductionCost)],
    ["Unit", money(operation.unitProductionCost)],
  ] as const;

  return (
    <Panel>
      <div className="grid grid-cols-2 divide-x divide-y divide-border sm:grid-cols-3 lg:grid-cols-9">
        {values.map(([label, value]) => (
          <div className="min-w-0 px-3 py-2" key={label}>
            <div className="text-[10px] uppercase tracking-wide text-muted">{label}</div>
            <div className="mt-0.5 truncate text-sm font-semibold text-foreground">{value}</div>
          </div>
        ))}
      </div>
    </Panel>
  );
}

function money(value: string | null) {
  return value == null ? <span className="text-xs text-muted">Incomplete</span> : <CostAmount value={value} />;
}
