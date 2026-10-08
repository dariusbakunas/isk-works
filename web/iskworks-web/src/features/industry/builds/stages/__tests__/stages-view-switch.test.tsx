import { createElement } from "react";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

// The Build workspace mounts `BuildGraphView` (hidden) alongside every other
// tab; React Flow's d3-zoom throws in jsdom, so stub the canvas -- same stub
// `materials-view-switch.test.tsx` uses.
vi.mock("@xyflow/react", () => ({
  ReactFlow: ({ ...rest }: Record<string, unknown>) =>
    createElement("div", { "aria-label": rest["aria-label"] }),
  Background: () => null,
  Controls: () => null,
  Handle: () => null,
  Position: { Top: "top", Bottom: "bottom", Left: "left", Right: "right" },
}));

import { BuildWorkspacePage } from "../../build-workspace-page";

const BUILD = {
  id: "build-ready",
  workspaceId: "workspace-1",
  ownerId: "owner-1",
  name: "Ready Rifter build",
  recipe: {
    kind: "manufacturing",
    sourceSdeDatasetId: "sde-1",
    sourceSdeVersion: "1",
    blueprintTypeId: 691,
    blueprintName: "Rifter Blueprint",
    durationSecondsPerRun: 6000,
    materials: [{ typeId: 34, typeName: "Tritanium", quantityPerRun: 100, sortOrder: 0 }],
    products: [{ typeId: 587, typeName: "Rifter", quantityPerRun: 1, sortOrder: 0 }],
    fingerprint: "recipe",
  },
  runs: 1,
  notes: "",
  revision: 2,
  createdAt: "2026-01-01T00:00:00Z",
  updatedAt: "2026-01-01T00:00:00Z",
  draftPlanning: {
    updatedAt: "2026-01-01T00:00:00Z",
    input: {
      materialScope: { regionId: 10000002 },
      outputScope: { regionId: 10000002 },
      manualPriceListId: null,
      expectedManualPriceListRevision: null,
      materialPricingPolicy: "highestBuy",
      outputPricingPolicy: "lowestSell",
      pricingSelections: [],
      blueprintSelection: null,
      manufacturingFacility: null,
      reactionFacility: null,
      facilityEivManual: false,
      componentResolutions: [],
      fulfillmentScopes: [],
    },
  },
  recipeCurrency: "current",
  activeSdeVersion: "1",
  productCategoryName: "Ship",
  productGroupName: "Frigate",
  selectedBlueprintOrigin: null,
  hasOwnedBlueprint: false,
};

// Scratch state for the Worksheet-parity tests: once the nested Mexallon
// edge is switched to Build, the re-projected plan carries its producer.
const producerState = { mexallonProduced: false, facilityId: null as string | null, facilitiesListed: false };

const FACILITY = {
  id: "fac-home",
  workspaceId: "workspace-1",
  name: "Home Raitaru",
  kind: "manual",
  role: "manufacturing",
  structureId: null,
  structureTypeId: null,
  structureTypeName: "",
  solarSystemId: null,
  solarSystemName: "",
  securityClass: "unknown",
  materialReductionPercent: "0",
  timeReductionPercent: "0",
  jobCostReductionPercent: "0",
  facilityTaxPercent: "0",
  sccSurchargePercent: "0",
  allianceSurchargePercent: "0",
  fixedSupplementalCost: "0",
  manualSystemCostIndex: "0.05",
  notes: "",
  rigs: [],
  archivedAt: null,
  revision: 1,
  createdAt: "",
  updatedAt: "",
};

function mexallonProducer() {
  const facilityName = producerState.facilityId ? "Home Raitaru" : null;
  return {
    node: {
      id: "build:producer-1",
      outputTypeId: 36,
      outputTypeName: "Mexallon",
      activity: "manufacturing",
      stage: 0,
      occurrenceIds: ["build:producer-1"],
      facilityId: producerState.facilityId,
      facilityName,
      effectiveMe: 0,
      effectiveTe: 0,
      requiredQuantity: 10,
      plannedInventoryQuantity: 0,
      productionDemand: 10,
      projectedOutput: 10,
      projectedRuns: 1,
      retainedSurplusQuantity: 0,
      retainedSurplusCost: null,
      materialComponentCost: null,
      ownInstallationCost: producerState.facilityId ? "123.00" : null,
      totalProductionCost: null,
      costComplete: false,
      consumers: [
        {
          nodeId: "build:child-1",
          occurrenceId: "build:child-1",
          quantity: 10,
          buildId: "child-1",
          dependencyId: "pd:2",
          fulfillmentScope: "missing",
          requiredQuantity: 10,
          plannedInventoryQuantity: 0,
        },
      ],
      productionMethods: [{ mode: "manufacturing", blueprintTypeId: 9_036 }],
      unitProductionCost: null,
      availableQuantity: 0,
    },
    occurrence: {
      id: "build:producer-1",
      nodeId: "build:producer-1",
      buildId: "producer-1",
      revision: 1,
      blueprintSelection: null,
      isRoot: false,
      stage: 0,
      activity: "manufacturing",
      outputTypeId: 36,
      outputTypeName: "Mexallon",
      blueprintOrFormulaTypeId: 9_036,
      blueprintOrFormulaName: "Mexallon Blueprint",
      facilityId: producerState.facilityId,
      facilityName,
      effectiveMe: 0,
      effectiveTe: 0,
      projectedRuns: 1,
      projectedOutput: 10,
      requiredQuantity: 10,
      plannedInventoryQuantity: 0,
      productionDemand: 10,
      retainedSurplusQuantity: 0,
      retainedSurplusCost: null,
      materialComponentCost: null,
      ownInstallationCost: null,
      totalProductionCost: null,
      unitProductionCost: null,
      costComplete: false,
      requirements: [],
    },
  };
}

function executionPlan(runs: number) {
  const base = baseExecutionPlan(runs);
  if (!producerState.mexallonProduced) return base;
  const producer = mexallonProducer();
  return {
    ...base,
    stages: [{ index: 0, nodeIds: [producer.node.id] }, { index: 1, nodeIds: ["root:build-ready"] }],
    nodes: [producer.node, ...base.nodes],
    occurrences: [producer.occurrence, ...base.occurrences],
    acquisitions: base.acquisitions.filter((line) => line.typeId !== 36),
  };
}

function baseExecutionPlan(runs: number) {
  return {
    rootNodeId: "root:build-ready",
    stages: [{ index: 0, nodeIds: ["root:build-ready"] }],
    nodes: [
      {
        id: "root:build-ready",
        outputTypeId: 587,
        outputTypeName: "Rifter",
        activity: "manufacturing",
        stage: 0,
        occurrenceIds: ["root:build-ready"],
        facilityId: null,
        facilityName: null,
        effectiveMe: null,
        effectiveTe: null,
        requiredQuantity: 0,
        plannedInventoryQuantity: 0,
        productionDemand: 0,
        projectedOutput: runs,
        projectedRuns: runs,
        retainedSurplusQuantity: 0,
        retainedSurplusCost: null,
        materialComponentCost: null,
        ownInstallationCost: null,
        totalProductionCost: null,
        costComplete: false,
        consumers: [],
        productionMethods: [],
      },
    ],
    edges: [],
    occurrences: [
      {
        id: "root:build-ready",
        nodeId: "root:build-ready",
        buildId: "build-ready",
        isRoot: true,
        stage: 0,
        activity: "manufacturing",
        outputTypeId: 587,
        outputTypeName: "Rifter",
        blueprintOrFormulaTypeId: 691,
        facilityId: null,
        facilityName: null,
        effectiveMe: null,
        effectiveTe: null,
        projectedRuns: runs,
        projectedOutput: runs,
        requiredQuantity: 0,
        plannedInventoryQuantity: 0,
        productionDemand: 0,
        retainedSurplusQuantity: 0,
        retainedSurplusCost: null,
        materialComponentCost: null,
        ownInstallationCost: null,
        totalProductionCost: null,
        costComplete: false,
        requirements: [],
      },
    ],
    acquisitions: [
      {
        typeId: 34,
        typeName: "Tritanium",
        requiredQuantity: 100 * runs,
        plannedInventoryQuantity: 0,
        shortageQuantity: 100 * runs,
        availableQuantity: 0,
        sourceStrategy: "buy",
        consumers: [
          {
            nodeId: "root:build-ready",
            occurrenceId: "root:build-ready",
            quantity: 100 * runs,
            buildId: "build-ready",
            dependencyId: "pd:1",
            fulfillmentScope: "missing",
            requiredQuantity: 100 * runs,
            plannedInventoryQuantity: 0,
          },
        ],
        productionMethods: [],
      },
      {
        typeId: 36,
        typeName: "Mexallon",
        requiredQuantity: 10,
        plannedInventoryQuantity: 0,
        shortageQuantity: 10,
        availableQuantity: 0,
        sourceStrategy: "buy",
        consumers: [
          {
            nodeId: "build:child-1",
            occurrenceId: "build:child-1",
            quantity: 10,
            buildId: "child-1",
            dependencyId: "pd:2",
            fulfillmentScope: "missing",
            requiredQuantity: 10,
            plannedInventoryQuantity: 0,
            freshCost: null,
            freshUnitPrice: null,
          },
        ],
        productionMethods: [{ mode: "manufacturing", blueprintTypeId: 9_036 }],
        freshCost: null,
        freshUnitPrice: null,
        freshPriceStale: false,
      },
    ],
    unresolved: [],
    complete: false,
    warnings: [],
    generatedAt: "2026-01-01T00:00:00Z",
    logistics: { destinations: [], totalVolumeM3: "0", volumeComplete: true },
  };
}

const executionPlanBodies: Array<Record<string, unknown>> = [];
const requestCounts = { preview: 0, graph: 0, resolutions: 0 };

function mockResponse(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
}

beforeEach(() => {
  executionPlanBodies.length = 0;
  requestCounts.preview = 0;
  requestCounts.graph = 0;
  requestCounts.resolutions = 0;
  producerState.mexallonProduced = false;
  producerState.facilityId = null;
  producerState.facilitiesListed = false;
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    if (url.endsWith("/api/builds/build-ready") && (!init?.method || init.method === "GET")) return mockResponse(BUILD);
    if (url.endsWith("/api/builds/build-ready") && init?.method === "PUT") {
      return mockResponse({ ...BUILD, revision: BUILD.revision + 1 });
    }
    const linkedMatch = url.match(/\/api\/builds\/([^/?]+)$/);
    if (linkedMatch && (!init?.method || init.method === "GET")) {
      return mockResponse({ ...BUILD, id: linkedMatch[1] });
    }
    if (url.endsWith("/api/sde")) {
      return mockResponse({
        configured: true,
        active: {
          importId: "i",
          sourceVersion: "1",
          sourceLabel: "x",
          sourceChecksum: "y",
          completedAt: "2026-01-01T00:00:00Z",
          counts: {},
        },
      });
    }
    if (url.endsWith("/api/price-sources")) return mockResponse([]);
    if (url.endsWith("/api/industry/facilities")) return mockResponse(producerState.facilitiesListed ? [FACILITY] : []);
    if (url.endsWith("/api/builds/build-ready/descendant-production-configuration") && init?.method === "PATCH") {
      const body = JSON.parse(String(init.body));
      producerState.facilityId = body.facilityProfileId ?? null;
      return mockResponse([]);
    }
    if (url.includes("/api/industry/blueprints/observations")) return mockResponse([]);
    if (url.includes("/api/builds/build-ready/linked-builds")) return mockResponse([]);
    if (url.includes("/api/market/regions")) return mockResponse([]);
    if (url.includes("/api/blueprints/search") || url.includes("/api/reaction-formulas/search")) return mockResponse([]);
    const planMatch = url.match(/\/api\/blueprints\/691\/plan\?runs=(\d+)/);
    if (planMatch) {
      const planRuns = Number(planMatch[1]);
      return mockResponse({
        blueprintTypeId: 691,
        blueprintName: "Rifter Blueprint",
        runs: planRuns,
        durationSeconds: 6000,
        materials: [{ typeId: 34, typeName: "Tritanium", quantityPerRun: 100, totalQuantity: 100 * planRuns }],
        products: [{ typeId: 587, typeName: "Rifter", quantityPerRun: 1, totalQuantity: planRuns }],
      });
    }
    if (url.endsWith("/api/build-plans/candidate-preview")) {
      requestCounts.preview += 1;
      return mockResponse({ error: {} }, 500);
    }
    if (url.endsWith("/api/builds/child-1/component-resolutions") && init?.method === "POST") {
      requestCounts.resolutions += 1;
      producerState.mexallonProduced = true;
      return mockResponse({ ...BUILD, id: "child-1", revision: 3 });
    }
    if (url.endsWith("/api/builds/child-1/linked-builds") && init?.method === "POST") {
      return mockResponse({ ...BUILD, id: "producer-1" });
    }
    if (url.includes("/materials") && init?.method === "POST") return mockResponse({ error: {} }, 500);
    if (url.includes("/graph") && init?.method === "POST") {
      requestCounts.graph += 1;
      return mockResponse({ error: {} }, 500);
    }
    if (url.includes("/execution-plan") && init?.method === "POST") {
      const body = JSON.parse(String(init.body));
      executionPlanBodies.push(body);
      return mockResponse(executionPlan(body.runs));
    }
    if (url.includes("/worksheet") && init?.method === "POST") {
      return mockResponse({ scope: { rootBuildId: "build-ready", focusedProducerId: null, label: "Root" }, groups: [], output: { typeId: 5876, typeName: "Rifter", quantity: 1, unitValue: null, totalValue: null, evidenceState: "incomplete" }, warnings: [], economicsAreAdditive: false, generatedAt: "2026-09-26T00:00:00Z" });
    }
    return mockResponse([], 200);
  });
  vi.stubGlobal("fetch", fetchMock);
});
afterEach(() => vi.unstubAllGlobals());

let currentSearch = "";
function LocationProbe() {
  currentSearch = useLocation().search;
  return null;
}

function renderAt(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <LocationProbe />
      <Routes>
        <Route element={<BuildWorkspacePage />} path="/builds/:buildId" />
      </Routes>
    </MemoryRouter>,
  );
}

test("the Build workspace is Worksheet, Plan, Logistics and Graph, with Worksheet by default", async () => {
  const user = userEvent.setup();
  renderAt("/builds/build-ready");

  const tablist = await screen.findByRole("tablist", { name: "Build view" });
  const tabNames = within(tablist)
    .getAllByRole("tab")
    .map((tab) => tab.textContent);
  expect(tabNames).toEqual(["Worksheet", "Plan", "Logistics", "Graph"]);
  expect(screen.queryByRole("tab", { name: "Economics" })).toBeNull();
  expect(screen.queryByRole("tab", { name: "Materials" })).toBeNull();
  expect(screen.getByRole("tab", { name: "Worksheet" })).toHaveAttribute("aria-selected", "true");
  expect(currentSearch).toBe("");
  await screen.findByRole("heading", { name: "Production Worksheet" });

  await user.click(screen.getByRole("tab", { name: "Plan" }));
  expect(currentSearch).toBe("?view=plan");
  await screen.findByRole("heading", { name: "Production Plan" });
  await user.click(screen.getByRole("tab", { name: "Worksheet" }));
  expect(currentSearch).toBe("");
});

// B. Build-level economics, runs and root settings are persistent context,
// visible on every view.
test("the persistent Build summary is visible on every Build view", async () => {
  const user = userEvent.setup();
  renderAt("/builds/build-ready");
  await screen.findByRole("heading", { name: "Production Worksheet" });

  for (const view of ["Plan", "Worksheet", "Logistics", "Graph"]) {
    await user.click(screen.getByRole("tab", { name: view }));
    const summary = screen.getByRole("region", { name: "Build summary" });
    expect(within(summary).getByLabelText("Runs")).toBeVisible();
    for (const label of ["Material cost", "Installation", "Total cost", "Revenue", "Estimated profit", "Profit margin"]) {
      expect(within(summary).getByText(label)).toBeVisible();
    }
    expect(within(summary).getByRole("button", { name: /Edit build settings/ })).toBeVisible();
  }
});

// C. old links resolve to a view that exists, and are normalized.
test("old ?view= links normalize while Worksheet remains directly addressable", async () => {
  const first = renderAt("/builds/build-ready?view=stages");
  await screen.findByRole("heading", { name: "Production Plan" });
  expect(screen.getByRole("tab", { name: "Plan" })).toHaveAttribute("aria-selected", "true");
  await waitFor(() => expect(currentSearch).toBe("?view=plan"));
  first.unmount();

  const second = renderAt("/builds/build-ready?view=worksheet");
  await screen.findByRole("heading", { name: "Production Worksheet" });
  await waitFor(() => expect(currentSearch).toBe(""));
  second.unmount();

  renderAt("/builds/build-ready?view=materials");
  await screen.findByRole("heading", { name: "Logistics" });
  expect(screen.getByRole("tab", { name: "Logistics" })).toHaveAttribute("aria-selected", "true");
  await waitFor(() => expect(currentSearch).toBe("?view=logistics"));
});

// D. /builds/:id/edit retains its explicit Plan destination with Build settings open.
test("/builds/:id/edit opens the Plan with Build settings open", async () => {
  render(
    <MemoryRouter initialEntries={["/builds/build-ready/edit"]}>
      <LocationProbe />
      <Routes>
        <Route element={<BuildWorkspacePage />} path="/builds/:buildId" />
        <Route element={<BuildWorkspacePage canonicalize />} path="/builds/:buildId/edit" />
      </Routes>
    </MemoryRouter>,
  );
  await screen.findByRole("heading", { name: "Production Plan" });
  expect(await screen.findByText("Build settings")).toBeInTheDocument();
  await waitFor(() => expect(currentSearch).toBe("?view=plan"));
});

// E. the request uses the same live unsaved overlay; runs are edited from
// the persistent summary on the Plan itself.
test("runs edited in the persistent summary re-plan the Plan with the unsaved overlay", async () => {
  const user = userEvent.setup();
  renderAt("/builds/build-ready?view=plan");
  await screen.findByRole("heading", { name: "Production Plan" });

  const runsInput = within(screen.getByRole("region", { name: "Build summary" })).getByLabelText("Runs");
  await user.clear(runsInput);
  await user.type(runsInput, "5");

  await waitFor(() => expect(executionPlanBodies.some((body) => body.runs === 5)).toBe(true));
  expect(executionPlanBodies[0]).toMatchObject({
    recipe: { mode: "manufacturing", blueprintTypeId: 691 },
    componentResolutions: [],
  });
  // The acquisition shortage reflects the overlay runs (100 * 5).
  await waitFor(() => expect(screen.getAllByText("500").length).toBeGreaterThan(0));
});

test("no execution-plan request while only Graph is active", async () => {
  const user = userEvent.setup();
  renderAt("/builds/build-ready?view=graph");
  await screen.findByRole("region", { name: "Build summary" });
  await new Promise((resolve) => setTimeout(resolve, 400));
  expect(executionPlanBodies).toHaveLength(0);

  await user.click(screen.getByRole("tab", { name: "Logistics" }));
  await waitFor(() => expect(executionPlanBodies).toHaveLength(1));
  await screen.findByRole("heading", { name: "Logistics" });
});

// F. A Plan sourcing write on a nested edge re-plans
// everything keyed on the editor's preview key -- the persistent Build
// economics (candidate preview) and the Plan now, Logistics and Graph when
// opened -- with no reload and no tab round trip required for the summary.
test("a Plan sourcing change refreshes the Build summary, Plan, Logistics and Graph", async () => {
  const user = userEvent.setup();
  renderAt("/builds/build-ready?view=plan");
  await screen.findByRole("heading", { name: "Production Plan" });
  await waitFor(() => expect(executionPlanBodies.length).toBeGreaterThan(0));
  await waitFor(() => expect(requestCounts.preview).toBeGreaterThan(0));
  const previewsBefore = requestCounts.preview;
  const plansBefore = executionPlanBodies.length;

  await user.click(await screen.findByText("Mexallon"));
  await user.click(
    within(screen.getByRole("radiogroup", { name: /Sourcing for/ })).getByRole("radio", { name: "Build" }),
  );

  await waitFor(() => expect(requestCounts.resolutions).toBe(1));
  await waitFor(() => expect(requestCounts.preview).toBeGreaterThan(previewsBefore));
  await waitFor(() => expect(executionPlanBodies.length).toBeGreaterThan(plansBefore));

  await user.click(screen.getByRole("tab", { name: "Graph" }));
  await waitFor(() => expect(requestCounts.graph).toBeGreaterThan(0));
  const plansAfterGraph = executionPlanBodies.length;
  await user.click(screen.getByRole("tab", { name: "Logistics" }));
  await waitFor(() => expect(executionPlanBodies.length).toBeGreaterThan(plansAfterGraph));
});

// G. Worksheet parity: after Buy -> Build the new producer is selected, so
// its blueprint / ME / TE / facility are configured right there -- no
// Graph, no hunting for the row.
test("switching an input to Build selects the new producer with its production configuration", async () => {
  producerState.facilitiesListed = true;
  const user = userEvent.setup();
  renderAt("/builds/build-ready?view=plan");
  await screen.findByRole("heading", { name: "Production Plan" });
  await user.click(await screen.findByText("Mexallon"));
  await user.click(
    within(screen.getByRole("radiogroup", { name: /Sourcing for/ })).getByRole("radio", { name: "Build" }),
  );
  // The input inspector (also titled "Mexallon") is replaced by the
  // producer's own inspector.
  await waitFor(() => expect(screen.getByText("Mexallon Blueprint")).toBeInTheDocument());
  const inspector = screen.getByRole("complementary", { name: "Mexallon" });
  expect(within(inspector).getByText("Stage 1")).toBeInTheDocument();
  expect(within(inspector).getByRole("button", { name: /^Production configuration/ })).toBeInTheDocument();
  expect(within(inspector).getByLabelText(/Material Efficiency/)).toBeInTheDocument();
  expect(within(inspector).getByLabelText("Facility")).toBeInTheDocument();
  expect(within(inspector).getByText(/Facility not selected/)).toBeInTheDocument();
});

// H. A producer facility change re-projects the Build economics, Plan (and
// with it Logistics/Graph) and keeps the same producer's inspector open,
// now showing the new facility and installation cost.
test("a producer facility change refreshes the Build summary and Plan, keeping the inspector open", async () => {
  producerState.facilitiesListed = true;
  producerState.mexallonProduced = true;
  const user = userEvent.setup();
  renderAt("/builds/build-ready?view=plan");
  await screen.findByRole("heading", { name: "Production Plan" });
  await user.click(await screen.findByText("Mexallon"));
  const inspector = await screen.findByRole("complementary", { name: "Mexallon" });
  await waitFor(() => expect(requestCounts.preview).toBeGreaterThan(0));
  const previewsBefore = requestCounts.preview;
  const plansBefore = executionPlanBodies.length;
  const facility = await within(inspector).findByRole("option", { name: "Home Raitaru" });
  await user.selectOptions(within(inspector).getByLabelText("Facility"), facility);
  await waitFor(() => expect(requestCounts.preview).toBeGreaterThan(previewsBefore));
  await waitFor(() => expect(executionPlanBodies.length).toBeGreaterThan(plansBefore));
  const refreshed = await screen.findByRole("complementary", { name: "Mexallon" });
  await waitFor(() => expect(within(refreshed).queryByText(/Facility not selected/)).toBeNull());
  expect((within(refreshed).getByLabelText("Facility") as HTMLSelectElement).value).toBe("fac-home");
});
