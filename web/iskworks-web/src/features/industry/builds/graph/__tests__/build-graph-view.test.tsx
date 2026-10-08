import { createElement } from "react";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// React Flow's d3-zoom pane handler throws inside jsdom on pointer events.
// The canvas itself is a runtime concern; here we stub it with a plain
// renderer that still drives `nodeTypes`, `onNodeClick` and `onPaneClick`,
// so `BuildGraphView`'s own wiring (node enrichment, selection, warnings,
// last-known-good) is what's under test.
vi.mock("@xyflow/react", () => ({
  ReactFlow: ({ nodes, nodeTypes, onNodeClick, onPaneClick, ...rest }: Record<string, unknown>) =>
    createElement(
      "div",
      { "aria-label": rest["aria-label"], onClick: onPaneClick as () => void },
      (nodes as Array<{ id: string; type: string; data: unknown; selected?: boolean }>).map(
        (node) => {
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
            createElement(Component, {
              id: node.id,
              data: node.data,
              selected: node.selected,
              type: node.type,
            }),
          );
        },
      ),
    ),
  Background: () => null,
  Controls: () => null,
  Handle: () => null,
  Position: { Top: "top", Bottom: "bottom", Left: "left", Right: "right" },
}));

import type { BuildWorksheetEditorModel } from "../../use-build-worksheet-editor";
import { BuildGraphView } from "../build-graph-view";

import {
  ROOT_BUILD_ID,
  acquisitionChild,
  rawAcquisitionChild,
  buildDetail,
  facilityProfile,
  productionChild,
  productionNode,
  projection,
  reactionBuildDetail,
  unresolvedBuildChild,
} from "./fixtures";

const previewBuildGraph = vi.fn();
const getBuild = vi.fn();
const setComponentResolution = vi.fn();
const clearComponentResolution = vi.fn();
const createLinkedBuild = vi.fn();
const setBuildBlueprintSelection = vi.fn();
const setBuildFacility = vi.fn();
const listBlueprintObservations = vi.fn();
vi.mock("../../../../../api/industry", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  previewBuildGraph: (...args: unknown[]) => previewBuildGraph(...args),
  getBuild: (...args: unknown[]) => getBuild(...args),
  setComponentResolution: (...args: unknown[]) => setComponentResolution(...args),
  clearComponentResolution: (...args: unknown[]) => clearComponentResolution(...args),
  createLinkedBuild: (...args: unknown[]) => createLinkedBuild(...args),
  setBuildBlueprintSelection: (...args: unknown[]) => setBuildBlueprintSelection(...args),
  setBuildFacility: (...args: unknown[]) => setBuildFacility(...args),
  listBlueprintObservations: (...args: unknown[]) => listBlueprintObservations(...args),
}));

const PREVIEW_KEY = JSON.stringify({
  recipe: { mode: "manufacturing", blueprintTypeId: 6830 },
  runs: 2,
  componentResolutions: [],
});

function fakeEditor(overrides?: Partial<BuildWorksheetEditorModel>): BuildWorksheetEditorModel {
  return {
    initialBuild: { id: ROOT_BUILD_ID },
    previewKey: PREVIEW_KEY,
    linkedBuildsByTypeId: {},
    linkedBuildPending: {},
    linkedBuildErrors: {},
    adoptLinkedBuild: vi.fn(),
    setComponentResolutions: vi.fn(),
    navigate: vi.fn(),
    // Root-enrichment inputs (all resident editor state).
    allFacilities: [],
    estimate: null,
    previewUpdating: false,
    selectedName: null,
    name: "Rifter build",
    runs: "2",
    recipe: null,
    source: undefined,
    // Root Build settings, rendered inline in the root node inspector.
    selected: {
      kind: "manufacturing",
      result: {
        blueprintTypeId: 6830,
        blueprintName: "Rifter Blueprint",
        productTypeId: 587,
        productName: "Rifter",
        groupName: "Frigate",
      },
    },
    // Root Build blueprint state for the canonical Blueprint section.
    blueprintMode: "manual",
    blueprintKind: "original",
    blueprintMe: "0",
    blueprintTe: "0",
    licensedRuns: "",
    blueprintNotes: "",
    observedBlueprints: [],
    selectedObservationId: "",
    setBlueprintMode: vi.fn(),
    setBlueprintKind: vi.fn(),
    setBlueprintMe: vi.fn(),
    setBlueprintTe: vi.fn(),
    setLicensedRuns: vi.fn(),
    setBlueprintNotes: vi.fn(),
    setSelectedObservationId: vi.fn(),
    manufacturing: { facilities: [], facilityId: "", setFacilityId: vi.fn() },
    reaction: { facilities: [], facilityId: "", setFacilityId: vi.fn() },
    rootFacilitySelection: {
      facilities: [],
      facilityId: "",
      setFacilityId: vi.fn(),
      selectedFacility: undefined,
      automaticEiv: null,
      eivError: "",
      eivLoading: false,
      manualEiv: false,
      estimatedItemValue: "",
      setManualEiv: vi.fn(),
      setEstimatedItemValue: vi.fn(),
    },
    materialScope: { regionId: 10_000_002 },
    setMaterialScope: vi.fn(),
    materialPricingPolicy: "highestBuy",
    setMaterialPricingPolicy: vi.fn(),
    outputScope: { regionId: 10_000_002 },
    setOutputScope: vi.fn(),
    outputPricingPolicy: "lowestSell",
    setOutputPricingPolicy: vi.fn(),
    sourceId: "",
    sources: [],
    setSourceId: vi.fn(),
    ...overrides,
  } as unknown as BuildWorksheetEditorModel;
}

function graphWithChildren() {
  return projection(
    productionNode({
      graphNodeId: `root:${ROOT_BUILD_ID}`,
      buildId: ROOT_BUILD_ID,
      typeId: 500,
      typeName: "Rifter",
      kind: "rootManufacturing",
      runs: 2,      children: [
        productionChild(
          productionNode({
            graphNodeId: "build:child-1",
            buildId: "child-1",
            typeId: 900,
            typeName: "Composite",
            kind: "reaction",
            parentBuildId: ROOT_BUILD_ID,
            parentComponentTypeId: 900,
            requiredQuantity: 4,
            netRequiredQuantity: 4,
            producingQuantity: 4,
          }),
          ROOT_BUILD_ID,
          900,
        ),
        acquisitionChild({
          graphNodeId: `buy:${ROOT_BUILD_ID}:35`,
          parentBuildId: ROOT_BUILD_ID,
          typeId: 35,
          typeName: "Pyerite",
        }),
      ],
    }),
  );
}

beforeEach(() => {
  setComponentResolution.mockReset().mockResolvedValue({ id: "x", revision: 2 });
  clearComponentResolution.mockReset().mockResolvedValue({ id: "x", revision: 2 });
  createLinkedBuild.mockReset().mockResolvedValue({ id: "linked-x" });
  setBuildBlueprintSelection.mockReset().mockImplementation(async (id: string) => buildDetail({ id, revision: 2 }));
  listBlueprintObservations.mockReset().mockResolvedValue([]);
  setBuildFacility.mockReset().mockImplementation(async (id: string) => buildDetail({ id, revision: 2 }));
  previewBuildGraph.mockReset();
  getBuild.mockReset();
  // Default: no linked-Build detail available. Individual enrichment tests
  // override this; other tests just get a local, harmless enrichment
  // error row that never touches page-level state.
  getBuild.mockRejectedValue(new Error("no build detail in this test"));
});
afterEach(() => vi.restoreAllMocks());

function branching() {
  return projection(
    productionNode({
      graphNodeId: `root:${ROOT_BUILD_ID}`,
      buildId: ROOT_BUILD_ID,
      typeId: 500,
      typeName: "Rifter",
      kind: "rootManufacturing",
      children: [
        productionChild(
          productionNode({
            graphNodeId: "build:A",
            buildId: "A",
            typeId: 200,
            typeName: "Assembly A",
            parentBuildId: ROOT_BUILD_ID,
            parentComponentTypeId: 200,
            children: [
              productionChild(
                productionNode({
                  graphNodeId: "build:B",
                  buildId: "B",
                  typeId: 201,
                  typeName: "Part B",
                  parentBuildId: "A",
                  parentComponentTypeId: 201,
                }),
                "A",
                201,
              ),
            ],
          }),
          ROOT_BUILD_ID,
          200,
        ),
      ],
    }),
  );
}

describe("BuildGraphView", () => {
  it("shows a hard error when the first load fails and there is no graph", async () => {
    previewBuildGraph.mockRejectedValue(new Error("network down"));
    render(<BuildGraphView active editor={fakeEditor()} />);
    expect(await screen.findByText("Couldn’t load graph")).toBeInTheDocument();
  });

  it("renders the canvas and node content once a projection loads", async () => {
    previewBuildGraph.mockResolvedValue(graphWithChildren());
    render(<BuildGraphView active editor={fakeEditor()} />);

    const canvas = await screen.findByTestId("build-graph-canvas");
    expect(within(canvas).getByText("Rifter")).toBeInTheDocument();
    expect(within(canvas).getByText("Composite")).toBeInTheDocument();
    // Every BUY requirement is its own node -- Pyerite is an acquisition node.
    expect(within(canvas).getByText("Pyerite")).toBeInTheDocument();
    // No inline material list on the root card.
    expect(within(canvas).queryByText(/root buy materials/i)).not.toBeInTheDocument();
  });

  it("shows Plan Summary in the rail when nothing is selected", async () => {
    previewBuildGraph.mockResolvedValue(graphWithChildren());
    render(<BuildGraphView active editor={fakeEditor()} />);
    const inspector = await screen.findByTestId("graph-inspector");
    expect(inspector).toHaveAttribute("data-graph-node-id", "");
    expect(inspector).toHaveTextContent("Plan Summary");
    expect(inspector).toHaveTextContent("Rifter");
  });

  it("shows graph node details in the rail on selection and clears to Plan Summary on pane click", async () => {
    previewBuildGraph.mockResolvedValue(graphWithChildren());
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor()} />);

    await screen.findByTestId("build-graph-canvas");
    await user.click(screen.getByText("Composite"));
    await waitFor(() =>
      expect(screen.getByTestId("graph-inspector")).toHaveAttribute(
        "data-graph-node-id",
        "build:child-1",
      ),
    );
    expect(screen.getByTestId("graph-inspector")).toHaveTextContent("Reaction");
    expect(screen.getByTestId("graph-inspector")).toHaveTextContent("child-1");

    // Clear via the inspector's canonical close control.
    await user.click(screen.getByRole("button", { name: "Close inspector" }));
    await waitFor(() =>
      expect(screen.getByTestId("graph-inspector")).toHaveTextContent("Plan Summary"),
    );
  });

  it("nested branches start collapsed; expand reveals descendants, collapse hides them again", async () => {
    previewBuildGraph.mockResolvedValue(branching());
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor()} />);

    const canvas = await screen.findByTestId("build-graph-canvas");
    // Assembly A is a non-root node with children -> collapsed by default.
    expect(within(canvas).queryByText("Part B")).not.toBeInTheDocument();
    expect(
      within(canvas).getByRole("button", { name: "Expand dependencies for Assembly A" }),
    ).toHaveTextContent("1 dependency hidden");

    await user.click(
      within(canvas).getByRole("button", { name: "Expand dependencies for Assembly A" }),
    );
    await waitFor(() => expect(within(canvas).getByText("Part B")).toBeInTheDocument());

    await user.click(
      within(canvas).getByRole("button", { name: "Collapse dependencies for Assembly A" }),
    );
    await waitFor(() =>
      expect(within(canvas).queryByText("Part B")).not.toBeInTheDocument(),
    );
    // The collapse click must not also select the node.
    expect(screen.getByTestId("graph-inspector")).toHaveAttribute("data-graph-node-id", "");
  });

  it("moves rail selection to the collapsed ancestor when the selected node is hidden", async () => {
    previewBuildGraph.mockResolvedValue(branching());
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor()} />);

    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(
      within(canvas).getByRole("button", { name: "Expand dependencies for Assembly A" }),
    );
    await user.click(within(canvas).getByText("Part B"));
    await waitFor(() =>
      expect(screen.getByTestId("graph-inspector")).toHaveAttribute(
        "data-graph-node-id",
        "build:B",
      ),
    );

    await user.click(
      within(canvas).getByRole("button", { name: "Collapse dependencies for Assembly A" }),
    );
    await waitFor(() =>
      expect(screen.getByTestId("graph-inspector")).toHaveAttribute(
        "data-graph-node-id",
        "build:A",
      ),
    );
  });

  it("surfaces projection warnings without dropping nodes", async () => {
    const withWarnings = graphWithChildren();
    withWarnings.warnings = [
      {
        graphNodeId: "build:child-1",
        code: "runsDiverged",
        message: "linked build produces 4 but the parent needs 6 after inventory",
      },
    ];
    previewBuildGraph.mockResolvedValue(withWarnings);
    render(<BuildGraphView active editor={fakeEditor()} />);

    const warnings = await screen.findByTestId("graph-warnings");
    expect(warnings).toHaveTextContent("runsDiverged");
    expect(within(screen.getByTestId("build-graph-canvas")).getByText("Composite")).toBeInTheDocument();
  });

  it("keeps the last graph visible and shows a refresh error on a transient failure", async () => {
    previewBuildGraph.mockResolvedValueOnce(graphWithChildren());
    const { rerender } = render(<BuildGraphView active editor={fakeEditor()} />);
    await screen.findByTestId("build-graph-canvas");

    previewBuildGraph.mockRejectedValueOnce(new Error("blip"));
    rerender(<BuildGraphView active editor={fakeEditor({ previewKey: JSON.stringify({ x: 3 }) })} />);

    await waitFor(() =>
      expect(screen.getByText(/Couldn’t refresh graph/)).toBeInTheDocument(),
    );
    expect(within(screen.getByTestId("build-graph-canvas")).getByText("Rifter")).toBeInTheDocument();
  });

  it("does not render the rail inspector or fetch while inactive", async () => {
    previewBuildGraph.mockResolvedValue(graphWithChildren());
    render(<BuildGraphView active={false} editor={fakeEditor()} />);
    // give the debounce a chance
    await new Promise((resolve) => setTimeout(resolve, 320));
    expect(previewBuildGraph).not.toHaveBeenCalled();
    expect(screen.queryByTestId("graph-inspector")).not.toBeInTheDocument();
  });

  it("shows the right rail details for each graph node type", async () => {
    previewBuildGraph.mockResolvedValue(graphWithChildren());
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor()} />);
    const canvas = await screen.findByTestId("build-graph-canvas");
    const inspector = () => screen.getByTestId("graph-inspector");

    // Root (manufacturing).
    await user.click(within(canvas).getByText("Rifter"));
    await waitFor(() => expect(inspector()).toHaveAttribute("data-graph-node-id", `root:${ROOT_BUILD_ID}`));
    expect(inspector()).toHaveTextContent("Manufacturing");
    expect(inspector()).toHaveTextContent("Runs");

    // Reaction production child.
    await user.click(within(canvas).getByText("Composite"));
    await waitFor(() => expect(inspector()).toHaveAttribute("data-graph-node-id", "build:child-1"));
    expect(inspector()).toHaveTextContent("Reaction");
    expect(inspector()).toHaveTextContent("Making");

    // Actionable Buy.
    await user.click(within(canvas).getByText("Pyerite"));
    await waitFor(() => expect(inspector()).toHaveAttribute("data-graph-node-id", `buy:${ROOT_BUILD_ID}:35`));
    expect(inspector()).toHaveTextContent("BUY MATERIAL");
    expect(within(inspector()).getByRole("radio", { name: "Buy" })).toBeInTheDocument();
  });

  it("shows unresolved Build details and remaps the inspector to Production once the linked Build lands", async () => {
    const unresolvedGraph = projection(
      productionNode({
        graphNodeId: `root:${ROOT_BUILD_ID}`,
        buildId: ROOT_BUILD_ID,
        typeId: 500,
        typeName: "Rifter",
        kind: "rootManufacturing",
        children: [
          unresolvedBuildChild({
            graphNodeId: `buy:${ROOT_BUILD_ID}:900`,
            parentBuildId: ROOT_BUILD_ID,
            typeId: 900,
            typeName: "Composite",
          }),
        ],
      }),
    );
    const resolvedGraph = projection(
      productionNode({
        graphNodeId: `root:${ROOT_BUILD_ID}`,
        buildId: ROOT_BUILD_ID,
        typeId: 500,
        typeName: "Rifter",
        kind: "rootManufacturing",
        children: [
          productionChild(
            productionNode({
              graphNodeId: "build:linked-900",
              buildId: "linked-900",
              typeId: 900,
              typeName: "Composite",
              kind: "reaction",
              parentBuildId: ROOT_BUILD_ID,
              parentComponentTypeId: 900,
            }),
            ROOT_BUILD_ID,
            900,
          ),
        ],
      }),
    );

    previewBuildGraph.mockResolvedValueOnce(unresolvedGraph);
    const user = userEvent.setup();
    const { rerender } = render(<BuildGraphView active editor={fakeEditor()} />);
    const canvas = await screen.findByTestId("build-graph-canvas");

    await user.click(within(canvas).getByText("Composite"));
    await waitFor(() =>
      expect(screen.getByTestId("graph-inspector")).toHaveAttribute(
        "data-graph-node-id",
        `buy:${ROOT_BUILD_ID}:900`,
      ),
    );
    expect(screen.getByTestId("graph-inspector")).toHaveTextContent("Resolving linked build…");

    previewBuildGraph.mockResolvedValueOnce(resolvedGraph);
    rerender(
      <BuildGraphView
        active
        editor={fakeEditor({
          linkedBuildsByTypeId: { 900: { id: "linked-900" } } as unknown as Record<
            number,
            never
          >,
        })}
      />,
    );
    await waitFor(() =>
      expect(screen.getByTestId("graph-inspector")).toHaveAttribute(
        "data-graph-node-id",
        "build:linked-900",
      ),
    );
    expect(screen.getByTestId("graph-inspector")).toHaveTextContent("Making");
  });

  it("keeps a stable selection across a refresh and shows the fresh data", async () => {
    previewBuildGraph.mockResolvedValueOnce(graphWithChildren());
    const user = userEvent.setup();
    const { rerender } = render(<BuildGraphView active editor={fakeEditor()} />);
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(within(canvas).getByText("Composite"));
    await waitFor(() =>
      expect(screen.getByTestId("graph-inspector")).toHaveAttribute("data-graph-node-id", "build:child-1"),
    );

    const refreshed = graphWithChildren();
    (refreshed.root.children[0] as { runs: number }).runs = 42;
    previewBuildGraph.mockResolvedValueOnce(refreshed);
    rerender(<BuildGraphView active editor={fakeEditor({ previewKey: JSON.stringify({ x: 7 }) })} />);

    await waitFor(() =>
      expect(screen.getByTestId("graph-inspector")).toHaveTextContent("42"),
    );
    expect(screen.getByTestId("graph-inspector")).toHaveAttribute("data-graph-node-id", "build:child-1");
  });

  // ── root sourcing actions ──────────────────────────────────────────

  it("Buy → Build on an actionable root node goes through applyComponentResolution", async () => {
    previewBuildGraph.mockResolvedValue(graphWithChildren());
    const setComponentResolutions = vi.fn();
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor({ setComponentResolutions })} />);

    const canvas = await screen.findByTestId("build-graph-canvas");
    // Card action.
    await user.click(within(canvas).getByRole("button", { name: "Build Pyerite" }));
    await waitFor(() => expect(setComponentResolutions).toHaveBeenCalledTimes(1));
    // The updater adds { 35: { recipe: buildableRecipe } } and never touches a linked-build API.
    const updater = setComponentResolutions.mock.calls[0][0] as (
      prev: Record<number, unknown>,
    ) => Record<number, unknown>;
    expect(updater({})).toEqual({
      35: { recipe: { mode: "manufacturing", blueprintTypeId: 2035 } },
    });
    expect(previewBuildGraph).not.toHaveBeenCalledWith(
      expect.stringContaining("linked-builds"),
      expect.anything(),
      expect.anything(),
    );

    // Same action from the inspector.
    await user.click(within(canvas).getByText("Pyerite"));
    await waitFor(() =>
      expect(screen.getByTestId("graph-inspector")).toHaveAttribute(
        "data-graph-node-id",
        `buy:${ROOT_BUILD_ID}:35`,
      ),
    );
    await user.click(
      within(screen.getByTestId("graph-inspector")).getByRole("radio", { name: "Build" }),
    );
    await waitFor(() => expect(setComponentResolutions).toHaveBeenCalledTimes(2));
  });

  it("actionableBuy → unresolvedBuild → production, carrying selection buy: → build:", async () => {
    const buyGraph = graphWithChildren();
    const unresolvedGraph = projection(
      productionNode({
        graphNodeId: `root:${ROOT_BUILD_ID}`,
        buildId: ROOT_BUILD_ID,
        typeId: 500,
        typeName: "Rifter",
        kind: "rootManufacturing",
        children: [
          unresolvedBuildChild({
            graphNodeId: `buy:${ROOT_BUILD_ID}:35`,
            parentBuildId: ROOT_BUILD_ID,
            typeId: 35,
            typeName: "Pyerite",
          }),
        ],
      }),
    );
    const productionGraph = projection(
      productionNode({
        graphNodeId: `root:${ROOT_BUILD_ID}`,
        buildId: ROOT_BUILD_ID,
        typeId: 500,
        typeName: "Rifter",
        kind: "rootManufacturing",
        children: [
          productionChild(
            productionNode({
              graphNodeId: "build:linked-35",
              buildId: "linked-35",
              typeId: 35,
              typeName: "Pyerite",
              parentBuildId: ROOT_BUILD_ID,
              parentComponentTypeId: 35,
            }),
            ROOT_BUILD_ID,
            35,
          ),
        ],
      }),
    );
    previewBuildGraph.mockResolvedValueOnce(buyGraph);
    const user = userEvent.setup();
    const { rerender } = render(<BuildGraphView active editor={fakeEditor()} />);
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(within(canvas).getByText("Pyerite"));
    await waitFor(() =>
      expect(screen.getByTestId("graph-inspector")).toHaveAttribute(
        "data-graph-node-id",
        `buy:${ROOT_BUILD_ID}:35`,
      ),
    );

    // Editor resolution changed -> next graph is UnresolvedBuild.
    previewBuildGraph.mockResolvedValueOnce(unresolvedGraph);
    rerender(
      <BuildGraphView
        active
        editor={fakeEditor({
          previewKey: JSON.stringify({ x: 1 }),
          linkedBuildPending: { 35: true },
        })}
      />,
    );
    await waitFor(() =>
      expect(within(screen.getByTestId("build-graph-canvas")).getByText("Creating linked build…")).toBeInTheDocument(),
    );

    // Linked build lands -> refetch -> production; selection remaps.
    previewBuildGraph.mockResolvedValueOnce(productionGraph);
    rerender(
      <BuildGraphView
        active
        editor={fakeEditor({
          previewKey: JSON.stringify({ x: 1 }),
          linkedBuildsByTypeId: { 35: { id: "linked-35" } } as unknown as Record<number, never>,
        })}
      />,
    );
    await waitFor(() =>
      expect(screen.getByTestId("graph-inspector")).toHaveAttribute(
        "data-graph-node-id",
        "build:linked-35",
      ),
    );
  });

  it("Build → Buy is inspector-only, root-direct-only, and does not delete the linked build", async () => {
    // child-1 is a root-direct production node (parentBuildId === root).
    previewBuildGraph.mockResolvedValue(graphWithChildren());
    const setComponentResolutions = vi.fn();
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor({ setComponentResolutions })} />);

    const canvas = await screen.findByTestId("build-graph-canvas");
    // No BUY action on the card.
    await user.click(within(canvas).getByText("Composite"));
    await waitFor(() =>
      expect(screen.getByTestId("graph-inspector")).toHaveAttribute(
        "data-graph-node-id",
        "build:child-1",
      ),
    );
    const inspector = screen.getByTestId("graph-inspector");
    await user.click(within(inspector).getByRole("radio", { name: "Buy" }));
    const updater = setComponentResolutions.mock.calls[0][0] as (
      prev: Record<number, unknown>,
    ) => Record<number, unknown>;
    // applyComponentResolution(componentTypeId=900, null) -> removes the entry.
    expect(updater({ 900: { recipe: {} } })).toEqual({});
    // no DELETE-style call reached the graph API layer
    expect(
      previewBuildGraph.mock.calls.every(([url]) => !String(url).includes("linked-builds")),
    ).toBe(true);
  });

  it("offers Build → Buy on a deep (non-root) production node", async () => {
    previewBuildGraph.mockResolvedValue(branching()); // root → A → B
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor()} />);
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(
      within(canvas).getByRole("button", { name: "Expand dependencies for Assembly A" }),
    );
    await user.click(within(canvas).getByText("Part B"));
    await waitFor(() =>
      expect(screen.getByTestId("graph-inspector")).toHaveAttribute("data-graph-node-id", "build:B"),
    );
    // A deep node's sourcing acts on its owning Build (A), not the root.
    expect(
      within(screen.getByTestId("graph-inspector")).getByRole("radio", { name: "Buy" }),
    ).toBeInTheDocument();
  });

  it("a Graph sourcing write re-plans through the editor preview key (Build summary, Plan, Logistics refresh)", async () => {
    previewBuildGraph.mockResolvedValue(branching()); // root → A → B
    const bumpPreview = vi.fn();
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor({ bumpPreview })} />);
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(
      within(canvas).getByRole("button", { name: "Expand dependencies for Assembly A" }),
    );
    await user.click(within(canvas).getByText("Part B"));
    await waitFor(() =>
      expect(screen.getByTestId("graph-inspector")).toHaveAttribute("data-graph-node-id", "build:B"),
    );
    // The owning Build's revision, read by the sourcing write itself.
    getBuild.mockResolvedValue({ id: "A", revision: 4 });
    await user.click(
      within(screen.getByTestId("graph-inspector")).getByRole("radio", { name: "Buy" }),
    );

    await waitFor(() => expect(clearComponentResolution).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(bumpPreview).toHaveBeenCalledTimes(1));
  });

  it("Open linked build navigates explicitly; selecting the node alone does not", async () => {
    previewBuildGraph.mockResolvedValue(graphWithChildren());
    const navigate = vi.fn();
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor({ navigate })} />);

    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(within(canvas).getByText("Composite"));
    await waitFor(() =>
      expect(screen.getByTestId("graph-inspector")).toHaveAttribute(
        "data-graph-node-id",
        "build:child-1",
      ),
    );
    expect(navigate).not.toHaveBeenCalled();

    await user.click(
      within(screen.getByTestId("graph-inspector")).getByRole("button", {
        name: "Open linked build for Composite",
      }),
    );
    expect(navigate).toHaveBeenCalledWith(
      "/builds/root-build-1/producers/child-1?view=graph",
    );
  });

  it("the root node never shows a collapse control", async () => {
    previewBuildGraph.mockResolvedValue(branching());
    render(<BuildGraphView active editor={fakeEditor()} />);
    const canvas = await screen.findByTestId("build-graph-canvas");
    // A collapse/expand control exists for Assembly A but never for the root.
    expect(
      within(canvas).getByRole("button", { name: /dependencies for Assembly A/ }),
    ).toBeInTheDocument();
    expect(
      within(canvas).queryByRole("button", { name: /dependencies for Rifter/ }),
    ).not.toBeInTheDocument();
  });

  it("Plan Summary shows frontend-derived operation counts", async () => {
    previewBuildGraph.mockResolvedValue(branching()); // root, A (mfg), B (mfg)
    render(<BuildGraphView active editor={fakeEditor()} />);
    const inspector = await screen.findByTestId("graph-inspector");
    // A + B are manufacturing production nodes; no reactions, no actionable buys.
    expect(inspector).toHaveTextContent("2 Build · 0 Reaction · 0 Buy");
  });

  it("a card warning chip appears without dropping any node or edge", async () => {
    const withWarnings = branching();
    withWarnings.warnings = [
      { graphNodeId: "build:A", code: "runsDiverged", message: "producing below need" },
    ];
    previewBuildGraph.mockResolvedValue(withWarnings);
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor()} />);
    const canvas = await screen.findByTestId("build-graph-canvas");
    expect(within(canvas).getByText("Runs diverged")).toBeInTheDocument();
    expect(within(canvas).getByText("Assembly A")).toBeInTheDocument();
    // Part B lives under the collapsed Assembly A -- expanding shows it, and
    // the warning chip on A did not drop it from the topology.
    await user.click(
      within(canvas).getByRole("button", { name: "Expand dependencies for Assembly A" }),
    );
    expect(within(canvas).getByText("Part B")).toBeInTheDocument();
  });

  it("Escape clears the graph selection back to Plan Summary", async () => {
    previewBuildGraph.mockResolvedValue(graphWithChildren());
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor()} />);
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(within(canvas).getByText("Composite"));
    await waitFor(() =>
      expect(screen.getByTestId("graph-inspector")).toHaveAttribute(
        "data-graph-node-id",
        "build:child-1",
      ),
    );
    await user.keyboard("{Escape}");
    await waitFor(() =>
      expect(screen.getByTestId("graph-inspector")).toHaveAttribute("data-graph-node-id", ""),
    );
  });

  it("a Center-selected control appears only while a node is selected", async () => {
    previewBuildGraph.mockResolvedValue(graphWithChildren());
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor()} />);
    const canvas = await screen.findByTestId("build-graph-canvas");
    expect(
      screen.queryByRole("button", { name: "Center selected node" }),
    ).not.toBeInTheDocument();
    await user.click(within(canvas).getByText("Composite"));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Center selected node" })).toBeInTheDocument(),
    );
  });

  it("node imagery does not change graph node identity", async () => {
    previewBuildGraph.mockResolvedValue(graphWithChildren());
    render(<BuildGraphView active editor={fakeEditor()} />);
    const canvas = await screen.findByTestId("build-graph-canvas");
    // Root render image loaded or not, the node id stays graphNodeId.
    expect(canvas.querySelector(`[data-node-id="root:${ROOT_BUILD_ID}"]`)).not.toBeNull();
    expect(canvas.querySelector('[data-node-id="build:child-1"]')).not.toBeNull();
  });

  // ── lazy inspector enrichment ──────────────────────────────────────

  it("fetches linked-Build detail once for a linked production node; never for root or actionable Buy", async () => {
    previewBuildGraph.mockResolvedValue(graphWithChildren());
    getBuild.mockResolvedValue(reactionBuildDetail({ id: "child-1" }));
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor()} />);
    const canvas = await screen.findByTestId("build-graph-canvas");

    await user.click(within(canvas).getByText("Rifter"));
    await waitFor(() =>
      expect(screen.getByTestId("graph-inspector")).toHaveAttribute(
        "data-graph-node-id",
        `root:${ROOT_BUILD_ID}`,
      ),
    );
    expect(getBuild).not.toHaveBeenCalled();

    await user.click(within(canvas).getByText("Pyerite"));
    await waitFor(() =>
      expect(screen.getByTestId("graph-inspector")).toHaveAttribute(
        "data-graph-node-id",
        `buy:${ROOT_BUILD_ID}:35`,
      ),
    );
    expect(getBuild).not.toHaveBeenCalled();

    await user.click(within(canvas).getByText("Composite"));
    await waitFor(() => expect(getBuild).toHaveBeenCalledTimes(1));
    expect(getBuild).toHaveBeenCalledWith("child-1", expect.any(AbortSignal));
  });

  it("makes no eager linked-Build request on graph load", async () => {
    previewBuildGraph.mockResolvedValue(graphWithChildren());
    render(<BuildGraphView active editor={fakeEditor()} />);
    await screen.findByTestId("build-graph-canvas");
    await new Promise((resolve) => setTimeout(resolve, 60));
    expect(getBuild).not.toHaveBeenCalled();
  });

  it("keeps graph-derived inspector content while detail loads and still offers Open linked build", async () => {
    previewBuildGraph.mockResolvedValue(graphWithChildren());
    let resolveDetail: (value: unknown) => void = () => {};
    getBuild.mockImplementation(() => new Promise((resolve) => (resolveDetail = resolve)));
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor()} />);
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(within(canvas).getByText("Composite"));

    const inspector = await screen.findByTestId("graph-inspector");
    await waitFor(() => expect(inspector).toHaveTextContent("Loading build details…"));
    expect(inspector).toHaveTextContent("Making");
    expect(inspector).toHaveTextContent("Material");
    expect(
      within(inspector).getByRole("button", { name: "Open linked build for Composite" }),
    ).toBeInTheDocument();

    resolveDetail(reactionBuildDetail({ id: "child-1" }));
    await waitFor(() => expect(inspector).toHaveTextContent("Composite Reaction"));
  });

  it("linked-Build detail failure is local: Retry stays, page graph error untouched, retry recovers", async () => {
    previewBuildGraph.mockResolvedValue(graphWithChildren());
    getBuild.mockRejectedValueOnce(new Error("boom"));
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor()} />);
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(within(canvas).getByText("Composite"));

    const inspector = await screen.findByTestId("graph-inspector");
    await waitFor(() => expect(inspector).toHaveTextContent("Unable to load build details."));
    expect(inspector).toHaveTextContent("Making");
    expect(
      within(inspector).getByRole("button", { name: "Open linked build for Composite" }),
    ).toBeInTheDocument();
    expect(screen.queryByText("Couldn’t load graph")).not.toBeInTheDocument();

    getBuild.mockResolvedValueOnce(reactionBuildDetail({ id: "child-1" }));
    await user.click(within(inspector).getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(inspector).toHaveTextContent("Composite Reaction"));
    expect(inspector).not.toHaveTextContent("Unable to load build details.");
  });

  it("Manufacturing linked node: Blueprint section (name · BPO · ME/TE) + resolved facility", async () => {
    previewBuildGraph.mockResolvedValue(branching()); // root -> Assembly A (mfg, root-direct) -> Part B
    getBuild.mockResolvedValue(buildDetail({ id: "A" }));
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor({ allFacilities: [facilityProfile()] })} />);
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(within(canvas).getByText("Assembly A"));

    const inspector = await screen.findByTestId("graph-inspector");
    const blueprint = await within(inspector).findByRole("region", { name: "Blueprint" });
    expect(blueprint).toHaveTextContent("Composite Blueprint");
    // Stages, not Graph, now edits descendant blueprint/facility configuration.
    expect(within(blueprint).getByText("ME 8 · TE 14")).toBeInTheDocument();
    expect(within(blueprint).queryByLabelText("Material Efficiency (0-10)")).toBeNull();
    expect(within(blueprint).queryByLabelText("Time Efficiency (0-20)")).toBeNull();
    expect(inspector).toHaveTextContent("Sotiyo — Assembly");
    expect(inspector).toHaveTextContent("Jita");
    expect(inspector).not.toHaveTextContent("Reaction formula");
  });

  it("Reaction linked node: Formula section, no blueprint ME/TE/BPO", async () => {
    previewBuildGraph.mockResolvedValue(graphWithChildren()); // Composite is a reaction node
    getBuild.mockResolvedValue(reactionBuildDetail({ id: "child-1" }));
    const user = userEvent.setup();
    render(
      <BuildGraphView active editor={fakeEditor({ allFacilities: [facilityProfile({ role: "reaction" })] })} />,
    );
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(within(canvas).getByText("Composite"));

    const inspector = await screen.findByTestId("graph-inspector");
    await waitFor(() => expect(inspector).toHaveTextContent("Composite Reaction"));
    expect(inspector).toHaveTextContent("Reaction formula");
    expect(inspector).not.toHaveTextContent("Blueprint");
    expect(inspector).not.toHaveTextContent("BPO");
    expect(inspector).not.toHaveTextContent("ME 8");
    expect(inspector).toHaveTextContent("Sotiyo");
  });

  it("missing facility profile degrades to 'Unavailable' with no crash", async () => {
    previewBuildGraph.mockResolvedValue(branching());
    getBuild.mockResolvedValue(buildDetail({ id: "A" })); // references facility "fac-1"
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor({ allFacilities: [] })} />);
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(within(canvas).getByText("Assembly A"));

    const inspector = await screen.findByTestId("graph-inspector");
    await waitFor(() =>
      expect(within(inspector).getByRole("region", { name: "Blueprint" })).toBeInTheDocument(),
    );
    expect(inspector).toHaveTextContent("Unavailable");
  });

  it("observedAsset blueprint: read-only rendering, no invented ME/TE, no extra fetch", async () => {
    previewBuildGraph.mockResolvedValue(branching());
    const observed = buildDetail({ id: "A" });
    observed.draftPlanning!.input.blueprintSelection = {
      mode: "observedAsset",
      observationId: "obs-1",
    };
    getBuild.mockResolvedValue(observed);
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor({ allFacilities: [facilityProfile()] })} />);
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(within(canvas).getByText("Assembly A"));

    const inspector = await screen.findByTestId("graph-inspector");
    const blueprint = await within(inspector).findByRole("region", { name: "Blueprint" });
    // Stages, not Graph, now edits descendant blueprint configuration -- no
    // "Use available blueprint" selection control renders here anymore.
    expect(within(blueprint).queryByRole("button", { name: "Use available blueprint" })).toBeNull();
    // No fabricated ME 0 / TE 0 for an unresolved observed-asset selection.
    expect(within(blueprint).queryByText(/ME 0/)).toBeNull();
    expect(getBuild).toHaveBeenCalledTimes(1);
  });

  it("shows the DTO's authoritative effectiveMe/effectiveTe, not an inferred value", async () => {
    // The graph node carries the effective ME/TE resolved by the same
    // preview the worksheet uses; the persisted Build only has an
    // observed-asset selection with no ME on it.
    previewBuildGraph.mockResolvedValue(
      projection(
        productionNode({
          graphNodeId: `root:${ROOT_BUILD_ID}`,
          buildId: ROOT_BUILD_ID,
          typeId: 500,
          typeName: "Rifter",
          kind: "rootManufacturing",
          children: [
            productionChild(
              productionNode({
                graphNodeId: "build:A",
                buildId: "A",
                typeId: 200,
                typeName: "Assembly A",
                parentBuildId: ROOT_BUILD_ID,
                parentComponentTypeId: 200,
                effectiveMe: 10,
                effectiveTe: 20,
              }),
              ROOT_BUILD_ID,
              200,
            ),
          ],
        }),
      ),
    );
    const observed = buildDetail({ id: "A" });
    observed.draftPlanning!.input.blueprintSelection = {
      mode: "observedAsset",
      observationId: "obs-1",
    };
    getBuild.mockResolvedValue(observed);
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor({ allFacilities: [facilityProfile()] })} />);
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(within(canvas).getByText("Assembly A"));

    const inspector = await screen.findByTestId("graph-inspector");
    const blueprint = await within(inspector).findByRole("region", { name: "Blueprint" });
    expect(blueprint).toHaveTextContent("ME 10 · TE 20");
  });

  function graphWithLinkedRecipeState(state: string) {
    return projection(
      productionNode({
        graphNodeId: `root:${ROOT_BUILD_ID}`,
        buildId: ROOT_BUILD_ID,
        typeId: 500,
        typeName: "Rifter",
        kind: "rootManufacturing",
        children: [
          productionChild(
            productionNode({
              graphNodeId: "build:A",
              buildId: "A",
              typeId: 200,
              typeName: "Assembly A",
              parentBuildId: ROOT_BUILD_ID,
              parentComponentTypeId: 200,
              recipeCurrency: state as never,
            }),
            ROOT_BUILD_ID,
            200,
          ),
        ],
      }),
    );
  }

  it("inspector shows an amber chip + non-blaming explanation for recipeChanged", async () => {
    previewBuildGraph.mockResolvedValue(graphWithLinkedRecipeState("recipeChanged"));
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor()} />);
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(within(canvas).getByText("Assembly A"));

    const inspector = await screen.findByTestId("graph-inspector");
    await waitFor(() => expect(inspector).toHaveAttribute("data-graph-node-id", "build:A"));
    expect(inspector).toHaveTextContent("Recipe data changed");
    expect(inspector).toHaveTextContent(
      /current SDE recipe for this blueprint differs from the one captured/i,
    );
    // The ambiguous old wording is gone.
    expect(inspector).not.toHaveTextContent("Recipe changed");
  });

  it("inspector stays silent about recipeCurrency for olderSdeVersion", async () => {
    previewBuildGraph.mockResolvedValue(graphWithLinkedRecipeState("olderSdeVersion"));
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor()} />);
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(within(canvas).getByText("Assembly A"));

    const inspector = await screen.findByTestId("graph-inspector");
    await waitFor(() => expect(inspector).toHaveAttribute("data-graph-node-id", "build:A"));
    expect(inspector).not.toHaveTextContent("Recipe data changed");
    expect(inspector).not.toHaveTextContent("Recipe changed");
    expect(inspector).not.toHaveTextContent(/differs from the one captured/i);
  });

  it("root with no estimate: graph-derived fields stay, only preview-derived rows say Computing…, never fetches root", async () => {
    previewBuildGraph.mockResolvedValue(graphWithChildren());
    const user = userEvent.setup();
    render(
      <BuildGraphView
        active
        editor={fakeEditor({ estimate: null, selectedName: "Rifter Blueprint" })}
      />,
    );
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(within(canvas).getByText("Rifter"));

    const inspector = await screen.findByTestId("graph-inspector");
    await waitFor(() =>
      expect(inspector).toHaveAttribute("data-graph-node-id", `root:${ROOT_BUILD_ID}`),
    );
    expect(inspector).toHaveTextContent("Manufacturing");
    expect(inspector).toHaveTextContent("Runs");
    expect(inspector).toHaveTextContent("Material");
    expect(inspector).toHaveTextContent("Rifter Blueprint");
    expect(inspector).toHaveTextContent("Computing…");
    expect(getBuild).not.toHaveBeenCalled();
  });

  it("root with a full estimate shows blueprint, facility, planned duration and installation cost — no fetch", async () => {
    previewBuildGraph.mockResolvedValue(graphWithChildren());
    const estimate = {
      candidate: {
        blueprint: {
          blueprintName: "Rifter Blueprint",
          kind: "original",
          materialEfficiency: 10,
          timeEfficiency: 20,
          plannedDurationSeconds: 7200,
        },
        manufacturingFacility: {
          profile: facilityProfile(),
          plannedDurationSeconds: 7200,
          installationCost: {},
        },
        reactionFacility: null,
      },
      worksheet: { summary: { installationCost: "123.0000", totalCost: "456.0000" } },
      completeness: { installation: "complete" },
    };
    const user = userEvent.setup();
    render(
      <BuildGraphView
        active
        editor={fakeEditor({
          estimate: estimate as never,
          allFacilities: [facilityProfile()],
          selectedName: "Rifter Blueprint",
          blueprintMe: "10",
          blueprintTe: "20",
          rootFacilitySelection: {
            facilities: [facilityProfile()],
            facilityId: facilityProfile().id,
            setFacilityId: vi.fn(),
            selectedFacility: facilityProfile(),
            automaticEiv: null,
            eivError: "",
            eivLoading: false,
            manualEiv: false,
            estimatedItemValue: "",
            setManualEiv: vi.fn(),
            setEstimatedItemValue: vi.fn(),
          } as never,
        })}
      />,
    );
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(within(canvas).getByText("Rifter"));

    const inspector = await screen.findByTestId("graph-inspector");
    const blueprint = await within(inspector).findByRole("region", { name: "Blueprint" });
    expect(blueprint).toHaveTextContent("Rifter Blueprint");
    expect(within(blueprint).getByLabelText("Material Efficiency (0-10)")).toHaveValue("10");
    expect(within(blueprint).getByLabelText("Time Efficiency (0-20)")).toHaveValue("20");
    const facility = within(inspector).getByRole("region", { name: "Facility" });
    expect(within(facility).getByLabelText("Manufacturing Facility")).toBeInTheDocument();
    expect(inspector).toHaveTextContent("Planned duration");
    expect(within(inspector).getByRole("region", { name: "Cost" })).toHaveTextContent("Installation");
    expect(getBuild).not.toHaveBeenCalled();
  });

  // ── Build settings access from the graph inspector ─────────────────

  it("root Production node: Build settings are editable inline through the canonical sections (no rail-mode swap)", async () => {
    previewBuildGraph.mockResolvedValue(graphWithChildren());
    const setMaterialPricingPolicy = vi.fn();
    const navigate = vi.fn();
    const user = userEvent.setup();
    render(
      <BuildGraphView active editor={fakeEditor({ setMaterialPricingPolicy, navigate })} />,
    );

    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(within(canvas).getByText("Rifter"));
    await waitFor(() =>
      expect(screen.getByTestId("graph-inspector")).toHaveAttribute(
        "data-graph-node-id",
        `root:${ROOT_BUILD_ID}`,
      ),
    );

    // The canonical root inspector renders inline in the root node -- no
    // "Edit build settings" button, no separate rail mode.
    const inspector = within(screen.getByTestId("graph-inspector"));
    expect(inspector.getByRole("region", { name: "Pricing" })).toBeInTheDocument();
    expect(inspector.getByRole("region", { name: "Facility" })).toBeInTheDocument();
    expect(inspector.queryByRole("button", { name: /Edit build settings/ })).not.toBeInTheDocument();

    await user.selectOptions(
      inspector.getByLabelText("Material pricing"),
      "acquireQuantityFromSellOrders",
    );
    expect(setMaterialPricingPolicy).toHaveBeenCalledWith("acquireQuantityFromSellOrders");
    // The graph inspector stays put; no navigation for the root.
    expect(screen.getByTestId("graph-inspector")).toBeInTheDocument();
    expect(navigate).not.toHaveBeenCalled();
  });

  it("linked Production node: no Edit build settings action — only Open linked build", async () => {
    previewBuildGraph.mockResolvedValue(graphWithChildren());
    const openBuildSettings = vi.fn();
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor({ openBuildSettings })} />);

    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(within(canvas).getByText("Composite"));
    await waitFor(() =>
      expect(screen.getByTestId("graph-inspector")).toHaveAttribute(
        "data-graph-node-id",
        "build:child-1",
      ),
    );

    const inspector = within(screen.getByTestId("graph-inspector"));
    expect(
      inspector.queryByRole("button", { name: /Edit build settings/ }),
    ).not.toBeInTheDocument();
    expect(
      inspector.getByRole("button", { name: "Open linked build for Composite" }),
    ).toBeInTheDocument();
    expect(openBuildSettings).not.toHaveBeenCalled();
  });

  it("Buy and unresolved-Build nodes never expose Edit build settings", async () => {
    const unresolvedGraph = projection(
      productionNode({
        graphNodeId: `root:${ROOT_BUILD_ID}`,
        buildId: ROOT_BUILD_ID,
        typeId: 500,
        typeName: "Rifter",
        kind: "rootManufacturing",
        children: [
          acquisitionChild({
            graphNodeId: `buy:${ROOT_BUILD_ID}:35`,
            parentBuildId: ROOT_BUILD_ID,
            typeId: 35,
            typeName: "Pyerite",
          }),
          unresolvedBuildChild({
            graphNodeId: `buy:${ROOT_BUILD_ID}:900`,
            parentBuildId: ROOT_BUILD_ID,
            typeId: 900,
            typeName: "Composite",
          }),
        ],
      }),
    );
    previewBuildGraph.mockResolvedValue(unresolvedGraph);
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor()} />);
    const canvas = await screen.findByTestId("build-graph-canvas");

    await user.click(within(canvas).getByText("Pyerite"));
    await waitFor(() =>
      expect(screen.getByTestId("graph-inspector")).toHaveAttribute(
        "data-graph-node-id",
        `buy:${ROOT_BUILD_ID}:35`,
      ),
    );
    expect(
      within(screen.getByTestId("graph-inspector")).queryByRole("button", {
        name: /Edit build settings/,
      }),
    ).not.toBeInTheDocument();

    await user.click(within(canvas).getByText("Composite"));
    await waitFor(() =>
      expect(screen.getByTestId("graph-inspector")).toHaveAttribute(
        "data-graph-node-id",
        `buy:${ROOT_BUILD_ID}:900`,
      ),
    );
    expect(
      within(screen.getByTestId("graph-inspector")).queryByRole("button", {
        name: /Edit build settings/,
      }),
    ).not.toBeInTheDocument();
  });

  // ── recursive acquisition dependencies ────────────────────────────────
  //
  //   Root A
  //   ├─ Build B
  //   │  ├─ Buy X  (buildable)
  //   │  ├─ Buy Xraw (raw)
  //   │  └─ Build C
  //   │     └─ Reaction D
  //   │        └─ Buy Z (raw)
  //   └─ Buy X  (buildable, but a *different* parent -> distinct id)
  function deepHierarchy() {
    const d = productionNode({
      graphNodeId: "build:D",
      buildId: "D",
      typeId: 401,
      kind: "reaction",
      recipe: { mode: "reaction", reactionFormulaTypeId: 9401 },
      parentBuildId: "C",
      parentComponentTypeId: 401,
      children: [
        rawAcquisitionChild({ graphNodeId: "buy:D:34", parentBuildId: "D", typeId: 34, typeName: "Isogen" }),
      ],
    });
    const c = productionNode({
      graphNodeId: "build:C",
      buildId: "C",
      typeId: 300,
      parentBuildId: "B",
      parentComponentTypeId: 300,
      children: [productionChild(d, "C", 401)],
    });
    const b = productionNode({
      graphNodeId: "build:B",
      buildId: "B",
      typeId: 200,
      parentBuildId: ROOT_BUILD_ID,
      parentComponentTypeId: 200,
      children: [
        acquisitionChild({ graphNodeId: "buy:B:900", parentBuildId: "B", typeId: 900, typeName: "Widget" }),
        rawAcquisitionChild({ graphNodeId: "buy:B:35", parentBuildId: "B", typeId: 35, typeName: "Pyerite" }),
        productionChild(c, "B", 300),
      ],
    });
    return projection(
      productionNode({
        graphNodeId: `root:${ROOT_BUILD_ID}`,
        buildId: ROOT_BUILD_ID,
        typeId: 500,
        kind: "rootManufacturing",
        children: [
          productionChild(b, ROOT_BUILD_ID, 200),
          acquisitionChild({
            graphNodeId: `buy:${ROOT_BUILD_ID}:900`,
            parentBuildId: ROOT_BUILD_ID,
            typeId: 900,
            typeName: "Widget",
          }),
        ],
      }),
    );
  }

  it("nested production nodes start collapsed; expanding reveals buildable + raw acquisition children", async () => {
    previewBuildGraph.mockResolvedValue(deepHierarchy());
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor()} />);
    const canvas = await screen.findByTestId("build-graph-canvas");

    // B (and C, D) collapsed by default -> their acquisition children hidden.
    expect(within(canvas).queryByText("Pyerite")).not.toBeInTheDocument();

    await user.click(
      within(canvas).getByRole("button", { name: "Expand dependencies for Type 200" }),
    );
    // Both a buildable (Widget) and a raw (Pyerite) acquisition child of B appear.
    await waitFor(() =>
      expect(canvas.querySelector(`[data-node-id="buy:B:900"]`)).toBeInTheDocument(),
    );
    const buildableNode = canvas.querySelector(`[data-node-id="buy:B:900"]`) as HTMLElement;
    const rawNode = canvas.querySelector(`[data-node-id="buy:B:35"]`) as HTMLElement;
    expect(within(rawNode).getByText("Pyerite")).toBeInTheDocument();
    // Buildable one carries a Switch to BUILD action; raw does not.
    expect(within(buildableNode).getByRole("button", { name: "Build Widget" })).toBeInTheDocument();
    expect(within(rawNode).queryByRole("button", { name: /^Build / })).not.toBeInTheDocument();
  });

  it("the same type under two parents is two distinct acquisition nodes", async () => {
    previewBuildGraph.mockResolvedValue(deepHierarchy());
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor()} />);
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(
      within(canvas).getByRole("button", { name: "Expand dependencies for Type 200" }),
    );
    await waitFor(() =>
      expect(canvas.querySelector(`[data-node-id="buy:B:900"]`)).toBeInTheDocument(),
    );
    // Root's Widget and B's Widget are separate nodes -- never deduped by type.
    expect(canvas.querySelector(`[data-node-id="buy:${ROOT_BUILD_ID}:900"]`)).toBeInTheDocument();
    expect(canvas.querySelector(`[data-node-id="buy:B:900"]`)).toBeInTheDocument();
  });

  it("nested BUY → BUILD targets the owning Build, never the root", async () => {
    previewBuildGraph.mockResolvedValue(deepHierarchy());
    getBuild.mockResolvedValue({ id: "B", revision: 7 });
    const setComponentResolutions = vi.fn();
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor({ setComponentResolutions })} />);
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(
      within(canvas).getByRole("button", { name: "Expand dependencies for Type 200" }),
    );
    await waitFor(() =>
      expect(canvas.querySelector(`[data-node-id="buy:B:900"]`)).toBeInTheDocument(),
    );
    const bWidget = canvas.querySelector(`[data-node-id="buy:B:900"]`) as HTMLElement;
    await user.click(within(bWidget).getByRole("button", { name: "Build Widget" }));

    await waitFor(() => expect(setComponentResolution).toHaveBeenCalled());
    // Persist on B (the owning Build), then create-or-reuse the linked Build under B.
    expect(setComponentResolution).toHaveBeenCalledWith("B", {
      componentTypeId: 900,
      recipe: { mode: "manufacturing", blueprintTypeId: 2000 + 900 },
      expectedRevision: 7,
    });
    await waitFor(() =>
      expect(createLinkedBuild).toHaveBeenCalledWith("B", { componentTypeId: 900 }),
    );
    // The root editor is never touched for a nested switch.
    expect(setComponentResolutions).not.toHaveBeenCalled();
  });

  it("nested BUILD → BUY targets the owning Build and retains the linked row", async () => {
    // root -> Assembly A -> Part B (both production; B's owner is A).
    previewBuildGraph.mockResolvedValue(branching());
    // Detail fetch for the selected node (B) + the owner-revision read for A.
    getBuild.mockImplementation(async (id: string) => buildDetail({ id, revision: 4 }));
    const setComponentResolutions = vi.fn();
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor({ setComponentResolutions })} />);
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(
      within(canvas).getByRole("button", { name: "Expand dependencies for Assembly A" }),
    );
    await user.click(within(canvas).getByText("Part B"));
    await waitFor(() =>
      expect(screen.getByTestId("graph-inspector")).toHaveAttribute("data-graph-node-id", "build:B"),
    );
    await user.click(
      within(screen.getByTestId("graph-inspector")).getByRole("radio", { name: "Buy" }),
    );
    await waitFor(() =>
      expect(clearComponentResolution).toHaveBeenCalledWith("A", 201, 4),
    );
    expect(setComponentResolutions).not.toHaveBeenCalled();
  });

  it("expanding a reaction node exposes its own acquisition dependencies", async () => {
    previewBuildGraph.mockResolvedValue(deepHierarchy());
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor()} />);
    const canvas = await screen.findByTestId("build-graph-canvas");
    // Walk down: B -> C -> D (reaction) -> its raw Buy.
    await user.click(within(canvas).getByRole("button", { name: "Expand dependencies for Type 200" }));
    await user.click(await within(canvas).findByRole("button", { name: "Expand dependencies for Type 300" }));
    await user.click(await within(canvas).findByRole("button", { name: "Expand dependencies for Type 401" }));
    expect(await within(canvas).findByText("Isogen")).toBeInTheDocument();
  });

  // ── PR: in-place linked-Build settings editing (unified inspector) ──

  it("a linked node's blueprint is read-only in Graph -- Stages owns descendant configuration editing", async () => {
    previewBuildGraph.mockResolvedValue(branching()); // root -> Assembly A (id "A") -> Part B
    getBuild.mockResolvedValue(buildDetail({ id: "A" })); // ME 8 / TE 14, revision 1
    const setComponentResolutions = vi.fn();
    const user = userEvent.setup();
    render(
      <BuildGraphView
        active
        editor={fakeEditor({ setComponentResolutions, allFacilities: [facilityProfile()] })}
      />,
    );
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(within(canvas).getByText("Assembly A"));

    const inspector = await screen.findByTestId("graph-inspector");
    const blueprint = await within(inspector).findByRole("region", { name: "Blueprint" });
    expect(within(blueprint).queryByLabelText("Material Efficiency (0-10)")).toBeNull();
    expect(within(blueprint).getByText("ME 8 · TE 14")).toBeInTheDocument();
    expect(setBuildBlueprintSelection).not.toHaveBeenCalled();
    expect(setComponentResolutions).not.toHaveBeenCalled();
  });

  it("root node blueprint editing writes the worksheet editor's blueprint state, not a linked-Build patch", async () => {
    previewBuildGraph.mockResolvedValue(branching());
    const setBlueprintMe = vi.fn();
    const setBlueprintMode = vi.fn();
    const user = userEvent.setup();
    render(
      <BuildGraphView
        active
        editor={fakeEditor({
          setBlueprintMe,
          setBlueprintMode,
          selectedName: "Rifter Blueprint",
        })}
      />,
    );
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(within(canvas).getByText("Rifter"));

    const inspector = await screen.findByTestId("graph-inspector");
    const blueprint = await within(inspector).findByRole("region", { name: "Blueprint" });
    const me = within(blueprint).getByLabelText("Material Efficiency (0-10)");
    await user.clear(me);
    await user.type(me, "7");
    await user.tab();

    await waitFor(() => expect(setBlueprintMe).toHaveBeenCalledWith("7"));
    expect(setBlueprintMode).toHaveBeenCalledWith("manual");
    // Never a linked-Build patch for the root.
    expect(setBuildBlueprintSelection).not.toHaveBeenCalled();
  });

  it("a linked node's facility is read-only in Graph, never patched from here", async () => {
    previewBuildGraph.mockResolvedValue(branching());
    getBuild.mockResolvedValue(buildDetail({ id: "A" }));
    const setComponentResolutions = vi.fn();
    const user = userEvent.setup();
    render(
      <BuildGraphView
        active
        editor={fakeEditor({
          setComponentResolutions,
          allFacilities: [
            facilityProfile(),
            facilityProfile({ id: "fac-2", name: "Other Assembly", revision: 3 }),
          ],
        })}
      />,
    );
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(within(canvas).getByText("Assembly A"));

    const inspector = await screen.findByTestId("graph-inspector");
    const facility = await within(inspector).findByRole("region", { name: "Facility" });
    expect(within(facility).queryByRole("combobox", { name: "Facility" })).toBeNull();
    expect(setBuildFacility).not.toHaveBeenCalled();
    expect(setComponentResolutions).not.toHaveBeenCalled();
  });

  it("a reaction linked node shows facility read-only and never ME/TE", async () => {
    previewBuildGraph.mockResolvedValue(graphWithChildren()); // Composite is a reaction node
    getBuild.mockResolvedValue(reactionBuildDetail({ id: "child-1" }));
    const user = userEvent.setup();
    render(
      <BuildGraphView
        active
        editor={fakeEditor({ allFacilities: [facilityProfile({ role: "reaction" })] })}
      />,
    );
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(within(canvas).getByText("Composite"));

    const inspector = await screen.findByTestId("graph-inspector");
    const facility = await within(inspector).findByRole("region", { name: "Facility" });
    expect(within(facility).queryByRole("combobox", { name: "Facility" })).toBeNull();
    expect(within(inspector).queryByRole("region", { name: "Blueprint" })).not.toBeInTheDocument();
  });

  it("the root node uses the canonical root inspector, not the linked-node in-place controls", async () => {
    previewBuildGraph.mockResolvedValue(branching());
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor()} />);
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(within(canvas).getByText("Rifter"));

    const inspector = await screen.findByTestId("graph-inspector");
    await waitFor(() =>
      expect(inspector).toHaveAttribute("data-graph-node-id", `root:${ROOT_BUILD_ID}`),
    );
    // The root Blueprint / Facility / Pricing sections are editable through
    // the resident editor state via the SAME canonical builder the Worksheet
    // "build settings" mode uses -- no "Edit build settings" button.
    expect(within(inspector).getByRole("region", { name: "Blueprint" })).toBeInTheDocument();
    expect(within(inspector).getByRole("region", { name: "Facility" })).toBeInTheDocument();
    expect(within(inspector).getByRole("region", { name: "Pricing" })).toBeInTheDocument();
    expect(
      within(inspector).queryByRole("button", { name: /Edit build settings/ }),
    ).not.toBeInTheDocument();
  });

  // ── PR: canonical shared-section render ────────────────────────────

  it("renders the selected node through the shared canonical InspectorSections", async () => {
    previewBuildGraph.mockResolvedValue(branching());
    getBuild.mockResolvedValue(buildDetail({ id: "A" }));
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor({ allFacilities: [facilityProfile()] })} />);
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(within(canvas).getByText("Assembly A"));

    const inspector = await screen.findByTestId("graph-inspector");
    for (const name of ["Quantities", "Sourcing", "Blueprint", "Facility", "Cost"]) {
      expect(await within(inspector).findByRole("region", { name })).toBeInTheDocument();
    }
    // Reaction nodes get a "Formula" region and never a "Blueprint" one.
    await user.click(within(canvas).getByText("Rifter"));
    await user.click(within(canvas).getByText("Assembly A"));
  });

  it("a section stays collapsed after switching to another node and back", async () => {
    previewBuildGraph.mockResolvedValue(branching());
    getBuild.mockResolvedValue(buildDetail({ id: "A" }));
    const user = userEvent.setup();
    render(<BuildGraphView active editor={fakeEditor({ allFacilities: [facilityProfile()] })} />);
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(within(canvas).getByText("Assembly A"));

    const inspector = await screen.findByTestId("graph-inspector");
    const costHeader = await within(inspector).findByRole("button", { name: /Cost/ });
    expect(costHeader).toHaveAttribute("aria-expanded", "true");
    await user.click(costHeader);
    expect(within(inspector).getByRole("button", { name: /Cost/ })).toHaveAttribute(
      "aria-expanded",
      "false",
    );

    // Select the root, then come back to Assembly A -- Cost is still collapsed.
    await user.click(within(canvas).getByText("Rifter"));
    await user.click(within(canvas).getByText("Assembly A"));
    await waitFor(() =>
      expect(screen.getByTestId("graph-inspector")).toHaveAttribute(
        "data-graph-node-id",
        "build:A",
      ),
    );
    expect(
      within(screen.getByTestId("graph-inspector")).getByRole("button", { name: /Cost/ }),
    ).toHaveAttribute("aria-expanded", "false");
  });

  it("a reaction linked node renders a Formula region, never a Blueprint one", async () => {
    previewBuildGraph.mockResolvedValue(graphWithChildren());
    getBuild.mockResolvedValue(reactionBuildDetail({ id: "child-1" }));
    const user = userEvent.setup();
    render(
      <BuildGraphView active editor={fakeEditor({ allFacilities: [facilityProfile({ role: "reaction" })] })} />,
    );
    const canvas = await screen.findByTestId("build-graph-canvas");
    await user.click(within(canvas).getByText("Composite"));

    const inspector = await screen.findByTestId("graph-inspector");
    expect(await within(inspector).findByRole("region", { name: "Recipe" })).toBeInTheDocument();
    expect(within(inspector).queryByRole("region", { name: "Blueprint" })).not.toBeInTheDocument();
  });
});
