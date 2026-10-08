// The Build Plan view (the primary Build surface; "Stages" in code): the
// authoritative Execution Plan
// (`POST /api/builds/:build_id/execution-plan`), rendered as production
// dependency order -- "what can start first, what feeds what, how far from
// the final product." Inputs to source (external acquisition) are a
// logistics/sourcing input, not a production stage. This is dependency
// order only, never workflow state or execution readiness (no
// Ready/Blocked/Waiting/Complete/Unlocks anywhere in this view).
//
// Every stage/grouping/edge/quantity value here is read straight off the
// endpoint's response. This view does NOT calculate stages, group nodes,
// dedupe edges, aggregate runs/surplus, or derive consumer quantities --
// the backend already did all of that.

import { useCallback, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";

import type {
  AcquisitionLine,
  ExecutionNode,
  ExecutionPlanProjection,
  ExecutionStage,
} from "../../../../api/industry";
import { EveTypeImage } from "../../../../components/eve-type-image";
import {
  OperationalTable,
  OperationalTableRow,
  type OperationalColumn,
} from "../../../../components/operational-table";
import { EmptyState, InlineAlert, Panel } from "../../../../components/primitives";
import type { BuildWorksheetEditorModel } from "../use-build-worksheet-editor";
import { focusExecutionPlan } from "../focused-producer-projection";

import { CostAmount, ExecutionNodeRow, Quantity } from "./execution-node-row";
import { costWarningLabel } from "./stage-warnings";
import { StagesInspector, type StagesSelection } from "./stages-inspector";
import { useBuildExecutionPlan } from "./use-build-execution-plan";
import { usePlanInspector } from "./use-plan-inspector";

// Inventory and Facility are not table columns: they, along with Cost, made
// the table too wide and truncated on the right at desktop width. Facility
// appears as an Item subtitle and in the inspector; Inventory (planned
// reuse) is in the inspector's own Production section.
// Plan is the daily working screen, so operation
// economics sit in the table -- Total from tablet up, the Material /
// Install split on desktop (compact ISK, exact value on hover). The full
// breakdown (unit cost, surplus basis) lives in the inspector.
const NODE_COLUMNS: OperationalColumn[] = [
  { key: "item", label: "Item", width: "minmax(220px,1fr)", sticky: true },
  { key: "activity", label: "Activity", width: "104px" },
  { key: "required", label: "Required", width: "92px", align: "right", numeric: true },
  { key: "produce", label: "Produce", width: "92px", align: "right", numeric: true },
  { key: "runs", label: "Runs", width: "96px", align: "right", numeric: true },
  { key: "usedBy", label: "Used By", width: "96px", hideBelow: "tablet" },
  { key: "material", label: "Material", width: "104px", align: "right", numeric: true, hideBelow: "desktop" },
  { key: "install", label: "Install", width: "96px", align: "right", numeric: true, hideBelow: "desktop" },
  { key: "total", label: "Total", width: "104px", align: "right", numeric: true, hideBelow: "tablet" },
];

const ACQUISITION_COLUMNS: OperationalColumn[] = [
  { key: "item", label: "Item", width: "minmax(220px,1fr)", sticky: true },
  { key: "required", label: "Required", width: "110px", align: "right", numeric: true },
  { key: "inventory", label: "Planned Use", width: "120px", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "shortage", label: "Shortage", width: "120px", align: "right", numeric: true },
  { key: "unitPrice", label: "Unit Price", width: "104px", align: "right", numeric: true, hideBelow: "desktop" },
  { key: "cost", label: "Est. Cost", width: "112px", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "also", label: "Also", width: "80px", hideBelow: "desktop" },
];

export function BuildStagesView({
  editor,
  active,
  focusedProducerId,
}: {
  editor: BuildWorksheetEditorModel;
  active: boolean;
  focusedProducerId?: string;
}) {
  const buildId = editor.initialBuild?.id ?? "";
  const { plan: rootPlan, loading, refreshError, hardError, refetch } = useBuildExecutionPlan({
    buildId,
    previewKey: editor.previewKey,
    active,
    linkedBuildsByTypeId: editor.linkedBuildsByTypeId, linkedBuildsSettling: editor.linkedBuildsSettling,
  });
  const plan = useMemo(
    () => rootPlan && focusedProducerId ? focusExecutionPlan(rootPlan, focusedProducerId) : rootPlan,
    [rootPlan, focusedProducerId],
  );

  const [selection, setSelectionState] = useState<StagesSelection | null>(null);
  // The right rail holds one inspector at a time: selecting a Plan row
  // closes Build settings, and opening Build settings clears the selection.
  const closeSettings = editor.closeInspector;
  const setSelection = useCallback(
    (next: StagesSelection | null) => {
      setSelectionState(next);
      if (next) closeSettings?.();
    },
    [closeSettings],
  );
  const settingsOpen = editor.inspectorMode?.kind === "buildSettings";
  useEffect(() => {
    if (settingsOpen) setSelectionState(null);
  }, [settingsOpen]);

  // Preserve the selection across an execution-plan refresh (e.g. an
  // unsaved Build edit) as long as the selected entity still exists in the
  // new response -- the inspector itself always renders straight off the
  // live `plan`, so this effect only needs to close it when the entity is
  // gone, never to snapshot or refresh content itself.
  useEffect(() => {
    if (!plan || !selection) return;
    const stillExists =
      selection.kind === "production"
        ? plan.nodes.some((node) => node.id === selection.nodeId)
        : plan.acquisitions.some((line) => line.typeId === selection.typeId);
    if (!stillExists) setSelection(null);
  }, [plan, selection]);

  // Declared after the stale-selection effect above on purpose: when a sourcing
  // change swaps the selected row for its new producer, the producer select
  // must win over the clear.
  const { command, replan, sourcing, rootEditing } = usePlanInspector({
    editor,
    buildId,
    plan,
    refetch,
    onProducerCreated: useCallback(
      (nodeId: string) => setSelection({ kind: "production", nodeId }),
      [setSelection],
    ),
  });

  const nodesById = new Map<string, ExecutionNode>(plan?.nodes.map((node) => [node.id, node]) ?? []);
  // Root always holds the unique maximum stage (proven by the backend's own
  // stage algorithm: every other operation is strictly less deep), so the
  // LAST entry in the already-ordered `stages` array is always exactly the
  // root's stage -- reading it off, never recomputed.
  const finalStage = plan && plan.stages.length > 0 ? plan.stages[plan.stages.length - 1] : null;
  const productionStages = plan && finalStage ? plan.stages.slice(0, -1) : [];

  return (
    <section aria-labelledby="stages-heading" className="mt-1">
        <header className="mb-3">
        <h2 className="text-sm font-semibold text-foreground" id="stages-heading">
          Production Plan
        </h2>
        <p className="mt-0.5 text-sm text-muted">
          The work this Build needs, in the order it has to happen: every item is produced
          before anything that uses it.
        </p>
      </header>

      {hardError ? (
        <InlineAlert title="Plan could not be loaded">{hardError}</InlineAlert>
      ) : rootPlan && focusedProducerId && !plan ? (
        <InlineAlert title="Focused producer unavailable">
          This producer is not present in the authoritative root projection.
        </InlineAlert>
      ) : !plan ? (
        <Panel>
          <div className="p-3 text-sm text-muted" role="status">
            Calculating plan...
          </div>
        </Panel>
      ) : plan.nodes.length === 0 && plan.acquisitions.length === 0 ? (
        <EmptyState title="Nothing to plan yet">
          This Build has no production or acquisition evidence yet.
        </EmptyState>
      ) : (
        <>
          {refreshError ? (
            <div className="mb-2">
              <InlineAlert title="Plan may be out of date" tone="warning">
                {refreshError}
              </InlineAlert>
            </div>
          ) : null}
          {loading ? (
            <p className="mb-2 text-xs text-muted" role="status">
              Updating...
            </p>
          ) : null}

          {plan.warnings.length > 0 ? <StageWarningsBanner warnings={plan.warnings} /> : null}

          {plan.acquisitions.length > 0 ? (
            <AcquisitionSection
              acquisitions={plan.acquisitions}
              onSelectAcquisition={(typeId) => setSelection({ kind: "acquisition", typeId })}
              selectedTypeId={selection?.kind === "acquisition" ? selection.typeId : null}
            />
          ) : null}

          {productionStages.map((stage) => (
            <NodeSection
              key={stage.index}
              nodes={nodeList(stage, nodesById)}
              onSelectNode={(nodeId) => setSelection({ kind: "production", nodeId })}
              plan={plan}
              selectedNodeId={selection?.kind === "production" ? selection.nodeId : null}
              subtitle={stage.index === 0 ? "Earliest production" : undefined}
              title={`Stage ${stage.index + 1}`}
            />
          ))}

          {finalStage ? (
            <NodeSection
              emphasize
              nodes={nodeList(finalStage, nodesById)}
              onSelectNode={(nodeId) => setSelection({ kind: "production", nodeId })}
              plan={plan}
              selectedNodeId={selection?.kind === "production" ? selection.nodeId : null}
              title="Final Production"
            />
          ) : null}

          {command && active ? (
            <StagesInspector
              command={command}
              facilities={editor.allFacilities ?? []}
              onClose={() => setSelection(null)}
              onSelect={setSelection}
              // The inspector stays open on the same producer (resolved
              // by stable node id against the re-projected plan) so the
              // effect of a facility / ME change is visible in place.
              onConfigurationSaved={replan}
              plan={plan}
              rootBuildId={buildId}
              selection={selection}
              sourcing={sourcing}
              onOpenBuildSettings={editor.openBuildSettings}
              rootEditing={rootEditing}
            />
          ) : null}
        </>
      )}
    </section>
  );
}

function nodeList(stage: ExecutionStage, nodesById: Map<string, ExecutionNode>): ExecutionNode[] {
  return stage.nodeIds
    .map((id) => nodesById.get(id))
    .filter((node): node is ExecutionNode => node != null);
}

function NodeSection({
  title,
  subtitle,
  nodes,
  plan,
  selectedNodeId,
  onSelectNode,
  emphasize = false,
}: {
  title: string;
  subtitle?: string;
  nodes: ExecutionNode[];
  plan: ExecutionPlanProjection;
  selectedNodeId: string | null;
  onSelectNode: (nodeId: string) => void;
  emphasize?: boolean;
}) {
  if (nodes.length === 0) return null;
  return (
    <div className={emphasize ? "mb-5 rounded-md border border-primary/30 p-2" : "mb-5"}>
      <h3
        className={
          emphasize
            ? "mb-2 flex items-baseline gap-2 text-sm font-semibold text-foreground"
            : "mb-2 flex items-baseline gap-2 text-xs font-semibold uppercase tracking-wide text-muted"
        }
      >
        {title}
        {subtitle ? (
          <span className="text-[11px] font-normal normal-case text-muted">{subtitle}</span>
        ) : null}
      </h3>
      <OperationalTable
        ariaLabel={title}
        columns={NODE_COLUMNS}
        onSelectRow={onSelectNode}
        selectedRowKey={selectedNodeId}
      >
        <tbody>
          {nodes.map((node) => (
            <ExecutionNodeRow key={node.id} node={node} plan={plan} showCost />
          ))}
        </tbody>
      </OperationalTable>
    </div>
  );
}

function AcquisitionSection({
  acquisitions,
  selectedTypeId,
  onSelectAcquisition,
}: {
  acquisitions: AcquisitionLine[];
  selectedTypeId: number | null;
  onSelectAcquisition: (typeId: number) => void;
}) {
  return (
    <div className="mb-5">
      <h3 className="mb-2 flex items-baseline gap-2 text-xs font-semibold uppercase tracking-wide text-muted">
        Inputs to Source
        <span className="text-[11px] font-normal normal-case text-muted">
          Bought, not produced in this Build
        </span>
      </h3>
      <OperationalTable
        ariaLabel="Inputs to Source"
        columns={ACQUISITION_COLUMNS}
        onSelectRow={(key) => onSelectAcquisition(Number(key))}
        selectedRowKey={selectedTypeId === null ? null : String(selectedTypeId)}
      >
        <tbody>
          {acquisitions.map((line) => (
            <OperationalTableRow
              cells={{
                item: (
                  <span className="flex min-w-0 items-center gap-2">
                    <EveTypeImage size={24} typeId={line.typeId} typeName={line.typeName} />
                    <span className="min-w-0 truncate">{line.typeName}</span>
                  </span>
                ),
                required: <Quantity value={line.requiredQuantity} />,
                // Free stock only; stock open Epics hold is noted, never used.
                inventory: line.reservedQuantity > 0 ? (
                  <span
                    className="flex flex-col items-end leading-tight"
                    title={`${line.availableQuantity.toLocaleString()} free · ${line.reservedQuantity.toLocaleString()} reserved by Epics`}
                  >
                    <Quantity value={line.plannedInventoryQuantity} />
                    <span className="text-[11px] text-muted">
                      {line.reservedQuantity.toLocaleString()} reserved
                    </span>
                  </span>
                ) : (
                  <Quantity value={line.plannedInventoryQuantity} />
                ),
                shortage: <Quantity className="text-warning" value={line.shortageQuantity} />,
                // The fresh (to-buy) price and cost for the shortage, from the
                // cost projection -- "Unpriced" rather than a fabricated 0.
                unitPrice: line.freshUnitPrice ? (
                  <CostAmount value={line.freshUnitPrice} />
                ) : (
                  <span className="text-xs text-muted">—</span>
                ),
                cost: freshCostCell(line),
                also:
                  line.sourceStrategy === "mixed" ? (
                    <span
                      className="text-[10px] uppercase text-muted"
                      title="This type is also produced elsewhere in this Build"
                    >
                      Mixed
                    </span>
                  ) : null,
              }}
              key={line.typeId}
              rowKey={String(line.typeId)}
              status="warning"
            />
          ))}
        </tbody>
      </OperationalTable>
    </div>
  );
}

/** An input's estimated fresh (to-buy) cost for its shortage: compact ISK,
 * a stale marker when any contributing price is stale, "Unpriced" when any
 * consumer's price is unknown (never a partial sum). */
function freshCostCell(line: AcquisitionLine): ReactNode {
  if (line.freshCost == null) return <span className="text-xs text-warning">Unpriced</span>;
  return (
    <span className="inline-flex items-center gap-1">
      <CostAmount value={line.freshCost} />
      {line.freshPriceStale ? (
        <span className="text-[10px] uppercase text-warning" title="At least one price is stale">
          stale
        </span>
      ) : null}
    </span>
  );
}

function StageWarningsBanner({
  warnings,
}: {
  warnings: ExecutionPlanProjection["warnings"];
}) {
  const [open, setOpen] = useState(false);
  return (
    <div className="mb-3">
      <InlineAlert title={`${warnings.length} cost warning${warnings.length === 1 ? "" : "s"}`} tone="warning">
        <button
          className="text-xs underline"
          onClick={() => setOpen((current) => !current)}
          type="button"
        >
          {open ? "Hide details" : "Show details"}
        </button>
        {open ? <WarningList warnings={warnings} /> : null}
      </InlineAlert>
    </div>
  );
}

function WarningList({ warnings }: { warnings: ExecutionPlanProjection["warnings"] }): ReactNode {
  return (
    <ul className="mt-1 list-disc space-y-0.5 pl-4 text-xs">
      {warnings.map((warning, index) => (
        // Warnings carry no stable id and the list is static per render, so the index is the key.
        <li key={index}>{costWarningLabel(warning)}</li>
      ))}
    </ul>
  );
}
