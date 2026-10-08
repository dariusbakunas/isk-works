import { createElement } from "react";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

// Stub the React Flow canvas (d3-zoom misbehaves in jsdom) with a plain
// renderer that still drives `nodeTypes` + `onNodeClick`, so selection and
// the collapse control are exercised through `BuildGraphView`.
vi.mock("@xyflow/react", () => ({
  ReactFlow: ({ nodes, nodeTypes, onNodeClick, onPaneClick, ...rest }: Record<string, unknown>) =>
    createElement(
      "div",
      { "aria-label": rest["aria-label"], onClick: onPaneClick as () => void },
      (nodes as Array<{ id: string; type: string; data: unknown; selected?: boolean }>).map((node) => {
        const Component = (nodeTypes as Record<string, React.ComponentType>)[
          node.type
        ] as React.ComponentType<Record<string, unknown>>;
        return createElement(
          "div",
          {
            key: node.id,
            "data-node-id": node.id,
            onClick: (event: React.MouseEvent) => {
              event.stopPropagation();
              (onNodeClick as (e: unknown, n: unknown) => void)?.(event, node);
            },
          },
          createElement(Component, { id: node.id, data: node.data, selected: node.selected, type: node.type }),
        );
      }),
    ),
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
  parentBuildId: null,
  parentComponentTypeId: null,
};

function productionNode(over: Record<string, unknown>) {
  return {
    parentBuildId: null,
    parentComponentTypeId: null,
    kind: "manufacturing",
    recipe: { mode: "manufacturing", blueprintTypeId: 900 },
    runs: 1,
    requiredQuantity: 1,
    netRequiredQuantity: 1,
    producingQuantity: 1,
    surplus: 0,
    estimatedCost: null,
    costState: "incomplete",
    recipeCurrency: "current",
    effectiveMe: null,
    effectiveTe: null,
    children: [],
    ...over,
  };
}

function graphProjection(runs: number) {
  return {
    generatedAt: "2026-01-01T00:00:00Z",
    marketEvidence: [],
    warnings: [],
    root: productionNode({
      graphNodeId: "root:build-ready",
      buildId: "build-ready",
      typeId: 587,
      typeName: "Rifter",
      kind: "rootManufacturing",
      recipe: { mode: "manufacturing", blueprintTypeId: 691 },
      runs,
      requiredQuantity: null,
      netRequiredQuantity: null,
      producingQuantity: runs,
      children: [
        {
          nodeKind: "production",
          ...productionNode({
            graphNodeId: "build:assembly",
            buildId: "assembly",
            typeId: 200,
            typeName: "Assembly A",
            parentBuildId: "build-ready",
            parentComponentTypeId: 200,
            children: [
              {
                nodeKind: "production",
                ...productionNode({
                  graphNodeId: "build:part",
                  buildId: "part",
                  typeId: 201,
                  typeName: "Part B",
                  parentBuildId: "assembly",
                  parentComponentTypeId: 201,
                }),
              },
            ],
          }),
        },
      ],
    }),
  };
}

const graphBodies: Array<Record<string, unknown>> = [];

function mockResponse(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
}

beforeEach(() => {
  graphBodies.length = 0;
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    if (url.endsWith("/api/builds/build-ready") && (!init?.method || init.method === "GET")) return mockResponse(BUILD);
    if (url.endsWith("/api/builds/build-ready") && init?.method === "PUT") {
      return mockResponse({ ...BUILD, revision: BUILD.revision + 1 });
    }
    // The inspector lazily fetches the persisted Build behind a
    // selected linked Production node.
    const linkedMatch = url.match(/\/api\/builds\/([^/?]+)$/);
    if (linkedMatch && (!init?.method || init.method === "GET")) {
      return mockResponse({
        ...BUILD,
        id: linkedMatch[1],
        name: `${linkedMatch[1]} build`,
        parentBuildId: "build-ready",
        parentComponentTypeId: 200,
      });
    }
    if (url.endsWith("/api/sde")) {
      return mockResponse({ configured: true, active: { importId: "i", sourceVersion: "1", sourceLabel: "x", sourceChecksum: "y", completedAt: "2026-01-01T00:00:00Z", counts: {} } });
    }
    if (url.endsWith("/api/price-sources")) return mockResponse([]);
    if (url.endsWith("/api/industry/facilities")) return mockResponse([]);
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
    if (url.endsWith("/api/build-plans/candidate-preview")) return mockResponse({ error: {} }, 500);
    if (url.includes("/graph") && init?.method === "POST") {
      const body = JSON.parse(String(init.body));
      graphBodies.push(body);
      return mockResponse(graphProjection(body.runs));
    }
    // Any execution-plan request gets an empty plan.
    if (url.includes("/execution-plan") && init?.method === "POST") {
      return mockResponse({
        rootNodeId: "",
        stages: [],
        nodes: [],
        edges: [],
        occurrences: [],
        acquisitions: [],
        unresolved: [],
        complete: true,
        warnings: [],
        generatedAt: "2026-01-01T00:00:00Z",
        planner: "canonical",
        logistics: { destinations: [], totalVolumeM3: "0", volumeComplete: true },
      });
    }
    console.error(`Unexpected request: ${init?.method ?? "GET"} ${url}`);
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
        <Route path="/builds/:buildId" element={<BuildWorkspacePage />} />
      </Routes>
    </MemoryRouter>,
  );
}

test("switches from Plan to Graph via ?view=graph, toggles back (canvas stays mounted, hidden)", async () => {
  const user = userEvent.setup();
  renderAt("/builds/build-ready?view=plan");

  const graphTab = await screen.findByRole("tab", { name: "Graph" });
  expect(screen.getByRole("tab", { name: "Plan" })).toHaveAttribute("aria-selected", "true");
  expect(currentSearch).toBe("?view=plan");
  expect(screen.queryByLabelText("Build graph canvas")).not.toBeInTheDocument();

  await user.click(graphTab);
  await screen.findByLabelText("Build graph canvas");
  expect(graphTab).toHaveAttribute("aria-selected", "true");
  expect(currentSearch).toBe("?view=graph");
  await waitFor(() => expect(graphBodies.length).toBeGreaterThan(0));
  expect(graphBodies[0]).toMatchObject({
    runs: 1,
    recipe: { mode: "manufacturing", blueprintTypeId: 691 },
    componentResolutions: [],
  });

  await user.click(screen.getByRole("tab", { name: "Plan" }));
  expect(currentSearch).toBe("?view=plan");
  // Mounted-but-hidden: still in the DOM, not visible.
  expect(screen.getByLabelText("Build graph canvas")).not.toBeVisible();
  expect(await screen.findByLabelText("Runs")).toBeVisible();
});

test("opening a ?view=graph URL directly starts on the Graph view", async () => {
  renderAt("/builds/build-ready?view=graph");
  await screen.findByLabelText("Build graph canvas");
  expect(screen.getByRole("tab", { name: "Graph" })).toHaveAttribute("aria-selected", "true");
});

test("Graph expansion + selection survive Graph -> Plan -> Graph", async () => {
  const user = userEvent.setup();
  renderAt("/builds/build-ready?view=graph");

  const canvas = await screen.findByLabelText("Build graph canvas");
  // Assembly A is a non-root node with children -> collapsed by default.
  expect(within(canvas).queryByText("Part B")).not.toBeInTheDocument();
  await user.click(
    within(canvas).getByRole("button", { name: "Expand dependencies for Assembly A" }),
  );
  await waitFor(() => expect(within(canvas).getByText("Part B")).toBeInTheDocument());
  await user.click(within(canvas).getByText("Part B"));
  await waitFor(() =>
    expect(screen.getByTestId("graph-inspector")).toHaveAttribute("data-graph-node-id", "build:part"),
  );

  await user.click(screen.getByRole("tab", { name: "Plan" }));
  await user.click(screen.getByRole("tab", { name: "Graph" }));

  // Still expanded, still selected -- no refit, no reset.
  expect(within(canvas).getByText("Part B")).toBeInTheDocument();
  expect(
    within(canvas).getByRole("button", { name: "Collapse dependencies for Assembly A" }),
  ).toBeInTheDocument();
  expect(screen.getByTestId("graph-inspector")).toHaveAttribute("data-graph-node-id", "build:part");
});

test("no graph request while Plan is active; an edit made while hidden fires exactly one fresh request on return", async () => {
  const user = userEvent.setup();
  renderAt("/builds/build-ready");

  // Worksheet only so far -- nothing graphed.
  await screen.findByLabelText("Runs");
  expect(graphBodies).toHaveLength(0);

  await user.click(screen.getByRole("tab", { name: "Graph" }));
  await screen.findByLabelText("Build graph canvas");
  await waitFor(() => expect(graphBodies).toHaveLength(1));
  expect(within(screen.getByLabelText("Build graph canvas")).getByText("Rifter")).toBeInTheDocument();

  // Back to Worksheet, change runs -- graph is inactive, so no request. The
  // Worksheet body remounts, so re-query the input.
  await user.click(screen.getByRole("tab", { name: "Plan" }));
  const runsInput = await screen.findByLabelText("Runs");
  await user.clear(runsInput);
  await user.type(runsInput, "5");
  await new Promise((resolve) => setTimeout(resolve, 400));
  expect(graphBodies).toHaveLength(1);

  // Return to Graph -> exactly one fresh request with the new runs; the old
  // graph stays visible in the meantime.
  await user.click(screen.getByRole("tab", { name: "Graph" }));
  expect(within(screen.getByLabelText("Build graph canvas")).getByText("Rifter")).toBeInTheDocument();
  await waitFor(() => expect(graphBodies).toHaveLength(2));
  expect(graphBodies[1]).toMatchObject({ runs: 5 });
});

test("switching views does not reset editor planning state", async () => {
  const user = userEvent.setup();
  renderAt("/builds/build-ready");

  const runsInput = await screen.findByLabelText("Runs");
  await user.clear(runsInput);
  await user.type(runsInput, "9");

  await user.click(screen.getByRole("tab", { name: "Graph" }));
  await screen.findByLabelText("Build graph canvas");
  await user.click(screen.getByRole("tab", { name: "Plan" }));

  expect(await screen.findByLabelText("Runs")).toHaveValue(9);
});

test("Graph root node: the canonical root inspector is editable inline; a change persists to the root Build and re-projects the graph", async () => {
  const user = userEvent.setup();
  renderAt("/builds/build-ready?view=graph");

  const canvas = await screen.findByLabelText("Build graph canvas");
  await waitFor(() => expect(graphBodies).toHaveLength(1));

  await user.click(within(canvas).getByText("Rifter"));
  await waitFor(() =>
    expect(screen.getByTestId("graph-inspector")).toHaveAttribute(
      "data-graph-node-id",
      "root:build-ready",
    ),
  );

  // The Graph root node renders the SAME canonical root inspector the
  // Worksheet toolbar opens -- BuildSettingsPanel is decomposed into the
  // FACILITY + PRICING sections. No "Edit build settings" button, no swap.
  const inspector = within(screen.getByTestId("graph-inspector"));
  expect(inspector.getByRole("region", { name: "Pricing" })).toBeInTheDocument();
  expect(inspector.getByRole("region", { name: "Facility" })).toBeInTheDocument();
  expect(
    inspector.queryByRole("button", { name: /Edit build settings/ }),
  ).not.toBeInTheDocument();

  // A settings change persists exactly as a Worksheet edit would (PUT the
  // root Build) and the graph re-requests with no manual refresh.
  const putsBefore = graphBodies.length;
  await user.selectOptions(
    inspector.getByLabelText("Material pricing"),
    "acquireQuantityFromSellOrders",
  );
  await waitFor(() =>
    expect(
      (fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.some(
        ([url, init]) =>
          String(url).endsWith("/api/builds/build-ready") &&
          (init as RequestInit | undefined)?.method === "PUT",
      ),
    ).toBe(true),
  );
  await waitFor(() => expect(graphBodies.length).toBeGreaterThan(putsBefore));
  // The graph inspector stayed put the whole time.
  expect(screen.getByTestId("graph-inspector")).toHaveAttribute(
    "data-graph-node-id",
    "root:build-ready",
  );
});

test("Graph inspector for a linked node offers only Open linked build — no Edit build settings, no BuildSettingsPanel", async () => {
  const user = userEvent.setup();
  renderAt("/builds/build-ready?view=graph");

  const canvas = await screen.findByLabelText("Build graph canvas");
  await user.click(within(canvas).getByText("Assembly A"));
  await waitFor(() =>
    expect(screen.getByTestId("graph-inspector")).toHaveAttribute(
      "data-graph-node-id",
      "build:assembly",
    ),
  );

  const inspector = within(screen.getByTestId("graph-inspector"));
  expect(
    inspector.queryByRole("button", { name: /Edit build settings/ }),
  ).not.toBeInTheDocument();
  expect(inspector.queryByRole("region", { name: "Build settings" })).not.toBeInTheDocument();
  expect(
    inspector.getByRole("button", { name: "Open linked build for Assembly A" }),
  ).toBeInTheDocument();
});
