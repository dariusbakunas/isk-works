import { describe, expect, it } from "vitest";

import { applyGraphCollapse } from "../apply-graph-collapse";
import { nodeSize } from "../graph-layout";
import { layoutBuildGraph } from "../layout-build-graph";
import { toReactFlow } from "../to-react-flow";

import {
  ROOT_BUILD_ID,
  branchingProjection,
  mixedChainProjection,
  nestedProjection,
} from "./fixtures";

const ROOT = `root:${ROOT_BUILD_ID}`;

function byId(nodes: ReturnType<typeof layoutBuildGraph>) {
  return Object.fromEntries(nodes.map((n) => [n.id, n]));
}

/** Do two axis-aligned node boxes overlap? */
function overlaps(a: { position: { x: number; y: number }; width?: number; height?: number },
                  b: { position: { x: number; y: number }; width?: number; height?: number }) {
  const aw = a.width ?? 260;
  const ah = a.height ?? 120;
  const bw = b.width ?? 260;
  const bh = b.height ?? 120;
  return (
    a.position.x < b.position.x + bw &&
    a.position.x + aw > b.position.x &&
    a.position.y < b.position.y + bh &&
    a.position.y + ah > b.position.y
  );
}

describe("layoutBuildGraph (Dagre)", () => {
  it("places the root above every descendant", () => {
    const flow = toReactFlow(nestedProjection());
    const nodes = byId(layoutBuildGraph(flow.nodes, flow.edges));
    expect(nodes[ROOT].position.y).toBeLessThan(nodes["build:child-1"].position.y);
    expect(nodes["build:child-1"].position.y).toBeLessThan(
      nodes["build:grandchild-1"].position.y,
    );
  });

  it("does not overlap any two nodes", () => {
    const positioned = layoutBuildGraph(...flowOf(branchingProjection()));
    for (let i = 0; i < positioned.length; i += 1) {
      for (let j = i + 1; j < positioned.length; j += 1) {
        expect(overlaps(positioned[i], positioned[j])).toBe(false);
      }
    }
  });

  it("lays out a mixed Manufacturing -> Reaction -> Manufacturing chain top-down", () => {
    const flow = toReactFlow(mixedChainProjection());
    const nodes = byId(layoutBuildGraph(flow.nodes, flow.edges));
    expect(nodes[ROOT].position.y).toBeLessThan(nodes["build:r-child"].position.y);
    expect(nodes["build:r-child"].position.y).toBeLessThan(
      nodes["build:m-grandchild"].position.y,
    );
  });

  it("lays out only the visible topology after collapse -- no reserved hidden space", () => {
    const flow = toReactFlow(branchingProjection());
    const full = byId(layoutBuildGraph(flow.nodes, flow.edges));

    const collapsed = applyGraphCollapse(flow.nodes, flow.edges, new Set(["build:A"]));
    const laid = byId(layoutBuildGraph(collapsed.nodes, collapsed.edges));

    expect(Object.keys(laid).sort()).toEqual([ROOT, "build:A", "build:D"].sort());
    // The canvas got shorter: nothing sits where B/C used to be.
    const fullMaxY = Math.max(...Object.values(full).map((n) => n.position.y));
    const laidMaxY = Math.max(...Object.values(laid).map((n) => n.position.y));
    expect(laidMaxY).toBeLessThan(fullMaxY);
  });

  it("is deterministic for identical input and preserves node ids", () => {
    const [nodes, edges] = flowOf(nestedProjection());
    const a = layoutBuildGraph(nodes, edges);
    const b = layoutBuildGraph(nodes, edges);
    expect(a.map((n) => [n.id, n.position])).toEqual(b.map((n) => [n.id, n.position]));
    expect(a.map((n) => n.id)).toEqual(toReactFlow(nestedProjection()).nodes.map((n) => n.id));
  });

  it("sizes each node from the shared nodeSize() helper and does not mutate input", () => {
    const [nodes, edges] = flowOf(branchingProjection());
    const positionsBefore = nodes.map((n) => ({ ...n.position }));
    const laid = layoutBuildGraph(nodes, edges);
    for (const node of laid) {
      const expected = nodeSize({ data: node.data });
      expect(node.width).toBe(expected.width);
      expect(node.height).toBe(expected.height);
    }
    expect(nodes.map((n) => n.position)).toEqual(positionsBefore);
  });

  it("honours a custom sizer (used for the per-node warning chip)", () => {
    const [nodes, edges] = flowOf(nestedProjection());
    const laid = layoutBuildGraph(nodes, edges, () => ({ width: 111, height: 222 }));
    expect(laid.every((n) => n.width === 111 && n.height === 222)).toBe(true);
  });
});

function flowOf(projection: ReturnType<typeof mixedChainProjection>) {
  const flow = toReactFlow(projection);
  return [flow.nodes, flow.edges] as const;
}
