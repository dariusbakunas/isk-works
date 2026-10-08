import { afterEach, expect, test, vi } from "vitest";

import {
  postBuildWorksheet,
  type BuildWorksheetProjection,
  type PreviewBuildPlanCommand,
} from "../industry";

afterEach(() => vi.unstubAllGlobals());

const command = {
  recipe: { mode: "manufacturing", blueprintTypeId: 6830 },
  runs: 3,
  materialScope: { regionId: 10000002 },
  outputScope: { regionId: 10000002 },
  pricingSelections: [],
  componentResolutions: [],
  fulfillmentScopes: [],
} as PreviewBuildPlanCommand;

const response: BuildWorksheetProjection = {
  scope: { rootBuildId: "root", focusedProducerId: null, includeDownstream: false, label: "Root" },
  groups: [
    {
      key: "other",
      label: "Other",
      rowCount: 1,
      complete: false,
      rows: [
        {
          id: "row",
          typeId: 34,
          typeName: "Tritanium",
          categoryId: null,
          categoryName: null,
          groupId: null,
          groupName: null,
          sourcing: "buy",
          requiredQuantity: 10,
          coveredQuantity: 0,
          shortageQuantity: 10,
          coveragePercentage: null,
          evidenceState: "unpriced",
          pricing: { state: "unpriced", classification: "default", policy: null, sourceNote: null },
          unitCost: null,
          totalValue: null,
          producerBuildId: null,
          retainedSurplusQuantity: null,
          retainedSurplusBasis: null,
          warnings: [],
        },
      ],
    },
  ],
  output: {
    typeId: 5876,
    typeName: "Rifter",
    quantity: 3,
    unitValue: null,
    totalValue: null,
    evidenceState: "incomplete",
  },
  warnings: [],
  economicsAreAdditive: false,
  generatedAt: "2026-09-26T00:00:00Z",
};

test("postBuildWorksheet sends the overlay and focus without coercing incomplete values", async () => {
  const fetchMock = vi.fn(
    async (_url: RequestInfo | URL, _init?: RequestInit) =>
      new Response(JSON.stringify(response), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
  );
  vi.stubGlobal("fetch", fetchMock);
  const controller = new AbortController();

  const result = await postBuildWorksheet("root", command, "focus", true, controller.signal);

  const [url, init] = fetchMock.mock.calls[0];
  expect(String(url)).toContain("/api/builds/root/worksheet");
  expect(JSON.parse(String(init?.body))).toEqual({ command, focusedProducerId: "focus", includeDownstream: true });
  expect(init?.signal).toBe(controller.signal);
  expect(result.groups[0].rows[0].coveragePercentage).toBeNull();
  expect(result.groups[0].rows[0].totalValue).toBeNull();
});
