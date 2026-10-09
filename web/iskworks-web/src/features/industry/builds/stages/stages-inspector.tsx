// The Stages inspector: a read-only drill-down for one production node or
// one acquisition line, shown in a side inspector rather than expanded
// inline (expanding rows in place made the stage table unscannable). Reuses the same generic inspector shell and primitives
// every other Build workspace view uses (`PlannerInspectorShell` /
// `InspectorSection` / `InspectorCollapseProvider`) rather than a bespoke
// side panel, and -- like Graph's own inspector -- is owned and rendered by
// this view itself, never routed through the top-level workspace.
//
// Every value here is read straight off the endpoint's response; this
// component computes nothing (no run/surplus/cost math, and no proportional
// allocation for an acquisition consumer's "Used By" quantity -- see
// `AcquisitionConsumerRef`'s own doc comment in `../../../../api/industry`
// for why that quantity is each occurrence's own shortage contribution, not
// a derived split).
//
// Selection is by stable identity only (`ExecutionNode.id` /
// `AcquisitionLine.typeId`), never an array index, and is resolved against
// the *current* `plan` on every render -- a selected entity that has
// disappeared from a freshly returned plan simply renders nothing (the
// caller is responsible for clearing the stale selection; see
// `build-stages-view.tsx`).

import { ExternalLink } from "lucide-react";

import type {
  EpicNodeProgress,
  EpicPlanOverlay,
  ExecutionNode,
  ExecutionOccurrence,
  ExecutionPlanProjection,
  PreviewBuildPlanCommand,
  TicketStatus,
} from "../../../../api/industry";
import type { FacilityProfile } from "../../../../api/industry/facilities";
import { EveTypeImage } from "../../../../components/eve-type-image";
import { MoneyAmount } from "../../../../components/money";
import { Badge } from "../../../../components/primitives";
import { InspectorCollapseProvider } from "../../inspector/inspector-collapse";
import { InspectorRow, InspectorSection } from "../../inspector/inspector-section";
import { PlannerInspectorShell } from "../../../../components/planner-inspector-shell";
import { PricingBody } from "../../inspector/unified-item-inspector";

import type { RootRowEditing } from "./root-row-editing";

import { CreateOperationTicket, SourcingSwitch } from "./plan-actions";
import { OperationRequirements } from "./operation-requirements";
import { edgeKey, type PlanSourcing, type SourcingChoice } from "./use-plan-sourcing";

import { AcquisitionInspector } from "./acquisition-inspector";
import { DescendantConfigurationEditor } from "./descendant-configuration-editor";
import { MissingFacilityWarning, qty, ScopeRow, SourcingSplit } from "./stages-inspector-parts";

export type StagesSelection =
  | { kind: "production"; nodeId: string }
  | { kind: "acquisition"; typeId: number };

const money = (value: string | null) => (value != null ? <MoneyAmount value={value} /> : "Incomplete");

const TICKET_STATUS_LABELS: Record<TicketStatus, string> = {
  todo: "To do",
  inProgress: "In progress",
  complete: "Done",
  canceled: "Canceled",
};


export function StagesInspector({
  plan,
  selection,
  onClose,
  onSelect,
  rootBuildId,
  command,
  facilities,
  onConfigurationSaved,
  sourcing,
  onOpenBuildSettings,
  rootEditing = null,
  readOnly = false,
  epic = null,
}: {
  plan: ExecutionPlanProjection;
  selection: StagesSelection | null;
  onClose: () => void;
  onSelect: (selection: StagesSelection) => void;
  /** The root Build id -- path-authoritative for the descendant-
   * configuration mutation, mirrors `postBuildExecutionPlan`. */
  rootBuildId: string;
  /** The same live, unsaved overlay currently feeding this `plan` -- sent
   * back with a configuration edit so the server re-validates membership
   * under the exact plan the user is looking at. `null` when the draft's
   * overlay isn't ready (only a read-only inspector renders then). */
  command: PreviewBuildPlanCommand | null;
  /** Root-owned facility list (`editor.allFacilities`) -- Stages never
   * fetches its own copy. */
  facilities: FacilityProfile[];
  /** Called after a configuration edit lands, so the host can re-fetch the
   * plan (`useBuildExecutionPlan().refetch`). Stages never patches
   * runs/output/surplus/materials/cost locally. */
  onConfigurationSaved: () => void;
  /** Per-demand-edge sourcing changes. */
  sourcing: PlanSourcing;
  /** The root's configuration lives in Build
   * settings -- the Final Production inspector links there. */
  onOpenBuildSettings?: () => void;
  /** The root Build's row-level exceptions (per-row price
   * override, per-component fulfillment scope) -- editable only on the
   * root's own demand edges / rows, `null` when unavailable. */
  rootEditing?: RootRowEditing | null;
  /** An Epic's frozen plan: show everything, change nothing. Sourcing,
   * scope, configuration and pricing render as plain values, and Build
   * actions (settings, producer Build, ticket creation) are hidden. */
  readOnly?: boolean;
  /** With `readOnly`: the Epic's ticket and holdings per node / input. */
  epic?: EpicPlanOverlay | null;
}) {
  if (!selection) return null;
  // A read-only inspector never edits the root's rows either.
  const editing = readOnly ? null : rootEditing;

  if (selection.kind === "production") {
    const node = plan.nodes.find((candidate) => candidate.id === selection.nodeId);
    if (!node) return null;
    return (
      <ProductionInspector
        command={command}
        facilities={facilities}
        node={node}
        onClose={onClose}
        onSelect={onSelect}
        onConfigurationSaved={onConfigurationSaved}
        plan={plan}
        rootBuildId={rootBuildId}
        sourcing={sourcing}
        onOpenBuildSettings={readOnly ? undefined : onOpenBuildSettings}
        rootEditing={editing}
        readOnly={readOnly}
        epicNode={epic?.nodes[node.id] ?? null}
      />
    );
  }

  const line = plan.acquisitions.find((candidate) => candidate.typeId === selection.typeId);
  if (!line) return null;
  return (
    <AcquisitionInspector
      epicStock={epic?.acquisitions[String(line.typeId)] ?? null}
      line={line}
      onClose={onClose}
      plan={plan}
      readOnly={readOnly}
      rootBuildId={rootBuildId}
      rootEditing={editing}
      sourcing={sourcing}
    />
  );
}

function ProductionInspector({
  node,
  plan,
  onClose,
  onSelect,
  rootBuildId,
  command,
  facilities,
  onConfigurationSaved,
  sourcing,
  onOpenBuildSettings,
  rootEditing,
  readOnly,
  epicNode,
}: {
  node: ExecutionNode;
  plan: ExecutionPlanProjection;
  onClose: () => void;
  onSelect: (selection: StagesSelection) => void;
  rootBuildId: string;
  command: PreviewBuildPlanCommand | null;
  facilities: FacilityProfile[];
  onConfigurationSaved: () => void;
  sourcing: PlanSourcing;
  onOpenBuildSettings?: () => void;
  rootEditing: RootRowEditing | null;
  readOnly: boolean;
  epicNode: EpicNodeProgress | null;
}) {
  // Dependency order only -- never readiness terminology. The root
  // always holds the unique maximum stage index (see
  // `build-stages-view.tsx`'s own comment on this same fact), so comparing
  // against the last (already-ordered) stage's index is exact, never
  // recomputed.
  const finalStageIndex = plan.stages.length > 0 ? plan.stages[plan.stages.length - 1].index : null;
  const stageLabel = node.stage === finalStageIndex ? "Final Production" : `Stage ${node.stage + 1}`;

  const occurrencesById = new Map(plan.occurrences.map((occurrence) => [occurrence.id, occurrence]));
  const nodesById = new Map(plan.nodes.map((candidate) => [candidate.id, candidate]));
  const occurrences = node.occurrenceIds
    .map((id) => occurrencesById.get(id))
    .filter((occurrence): occurrence is ExecutionOccurrence => occurrence != null);

  // The root is never editable from Stages (Build settings owns root
  // configuration). A node is the root iff its sole occurrence is.
  const isRoot = occurrences.length === 1 && occurrences[0].isRoot;
  const currentMethod: SourcingChoice =
    node.activity === "reaction"
      ? { mode: "reaction", reactionFormulaTypeId: occurrences[0]?.blueprintOrFormulaTypeId ?? 0 }
      : { mode: "manufacturing", blueprintTypeId: occurrences[0]?.blueprintOrFormulaTypeId ?? 0 };

  // One operation is one producer Build: its configuration edit targets
  // exactly that Build.
  const editableOccurrences = occurrences.slice(0, 1);
  const boughtToo = isRoot ? null : plan.acquisitions.find((line) => line.typeId === node.outputTypeId) ?? null;
  const rootOutputPricing = isRoot ? rootEditing?.pricing(node.outputTypeId, "output") ?? null : null;

  return (
    <PlannerInspectorShell
      closeLabel="Close production inspector"
      dismissLabel="Dismiss production inspector"
      eyebrow={stageLabel}
      onClose={onClose}
      open
      returnFocusRowKey={node.id}
      title={node.outputTypeName}
    >
      <InspectorCollapseProvider>
        <div className="flex items-center gap-2 border-b border-border px-3 py-2">
          <EveTypeImage size={32} typeId={node.outputTypeId} typeName={node.outputTypeName} />
          <Badge square tone={node.activity === "reaction" ? "reaction" : "primary"}>
            {node.activity === "reaction" ? "REACT" : "MANUFACTURE"}
          </Badge>
        </div>

        {isRoot ? (
          // The root is the Build being planned: it has no upstream demand
          // edge, so there is no Buy / Build choice to make here.
          <div className="border-b border-border px-3 py-2">
            <p className="text-[10px] font-semibold uppercase tracking-wide text-muted">
              Final production
            </p>
            <p className="mt-0.5 text-[11px] text-muted">
              {readOnly
                ? "This is the Epic's final product, as frozen when the Epic was created."
                : "This is the Build itself. Its blueprint, facility and pricing are configured in Build settings."}
            </p>
            {onOpenBuildSettings ? (
              <button className="iw-button-secondary mt-2" onClick={onOpenBuildSettings} type="button">
                Edit build settings
              </button>
            ) : null}
          </div>
        ) : (
          // Demand-edge ownership: each consumer's sourcing (and, for the
          // root's own edges, fulfillment scope) is that edge's choice.
          // Production configuration is NOT here -- it belongs to the one
          // producer below, shown once however many consumers it serves.
          <InspectorSection
            defaultExpanded
            id="used-by"
            label="Used by / Sourcing"
            summary={`${node.consumers.length} consumer${node.consumers.length === 1 ? "" : "s"}`}
          >
            <p className="mb-2 text-[11px] text-muted">
              {node.consumers.length === 1
                ? `One consumer uses this ${node.activity === "reaction" ? "reaction" : "manufacturing"} operation.`
                : `One ${node.activity === "reaction" ? "reaction" : "manufacturing"} operation serves ${node.consumers.length} consumers. Each consumer's sourcing is its own choice -- changing one leaves the others on this operation.`}
            </p>
            <ul className="space-y-2">
              {node.consumers.map((consumer, index) => {
                const consumerName = nodesById.get(consumer.nodeId)?.outputTypeName ?? "Unknown";
                const key = edgeKey(consumer.buildId, node.outputTypeId);
                return (
                  <li
                    className="rounded border border-border p-2"
                    key={`${consumer.nodeId}-${consumer.occurrenceId}-${index}`}
                  >
                    <p className="mb-1 text-xs font-semibold">{consumerName}</p>
                    <InspectorRow label="Required" value={qty(consumer.requiredQuantity)} />
                    <ScopeRow
                      consumerName={consumerName}
                      rootEditing={consumer.buildId === rootBuildId ? rootEditing : null}
                      scope={consumer.fulfillmentScope}
                      typeId={node.outputTypeId}
                    />
                    <InspectorRow label="Planned inventory use" value={qty(consumer.plannedInventoryQuantity)} />
                    <InspectorRow label="Production demand" value={qty(consumer.quantity)} />
                    <div className="mt-1.5 flex items-center justify-between gap-2">
                      <span className="text-[10px] font-semibold uppercase tracking-wide text-muted">
                        Source
                      </span>
                      {readOnly ? (
                        <span className="text-xs">{node.activity === "reaction" ? "React" : "Build"}</span>
                      ) : (
                        <SourcingSwitch
                          consumerName={consumerName}
                          current={currentMethod}
                          disabled={false}
                          error={sourcing.errorByEdge[key] ?? null}
                          methods={node.productionMethods ?? []}
                          onChange={(choice: SourcingChoice) =>
                            sourcing.change(consumer.buildId, node.outputTypeId, choice)
                          }
                          pending={Boolean(sourcing.pendingByEdge[key])}
                        />
                      )}
                    </div>
                  </li>
                );
              })}
            </ul>
          </InspectorSection>
        )}

        {/* Producer ownership: blueprint/formula, ME/TE and facility belong
            to this one operation -- one configuration however many
            consumers it serves. */}
        <InspectorSection defaultExpanded id="configuration" label="Production configuration">
          {!isRoot && node.consumers.length > 1 ? (
            <p className="mb-2 text-[11px] text-muted">
              One configuration for this operation -- it applies to all {node.consumers.length}{" "}
              consumers above.
            </p>
          ) : null}
          {isRoot || readOnly || !command ? (
            <>
              <InspectorRow
                label={node.activity === "reaction" ? "Reaction formula" : "Blueprint"}
                value={occurrences[0]?.blueprintOrFormulaName || "—"}
              />
              {node.activity === "manufacturing" ? (
                <>
                  <InspectorRow label="ME" value={node.effectiveMe ?? "—"} />
                  <InspectorRow label="TE" value={node.effectiveTe ?? "—"} />
                </>
              ) : null}
              {readOnly ? (
                // The Epic froze no facility, only its cost evidence.
                <InspectorRow label="Facility" value={node.facilityName ?? "Not recorded"} />
              ) : (
                <>
                  <InspectorRow label="Facility" value={node.facilityName ?? "No facility"} />
                  {!node.facilityId ? <MissingFacilityWarning /> : null}
                </>
              )}
              <p className="pt-1 text-[11px] text-muted">
                {readOnly
                  ? "Frozen with the Epic. Choose No Epic to change the Build's configuration."
                  : isRoot
                    ? "Root configuration is edited in Build settings."
                    : "Loading the Build's configuration..."}
              </p>
            </>
          ) : (
            <DescendantConfigurationEditor
              activity={node.activity}
              blueprintOrFormulaName={editableOccurrences[0]?.blueprintOrFormulaName ?? null}
              blueprintSelection={editableOccurrences[0]?.blueprintSelection ?? null}
              command={command}
              currentFacilityId={node.facilityId}
              currentFacilityName={node.facilityName}
              effectiveMe={node.effectiveMe}
              effectiveTe={node.effectiveTe}
              facilities={facilities}
              members={editableOccurrences}
              onSaved={onConfigurationSaved}
              requiredRuns={editableOccurrences[0]?.projectedRuns ?? 1}
              rootBuildId={rootBuildId}
            />
          )}
          {!isRoot && !readOnly && editableOccurrences.length === 1 ? (
            // The producer's own Build still owns its row-level exceptions
            // (its inputs' price overrides, its descendants' scope) --
            // reachable, not re-implemented here.
            <a
              className="mt-2 inline-flex items-center gap-1 text-[11px] font-semibold text-primary hover:underline"
              href={`/builds/${rootBuildId}/producers/${editableOccurrences[0].buildId}`}
            >
              Open producer Build
              <ExternalLink aria-hidden="true" className="h-3 w-3" />
            </a>
          ) : null}
        </InspectorSection>

        <InspectorSection defaultExpanded id="production" label="Production">
          {boughtToo ? (
            <SourcingSplit acquisition={boughtToo} nodesById={nodesById} produced={[node]} />
          ) : null}
          <InspectorRow label="Required" value={qty(node.requiredQuantity)} />
          <InspectorRow label="Planned inventory use" value={qty(node.plannedInventoryQuantity)} />
          {!isRoot ? <InspectorRow label="Available (whole tree)" value={qty(node.availableQuantity ?? 0)} /> : null}
          <InspectorRow label="Production demand" value={qty(node.productionDemand)} />
          <InspectorRow
            label="Runs"
            value={
              node.occurrenceIds.length > 1 ? `${qty(node.projectedRuns)} total` : qty(node.projectedRuns)
            }
          />
          <InspectorRow label="Projected output" value={qty(node.projectedOutput)} />
          <InspectorRow label="Retained surplus" value={qty(node.retainedSurplusQuantity)} />
          <InspectorRow
            label="Surplus basis"
            value={node.retainedSurplusCost != null ? <MoneyAmount value={node.retainedSurplusCost} /> : "—"}
          />
        </InspectorSection>

        {occurrences.length === 1 ? (
          <InspectorSection defaultExpanded id="requirements" label="Requirements">
            <OperationRequirements
              acquisitions={plan.acquisitions}
              nodes={plan.nodes}
              onSelect={onSelect}
              requirements={occurrences[0].requirements}
            />
          </InspectorSection>
        ) : null}

        {isRoot && rootOutputPricing && rootEditing ? (
          // The root output row's sale-price exception (the Worksheet's
          // output-row pricing). Produced operations never carry a
          // purchase-price override -- their cost is the production cost.
          <InspectorSection defaultExpanded id="output-pricing" label="Output pricing">
            <InspectorRow
              label="Effective unit price"
              value={rootOutputPricing.unitPrice != null ? <MoneyAmount value={rootOutputPricing.unitPrice} /> : "Unpriced"}
            />
            <InspectorRow label="Source" value={rootOutputPricing.summary} />
            <PricingBody onChange={rootEditing.onPricingChange} slice={rootOutputPricing} />
          </InspectorSection>
        ) : null}

        <InspectorSection defaultExpanded id="economics" label="Economics">
          <InspectorRow label="Material/component" value={money(node.materialComponentCost)} />
          <InspectorRow label="Own installation" value={money(node.ownInstallationCost)} />
          <InspectorRow label="Total production" value={money(node.totalProductionCost)} />
          {node.unitProductionCost != null ? (
            <InspectorRow label="Unit production cost" value={<MoneyAmount value={node.unitProductionCost} />} />
          ) : null}
          {node.retainedSurplusQuantity > 0 ? (
            <InspectorRow label="Retained surplus basis" value={money(node.retainedSurplusCost)} />
          ) : null}
          <p className="pt-1 text-[11px] text-muted">
            Material/component is this operation&rsquo;s direct inputs plus the consumed share of
            the production it draws on; installation is this operation&rsquo;s own job cost only.
          </p>
          {!node.costComplete ? (
            <p className="pt-1 text-[11px] text-muted">
              Cost is incomplete (a missing price, facility or cost index) -- known quantities above
              are still authoritative.
            </p>
          ) : null}
        </InspectorSection>

        <InspectorSection defaultExpanded id="ticket" label="Ticket">
          {readOnly ? (
            <EpicNodeTicket epicNode={epicNode} isRoot={isRoot} />
          ) : occurrences[0] ? (
            <CreateOperationTicket
              activity={node.activity}
              buildId={occurrences[0].buildId}
              key={occurrences[0].id}
              outputName={node.outputTypeName}
              runs={node.projectedRuns}
            />
          ) : null}
        </InspectorSection>

      </InspectorCollapseProvider>
    </PlannerInspectorShell>
  );
}

/** An Epic step's ticket, and (for an intermediate) what the Epic holds of
 * its output for the steps it feeds. */
function EpicNodeTicket({ epicNode, isRoot }: { epicNode: EpicNodeProgress | null; isRoot: boolean }) {
  if (!epicNode) return <p className="text-xs text-muted">No Epic details.</p>;
  return (
    <>
      <InspectorRow
        label="Ticket"
        value={epicNode.ticketDisplayId
          ? `${epicNode.ticketDisplayId}${epicNode.ticketStatus ? ` · ${TICKET_STATUS_LABELS[epicNode.ticketStatus]}` : ""}`
          : "No ticket"}
      />
      {!isRoot ? (
        <>
          <InspectorRow label="Output reserved" value={qty(epicNode.output.reserved)} />
          <InspectorRow label="Output used" value={qty(epicNode.output.consumed)} />
          <InspectorRow label="Still needed" value={qty(epicNode.output.remainingNeed)} />
        </>
      ) : null}
    </>
  );
}
