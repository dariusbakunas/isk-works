import { afterEach, expect, test, vi } from "vitest";

import {
  postBuildMaterials,
  type BuildMaterialsSummary,
  type PreviewBuildPlanCommand,
} from "../industry";
import { ApiError } from "../workspace";

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
  fulfillmentScopes: [{ typeId: 34, scope: "full" }],
  buildId: "build-1",
};

const RESPONSE: BuildMaterialsSummary = {
  buildId: "build-1",
  generatedAt: "2026-01-01T00:00:00Z",
  rows: [
    {
      typeId: 34,
      typeName: "Tritanium",
      requiredQuantity: 2_000,
      availableQuantity: 1_000,
      allocatedQuantity: 1_000,
      shortageQuantity: 1_000,
      fullyCovered: false,
      strategy: "buy",
      provisional: false,
    },
  ],
  nodeAllocations: [
    {
      buildId: "build-1",
      graphNodeId: "root:build-1",
      treePath: [],
      typeId: 900,
      typeName: "Composite Armor Plate",
      requiredQuantity: 2_852,
      allocatedQuantity: 2_852,
      shortageQuantity: 0,
      scope: "missing",
      resolution: "build",
      provisional: false,
      childRuns: 0,
      outputPerRun: 1,
      producedQuantity: 0,
      surplusQuantity: 0,
    },
    {
      buildId: "child-1",
      graphNodeId: "build:child-1",
      treePath: [900],
      typeId: 34,
      typeName: "Tritanium",
      requiredQuantity: 1_000,
      allocatedQuantity: 0,
      shortageQuantity: 1_000,
      scope: "missing",
      resolution: "buy",
      provisional: false,
      childRuns: 0,
      outputPerRun: 0,
      producedQuantity: 0,
      surplusQuantity: 0,
    },
  ],
  sources: [
    {
      buildId: "child-1",
      graphNodeId: "build:child-1",
      typeId: 34,
      typeName: "Tritanium",
      requiredQuantity: 1_000,
      allocatedQuantity: 0,
      shortageQuantity: 1_000,
      scope: "missing",
      provisional: false,
      treePath: [900],
    },
  ],
  warnings: [
    { code: "unresolvedBuild", buildId: "child-1", typeId: 950, message: "not linked yet" },
  ],
};

test("postBuildMaterials POSTs the overlay to /api/builds/:id/materials and parses the full summary", async () => {
  const fetchMock = vi.fn(
    async (_url: RequestInfo | URL, _init?: RequestInit) =>
      new Response(JSON.stringify(RESPONSE), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
  );
  vi.stubGlobal("fetch", fetchMock);

  const summary = await postBuildMaterials("build-1", OVERLAY);

  const [url, init] = fetchMock.mock.calls[0];
  expect(String(url)).toContain("/api/builds/build-1/materials");
  expect(init?.method).toBe("POST");
  expect(JSON.parse(String(init?.body))).toEqual(OVERLAY);

  // rows + the two retained collections all survive the type/parse layer.
  expect(summary.rows[0].shortageQuantity).toBe(1_000);
  expect(summary.nodeAllocations).toHaveLength(2);
  expect(summary.nodeAllocations[1]).toMatchObject({ buildId: "child-1", scope: "missing" });
  expect(summary.sources[0]).toMatchObject({ graphNodeId: "build:child-1", treePath: [900] });
  expect(summary.warnings[0].code).toBe("unresolvedBuild");
});

test("a 422 build_materials_incomplete surfaces as an ApiError with the curated code", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(
      async () =>
        new Response(
          JSON.stringify({
            error: { code: "build_materials_incomplete", message: "The materials breakdown is unavailable." },
          }),
          { status: 422, headers: { "content-type": "application/json" } },
        ),
    ),
  );

  await expect(postBuildMaterials("build-1", OVERLAY)).rejects.toMatchObject({
    status: 422,
    body: { code: "build_materials_incomplete" },
  });
  await expect(postBuildMaterials("build-1", OVERLAY)).rejects.toBeInstanceOf(ApiError);
});
