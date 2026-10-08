// Pure adapter: domain `BuildGraphProjection` -> flat React Flow
// `Node[]` / `Edge[]`. No React, no layout, no side effects.
//
// The React Flow node id is the domain `graphNodeId` verbatim
// (`root:` / `build:` / `buy:` -- never a fresh client id), so selection and
// diffing key on the same identity the backend emits.

import type { Edge, Node } from "@xyflow/react";

import type {
  AcquisitionNode,
  BuildGraphProjection,
  ProductionNode,
  UnresolvedBuildNode,
} from "../../../../api/industry";

export type BuildGraphNodeData =
  | { nodeType: "root"; depth: number; node: ProductionNode }
  | { nodeType: "production"; depth: number; node: ProductionNode }
  | { nodeType: "acquisition"; depth: number; node: AcquisitionNode }
  | { nodeType: "unresolvedBuild"; depth: number; node: UnresolvedBuildNode };

export type BuildGraphNode = Node<BuildGraphNodeData>;

/** Which kind of node an edge points at -- drives edge styling. */
export type EdgeChildKind =
  | "production"
  | "reaction"
  | "acquisition"
  | "unresolvedBuild";

export interface BuildGraphEdgeData {
  childKind: EdgeChildKind;
  [key: string]: unknown;
}

export type BuildGraphEdge = Edge<BuildGraphEdgeData>;

export interface BuildGraphFlow {
  nodes: BuildGraphNode[];
  edges: BuildGraphEdge[];
  /**
   * `"<parentBuildId>:<componentTypeId>" -> graphNodeId` for every child
   * slot (actionable Buy, unresolved Build, and linked production node).
   * Lets a `buy:` selection survive the transition to `build:<linkedId>`
   * once the linked Build is persisted.
   */
  slotIndex: Map<string, string>;
}

function makeNode(
  id: string,
  data: BuildGraphNodeData,
): BuildGraphNode {
  return {
    id,
    type: data.nodeType,
    position: { x: 0, y: 0 },
    data,
  };
}

/** Flatten a projection into React Flow nodes + edges. Deterministic. */
export function toReactFlow(projection: BuildGraphProjection): BuildGraphFlow {
  const nodes: BuildGraphNode[] = [];
  const edges: BuildGraphEdge[] = [];
  const slotIndex = new Map<string, string>();

  const walkProduction = (node: ProductionNode, depth: number, isRoot: boolean) => {
    nodes.push(
      makeNode(node.graphNodeId, {
        nodeType: isRoot ? "root" : "production",
        depth,
        node,
      }),
    );
    if (!isRoot && node.parentBuildId != null && node.parentComponentTypeId != null) {
      slotIndex.set(`${node.parentBuildId}:${node.parentComponentTypeId}`, node.graphNodeId);
    }
    for (const child of node.children) {
      const childKind: EdgeChildKind =
        child.nodeKind === "production" || child.nodeKind === "producerReference"
          ? child.kind === "reaction" || child.kind === "rootReaction"
            ? "reaction"
            : "production"
          : child.nodeKind;
      edges.push({
        id: `${node.graphNodeId}->${child.graphNodeId}`,
        source: node.graphNodeId,
        target: child.graphNodeId,
        // Bezier (React Flow's "default"): smooth vertical-tangent curves
        // per the design -- no orthogonal elbows.
        type: "default",
        data: { childKind },
      });
      switch (child.nodeKind) {
        case "production":
          walkProduction(child, depth + 1, false);
          break;
        case "acquisition":
          nodes.push(
            makeNode(child.graphNodeId, {
              nodeType: "acquisition",
              depth: depth + 1,
              node: child,
            }),
          );
          slotIndex.set(`${child.parentBuildId}:${child.typeId}`, child.graphNodeId);
          break;
        case "unresolvedBuild":
          nodes.push(
            makeNode(child.graphNodeId, {
              nodeType: "unresolvedBuild",
              depth: depth + 1,
              node: child,
            }),
          );
          slotIndex.set(`${child.parentBuildId}:${child.typeId}`, child.graphNodeId);
          break;
        case "producerReference":
          // A canonical producer shared by several consumers is ONE node
          // (drawn in full under its first consumer): this consumer only
          // adds its edge to that same node -- never a duplicate node, never
          // its runs/output/surplus/cost again.
          slotIndex.set(`${child.parentBuildId}:${child.typeId}`, child.graphNodeId);
          break;
      }
    }
  };

  walkProduction(projection.root, 0, true);
  return { nodes, edges, slotIndex };
}
