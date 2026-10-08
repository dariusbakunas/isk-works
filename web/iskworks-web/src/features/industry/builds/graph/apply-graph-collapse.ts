// Pure frontend collapse projection:
//
//   complete React Flow nodes + edges  (from toReactFlow)
//        + collapsedNodeIds
//   -> visible nodes + edges + hidden-count per collapsed node
//
// The authoritative graph is never mutated -- collapse is a view filter.
// Dagre only ever sees the visible result, so it never learns what
// "collapsed" means.

import type { Edge } from "@xyflow/react";

import type { BuildGraphEdge, BuildGraphNode } from "./to-react-flow";

export interface CollapsedFlow {
  nodes: BuildGraphNode[];
  edges: BuildGraphEdge[];
  /**
   * `graphNodeId -> count of graph nodes hidden below it`, one entry per
   * collapsed node that is actually present. Compact Buy materials are node
   * data, not graph nodes, so they never count.
   */
  hiddenCountByNodeId: Map<string, number>;
}

function childrenMap(edges: Edge[]): Map<string, string[]> {
  const map = new Map<string, string[]>();
  for (const edge of edges) {
    const list = map.get(edge.source) ?? [];
    list.push(edge.target);
    map.set(edge.source, list);
  }
  return map;
}

/** Every graph node transitively below `nodeId` (excludes `nodeId` itself). */
export function descendantIds(nodeId: string, edges: Edge[]): Set<string> {
  const children = childrenMap(edges);
  const out = new Set<string>();
  const stack = [...(children.get(nodeId) ?? [])];
  while (stack.length > 0) {
    const next = stack.pop() as string;
    if (out.has(next)) continue;
    out.add(next);
    for (const grandChild of children.get(next) ?? []) stack.push(grandChild);
  }
  return out;
}

/** Node ids that have at least one graph child edge (collapse candidates). */
export function nodesWithGraphChildren(edges: Edge[]): Set<string> {
  return new Set(edges.map((edge) => edge.source));
}

/**
 * Filter a complete flow down to what's visible given `collapsedNodeIds`.
 * A collapsed node stays visible; everything that is only reachable through
 * it (production, actionable, unresolved nodes and every edge touching them)
 * is hidden. Canonical producers make the graph a DAG: a producer shared by
 * several consumers stays visible while ANY of its consumers is expanded, so
 * visibility is reachability from the graph's roots through non-collapsed
 * nodes -- for a tree this is exactly "every descendant of a collapsed node
 * is hidden". A collapsed node's own outgoing edges are never drawn.
 * Deterministic; input arrays are not mutated.
 */
export function applyGraphCollapse(
  nodes: BuildGraphNode[],
  edges: BuildGraphEdge[],
  collapsedNodeIds: ReadonlySet<string>,
): CollapsedFlow {
  const present = new Set(nodes.map((node) => node.id));
  const collapsed = new Set([...collapsedNodeIds].filter((id) => present.has(id)));
  const children = childrenMap(edges);
  const hasParent = new Set(edges.map((edge) => edge.target));

  const visible = new Set<string>();
  const stack = nodes.filter((node) => !hasParent.has(node.id)).map((node) => node.id);
  while (stack.length > 0) {
    const next = stack.pop() as string;
    if (visible.has(next)) continue;
    visible.add(next);
    if (collapsed.has(next)) continue;
    for (const child of children.get(next) ?? []) stack.push(child);
  }

  const hiddenCountByNodeId = new Map<string, number>();
  for (const collapsedId of collapsed) {
    let count = 0;
    for (const id of descendantIds(collapsedId, edges)) {
      if (!visible.has(id)) count += 1;
    }
    hiddenCountByNodeId.set(collapsedId, count);
  }

  return {
    nodes: nodes.filter((node) => visible.has(node.id)),
    edges: edges.filter(
      (edge) =>
        visible.has(edge.source) && visible.has(edge.target) && !collapsed.has(edge.source),
    ),
    hiddenCountByNodeId,
  };
}
