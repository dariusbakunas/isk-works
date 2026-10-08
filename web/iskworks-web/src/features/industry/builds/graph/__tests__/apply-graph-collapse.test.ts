import { describe, expect, it } from "vitest";

import {
  applyGraphCollapse,
  descendantIds,
  nodesWithGraphChildren,
} from "../apply-graph-collapse";
import { toReactFlow } from "../to-react-flow";

import { ROOT_BUILD_ID, branchingProjection, nestedProjection } from "./fixtures";

const ROOT = `root:${ROOT_BUILD_ID}`;

describe("applyGraphCollapse", () => {
  it("returns the topology unchanged when nothing is collapsed", () => {
    const flow = toReactFlow(branchingProjection());
    const result = applyGraphCollapse(flow.nodes, flow.edges, new Set());
    expect(result.nodes.map((n) => n.id)).toEqual(flow.nodes.map((n) => n.id));
    expect(result.edges.map((e) => e.id)).toEqual(flow.edges.map((e) => e.id));
    expect(result.hiddenCountByNodeId.size).toBe(0);
  });

  it("is a no-op when a leaf node is collapsed", () => {
    const flow = toReactFlow(branchingProjection());
    const result = applyGraphCollapse(flow.nodes, flow.edges, new Set(["build:B"]));
    expect(result.nodes.map((n) => n.id)).toEqual(flow.nodes.map((n) => n.id));
    expect(result.hiddenCountByNodeId.get("build:B")).toBe(0);
  });

  it("collapsing A hides B, C and the actionable Buy under A; A stays visible", () => {
    const flow = toReactFlow(branchingProjection());
    const result = applyGraphCollapse(flow.nodes, flow.edges, new Set(["build:A"]));
    const visible = result.nodes.map((n) => n.id);
    expect(visible).toContain("build:A");
    expect(visible).not.toContain("build:B");
    expect(visible).not.toContain("build:C");
    expect(visible).not.toContain("buy:A:210");
    // Sibling branch D is untouched.
    expect(visible).toContain("build:D");
    expect(visible).toContain(ROOT);
  });

  it("removes every edge into the hidden subtree but keeps the edge to the collapsed node", () => {
    const flow = toReactFlow(branchingProjection());
    const result = applyGraphCollapse(flow.nodes, flow.edges, new Set(["build:A"]));
    const edgeIds = result.edges.map((e) => e.id);
    expect(edgeIds).toContain(`${ROOT}->build:A`);
    expect(edgeIds).toContain(`${ROOT}->build:D`);
    expect(edgeIds).not.toContain("build:A->build:B");
    expect(edgeIds).not.toContain("build:A->build:C");
    expect(edgeIds).not.toContain("build:A->buy:A:210");
  });

  it("hidden count = every graph node below the collapsed node (not compact materials)", () => {
    const flow = toReactFlow(branchingProjection());
    const result = applyGraphCollapse(flow.nodes, flow.edges, new Set(["build:A"]));
    // B, C and the actionable Buy = 3 graph nodes. Tritanium is a compact
    // material on A, not a graph node, so it is not counted.
    expect(result.hiddenCountByNodeId.get("build:A")).toBe(3);
  });

  it("nested collapse: the outer count still includes the inner collapsed subtree", () => {
    const flow = toReactFlow(branchingProjection());
    const result = applyGraphCollapse(
      flow.nodes,
      flow.edges,
      new Set(["build:A", "build:B"]),
    );
    expect(result.nodes.map((n) => n.id)).not.toContain("build:B");
    expect(result.hiddenCountByNodeId.get("build:A")).toBe(3);
  });

  it("ignores a collapsed id that is not present (stale)", () => {
    const flow = toReactFlow(branchingProjection());
    const result = applyGraphCollapse(flow.nodes, flow.edges, new Set(["build:gone"]));
    expect(result.nodes.map((n) => n.id)).toEqual(flow.nodes.map((n) => n.id));
    expect(result.hiddenCountByNodeId.has("build:gone")).toBe(false);
  });

  it("is deterministic and does not mutate its inputs", () => {
    const flow = toReactFlow(branchingProjection());
    const nodesBefore = JSON.stringify(flow.nodes);
    const edgesBefore = JSON.stringify(flow.edges);
    const a = applyGraphCollapse(flow.nodes, flow.edges, new Set(["build:A"]));
    const b = applyGraphCollapse(flow.nodes, flow.edges, new Set(["build:A"]));
    expect(a.nodes.map((n) => n.id)).toEqual(b.nodes.map((n) => n.id));
    expect(a.edges.map((e) => e.id)).toEqual(b.edges.map((e) => e.id));
    expect(JSON.stringify(flow.nodes)).toBe(nodesBefore);
    expect(JSON.stringify(flow.edges)).toBe(edgesBefore);
  });
});

describe("applyGraphCollapse over a canonical shared producer (DAG)", () => {
  // root -> A -> P, root -> B -> P: P is ONE canonical producer node.
  const node = (id: string) => ({
    id,
    type: "production",
    position: { x: 0, y: 0 },
    data: { nodeType: "production", depth: 1, node: {} },
  });
  const edge = (source: string, target: string) => ({
    id: `${source}->${target}`,
    source,
    target,
    data: { childKind: "production" },
  });
  const nodes = [node("root:r"), node("build:A"), node("build:B"), node("build:P"), node("build:Q")];
  const edges = [
    edge("root:r", "build:A"),
    edge("root:r", "build:B"),
    edge("build:A", "build:P"),
    edge("build:B", "build:P"),
    edge("build:P", "build:Q"),
  ];

  it("keeps a shared producer visible while any of its consumers is expanded", () => {
    const result = applyGraphCollapse(
      nodes as never,
      edges as never,
      new Set(["build:A"]),
    );
    const visible = result.nodes.map((n) => n.id);
    expect(visible).toContain("build:P");
    expect(visible).toContain("build:Q");
    const edgeIds = result.edges.map((e) => e.id);
    expect(edgeIds).toContain("build:B->build:P");
    expect(edgeIds).not.toContain("build:A->build:P");
    // Nothing is hidden only because of A.
    expect(result.hiddenCountByNodeId.get("build:A")).toBe(0);
  });

  it("hides the shared producer once every consumer is collapsed", () => {
    const result = applyGraphCollapse(
      nodes as never,
      edges as never,
      new Set(["build:A", "build:B"]),
    );
    const visible = result.nodes.map((n) => n.id);
    expect(visible).not.toContain("build:P");
    expect(visible).not.toContain("build:Q");
    expect(result.hiddenCountByNodeId.get("build:A")).toBe(2);
    expect(result.hiddenCountByNodeId.get("build:B")).toBe(2);
  });
});

describe("descendantIds / nodesWithGraphChildren", () => {
  it("descendantIds returns the transitive subtree, excluding the node itself", () => {
    const flow = toReactFlow(branchingProjection());
    expect(descendantIds("build:A", flow.edges)).toEqual(
      new Set(["build:B", "build:C", "buy:A:210"]),
    );
    expect(descendantIds("build:B", flow.edges).size).toBe(0);
  });

  it("nodesWithGraphChildren lists only nodes with at least one child edge", () => {
    const flow = toReactFlow(nestedProjection());
    expect(nodesWithGraphChildren(flow.edges)).toEqual(
      new Set([`root:${ROOT_BUILD_ID}`, "build:child-1"]),
    );
  });
});
