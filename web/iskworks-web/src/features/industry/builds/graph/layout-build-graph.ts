// Build Graph layout -- Dagre, top-to-bottom hierarchy.
//
// Public seam: `layoutBuildGraph(nodes, edges)` takes
// flat React Flow nodes + edges and returns positioned copies, so
// `build-graph-view.tsx` stays layout-engine-unaware. Callers pass only the
// *visible* nodes/edges (post-collapse) -- Dagre never learns what
// "collapsed" means and never reserves space for hidden branches.

import dagre from "@dagrejs/dagre";
import type { Edge } from "@xyflow/react";

import { GRAPH_LAYOUT, nodeSize, type NodeSize } from "./graph-layout";
import type { BuildGraphNode } from "./to-react-flow";

/**
 * How each node's box is measured. Defaults to `nodeSize` on the node's own
 * data; `use-build-graph.ts` passes a closure that also folds in the
 * per-node warning chip so Dagre reserves exactly what the shell renders.
 */
export type NodeSizer = (node: BuildGraphNode) => NodeSize;

const defaultSizer: NodeSizer = (node) => nodeSize({ data: node.data });

/**
 * Position every node with Dagre. Pure: returns new node objects, input
 * untouched. Node ids are preserved verbatim. Deterministic for identical
 * input (Dagre's default ordering is stable given a stable node/edge
 * insertion order).
 */
export function layoutBuildGraph(
  nodes: BuildGraphNode[],
  edges: Edge[],
  sizeOf: NodeSizer = defaultSizer,
): BuildGraphNode[] {
  if (nodes.length === 0) return [];

  const graph = new dagre.graphlib.Graph();
  graph.setGraph({
    rankdir: GRAPH_LAYOUT.rankdir,
    nodesep: GRAPH_LAYOUT.nodesep,
    ranksep: GRAPH_LAYOUT.ranksep,
    marginx: GRAPH_LAYOUT.marginx,
    marginy: GRAPH_LAYOUT.marginy,
  });
  graph.setDefaultEdgeLabel(() => ({}));

  const sizes = new Map<string, NodeSize>();
  for (const node of nodes) {
    const size = sizeOf(node);
    sizes.set(node.id, size);
    graph.setNode(node.id, { width: size.width, height: size.height });
  }
  const present = new Set(nodes.map((node) => node.id));
  for (const edge of edges) {
    if (present.has(edge.source) && present.has(edge.target)) {
      graph.setEdge(edge.source, edge.target);
    }
  }

  dagre.layout(graph);

  return nodes.map((node) => {
    const laid = graph.node(node.id);
    const size = sizes.get(node.id) ?? { width: 0, height: 0 };
    // Dagre reports the node centre; React Flow positions the top-left.
    return {
      ...node,
      position: laid
        ? { x: laid.x - size.width / 2, y: laid.y - size.height / 2 }
        : { x: 0, y: 0 },
      width: size.width,
      height: size.height,
    };
  });
}
