// The Build Graph view: styling, root-level Buy/Build wiring through the
// existing editor path, edge semantics, full-height canvas. The graph
// pipeline (toReactFlow -> applyGraphCollapse -> layoutBuildGraph -> React
// Flow) and its collapse/selection interaction state live elsewhere --
// this file only presents and wires.

import "@xyflow/react/dist/style.css";

import { Maximize2, Minus, Plus, Crosshair } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Background,
  ReactFlow,
  type Edge,
  type Node,
  type NodeMouseHandler,
  type ReactFlowInstance,
} from "@xyflow/react";

import {
  listBlueprintObservations,
  type BlueprintObservation,
  type PlannerPricingSelection,
  type RecipeSelection,
} from "../../../../api/industry";
import { InlineAlert } from "../../../../components/primitives";
import {
  applyComponentResolution,
  applyFulfillmentScope,
  applyPricingSelection,
} from "../components/planner-panels";
import { useBuildSettings } from "../use-build-settings";
import type { BuildWorksheetEditorModel } from "../use-build-worksheet-editor";

import { buildGraphNodeTypes } from "./graph-node";
import { descendantIds } from "./apply-graph-collapse";
import { GraphInspector, type GraphOpCounts } from "./graph-inspector";
import { isCardWarning } from "./graph-warnings";
import type { BuildGraphEdge, BuildGraphNode } from "./to-react-flow";
import { useBuildGraph } from "./use-build-graph";
import { useGraphNodeDetail } from "./use-graph-node-detail";
import { useGraphSourcing } from "./use-graph-sourcing";

/** graphNodeIds on the path root -> selected (inclusive), for edge emphasis. */
function selectedPath(selectedId: string | null, edges: BuildGraphEdge[]): Set<string> {
  if (!selectedId) return new Set();
  const parentOf = new Map(edges.map((edge) => [edge.target, edge.source]));
  const path = new Set<string>([selectedId]);
  let cursor: string | undefined = selectedId;
  while (cursor && parentOf.has(cursor)) {
    cursor = parentOf.get(cursor);
    if (cursor) path.add(cursor);
  }
  return path;
}

function styleEdge(edge: BuildGraphEdge, onPath: boolean): Edge {
  const kind = edge.data?.childKind ?? "production";
  const quiet = kind === "acquisition" || kind === "unresolvedBuild";
  const stroke =
    kind === "reaction"
      ? "var(--color-reaction)"
      : quiet
        ? "var(--color-muted)"
        : "var(--color-primary)";
  return {
    ...edge,
    style: {
      stroke,
      strokeWidth: onPath ? (quiet ? 1.5 : 2.25) : quiet ? 1 : 1.5,
      strokeDasharray: quiet ? "4 3" : undefined,
      opacity: onPath ? 0.95 : quiet ? 0.35 : 0.55,
    },
  };
}

export function BuildGraphView({
  editor,
  active,
  focusedProducerId,
}: {
  editor: BuildWorksheetEditorModel;
  active: boolean;
  focusedProducerId?: string;
}) {
  const buildId = editor.initialBuild?.id ?? "";
  const graph = useBuildGraph({
    buildId,
    previewKey: editor.previewKey,
    active,
    linkedBuildsByTypeId: editor.linkedBuildsByTypeId, linkedBuildsSettling: editor.linkedBuildsSettling,
  });
  const focusedFlow = useMemo(() => {
    if (!focusedProducerId) return graph.flow;
    const focusNode = graph.flow.complete.nodes.find(
      (node) => "buildId" in node.data.node && node.data.node.buildId === focusedProducerId,
    );
    if (!focusNode) return { ...graph.flow, positioned: [], edges: [], complete: { ...graph.flow.complete, nodes: [], edges: [] } };
    const retained = descendantIds(focusNode.id, graph.flow.complete.edges);
    retained.add(focusNode.id);
    return {
      ...graph.flow,
      positioned: graph.flow.positioned.filter((node) => retained.has(node.id)),
      edges: graph.flow.edges.filter((edge) => retained.has(edge.source) && retained.has(edge.target)),
      complete: {
        ...graph.flow.complete,
        nodes: graph.flow.complete.nodes.filter((node) => retained.has(node.id)),
        edges: graph.flow.complete.edges.filter((edge) => retained.has(edge.source) && retained.has(edge.target)),
      },
    };
  }, [focusedProducerId, graph.flow]);

  const {
    linkedBuildPending,
    linkedBuildErrors,
    setComponentResolutions,
    setPrices,
    setItemPricingPolicies,
    navigate,
  } = editor;
  const { setSelectedGraphNodeId: setGraphSelection, toggleCollapse } = graph;
  // One right-rail inspector at a time: selecting a
  // node closes Build settings; opening Build settings clears the node.
  const closeSettings = editor.closeInspector;
  const setSelectedGraphNodeId = useCallback(
    (next: string | null) => {
      setGraphSelection(next);
      if (next) closeSettings?.();
    },
    [setGraphSelection, closeSettings],
  );
  const settingsOpen = editor.inspectorMode?.kind === "buildSettings";
  useEffect(() => {
    if (settingsOpen) setGraphSelection(null);
  }, [settingsOpen, setGraphSelection]);
  // Build-ID-addressable sourcing for *non-root* nodes -- targets the Build
  // that owns the requirement, never the root editor.
  // A write re-projects everything keyed on the
  // editor's preview key -- the persistent Build economics, Plan,
  // Logistics and this Graph -- not just the Graph.
  const replan = editor.bumpPreview ?? graph.refetch;
  const sourcing = useGraphSourcing(replan);

  const rfRef = useRef<ReactFlowInstance<BuildGraphNode, Edge> | null>(null);
  const fittedRef = useRef(false);

  useEffect(() => {
    if (!active || !graph.projection || fittedRef.current || !rfRef.current) return;
    rfRef.current.fitView({ duration: 0 });
    fittedRef.current = true;
  }, [active, graph.projection]);

  const onBuyBuild = useCallback(
    (typeId: number, recipe: RecipeSelection) =>
      applyComponentResolution(typeId, recipe, setComponentResolutions),
    [setComponentResolutions],
  );
  const onBuildBuy = useCallback(
    (componentTypeId: number) => applyComponentResolution(componentTypeId, null, setComponentResolutions),
    [setComponentResolutions],
  );
  const onFulfillmentScope = useCallback(
    (componentTypeId: number, scope: "missing" | "full") =>
      applyFulfillmentScope(
        componentTypeId,
        scope === "full" ? "full" : null,
        editor.setFulfillmentScopes,
      ),
    [editor.setFulfillmentScopes],
  );
  const onOpenLinkedBuild = useCallback(
    (linkedBuildId: string) => navigate(`/builds/${buildId}/producers/${linkedBuildId}?view=graph`),
    [navigate, buildId],
  );
  const onCopyBuildId = useCallback((id: string) => {
    void navigator.clipboard?.writeText(id);
  }, []);
  const onPricingChange = useCallback(
    (selection: PlannerPricingSelection) =>
      applyPricingSelection(selection, setPrices, setItemPricingPolicies),
    [setPrices, setItemPricingPolicies],
  );
  const rootBuildId = graph.projection?.root.buildId ?? null;
  const rootNodeId = rootBuildId ? `root:${rootBuildId}` : null;

  const nodes = useMemo<Node[]>(
    () =>
      focusedFlow.positioned.map((node) => {
        // Root is collapsible in topology but the control is suppressed --
        // collapsing it hides the whole graph for no gain.
        const collapsible =
          focusedFlow.collapsibleNodeIds.has(node.id) && node.data.nodeType !== "root";
        const warnings = focusedFlow.warningsByNodeId.get(node.id) ?? [];
        const cardWarnings = warnings.filter((w) => isCardWarning(w.code));
        let lifecycle: { pending?: boolean; error?: string | null } = {};
        if (node.data.nodeType === "unresolvedBuild") {
          const buyNode = node.data.node;
          // Root slot: the editor owns the lifecycle. Nested slot: `sourcing`.
          lifecycle =
            buyNode.parentBuildId === rootBuildId
              ? {
                  pending: Boolean(linkedBuildPending[buyNode.typeId]),
                  error: linkedBuildErrors[buyNode.typeId] ?? null,
                }
              : {
                  pending: Boolean(sourcing.pendingByNodeId[node.id]),
                  error: sourcing.errorByNodeId[node.id] ?? null,
                };
        } else if (node.data.nodeType === "acquisition") {
          lifecycle = {
            pending: Boolean(sourcing.pendingByNodeId[node.id]),
            error: sourcing.errorByNodeId[node.id] ?? null,
          };
        }
        let onNodeBuyBuild: (() => void) | undefined;
        if (node.data.nodeType === "acquisition") {
          const buyNode = node.data.node;
          const recipe = buyNode.buildableRecipe;
          if (recipe != null) {
            onNodeBuyBuild =
              buyNode.parentBuildId === rootBuildId
                ? () => onBuyBuild(buyNode.typeId, recipe)
                : () =>
                    sourcing.switchToBuild(
                      node.id,
                      buyNode.parentBuildId,
                      buyNode.typeId,
                      recipe,
                    );
          }
        }
        return {
          ...node,
          selected: node.id === graph.selectedGraphNodeId,
          data: {
            ...node.data,
            ...lifecycle,
            warnings: cardWarnings,
            collapsible,
            collapsed: graph.collapsedNodeIds.has(node.id),
            hiddenCount: focusedFlow.hiddenCountByNodeId.get(node.id) ?? 0,
            onToggleCollapse: collapsible ? () => toggleCollapse(node.id) : undefined,
            onSelect: () => setSelectedGraphNodeId(node.id),
            onBuyBuild: onNodeBuyBuild,
          },
        };
      }),
    [
      focusedFlow.positioned,
      focusedFlow.collapsibleNodeIds,
      focusedFlow.hiddenCountByNodeId,
      focusedFlow.warningsByNodeId,
      graph.collapsedNodeIds,
      graph.selectedGraphNodeId,
      rootBuildId,
      toggleCollapse,
      setSelectedGraphNodeId,
      onBuyBuild,
      linkedBuildPending,
      linkedBuildErrors,
      sourcing,
    ],
  );

  const edges = useMemo<Edge[]>(() => {
    const onPath = selectedPath(graph.selectedGraphNodeId, focusedFlow.edges);
    return focusedFlow.edges.map((edge) =>
      styleEdge(edge, onPath.has(edge.source) && onPath.has(edge.target)),
    );
  }, [focusedFlow.edges, graph.selectedGraphNodeId]);

  const selectedNode = useMemo(
    () => focusedFlow.complete.nodes.find((node) => node.id === graph.selectedGraphNodeId),
    [focusedFlow.complete.nodes, graph.selectedGraphNodeId],
  );

  // Lazily load the persisted Build behind a selected *linked*
  // Production node (root / buy / unresolved never fetch). Cached by
  // buildId for this view's mount lifetime; race- and inactive-safe.
  const nodeDetail = useGraphNodeDetail({ active, selectedNode });

  // In-place "common settings" editing for a selected *linked* Production
  // node -- targets that Build by id (never the root editor). A successful
  // patch re-projects the graph.
  const selectedLinkedNode =
    active &&
    selectedNode?.data.nodeType === "production" &&
    selectedNode.data.node.buildId !== rootBuildId
      ? selectedNode.data.node
      : null;
  const selectedLinkedBuildId = selectedLinkedNode?.buildId ?? null;
  // Seed from the freshest copy of this Build available -- the editor's
  // linked-build map (kept in step with Worksheet-side edits) if it holds
  // this build id, else the one `useGraphNodeDetail` fetched. Either way no
  // extra GET for the common case.
  const editorLinkedCopy =
    selectedLinkedNode?.parentComponentTypeId != null
      ? editor.linkedBuildsByTypeId?.[selectedLinkedNode.parentComponentTypeId]
      : undefined;
  const linkedSettings = useBuildSettings(
    selectedLinkedBuildId,
    replan,
    selectedLinkedBuildId
      ? editorLinkedCopy?.id === selectedLinkedBuildId
        ? editorLinkedCopy
        : nodeDetail.detail
      : null,
  );

  // A linked Build's settings edited here (by id) must also reach the
  // Worksheet inspector, which reads the editor's linked-build map -- adopt
  // the freshest copy of this Build back into it after every load/patch.
  useEffect(() => {
    if (linkedSettings.build) editor.adoptLinkedBuild(linkedSettings.build);
  }, [linkedSettings.build, editor.adoptLinkedBuild]);

  // Owned blueprint instances for the selected linked *manufacturing* node,
  // so its Blueprint section can offer "Use existing blueprint".
  const selectedBlueprintTypeId =
    active &&
    selectedNode?.data.nodeType === "production" &&
    selectedNode.data.node.recipe.mode === "manufacturing"
      ? selectedNode.data.node.recipe.blueprintTypeId
      : null;
  const [observations, setObservations] = useState<BlueprintObservation[]>([]);
  useEffect(() => {
    if (selectedBlueprintTypeId == null) {
      setObservations([]);
      return;
    }
    let cancelled = false;
    listBlueprintObservations(selectedBlueprintTypeId)
      .then((rows) => {
        if (!cancelled) setObservations(rows);
      })
      .catch(() => {
        if (!cancelled) setObservations([]);
      });
    return () => {
      cancelled = true;
    };
  }, [selectedBlueprintTypeId]);

  const opCounts = useMemo<GraphOpCounts>(() => {
    let build = 0;
    let reaction = 0;
    let buy = 0;
    for (const node of graph.flow.complete.nodes) {
      if (node.data.nodeType === "production") {
        if (node.data.node.kind === "reaction" || node.data.node.kind === "rootReaction") {
          reaction += 1;
        } else {
          build += 1;
        }
      } else if (node.data.nodeType === "acquisition") {
        buy += 1;
      }
    }
    return { build, reaction, buy };
  }, [graph.flow.complete.nodes]);

  const nodeWarnings = useMemo(
    () =>
      (selectedNode && graph.flow.warningsByNodeId.get(selectedNode.id)) ?? [],
    [selectedNode, graph.flow.warningsByNodeId],
  );

  // The inspector's BUY<->BUILD buttons act on the *currently selected* node,
  // so route them the same way the card does: root -> editor, nested ->
  // `sourcing` (owning Build).
  const inspectorBuyBuild = useCallback(
    (typeId: number, recipe: RecipeSelection) => {
      const data = selectedNode?.data;
      if (data?.nodeType === "acquisition" && data.node.parentBuildId !== rootBuildId) {
        sourcing.switchToBuild(selectedNode!.id, data.node.parentBuildId, typeId, recipe);
      } else {
        onBuyBuild(typeId, recipe);
      }
    },
    [selectedNode, rootBuildId, sourcing, onBuyBuild],
  );
  const inspectorBuildBuy = useCallback(
    (componentTypeId: number) => {
      const data = selectedNode?.data;
      if (
        data?.nodeType === "production" &&
        data.node.parentBuildId &&
        data.node.parentBuildId !== rootBuildId
      ) {
        sourcing.switchToBuy(selectedNode!.id, data.node.parentBuildId, componentTypeId);
      } else {
        onBuildBuy(componentTypeId);
      }
    },
    [selectedNode, rootBuildId, sourcing, onBuildBuy],
  );

  const onNodeClick = useCallback<NodeMouseHandler>(
    (_event, node) => setSelectedGraphNodeId(node.id),
    [setSelectedGraphNodeId],
  );

  const centerSelected = useCallback(() => {
    if (graph.selectedGraphNodeId) {
      rfRef.current?.fitView({ nodes: [{ id: graph.selectedGraphNodeId }], duration: 200, maxZoom: 1 });
    }
  }, [graph.selectedGraphNodeId]);

  const warnings = graph.projection?.warnings ?? [];
  const showStatus =
    active && !graph.projection ? (graph.hardError ? "hard-error" : "empty") : null;

  if (active && !buildId) {
    return (
      <InlineAlert title="Graph unavailable">
        Save this Build before opening the Graph view.
      </InlineAlert>
    );
  }

  return (
    <section aria-label="Build graph" className="mt-4 space-y-3">
      {showStatus === "hard-error" ? (
        <InlineAlert title="Couldn’t load graph">{graph.hardError}</InlineAlert>
      ) : null}
      {showStatus === "empty" ? (
        <p className="border-y border-border py-5 text-sm text-muted" role="status">
          {graph.loading ? "Loading graph…" : "The graph will appear here."}
        </p>
      ) : null}

      {graph.projection ? (
        <>
          {active && graph.refreshError ? (
            <p className="text-xs text-muted" role="status">
              Couldn’t refresh graph — showing the last version.
            </p>
          ) : null}
          <div
            className="relative w-full rounded border border-border"
            data-testid="build-graph-canvas"
            style={{ height: "min(72vh, 820px)", minHeight: 520 }}
          >
            <ReactFlow
              aria-label="Build graph canvas"
              edges={edges}
              minZoom={0.1}
              nodes={nodes}
              nodesConnectable={false}
              nodesDraggable={false}
              nodeTypes={buildGraphNodeTypes}
              onInit={(instance) => {
                rfRef.current = instance as unknown as ReactFlowInstance<BuildGraphNode, Edge>;
              }}
              onNodeClick={onNodeClick}
              onPaneClick={() => setSelectedGraphNodeId(null)}
              proOptions={{ hideAttribution: true }}
            >
              <Background />
            </ReactFlow>
            {active ? (
              <div
                aria-label="Graph controls"
                className="absolute bottom-3 right-3 z-10 flex flex-col gap-1 rounded border border-border bg-panel p-1 text-muted shadow"
                role="group"
              >
                <button
                  aria-label="Zoom in"
                  className="grid h-7 w-7 place-items-center rounded hover:bg-panel-strong hover:text-foreground"
                  onClick={() => rfRef.current?.zoomIn({ duration: 150 })}
                  type="button"
                >
                  <Plus aria-hidden="true" className="h-4 w-4" />
                </button>
                <button
                  aria-label="Zoom out"
                  className="grid h-7 w-7 place-items-center rounded hover:bg-panel-strong hover:text-foreground"
                  onClick={() => rfRef.current?.zoomOut({ duration: 150 })}
                  type="button"
                >
                  <Minus aria-hidden="true" className="h-4 w-4" />
                </button>
                <button
                  aria-label="Fit graph"
                  className="grid h-7 w-7 place-items-center rounded hover:bg-panel-strong hover:text-foreground"
                  onClick={() => rfRef.current?.fitView({ duration: 200 })}
                  type="button"
                >
                  <Maximize2 aria-hidden="true" className="h-4 w-4" />
                </button>
                {graph.selectedGraphNodeId ? (
                  <button
                    aria-label="Center selected node"
                    className="grid h-7 w-7 place-items-center rounded hover:bg-panel-strong hover:text-foreground"
                    onClick={centerSelected}
                    type="button"
                  >
                    <Crosshair aria-hidden="true" className="h-4 w-4" />
                  </button>
                ) : null}
              </div>
            ) : null}
          </div>
          {active && warnings.length > 0 ? (
            <div className="rounded border border-border p-3" data-testid="graph-warnings">
              <h3 className="mb-2 text-xs font-semibold uppercase tracking-wide text-muted">
                Warnings
              </h3>
              <ul className="space-y-1 text-xs">
                {warnings.map((warning, index) => (
                  <li key={`${warning.graphNodeId}-${warning.code}-${index}`}>
                    <span className="font-mono">{warning.code}</span> — {warning.message}
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
        </>
      ) : null}

      <GraphInspector
        active={active}
        allFacilities={editor.allFacilities ?? []}
        allowMarketPolicyOverride={editor.source?.kind !== "manual"}
        linkedSettings={selectedLinkedBuildId ? linkedSettings : undefined}
        nodeDetail={nodeDetail.detail}
        nodeDetailError={nodeDetail.error}
        nodeDetailLoading={nodeDetail.loading}
        nodeWarnings={nodeWarnings}
        observations={observations}
        onBuildBuy={inspectorBuildBuy}
        onBuyBuild={inspectorBuyBuild}
        editor={editor}
        fulfillmentScopeByTypeId={editor.fulfillmentScopes ?? {}}
        onClearSelection={() => setSelectedGraphNodeId(null)}
        onCopyBuildId={onCopyBuildId}
        onFulfillmentScope={onFulfillmentScope}
        onOpenLinkedBuild={onOpenLinkedBuild}
        onPricingChange={onPricingChange}
        onRetryNodeDetail={nodeDetail.retry}
        opCounts={opCounts}
        pricingContext={null}
        projection={graph.projection}
        rootBuildId={rootNodeId ? rootBuildId : null}
        selectedNode={selectedNode}
        sourcingPending={selectedNode ? Boolean(sourcing.pendingByNodeId[selectedNode.id]) : false}
        worksheet={editor.estimate?.worksheet ?? null}
      />
    </section>
  );
}
