// Graph selection detail rail. Builds a canonical `InspectorModel` +
// `InspectorActions` from the selected node via `buildGraphInspector` and
// renders the shared `UnifiedItemInspector` -- the exact same component the
// Worksheet item inspector renders. There is no Graph-specific inspector
// body any more; only the portal shell and the no-selection Plan Summary.


import { useEffect, useId, type ReactNode } from "react";
import { createPortal } from "react-dom";

import type {
  BlueprintObservation,
  Build,
  BuildGraphProjection,
  FacilityProfile,
  GraphWarning,
  PlannerPricingSelection,
  ProductionWorksheet,
  RecipeCurrency,
  RecipeSelection,
  WorksheetItem,
} from "../../../../api/industry";
import { EveTypeImage } from "../../../../components/eve-type-image";
import { formatIskSummary } from "../../../../components/money";
import { buildGraphInspector } from "../../inspector/adapters/graph-node";
import { buildRootInspector } from "../../inspector/adapters/root-build";
import { InspectorCollapseProvider } from "../../inspector/inspector-collapse";
import { InspectorRow, InspectorSection } from "../../inspector/inspector-section";
import { UnifiedItemInspector } from "../../inspector/unified-item-inspector";
import type { BuildSettings } from "../use-build-settings";
import type { BuildWorksheetEditorModel } from "../use-build-worksheet-editor";

import {
  linkedProductionEnrichment,
  type ProductionEnrichment,
} from "./graph-inspector-enrichment";
import type { BuildGraphNode } from "./to-react-flow";

const NUMBER = new Intl.NumberFormat("en-US");

export interface GraphOpCounts {
  build: number;
  reaction: number;
  buy: number;
}

function recipeKindLabel(kind: string): string {
  return kind === "rootReaction" || kind === "reaction" ? "Reaction" : "Manufacturing";
}

function costLine(costState: string, estimatedCost: string | null): string {
  if (costState === "known" && estimatedCost != null) return formatIskSummary(estimatedCost);
  return costState === "incomplete"
    ? "Incomplete"
    : costState === "stale"
      ? "Stale"
      : costState === "unresolved"
        ? "Unresolved"
        : "Not computed";
}

/** Never renders "0 ISK" for a component that hasn't actually resolved --
 * `null` (incomplete) reads as "—", exactly like the shared cost section. */
function costComponentLine(value: string | null): string {
  return value != null ? formatIskSummary(value) : "—";
}

function NodeInspectorBody({
  readOnly,
  selectedNode,
  editor,
  nodeWarnings,
  enrichment,
  detailLoading,
  detailError,
  onRetryDetail,
  linkedSettings,
  allFacilities,
  observations,
  worksheetItem,
  pricingContext,
  allowMarketPolicyOverride,
  rootTypeId,
  recipeCurrency,
  sourcingPending,
  onBuyBuild,
  onBuildBuy,
  onOpenLinkedBuild,
  onCopyBuildId,
  onPricingChange,
  onClose,
  onScope,
  fulfillmentScope,
}: {
  selectedNode: BuildGraphNode;
  editor: BuildWorksheetEditorModel;
  nodeWarnings: GraphWarning[];
  enrichment: ProductionEnrichment | null;
  detailLoading: boolean;
  detailError: string | null;
  onRetryDetail: () => void;
  linkedSettings: BuildSettings | undefined;
  allFacilities: FacilityProfile[];
  observations: BlueprintObservation[];
  worksheetItem: WorksheetItem | null;
  pricingContext: { sourceName: string; sourceRevision: number; capturedAt: string } | null;
  allowMarketPolicyOverride: boolean;
  rootTypeId: number | null;
  recipeCurrency: RecipeCurrency;
  sourcingPending: boolean;
  onBuyBuild: (typeId: number, recipe: RecipeSelection) => void;
  onBuildBuy: (componentTypeId: number) => void;
  onOpenLinkedBuild: (buildId: string) => void;
  onCopyBuildId: (buildId: string) => void;
  onPricingChange: (selection: PlannerPricingSelection) => void;
  onClose: () => void;
  onScope: ((scope: "missing" | "full") => void) | undefined;
  fulfillmentScope: "missing" | "full" | undefined;
  readOnly: boolean;
}) {
  const { nodeType } = selectedNode.data;
  const isRoot = nodeType === "root";
  const lifecycle = selectedNode.data as unknown as { pending?: boolean; error?: string | null };

  // The root Build is not a graph-node target -- both views build it from the
  // one shared editor via the same canonical builder.
  if (isRoot) {
    const root = buildRootInspector(editor, { onClose });
    return <UnifiedItemInspector actions={root.actions} model={root.model} readOnly={readOnly} />;
  }

  const acqNode = nodeType === "acquisition" ? selectedNode.data.node : null;
  const prodNode = nodeType === "production" ? selectedNode.data.node : null;

  const { model, actions } = buildGraphInspector(selectedNode.data, {
    enrichment,
    warnings: nodeWarnings,
    recipeCurrency,
    linkedSettings,
    allFacilities,
    observations,
    worksheetItem,
    pricingContext,
    allowMarketPolicyOverride,
    rootTypeId,
    fulfillmentScope,
    sourcingPending: sourcingPending || Boolean(lifecycle.pending),
    handlers: {
      onBuy:
        prodNode && prodNode.parentComponentTypeId != null
          ? () => onBuildBuy(prodNode.parentComponentTypeId as number)
          : undefined,
      onBuild:
        acqNode && acqNode.buildableRecipe != null
          ? () => onBuyBuild(acqNode.typeId, acqNode.buildableRecipe as RecipeSelection)
          : undefined,
      onScope,
      onOpenLinkedBuild,
      onCopyBuildId,
      onPricingChange,
    },
  });

  // Non-model chrome that stays view-specific: the detail-loading placeholder
  // for a linked node whose persisted Build is still in flight, and the
  // unresolved-build status line.
  const footer: ReactNode = (
    <>
      {nodeType === "production" && !enrichment ? (
        <div className="border-t border-border px-3 py-2">
          {detailError ? (
            <div className="space-y-1.5">
              <p className="text-xs text-muted">Unable to load build details.</p>
              <button className="iw-button-secondary" onClick={onRetryDetail} type="button">
                Retry
              </button>
            </div>
          ) : (
            <p className="text-xs text-muted">
              {detailLoading ? "Loading build details…" : "No build details."}
            </p>
          )}
        </div>
      ) : null}
      {nodeType === "unresolvedBuild" ? (
        <div className="border-t border-border px-3 py-2 text-xs text-muted" role="status">
          {lifecycle.error
            ? `Linked build creation failed — ${lifecycle.error}`
            : lifecycle.pending
              ? "Creating linked build…"
              : "Resolving linked build…"}
        </div>
      ) : null}
    </>
  );

  return <UnifiedItemInspector actions={{ ...actions, footer, onClose }} model={model} readOnly={readOnly} />;
}

function PlanSummary({
  projection,
  opCounts,
}: {
  projection: BuildGraphProjection;
  opCounts: GraphOpCounts;
}) {
  const root = projection.root;
  return (
    <>
      <div className="flex items-center gap-2 px-3 py-2">
        <EveTypeImage size={40} typeId={root.typeId} typeName={root.typeName} variation="render" />
        <div className="min-w-0">
          <div className="truncate text-sm font-semibold">{root.typeName}</div>
          <div className="text-[11px] text-muted">{recipeKindLabel(root.kind)}</div>
        </div>
      </div>
      <InspectorSection defaultExpanded id="plan" label="Plan">
        <InspectorRow label="Runs" value={NUMBER.format(root.runs)} />
        {root.persistedRuns !== root.runs ? (
          <InspectorRow label="Saved Build runs" value={NUMBER.format(root.persistedRuns)} />
        ) : null}
        <InspectorRow label="Material / Component Cost" value={costComponentLine(root.materialComponentCost)} />
        <InspectorRow label="Installation" value={costComponentLine(root.ownInstallationCost)} />
        <InspectorRow label="Total Production Cost" value={costLine(root.costState, root.estimatedCost)} />
        <InspectorRow
          label="Warnings"
          value={projection.warnings.length === 0 ? "None" : NUMBER.format(projection.warnings.length)}
        />
      </InspectorSection>
      <InspectorSection defaultExpanded id="operations" label="Operations">
        <InspectorRow
          label="Breakdown"
          value={`${opCounts.build} Build · ${opCounts.reaction} Reaction · ${opCounts.buy} Buy`}
        />
      </InspectorSection>
    </>
  );
}

// ─── shell ───────────────────────────────────────────────────────────────

export function GraphInspector({
  active,
  projection,
  selectedNode,
  opCounts,
  nodeWarnings,
  rootBuildId,
  onClearSelection,
  onBuyBuild,
  onBuildBuy,
  onOpenLinkedBuild,
  onCopyBuildId,
  onPricingChange,
  onFulfillmentScope,
  fulfillmentScopeByTypeId,
  editor,
  nodeDetail,
  nodeDetailLoading,
  nodeDetailError,
  onRetryNodeDetail,
  linkedSettings,
  observations,
  worksheet,
  pricingContext,
  allowMarketPolicyOverride,
  sourcingPending,
  allFacilities,
  readOnly = false,
}: {
  active: boolean;
  projection: BuildGraphProjection | null;
  selectedNode: BuildGraphNode | undefined;
  opCounts: GraphOpCounts;
  nodeWarnings: GraphWarning[];
  rootBuildId: string | null;
  onClearSelection: () => void;
  onBuyBuild: (typeId: number, recipe: RecipeSelection) => void;
  onBuildBuy: (componentTypeId: number) => void;
  onOpenLinkedBuild: (buildId: string) => void;
  onCopyBuildId: (buildId: string) => void;
  onPricingChange: (selection: PlannerPricingSelection) => void;
  /** Set the shortage-only / full quantity scope for a component of the
   * root Build (the same override the Worksheet writes). */
  onFulfillmentScope: (componentTypeId: number, scope: "missing" | "full") => void;
  /** Current per-component quantity-scope overrides on the root editor. */
  fulfillmentScopeByTypeId: Record<number, "missing" | "full">;
  /** The resident build editor -- the authoritative root state, shared
   * with the Worksheet inspector. */
  editor: BuildWorksheetEditorModel;
  nodeDetail: Build | null;
  nodeDetailLoading: boolean;
  nodeDetailError: string | null;
  onRetryNodeDetail: () => void;
  linkedSettings: BuildSettings | undefined;
  /** Owned blueprint instances for the selected linked manufacturing node. */
  observations: BlueprintObservation[];
  /** Resident preview worksheet -- supplies the material slices for a
   * selected acquisition node (coverage / pricing / value / used-by). */
  worksheet: ProductionWorksheet | null;
  pricingContext: { sourceName: string; sourceRevision: number; capturedAt: string } | null;
  allowMarketPolicyOverride: boolean;
  /** A BUY <-> BUILD switch is in flight. */
  sourcingPending: boolean;
  allFacilities: FacilityProfile[];
  /** An Epic is selected: show values, change nothing. */
  readOnly?: boolean;
}) {
  const titleId = useId();

  useEffect(() => {
    if (!active) return;
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape" && selectedNode) {
        event.preventDefault();
        onClearSelection();
      }
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [active, selectedNode, onClearSelection]);

  if (!active || !projection) return null;

  const title = selectedNode ? selectedNode.data.node.typeName : "Plan Summary";

  let body: ReactNode;
  if (!selectedNode) {
    body = <PlanSummary opCounts={opCounts} projection={projection} />;
  } else {
    const { nodeType } = selectedNode.data;
    const isRoot = nodeType === "root";
    let enrichment: ProductionEnrichment | null = null;
    let recipeCurrency: RecipeCurrency = "current";
    if (nodeType === "root" || nodeType === "production") {
      const n = selectedNode.data.node;
      recipeCurrency = n.recipeCurrency;
      enrichment =
        isRoot || !nodeDetail
          ? null
          : linkedProductionEnrichment(nodeDetail, allFacilities, {
              me: n.effectiveMe,
              te: n.effectiveTe,
            });
    }
    const nodeTypeId = selectedNode.data.node.typeId;
    const worksheetItem =
      (nodeType === "acquisition" || nodeType === "production") && !isRoot && worksheet
        ? [
            ...worksheet.groups.flatMap((group) => group.items),
            ...worksheet.output.items,
          ].find((row) => row.typeId === nodeTypeId && row.role === "material") ?? null
        : null;
    // Quantity scope is the same parent-Build override the Worksheet reads --
    // available for a linked node whose parent IS the root (the only linked
    // Builds the Worksheet can also select).
    const scopeComponentTypeId =
      nodeType === "production" && !isRoot && selectedNode.data.node.parentBuildId === rootBuildId
        ? selectedNode.data.node.parentComponentTypeId
        : null;

    body = (
      <InspectorCollapseProvider>
        <NodeInspectorBody
          readOnly={readOnly}
          allFacilities={allFacilities}
          allowMarketPolicyOverride={allowMarketPolicyOverride}
          detailError={isRoot ? null : nodeDetailError}
          detailLoading={isRoot ? false : nodeDetailLoading}
          enrichment={enrichment}
          fulfillmentScope={
            scopeComponentTypeId != null
              ? fulfillmentScopeByTypeId?.[scopeComponentTypeId] ?? "missing"
              : undefined
          }
          linkedSettings={isRoot ? undefined : linkedSettings}
          nodeWarnings={nodeWarnings}
          observations={observations}
          onBuildBuy={onBuildBuy}
          onBuyBuild={onBuyBuild}
          onClose={onClearSelection}
          onCopyBuildId={onCopyBuildId}
          onOpenLinkedBuild={onOpenLinkedBuild}
          onPricingChange={onPricingChange}
          onRetryDetail={onRetryNodeDetail}
          onScope={
            scopeComponentTypeId != null && onFulfillmentScope
              ? (scope) => onFulfillmentScope(scopeComponentTypeId, scope)
              : undefined
          }
          pricingContext={pricingContext}
          editor={editor}
          recipeCurrency={recipeCurrency}
          rootTypeId={projection.root.typeId}
          selectedNode={selectedNode}
          sourcingPending={sourcingPending}
          worksheetItem={worksheetItem}
        />
      </InspectorCollapseProvider>
    );
  }

  const panel = (
    <aside
      aria-labelledby={titleId}
      className="iw-planner-inspector fixed inset-x-3 bottom-3 z-50 max-h-[70vh] overflow-y-auto border border-border bg-panel shadow-2xl lg:sticky lg:top-0 lg:z-auto lg:max-h-screen lg:border-0 lg:shadow-none lg:w-72"
      data-graph-node-id={selectedNode?.id ?? ""}
      data-testid="graph-inspector"
    >
      <h2 className="sr-only" id={titleId}>
        {title}
      </h2>
      {selectedNode ? null : (
        <div className="px-3 pb-1 pt-3">
          <span className="block text-[10px] font-semibold uppercase text-muted">Graph</span>
        </div>
      )}
      {body}
    </aside>
  );

  const rail = typeof document === "undefined" ? null : document.getElementById("app-right-rail");
  return rail ? createPortal(panel, rail) : panel;
}
