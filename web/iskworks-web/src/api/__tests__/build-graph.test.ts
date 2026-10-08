import { afterEach, expect, test, vi } from "vitest";

import {
  previewBuildGraph,
  type BuildGraphProjection,
  type GraphChild,
  type PreviewBuildPlanCommand,
} from "../industry";

afterEach(() => vi.unstubAllGlobals());

const OVERLAY: PreviewBuildPlanCommand = {
  recipe: { mode: "manufacturing", blueprintTypeId: 6830 },
  runs: 3,
  materialScope: { regionId: 10000002 },
  outputScope: { regionId: 10000002 },
  pricingSelections: [],
  componentResolutions: [
    { typeId: 900, recipe: { mode: "manufacturing", blueprintTypeId: 9002 } },
  ],
  fulfillmentScopes: [],
  buildId: "build-1",
};

test("previewBuildGraph POSTs the live PreviewBuildPlanCommand to /api/builds/:id/graph", async () => {
  const fetchMock = vi.fn(
    async (_url: RequestInfo | URL, _init?: RequestInit) =>
      new Response(JSON.stringify({ root: {}, warnings: [], generatedAt: "" }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
  );
  vi.stubGlobal("fetch", fetchMock);

  await previewBuildGraph("build-1", OVERLAY);

  expect(fetchMock).toHaveBeenCalledTimes(1);
  const [url, init] = fetchMock.mock.calls[0];
  expect(String(url)).toContain("/api/builds/build-1/graph");
  expect(init?.method).toBe("POST");
  expect(JSON.parse(String(init?.body))).toEqual(OVERLAY);
});

test("decodes nodeKind and keeps it distinct from ProductionNode.kind", async () => {
  const wire: BuildGraphProjection = {
    generatedAt: "2026-01-01T00:00:00Z",
    marketEvidence: [],
    warnings: [],
    root: {
      graphNodeId: "root:build-1",
      buildId: "build-1",
      parentBuildId: null,
      parentComponentTypeId: null,
      typeId: 500,
      typeName: "Root",
      kind: "rootManufacturing",
      recipe: { mode: "manufacturing", blueprintTypeId: 6830 },
      runs: 3,
      persistedRuns: 3,
      requiredQuantity: null,
      netRequiredQuantity: null,
      producingQuantity: 3,
      surplus: 0,
      estimatedCost: null,
      materialComponentCost: null,
      ownInstallationCost: null,
      costState: "notComputed",
      recipeCurrency: "current",
      effectiveMe: null,
      effectiveTe: null,
      children: [
        {
          nodeKind: "production",
          graphNodeId: "build:child-1",
          buildId: "child-1",
          parentBuildId: "build-1",
          parentComponentTypeId: 900,
          typeId: 900,
          typeName: "Comp",
          kind: "reaction",
          recipe: { mode: "reaction", reactionFormulaTypeId: 3000 },
          runs: 1,
          persistedRuns: 1,
          requiredQuantity: 4,
          netRequiredQuantity: 4,
          producingQuantity: 4,
          surplus: 0,
          estimatedCost: null,
          materialComponentCost: null,
          ownInstallationCost: null,
          costState: "incomplete",
          recipeCurrency: "current",
          effectiveMe: null,
          effectiveTe: null,
          children: [],
        },
      ],
    },
  };
  vi.stubGlobal(
    "fetch",
    vi.fn(async () =>
      new Response(JSON.stringify(wire), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    ),
  );

  const result = await previewBuildGraph("build-1", OVERLAY);
  const child: GraphChild = result.root.children[0];
  expect(child.nodeKind).toBe("production");
  if (child.nodeKind === "production") {
    // The GraphChild discriminant and the ProductionKind are separate
    // values on the same object -- one says "this is a production node",
    // the other says "it's a reaction job".
    expect(child.kind).toBe("reaction");
    expect(child.nodeKind).not.toBe(child.kind);
  }
  expect(result.root.kind).toBe("rootManufacturing");
});
