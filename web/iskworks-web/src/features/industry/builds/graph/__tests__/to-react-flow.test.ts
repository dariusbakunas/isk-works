import { describe, expect, it } from "vitest";

import { toReactFlow } from "../to-react-flow";

import {
  ROOT_BUILD_ID,
  acquisitionChild,
  nestedProjection,
  productionChild,
  productionNode,
  projection,
  unresolvedBuildChild,
} from "./fixtures";

describe("toReactFlow", () => {
  it("maps a root-only projection to a single root node with no edges", () => {
    const flow = toReactFlow(
      projection(
        productionNode({
          graphNodeId: `root:${ROOT_BUILD_ID}`,
          buildId: ROOT_BUILD_ID,
          typeId: 500,
          kind: "rootManufacturing",
        }),
      ),
    );
    expect(flow.nodes).toHaveLength(1);
    expect(flow.nodes[0].id).toBe(`root:${ROOT_BUILD_ID}`);
    expect(flow.nodes[0].type).toBe("root");
    expect(flow.edges).toHaveLength(0);
  });

  it("emits a production child node + edge and keeps node ids equal to graphNodeId", () => {
    const child = productionNode({
      graphNodeId: "build:child-1",
      buildId: "child-1",
      typeId: 900,
      parentBuildId: ROOT_BUILD_ID,
      parentComponentTypeId: 900,
    });
    const flow = toReactFlow(
      projection(
        productionNode({
          graphNodeId: `root:${ROOT_BUILD_ID}`,
          buildId: ROOT_BUILD_ID,
          typeId: 500,
          kind: "rootManufacturing",
          children: [productionChild(child, ROOT_BUILD_ID, 900)],
        }),
      ),
    );
    expect(flow.nodes.map((node) => node.id)).toEqual([
      `root:${ROOT_BUILD_ID}`,
      "build:child-1",
    ]);
    expect(flow.edges).toEqual([
      {
        id: `root:${ROOT_BUILD_ID}->build:child-1`,
        source: `root:${ROOT_BUILD_ID}`,
        target: "build:child-1",
        type: "default",
        data: { childKind: "production" },
      },
    ]);
  });

  it("tags each edge with the kind of node it points at (deterministically)", () => {
    const flow = toReactFlow(
      projection(
        productionNode({
          graphNodeId: `root:${ROOT_BUILD_ID}`,
          buildId: ROOT_BUILD_ID,
          typeId: 500,
          kind: "rootManufacturing",
          children: [
            productionChild(
              productionNode({
                graphNodeId: "build:mfg",
                buildId: "mfg",
                typeId: 1,
                kind: "manufacturing",
                parentBuildId: ROOT_BUILD_ID,
                parentComponentTypeId: 1,
              }),
              ROOT_BUILD_ID,
              1,
            ),
            productionChild(
              productionNode({
                graphNodeId: "build:rxn",
                buildId: "rxn",
                typeId: 2,
                kind: "reaction",
                recipe: { mode: "reaction", reactionFormulaTypeId: 20 },
                parentBuildId: ROOT_BUILD_ID,
                parentComponentTypeId: 2,
              }),
              ROOT_BUILD_ID,
              2,
            ),
            acquisitionChild({
              graphNodeId: `buy:${ROOT_BUILD_ID}:3`,
              parentBuildId: ROOT_BUILD_ID,
              typeId: 3,
            }),
            unresolvedBuildChild({
              graphNodeId: `buy:${ROOT_BUILD_ID}:4`,
              parentBuildId: ROOT_BUILD_ID,
              typeId: 4,
            }),
          ],
        }),
      ),
    );
    const kinds = Object.fromEntries(
      flow.edges.map((edge) => [edge.target, edge.data?.childKind]),
    );
    expect(kinds).toEqual({
      "build:mfg": "production",
      "build:rxn": "reaction",
      [`buy:${ROOT_BUILD_ID}:3`]: "acquisition",
      [`buy:${ROOT_BUILD_ID}:4`]: "unresolvedBuild",
    });
    expect(flow.edges.every((edge) => edge.type === "default")).toBe(true);
    // Deterministic.
    expect(toReactFlow.name).toBe("toReactFlow");
  });

  it("flattens a nested production child and connects the deep edge", () => {
    const flow = toReactFlow(nestedProjection());
    expect(flow.nodes.map((node) => node.id)).toEqual([
      `root:${ROOT_BUILD_ID}`,
      "build:child-1",
      "build:grandchild-1",
    ]);
    expect(flow.edges.map((edge) => [edge.source, edge.target])).toEqual([
      [`root:${ROOT_BUILD_ID}`, "build:child-1"],
      ["build:child-1", "build:grandchild-1"],
    ]);
    expect(flow.nodes[2].data.depth).toBe(2);
  });

  it("represents an actionable Buy child as its own node", () => {
    const flow = toReactFlow(
      projection(
        productionNode({
          graphNodeId: `root:${ROOT_BUILD_ID}`,
          buildId: ROOT_BUILD_ID,
          typeId: 500,
          kind: "rootManufacturing",
          children: [
            acquisitionChild({
              graphNodeId: `buy:${ROOT_BUILD_ID}:34`,
              parentBuildId: ROOT_BUILD_ID,
              typeId: 34,
            }),
          ],
        }),
      ),
    );
    const buy = flow.nodes.find((node) => node.type === "acquisition");
    expect(buy?.id).toBe(`buy:${ROOT_BUILD_ID}:34`);
    expect(flow.slotIndex.get(`${ROOT_BUILD_ID}:34`)).toBe(`buy:${ROOT_BUILD_ID}:34`);
  });

  it("represents an unresolved Build child as its own node on the buy slot", () => {
    const flow = toReactFlow(
      projection(
        productionNode({
          graphNodeId: `root:${ROOT_BUILD_ID}`,
          buildId: ROOT_BUILD_ID,
          typeId: 500,
          kind: "rootManufacturing",
          children: [
            unresolvedBuildChild({
              graphNodeId: `buy:${ROOT_BUILD_ID}:900`,
              parentBuildId: ROOT_BUILD_ID,
              typeId: 900,
            }),
          ],
        }),
      ),
    );
    const unresolved = flow.nodes.find((node) => node.type === "unresolvedBuild");
    expect(unresolved?.id).toBe(`buy:${ROOT_BUILD_ID}:900`);
    expect(flow.slotIndex.get(`${ROOT_BUILD_ID}:900`)).toBe(`buy:${ROOT_BUILD_ID}:900`);
  });

  it("emits every direct BUY requirement as its own acquisition node + edge", () => {
    const flow = toReactFlow(
      projection(
        productionNode({
          graphNodeId: `root:${ROOT_BUILD_ID}`,
          buildId: ROOT_BUILD_ID,
          typeId: 500,
          kind: "rootManufacturing",
          children: [
            acquisitionChild({
              graphNodeId: `buy:${ROOT_BUILD_ID}:34`,
              parentBuildId: ROOT_BUILD_ID,
              typeId: 34,
              typeName: "Tritanium",
              buildableRecipe: null,
            }),
            acquisitionChild({
              graphNodeId: `buy:${ROOT_BUILD_ID}:87`,
              parentBuildId: ROOT_BUILD_ID,
              typeId: 87,
              typeName: "Parts",
            }),
          ],
        }),
      ),
    );
    expect(flow.nodes.map((n) => n.id).sort()).toEqual(
      [`root:${ROOT_BUILD_ID}`, `buy:${ROOT_BUILD_ID}:34`, `buy:${ROOT_BUILD_ID}:87`].sort(),
    );
    for (const id of [`buy:${ROOT_BUILD_ID}:34`, `buy:${ROOT_BUILD_ID}:87`]) {
      expect(flow.nodes.find((n) => n.id === id)?.type).toBe("acquisition");
      expect(flow.edges.some((e) => e.source === `root:${ROOT_BUILD_ID}` && e.target === id)).toBe(true);
    }
  });

  it("is deterministic for the same projection", () => {
    const a = toReactFlow(nestedProjection());
    const b = toReactFlow(nestedProjection());
    expect(a.nodes).toEqual(b.nodes);
    expect(a.edges).toEqual(b.edges);
  });
  it("draws a canonical producer shared by two consumers as ONE node with one edge per consumer", () => {
    // Ferrogel serves Plasma Thruster and
    // Deflection Shield Emitter. The projection draws it in full under the
    // first consumer and as a `producerReference` alias (same graphNodeId)
    // under the second.
    const ferrogel = productionNode({
      graphNodeId: "build:ferrogel",
      buildId: "ferrogel",
      typeId: 16683,
      kind: "reaction",
      parentBuildId: "thruster",
      parentComponentTypeId: 16683,
      runs: 4,
      producingQuantity: 1600,
      requiredQuantity: 1407,
      netRequiredQuantity: 1407,
      surplus: 193,
    });
    const thruster = productionNode({
      graphNodeId: "build:thruster",
      buildId: "thruster",
      typeId: 11532,
      parentBuildId: ROOT_BUILD_ID,
      parentComponentTypeId: 11532,
      children: [productionChild(ferrogel, "thruster", 16683)],
    });
    const emitter = productionNode({
      graphNodeId: "build:emitter",
      buildId: "emitter",
      typeId: 11557,
      parentBuildId: ROOT_BUILD_ID,
      parentComponentTypeId: 11557,
      children: [
        {
          nodeKind: "producerReference",
          graphNodeId: "build:ferrogel",
          buildId: "ferrogel",
          parentBuildId: "emitter",
          dependencyId: "pd:emitter-ferrogel",
          typeId: 16683,
          typeName: "Ferrogel",
          kind: "reaction",
          requiredQuantity: 1206,
          netRequiredQuantity: 1206,
        },
      ],
    });
    const flow = toReactFlow(
      projection(
        productionNode({
          graphNodeId: `root:${ROOT_BUILD_ID}`,
          buildId: ROOT_BUILD_ID,
          typeId: 500,
          kind: "rootManufacturing",
          children: [
            productionChild(thruster, ROOT_BUILD_ID, 11532),
            productionChild(emitter, ROOT_BUILD_ID, 11557),
          ],
        }),
      ),
    );

    const ferrogelNodes = flow.nodes.filter((node) => node.id === "build:ferrogel");
    expect(ferrogelNodes).toHaveLength(1);
    const intoFerrogel = flow.edges.filter((edge) => edge.target === "build:ferrogel");
    expect(intoFerrogel.map((edge) => edge.source).sort()).toEqual([
      "build:emitter",
      "build:thruster",
    ]);
    expect(intoFerrogel.every((edge) => edge.data?.childKind === "reaction")).toBe(true);
    // Both consumer slots resolve to the one producer node.
    expect(flow.slotIndex.get("thruster:16683")).toBe("build:ferrogel");
    expect(flow.slotIndex.get("emitter:16683")).toBe("build:ferrogel");
    // Node ids stay unique (React Flow requirement).
    expect(new Set(flow.nodes.map((node) => node.id)).size).toBe(flow.nodes.length);
  });
});

