import { AlertTriangle, CheckCircle2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate, useSearchParams } from "react-router";

import type { BuildWorksheetRow } from "../../../../api/industry";
import { EmptyState, InlineAlert, Panel } from "../../../../components/primitives";
import { focusExecutionPlan } from "../focused-producer-projection";
import { StagesInspector } from "../stages/stages-inspector";
import { useBuildExecutionPlan } from "../stages/use-build-execution-plan";
import { usePlanInspector } from "../stages/use-plan-inspector";
import type { BuildWorksheetEditorModel } from "../use-build-worksheet-editor";
import { BuildWorksheetTable } from "./build-worksheet-table";
import { useBuildWorksheet } from "./use-build-worksheet";
import { rowKeyForTarget, scopeAcquisitionToConsumer, selectionForTarget, targetForRow, type WorksheetTarget } from "./worksheet-selection";

export function BuildWorksheetView({ active, editor, focusedProducerId, readOnly = false }: { active: boolean; editor: BuildWorksheetEditorModel; focusedProducerId?: string; /** An Epic is selected: the inspector shows values and changes nothing. */ readOnly?: boolean }) {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const includeDownstream = searchParams.get("downstream") === "1";
  // The inspector tracks what a row stands for (its producer Build, or the
  // bought type), not the row id, which changes whenever sourcing changes.
  const [target, setTarget] = useState<WorksheetTarget | null>(null);
  const [inspecting, setInspecting] = useState(false);
  const toggleDownstream = () => {
    // Merged and direct rows mean different things, so drop the selection.
    setTarget(null);
    setSearchParams((current) => {
      const next = new URLSearchParams(current);
      if (includeDownstream) next.delete("downstream"); else next.set("downstream", "1");
      return next;
    }, { replace: true });
  };
  const { worksheet, loading, refreshError, hardError } = useBuildWorksheet({
    rootBuildId: editor.initialBuild?.id ?? "", focusedProducerId: focusedProducerId ?? null, includeDownstream,
    previewKey: editor.previewKey, active, linkedBuildsByTypeId: editor.linkedBuildsByTypeId, linkedBuildsSettling: editor.linkedBuildsSettling,
  });
  const rows = worksheet?.groups.flatMap((group) => group.rows) ?? [];

  // The Plan inspector reads the execution plan, fetched only once a row has
  // been selected so an unused Worksheet costs no second planning walk.
  const buildId = editor.initialBuild?.id ?? "";
  const { plan: rootPlan, refetch } = useBuildExecutionPlan({
    buildId, previewKey: editor.previewKey, active: active && inspecting, linkedBuildsByTypeId: editor.linkedBuildsByTypeId, linkedBuildsSettling: editor.linkedBuildsSettling,
  });
  const plan = useMemo(
    () => rootPlan && focusedProducerId ? focusExecutionPlan(rootPlan, focusedProducerId) : rootPlan,
    [rootPlan, focusedProducerId],
  );
  const selection = useMemo(() => plan ? selectionForTarget(plan, target) : null, [plan, target]);
  // The right rail holds one inspector at a time (Build settings, Plan, Graph).
  const closeSettings = editor.closeInspector;
  const selectRow = (row: BuildWorksheetRow) => {
    const next = targetForRow(row);
    if (!next) return;
    setInspecting(true);
    setTarget(next);
    closeSettings?.();
  };
  const settingsOpen = editor.inspectorMode?.kind === "buildSettings";
  useEffect(() => { if (settingsOpen) setTarget(null); }, [settingsOpen]);
  // Declared before usePlanInspector on purpose: when a sourcing change swaps
  // the selected row for its new producer, that select must win over this clear.
  useEffect(() => { if (plan && target && !selection) setTarget(null); }, [plan, target, selection]);
  const { command, replan, sourcing, rootEditing } = usePlanInspector({
    editor, buildId, plan, refetch,
    onProducerCreated: useCallback((nodeId: string) => {
      const occurrence = plan?.occurrences.find((candidate) => candidate.nodeId === nodeId);
      if (occurrence) setTarget({ kind: "producer", buildId: occurrence.buildId });
    }, [plan]),
  });
  const inspectorPlan = useMemo(() => {
    if (!plan || includeDownstream || selection?.kind !== "acquisition") return plan;
    const scopeBuildId = focusedProducerId ?? buildId;
    return { ...plan, acquisitions: plan.acquisitions.map((line) => line.typeId === selection.typeId ? scopeAcquisitionToConsumer(line, scopeBuildId) : line) };
  }, [plan, includeDownstream, selection, focusedProducerId, buildId]);
  const materialsMissing = rows.some((row) => (row.shortageQuantity ?? 0) > 0);
  return <section aria-labelledby="worksheet-heading" className="@container mt-1 min-w-0">
    <header className="flex min-h-8 items-center justify-between gap-2 border-b border-border px-2">
      <div className="flex min-w-0 items-baseline gap-2">
        <h2 className="shrink-0 text-sm font-semibold text-foreground" id="worksheet-heading">Production Worksheet</h2>
        {includeDownstream ? <span className="truncate text-xs text-muted">Nested values are not additive.</span> : null}
      </div>
      <div className="flex shrink-0 items-center gap-3">
        {worksheet ? <span className="inline-flex items-center gap-1.5 whitespace-nowrap text-xs text-muted">
          {materialsMissing
            ? <AlertTriangle aria-hidden="true" className="h-3.5 w-3.5 text-warning" />
            : <CheckCircle2 aria-hidden="true" className="h-3.5 w-3.5 text-positive" />}
          {materialsMissing ? "Materials missing" : "Inventory covered"}
        </span> : null}
        <button aria-checked={includeDownstream} className="group inline-flex items-center gap-2 text-xs text-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary" onClick={toggleDownstream} role="switch" type="button">
          <span aria-hidden="true" className={`relative inline-block h-4 w-7 shrink-0 rounded-full border transition-colors ${includeDownstream ? "border-primary bg-primary" : "border-border bg-panel-strong"}`}>
            <span className={`absolute top-0.5 h-2.5 w-2.5 rounded-full transition-all ${includeDownstream ? "left-3.5 bg-primary-foreground" : "left-0.5 bg-muted"}`} />
          </span>
          Include downstream
        </button>
      </div>
    </header>
    {hardError ? <InlineAlert title="Worksheet could not be loaded">{hardError}</InlineAlert>
      : !worksheet ? <Panel><div className="p-3 text-sm text-muted" role="status">Calculating worksheet...</div></Panel>
      : worksheet.groups.length === 0 ? <EmptyState title="No requirements">This Build has no material requirements.</EmptyState>
      : <>{refreshError ? <div className="mb-2"><InlineAlert title="Worksheet may be out of date" tone="warning">{refreshError}</InlineAlert></div> : null}{loading ? <p className="mb-2 text-xs text-muted" role="status">Updating...</p> : null}<BuildWorksheetTable
          onOpenOutput={() => navigate(focusedProducerId ? `/builds/${worksheet.scope.rootBuildId}/producers/${focusedProducerId}?view=plan` : `/builds/${worksheet.scope.rootBuildId}?view=plan`)}
          onSelectRow={selectRow}
          selectedRowKey={rowKeyForTarget(rows, target)}
          worksheet={worksheet}
        />{command && active && inspectorPlan ? <StagesInspector
          readOnly={readOnly}
          command={command}
          facilities={editor.allFacilities ?? []}
          onClose={() => setTarget(null)}
          onConfigurationSaved={replan}
          onOpenBuildSettings={editor.openBuildSettings}
          onSelect={(next) => {
            if (next.kind === "acquisition") setTarget({ kind: "buy", typeId: next.typeId });
            else {
              const occurrence = inspectorPlan.occurrences.find((candidate) => candidate.nodeId === next.nodeId);
              if (occurrence) setTarget({ kind: "producer", buildId: occurrence.buildId });
            }
          }}
          plan={inspectorPlan}
          rootBuildId={buildId}
          rootEditing={rootEditing}
          selection={selection}
          sourcing={sourcing}
        /> : null}</>}
  </section>;
}
