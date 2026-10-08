import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  previewBuildGraph,
  type BuildGraphProjection,
  type GraphWarning,
  type PreviewBuildPlanCommand,
} from "../../../../api/industry";
import { apiMessage } from "../../shared/api-error";

import {
  applyGraphCollapse,
  descendantIds,
  nodesWithGraphChildren,
} from "./apply-graph-collapse";
import { nodeSize } from "./graph-layout";
import { resolveGraphNodeIdentity } from "./graph-slot-identity";
import { warningsByNodeId } from "./graph-warnings";
import { layoutBuildGraph } from "./layout-build-graph";
import {
  toReactFlow,
  type BuildGraphEdge,
  type BuildGraphFlow,
  type BuildGraphNode,
} from "./to-react-flow";

const REFRESH_DEBOUNCE_MS = 250;
const EMPTY_FLOW: BuildGraphFlow = { nodes: [], edges: [], slotIndex: new Map() };

export interface UseBuildGraphArgs {
  buildId: string;
  /** The editor's `previewKey` -- a `JSON.stringify` of the exact
   * `PreviewBuildPlanCommand` a preview sends. */
  previewKey: string;
  /** Only fetch while the Graph tab is the active view. */
  active: boolean;
  /** The editor-owned linked-build lifecycle map (only `.id` is read). A
   * new/changed entry refetches so an `unresolvedBuild` slot becomes a real
   * `production` node. No polling. */
  linkedBuildsByTypeId: Record<number, { id: string }>;
  /** A linked-build create/reuse is in flight. The server-side plan is about
   * to change, so hold the fetch until it settles rather than sending one
   * request per linked build and aborting all but the last. */
  linkedBuildsSettling?: boolean;
}

export interface BuildGraphFlowState {
  /** The complete authoritative topology (never collapse-filtered). */
  complete: BuildGraphFlow;
  /** Visible nodes, Dagre-positioned. */
  positioned: BuildGraphNode[];
  /** Visible edges. */
  edges: BuildGraphEdge[];
  /** `graphNodeId -> hidden graph-node count`, per collapsed node. */
  hiddenCountByNodeId: Map<string, number>;
  /** Node ids that have graph children (eligible for a collapse control). */
  collapsibleNodeIds: Set<string>;
  /** `graphNodeId -> warnings targeting it`, from `projection.warnings`. */
  warningsByNodeId: Map<string, GraphWarning[]>;
}

export interface UseBuildGraphResult {
  projection: BuildGraphProjection | null;
  flow: BuildGraphFlowState;
  loading: boolean;
  refreshError: string | null;
  hardError: string | null;
  refetch: () => void;
  selectedGraphNodeId: string | null;
  setSelectedGraphNodeId: (id: string | null) => void;
  collapsedNodeIds: ReadonlySet<string>;
  toggleCollapse: (id: string) => void;
}

export function useBuildGraph({
  buildId,
  previewKey,
  active,
  linkedBuildsByTypeId,
  linkedBuildsSettling = false,
}: UseBuildGraphArgs): UseBuildGraphResult {
  const [projection, setProjection] = useState<BuildGraphProjection | null>(null);
  const [loading, setLoading] = useState(false);
  const [refreshError, setRefreshError] = useState<string | null>(null);
  const [hardError, setHardError] = useState<string | null>(null);
  const [selectedGraphNodeId, setSelectedGraphNodeId] = useState<string | null>(null);
  // Non-root Production/Reaction nodes are collapsed by *default* (density
  // control). This set holds only the ones the user has explicitly expanded;
  // everything else with graph children is collapsed. Carried across a
  // re-projection by slot identity.
  const [userExpandedIds, setUserExpandedIds] = useState<ReadonlySet<string>>(new Set());
  const [refetchToken, setRefetchToken] = useState(0);

  const projectionRef = useRef<BuildGraphProjection | null>(null);
  projectionRef.current = projection;
  const selectedRef = useRef<string | null>(null);
  selectedRef.current = selectedGraphNodeId;

  const linkedSignal = useMemo(
    () =>
      Object.entries(linkedBuildsByTypeId)
        .map(([typeId, build]) => `${typeId}:${build.id}`)
        .sort()
        .join(","),
    [linkedBuildsByTypeId],
  );

  const refetch = useCallback(() => setRefetchToken((token) => token + 1), []);

  const toggleCollapse = useCallback((id: string) => {
    setUserExpandedIds((previous) => {
      const next = new Set(previous);
      if (next.has(id)) {
        // Explicitly expanded -> collapse it back to the default.
        next.delete(id);
        // Selection must never point at a now-hidden descendant: pull it up
        // to the collapsed ancestor. A selected collapsed node stays
        // selected (it isn't its own descendant).
        const selected = selectedRef.current;
        if (selected && projectionRef.current) {
          const edges = toReactFlow(projectionRef.current).edges;
          if (descendantIds(id, edges).has(selected)) setSelectedGraphNodeId(id);
        }
        return next;
      }
      next.add(id);
      return next;
    });
  }, []);

  useEffect(() => {
    if (!active || !previewKey) return;
    if (linkedBuildsSettling) {
      setLoading(true);
      return;
    }

    let command: PreviewBuildPlanCommand;
    try {
      command = JSON.parse(previewKey) as PreviewBuildPlanCommand;
    } catch {
      return;
    }

    const controller = new AbortController();
    setLoading(true);
    const timer = window.setTimeout(() => {
      void previewBuildGraph(buildId, command, controller.signal)
        .then((next) => {
          if (controller.signal.aborted) return;
          carryInteractionState(projectionRef.current, next);
          setProjection(next);
          setRefreshError(null);
          setHardError(null);
        })
        .catch((error) => {
          if (controller.signal.aborted) return;
          const message = apiMessage(error);
          // Last-known-good: only blank the view when there is nothing to keep.
          if (projectionRef.current) setRefreshError(message);
          else setHardError(message);
        })
        .finally(() => {
          if (!controller.signal.aborted) setLoading(false);
        });
    }, REFRESH_DEBOUNCE_MS);

    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };

    // Carry selection + collapse across the refresh using the one slot
    // identity resolver: an unchanged id stays; a `buy:` slot follows to its
    // `build:<linkedId>`; a slot that's gone is dropped.
    function carryInteractionState(
      previous: BuildGraphProjection | null,
      nextProjection: BuildGraphProjection,
    ) {
      const nextFlow = toReactFlow(nextProjection);
      const previousSlotIndex = previous ? toReactFlow(previous).slotIndex : new Map();

      const selected = selectedRef.current;
      if (selected) {
        setSelectedGraphNodeId(
          resolveGraphNodeIdentity(selected, previousSlotIndex, nextFlow),
        );
      }

      // An explicit-expand entry survives only if it resolves to a node that
      // still has graph children -- a `build:` that flipped to a childless
      // `buy:` slot is pruned so no stale expand state lingers.
      const stillCollapsible = nodesWithGraphChildren(nextFlow.edges);
      setUserExpandedIds((previousExpanded) => {
        const next = new Set<string>();
        for (const id of previousExpanded) {
          const resolved = resolveGraphNodeIdentity(id, previousSlotIndex, nextFlow);
          if (resolved && stillCollapsible.has(resolved)) next.add(resolved);
        }
        return next.size === previousExpanded.size &&
          [...next].every((id) => previousExpanded.has(id))
          ? previousExpanded
          : next;
      });
    }
  }, [buildId, previewKey, active, linkedSignal, linkedBuildsSettling, refetchToken]);

  // Every non-root node with graph children is collapsed unless the user
  // explicitly expanded it.
  const collapsedNodeIds = useMemo<ReadonlySet<string>>(() => {
    if (!projection) return new Set();
    const complete = toReactFlow(projection);
    const withChildren = nodesWithGraphChildren(complete.edges);
    const next = new Set<string>();
    for (const node of complete.nodes) {
      if (
        node.data.nodeType !== "root" &&
        withChildren.has(node.id) &&
        !userExpandedIds.has(node.id)
      ) {
        next.add(node.id);
      }
    }
    return next;
  }, [projection, userExpandedIds]);

  const flow = useMemo<BuildGraphFlowState>(() => {
    const complete = projection ? toReactFlow(projection) : EMPTY_FLOW;
    const collapsed = applyGraphCollapse(complete.nodes, complete.edges, collapsedNodeIds);
    const warnings = warningsByNodeId(projection?.warnings ?? []);
    const positioned = layoutBuildGraph(collapsed.nodes, collapsed.edges, (node) =>
      nodeSize({
        data: node.data,
        warnings: warnings.get(node.id),
        collapsedSummary: collapsedNodeIds.has(node.id),
      }),
    );
    return {
      complete,
      positioned,
      edges: collapsed.edges,
      hiddenCountByNodeId: collapsed.hiddenCountByNodeId,
      collapsibleNodeIds: nodesWithGraphChildren(complete.edges),
      warningsByNodeId: warnings,
    };
  }, [projection, collapsedNodeIds]);

  return {
    projection,
    flow,
    loading,
    refreshError,
    hardError,
    refetch,
    selectedGraphNodeId,
    setSelectedGraphNodeId,
    collapsedNodeIds,
    toggleCollapse,
  };
}
