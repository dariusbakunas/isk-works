import { describe, expect, it } from "vitest";

import type { GraphWarning } from "../../../../../api/industry";
import { nodeSize } from "../graph-layout";
import { toReactFlow } from "../to-react-flow";

import { ROOT_BUILD_ID, productionNode, projection } from "./fixtures";

function rootData(overrides = {}) {
  return {
    nodeType: "root" as const,
    depth: 0,
    node: productionNode({
      graphNodeId: `root:${ROOT_BUILD_ID}`,
      buildId: ROOT_BUILD_ID,
      typeId: 500,
      kind: "rootManufacturing",
      ...overrides,
    }),
  };
}

function productionData(overrides = {}) {
  return {
    nodeType: "production" as const,
    depth: 1,
    node: productionNode({
      graphNodeId: "build:x",
      buildId: "x",
      typeId: 900,
      parentBuildId: ROOT_BUILD_ID,
      parentComponentTypeId: 900,
      ...overrides,
    }),
  };
}

describe("nodeSize", () => {
  it("is deterministic and bounded for the same input", () => {
    const a = nodeSize({ data: productionData() });
    const b = nodeSize({ data: productionData() });
    expect(a).toEqual(b);
    expect(a.width).toBeGreaterThan(0);
    expect(a.height).toBeGreaterThan(0);
  });

  it("gives root the widest box; unresolved the shortest base", () => {
    const root = nodeSize({ data: rootData() });
    const production = nodeSize({
      data: {
        nodeType: "acquisition" as const,
        depth: 1,
        node: {
          graphNodeId: "buy:x",
          parentBuildId: ROOT_BUILD_ID,
          typeId: 1,
          typeName: "x",
          requiredQuantity: 1,
          missingQuantity: 1,
          buildableRecipe: { mode: "manufacturing", blueprintTypeId: 1 },
          estimatedCost: null,
          costState: "notComputed",
          warning: null,
        },
      },
    });
    expect(root.width).toBeGreaterThanOrEqual(production.width);
  });

  it("adds a fixed one-line summary block only when the node is collapsed", () => {
    const expanded = nodeSize({ data: productionData() }).height;
    const collapsed = nodeSize({ data: productionData(), collapsedSummary: true }).height;
    expect(collapsed).toBeGreaterThan(expanded);
    // It's a fixed block -- independent of how many dependencies there are.
    const collapsedManyChildren = nodeSize({
      data: productionData({
        children: [1, 2, 3, 4, 5, 6].map((n) => ({
          nodeKind: "production" as const,
          ...productionNode({ graphNodeId: `c${n}`, buildId: `c${n}`, typeId: n }),
        })),
      }),
      collapsedSummary: true,
    }).height;
    expect(collapsedManyChildren).toBe(collapsed);
  });

  it("reserves a chip row only for recipeCurrency states that render a chip", () => {
    const base = nodeSize({ data: productionData({ recipeCurrency: "current" }) }).height;
    // Quiet states -> no extra row.
    for (const quiet of ["olderSdeVersion", "unableToCompare"] as const) {
      expect(nodeSize({ data: productionData({ recipeCurrency: quiet }) }).height).toBe(base);
    }
    // Chip states -> one extra row.
    for (const loud of [
      "recipeChanged",
      "blueprintNoLongerAvailable",
      "reactionFormulaNoLongerAvailable",
    ] as const) {
      expect(
        nodeSize({ data: productionData({ recipeCurrency: loud }) }).height,
      ).toBeGreaterThan(base);
    }
  });

  it("reserves a chip row when a card warning targets the node", () => {
    const warning: GraphWarning = {
      graphNodeId: "build:x",
      code: "runsDiverged",
      message: "m",
    };
    const clean = nodeSize({ data: productionData() }).height;
    const warned = nodeSize({ data: productionData(), warnings: [warning] }).height;
    expect(warned).toBeGreaterThan(clean);
  });

  it("the root card height is independent of its dependency count", () => {
    const bare = nodeSize({ data: rootData() }).height;
    const withChildren = nodeSize({
      data: rootData({
        children: [1, 2, 3].map((n) => ({
          nodeKind: "production" as const,
          ...productionNode({ graphNodeId: `c${n}`, buildId: `c${n}`, typeId: n }),
        })),
      }),
    }).height;
    expect(withChildren).toBe(bare);
  });

  it("does not mutate the projection topology when used through toReactFlow", () => {
    const proj = projection(
      productionNode({
        graphNodeId: `root:${ROOT_BUILD_ID}`,
        buildId: ROOT_BUILD_ID,
        typeId: 500,
        kind: "rootManufacturing",
      }),
    );
    const before = JSON.stringify(proj);
    const flow = toReactFlow(proj);
    flow.nodes.forEach((n) => nodeSize({ data: n.data }));
    expect(JSON.stringify(proj)).toBe(before);
  });
});
