import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type {
  AcquisitionConsumerRef,
  AcquisitionLine,
  CostWarning,
  ExecutionNode,
  ExecutionOccurrence,
  ExecutionPlanProjection,
  ExecutionStage,
} from "../../../../../api/industry";
import type { BuildWorksheetEditorModel } from "../../use-build-worksheet-editor";
import { BuildStagesView } from "../build-stages-view";

const postBuildExecutionPlan = vi.fn();
vi.mock("../../../../../api/industry", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  postBuildExecutionPlan: (...args: unknown[]) => postBuildExecutionPlan(...args),
}));

const OVERLAY = {
  recipe: { mode: "manufacturing", blueprintTypeId: 80_000 },
  runs: 1,
  componentResolutions: [],
};


/** A production consumer edge: `quantity` is its production demand. */

function editorStub(over: Partial<BuildWorksheetEditorModel> = {}): BuildWorksheetEditorModel {
  return {
    initialBuild: { id: "build-1" },
    previewKey: JSON.stringify(OVERLAY),
    linkedBuildsByTypeId: {},
    ...over,
  } as unknown as BuildWorksheetEditorModel;
}

function occurrence(
  over: Partial<ExecutionOccurrence> & { id: string; nodeId: string },
): ExecutionOccurrence {
  return {
    buildId: over.id,
    revision: 1,
    blueprintSelection: null,
    isRoot: false,
    stage: 0,
    activity: "manufacturing",
    outputTypeId: 1,
    outputTypeName: "Item",
    blueprintOrFormulaTypeId: 1,
    blueprintOrFormulaName: "Item Blueprint",
    facilityId: null,
    facilityName: null,
    effectiveMe: null,
    effectiveTe: null,
    projectedRuns: 1,
    projectedOutput: 1,
    requiredQuantity: 0,
    plannedInventoryQuantity: 0,
    productionDemand: 0,
    retainedSurplusQuantity: 0,
    retainedSurplusCost: null,
    materialComponentCost: null,
    ownInstallationCost: null,
    totalProductionCost: null,
    unitProductionCost: null,
    costComplete: true,
    requirements: [],
    ...over,
  };
}

function node(over: Partial<ExecutionNode> & { id: string; occurrenceIds: string[] }): ExecutionNode {
  return {
    outputTypeId: 1,
    outputTypeName: "Item",
    activity: "manufacturing",
    stage: 0,
    facilityId: null,
    facilityName: null,
    effectiveMe: null,
    effectiveTe: null,
    requiredQuantity: 0,
    plannedInventoryQuantity: 0,
    productionDemand: 0,
    projectedOutput: 0,
    projectedRuns: 0,
    retainedSurplusQuantity: 0,
    retainedSurplusCost: null,
    materialComponentCost: null,
    ownInstallationCost: null,
    totalProductionCost: null,
    costComplete: true,
    consumers: [],
    productionMethods: [],
    unitProductionCost: null,
    availableQuantity: 0,
    ...over,
  };
}

function acquisition(over: Partial<AcquisitionLine> & { typeId: number }): AcquisitionLine {
  return {
    typeName: `Type ${over.typeId}`,
    requiredQuantity: 100,
    plannedInventoryQuantity: 0,
    shortageQuantity: 100,
    availableQuantity: 0,
    reservedQuantity: 0,
    sourceStrategy: "buy",
    consumers: [],
    productionMethods: [],
    freshCost: null,
    freshUnitPrice: null,
    freshPriceStale: false,
    ...over,
  };
}

/** One acquisition demand edge: `quantity` is the consumer's own shortage;
 * the consuming Build defaults to the occurrence id. */
function edge(
  nodeId: string,
  occurrenceId: string,
  quantity: number,
  over: Partial<AcquisitionConsumerRef> = {},
): AcquisitionConsumerRef {
  return {
    nodeId,
    occurrenceId,
    quantity,
    buildId: occurrenceId,
    dependencyId: `pd:${occurrenceId}`,
    fulfillmentScope: "missing",
    requiredQuantity: quantity,
    plannedInventoryQuantity: 0,
    freshCost: null,
    freshUnitPrice: null,
    ...over,
  };
}

function stage(index: number, nodeIds: string[]): ExecutionStage {
  return { index, nodeIds };
}

function plan(over: Partial<ExecutionPlanProjection> = {}): ExecutionPlanProjection {
  return {
    rootNodeId: "root",
    stages: [],
    nodes: [],
    edges: [],
    occurrences: [],
    acquisitions: [],
    unresolved: [],
    complete: true,
    warnings: [],
    generatedAt: "2026-01-01T00:00:00Z",
    logistics: { destinations: [], totalVolumeM3: "0", volumeComplete: true },
    ...over,
  };
}

async function flush() {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(300);
  });
}

function renderView(over?: Partial<BuildWorksheetEditorModel>) {
  return render(<BuildStagesView active editor={editorStub(over)} />);
}

function rowFor(id: string): HTMLElement {
  const el = document.querySelector<HTMLElement>(`tr[data-row-key="${id}"]`);
  if (!el) throw new Error(`no rendered row for ${id}`);
  return el;
}

function sectionHeadings(): string[] {
  return [...document.querySelectorAll("h3")].map((el) => el.textContent ?? "");
}

/** The inspector's own section body -- scoped to the collapsible section
 * button named `label` (see `InspectorSection`'s own tests for this exact
 * query convention: the button, not an h3/h4 role, carries the section's
 * accessible name). */
function inspectorSection(label: string): HTMLElement {
  // Anchored: "Production" must not also match "Production configuration".
  const pattern = label === "Production" ? "^Production(?!s| configuration)" : `^${label}`;
  const button = screen.getByRole("button", { name: new RegExp(pattern) });
  const section = button.closest("section");
  if (!section) throw new Error(`no section for ${label}`);
  return section;
}

beforeEach(() => {
  vi.useFakeTimers();
  postBuildExecutionPlan.mockReset();
});
afterEach(() => {
  vi.runOnlyPendingTimers();
  vi.useRealTimers();
});

describe("BuildStagesView", () => {
  // A. Buy-only Build
  it("a Buy-only Build shows Inputs to Source and Final Production, no intermediate stage", async () => {
    const root = occurrence({ id: "root", nodeId: "root", isRoot: true, stage: 0, outputTypeName: "Rifter" });
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["root"])],
        nodes: [node({ id: "root", occurrenceIds: ["root"], outputTypeName: "Rifter", stage: 0 })],
        occurrences: [root],
        acquisitions: [acquisition({ typeId: 34, typeName: "Tritanium", shortageQuantity: 400, requiredQuantity: 400 })],
      }),
    );
    renderView();
    await flush();

    expect(screen.getByText("Inputs to Source")).toBeInTheDocument();
    expect(within(rowFor("34")).getByText("Tritanium")).toBeInTheDocument();
    expect(screen.getByText("Final Production")).toBeInTheDocument();
    expect(within(rowFor("root")).getByText("Rifter")).toBeInTheDocument();
    expect(screen.queryByText("Stage 1")).not.toBeInTheDocument();
  });

  // Planning counts free stock; what open Epics hold is noted next to the
  // planned use, never counted in it.
  it("notes stock reserved by Epics next to an input's planned use", async () => {
    const root = occurrence({ id: "root", nodeId: "root", isRoot: true, stage: 0, outputTypeName: "Rifter" });
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["root"])],
        nodes: [node({ id: "root", occurrenceIds: ["root"], outputTypeName: "Rifter", stage: 0 })],
        occurrences: [root],
        acquisitions: [
          acquisition({
            typeId: 34,
            typeName: "Tritanium",
            requiredQuantity: 1_000,
            plannedInventoryQuantity: 30,
            shortageQuantity: 970,
            availableQuantity: 30,
            reservedQuantity: 70,
          }),
          acquisition({ typeId: 35, typeName: "Pyerite", requiredQuantity: 200, shortageQuantity: 200 }),
        ],
      }),
    );
    renderView();
    await flush();

    const tritanium = rowFor("34");
    expect(within(tritanium).getByText("70 reserved")).toBeInTheDocument();
    expect(within(tritanium).getByTitle("30 free · 70 reserved by Epics")).toBeInTheDocument();
    expect(within(rowFor("35")).queryByText(/reserved/)).not.toBeInTheDocument();
  });

  // The final product has no downstream requirement, so the backend reports
  // 0 for it; the row must not read as "produces nothing".
  it("Final Production shows the build's output, not a zero requirement/demand", async () => {
    const root = occurrence({ id: "root", nodeId: "root", isRoot: true, stage: 0, outputTypeName: "Fernite Alloy", projectedOutput: 8000, projectedRuns: 40 });
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["root"])],
        nodes: [node({ id: "root", occurrenceIds: ["root"], outputTypeName: "Fernite Alloy", stage: 0, projectedOutput: 8000, projectedRuns: 40 })],
        occurrences: [root],
      }),
    );
    renderView();
    await flush();

    const row = rowFor("root");
    expect(within(row).getByText("8,000")).toBeInTheDocument();
    expect(within(row).getByText("40")).toBeInTheDocument();
    expect(within(row).queryByText("0")).not.toBeInTheDocument();
  });

  // D. one child Build
  it("one child Build renders Stage 1 (child) and Final Production (root)", async () => {
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["child"]), stage(1, ["root"])],
        nodes: [
          node({ id: "child", occurrenceIds: ["child"], outputTypeName: "Hull Section", stage: 0 }),
          node({ id: "root", occurrenceIds: ["root"], outputTypeName: "Rifter", stage: 1 }),
        ],
        occurrences: [
          occurrence({ id: "child", nodeId: "child", outputTypeName: "Hull Section", stage: 0 }),
          occurrence({ id: "root", nodeId: "root", isRoot: true, outputTypeName: "Rifter", stage: 1 }),
        ],
      }),
    );
    renderView();
    await flush();

    expect(screen.getByText("Stage 1")).toBeInTheDocument();
    expect(within(rowFor("child")).getByText("Hull Section")).toBeInTheDocument();
    expect(screen.getByText("Final Production")).toBeInTheDocument();
    expect(within(rowFor("root")).getByText("Rifter")).toBeInTheDocument();
  });

  // E. three-stage chain -- ascending order top to bottom
  it("a three-level chain renders stages in ascending dependency order", async () => {
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["leaf"]), stage(1, ["mid"]), stage(2, ["root"])],
        nodes: [
          node({ id: "leaf", occurrenceIds: ["leaf"], outputTypeName: "Pyerite", stage: 0 }),
          node({ id: "mid", occurrenceIds: ["mid"], outputTypeName: "Hull Section", stage: 1 }),
          node({ id: "root", occurrenceIds: ["root"], outputTypeName: "Rifter", stage: 2 }),
        ],
        occurrences: [
          occurrence({ id: "leaf", nodeId: "leaf", outputTypeName: "Pyerite", stage: 0 }),
          occurrence({ id: "mid", nodeId: "mid", outputTypeName: "Hull Section", stage: 1 }),
          occurrence({ id: "root", nodeId: "root", isRoot: true, outputTypeName: "Rifter", stage: 2 }),
        ],
      }),
    );
    renderView();
    await flush();

    const headings = sectionHeadings();
    const order = ["Stage 1", "Stage 2", "Final Production"].map((label) =>
      headings.findIndex((text) => text.startsWith(label)),
    );
    expect(order).toEqual([...order].sort((a, b) => a - b));
    expect(order.every((index) => index >= 0)).toBe(true);
  });

  // B. inline expansion is gone -- no chevron, no detail row before a click.
  it("renders no inline expand affordance -- rows are plain, and clicking opens the inspector instead of an inline row", async () => {
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["root"])],
        nodes: [node({ id: "root", occurrenceIds: ["root"], outputTypeName: "Root", stage: 0 })],
        occurrences: [occurrence({ id: "root", nodeId: "root", isRoot: true, outputTypeName: "Root", stage: 0 })],
      }),
    );
    renderView();
    await flush();

    // Only one row for "root" exists before any click -- no sibling detail
    // <tr> injected inline.
    expect(document.querySelectorAll("tr")).toHaveLength(2); // header + the one row
    expect(screen.queryByRole("button", { name: /Economics/ })).not.toBeInTheDocument();

    fireEvent.click(rowFor("root"));
    // Still exactly one row in the table -- the detail rendered elsewhere
    // (the inspector), not as a second <tr>.
    expect(document.querySelectorAll("tr")).toHaveLength(2);
    expect(screen.getByRole("button", { name: /Economics/ })).toBeInTheDocument();
  });

  // H. incompatible same-type nodes remain separate
  it("two same-type nodes with different facilities render as separate rows", async () => {
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["neo-a", "neo-b"]), stage(1, ["root"])],
        nodes: [
          node({
            id: "neo-a",
            occurrenceIds: ["neo-a"],
            outputTypeName: "Neo Mercurite",
            facilityName: "Facility A",
            stage: 0,
          }),
          node({
            id: "neo-b",
            occurrenceIds: ["neo-b"],
            outputTypeName: "Neo Mercurite",
            facilityName: "Facility B",
            stage: 0,
          }),
          node({ id: "root", occurrenceIds: ["root"], outputTypeName: "Muninn", stage: 1 }),
        ],
        occurrences: [
          occurrence({ id: "neo-a", nodeId: "neo-a", outputTypeName: "Neo Mercurite", facilityName: "Facility A" }),
          occurrence({ id: "neo-b", nodeId: "neo-b", outputTypeName: "Neo Mercurite", facilityName: "Facility B" }),
          occurrence({ id: "root", nodeId: "root", isRoot: true, outputTypeName: "Muninn", stage: 1 }),
        ],
      }),
    );
    renderView();
    await flush();

    expect(document.querySelectorAll('tr[data-row-key="neo-a"]')).toHaveLength(1);
    expect(document.querySelectorAll('tr[data-row-key="neo-b"]')).toHaveLength(1);
    expect(within(rowFor("neo-a")).getAllByText("Facility A").length).toBeGreaterThan(0);
    expect(within(rowFor("neo-b")).getAllByText("Facility B").length).toBeGreaterThan(0);
  });

  // I. grouped runs displayed verbatim, never recomputed
  it("grouped runs render exactly the API-supplied sum, labeled as a total", async () => {
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["shared"]), stage(1, ["root"])],
        nodes: [
          node({
            id: "shared",
            occurrenceIds: ["a", "b"],
            outputTypeName: "Shared",
            stage: 0,
            projectedRuns: 144, // 86 + 58, NOT ceil(1000/7)=143
          }),
          node({ id: "root", occurrenceIds: ["root"], outputTypeName: "Root", stage: 1 }),
        ],
        occurrences: [
          occurrence({ id: "a", nodeId: "shared", outputTypeName: "Shared" }),
          occurrence({ id: "b", nodeId: "shared", outputTypeName: "Shared" }),
          occurrence({ id: "root", nodeId: "root", isRoot: true, outputTypeName: "Root", stage: 1 }),
        ],
      }),
    );
    renderView();
    await flush();

    const r = within(rowFor("shared"));
    expect(r.getByText("144")).toBeInTheDocument();
    expect(r.getByText("total")).toBeInTheDocument();
    expect(r.queryByText("143")).not.toBeInTheDocument();
  });

  // J. surplus quantity shown even when surplus cost is incomplete
  it("retained surplus quantity is shown in the inspector even when its cost basis is unknown", async () => {
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["shared"]), stage(1, ["root"])],
        nodes: [
          node({
            id: "shared",
            occurrenceIds: ["a", "b"],
            outputTypeName: "Shared",
            stage: 0,
            retainedSurplusQuantity: 300,
            retainedSurplusCost: null,
            costComplete: false,
          }),
          node({ id: "root", occurrenceIds: ["root"], outputTypeName: "Root", stage: 1 }),
        ],
        occurrences: [
          occurrence({ id: "a", nodeId: "shared", outputTypeName: "Shared" }),
          occurrence({ id: "b", nodeId: "shared", outputTypeName: "Shared" }),
          occurrence({ id: "root", nodeId: "root", isRoot: true, outputTypeName: "Root", stage: 1 }),
        ],
      }),
    );
    renderView();
    await flush();

    fireEvent.click(rowFor("shared"));

    const production = within(inspectorSection("Production"));
    expect(production.getByText("Retained surplus")).toBeInTheDocument();
    // Surplus quantity known (300), surplus basis unknown (--), never "0".
    expect(production.getByText("Retained surplus").closest("div")).toHaveTextContent("300");
    expect(production.getByText("Surplus basis").closest("div")).toHaveTextContent("—");
  });

  // K. fully-covered child absent
  it("a fully-covered pruned child never appears as a stage row", async () => {
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["root"])],
        nodes: [node({ id: "root", occurrenceIds: ["root"], outputTypeName: "Root", stage: 0 })],
        occurrences: [occurrence({ id: "root", nodeId: "root", isRoot: true, outputTypeName: "Root", stage: 0 })],
      }),
    );
    renderView();
    await flush();
    expect(screen.queryByText("Hull Section")).not.toBeInTheDocument();
  });

  // Manufacturing inspector renders ME/TE; Reaction inspector does not
  // fabricate ME/TE (C/D of the acquisition/production inspector list).
  it("production inspector renders ME/TE for manufacturing but never fabricates them for a reaction", async () => {
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["react"]), stage(1, ["root"])],
        nodes: [
          node({
            id: "react",
            occurrenceIds: ["react"],
            outputTypeName: "Neo Mercurite",
            activity: "reaction",
            stage: 0,
            effectiveMe: null,
            effectiveTe: null,
          }),
          node({
            id: "root",
            occurrenceIds: ["root"],
            outputTypeName: "Rifter",
            activity: "manufacturing",
            stage: 1,
            effectiveMe: 10,
            effectiveTe: 20,
          }),
        ],
        occurrences: [
          occurrence({ id: "react", nodeId: "react", outputTypeName: "Neo Mercurite", activity: "reaction" }),
          occurrence({
            id: "root",
            nodeId: "root",
            isRoot: true,
            outputTypeName: "Rifter",
            activity: "manufacturing",
            stage: 1,
          }),
        ],
      }),
    );
    renderView();
    await flush();

    fireEvent.click(rowFor("root"));
    const manufacturingConfig = within(inspectorSection("Production configuration"));
    expect(manufacturingConfig.getByText("ME")).toBeInTheDocument();
    expect(manufacturingConfig.getByText("TE")).toBeInTheDocument();
    expect(manufacturingConfig.getByText("10")).toBeInTheDocument();
    expect(manufacturingConfig.getByText("20")).toBeInTheDocument();

    fireEvent.click(rowFor("react"));
    const reactionConfig = within(inspectorSection("Production configuration"));
    expect(reactionConfig.queryByText("ME")).not.toBeInTheDocument();
    expect(reactionConfig.queryByText("TE")).not.toBeInTheDocument();
  });

  // M. Reaction chain: activity badges match, ascending order
  it("a Reaction -> Reaction -> Manufacturing -> Root chain shows correct activity per row", async () => {
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["neo"]), stage(1, ["nano"]), stage(2, ["component"]), stage(3, ["root"])],
        nodes: [
          node({ id: "neo", occurrenceIds: ["neo"], outputTypeName: "Neo Mercurite", activity: "reaction", stage: 0 }),
          node({ id: "nano", occurrenceIds: ["nano"], outputTypeName: "Nanotransistors", activity: "reaction", stage: 1 }),
          node({ id: "component", occurrenceIds: ["component"], outputTypeName: "Component", activity: "manufacturing", stage: 2 }),
          node({ id: "root", occurrenceIds: ["root"], outputTypeName: "Ship", activity: "manufacturing", stage: 3 }),
        ],
        occurrences: [
          occurrence({ id: "neo", nodeId: "neo", outputTypeName: "Neo Mercurite", activity: "reaction" }),
          occurrence({ id: "nano", nodeId: "nano", outputTypeName: "Nanotransistors", activity: "reaction", stage: 1 }),
          occurrence({ id: "component", nodeId: "component", outputTypeName: "Component", activity: "manufacturing", stage: 2 }),
          occurrence({ id: "root", nodeId: "root", isRoot: true, outputTypeName: "Ship", activity: "manufacturing", stage: 3 }),
        ],
      }),
    );
    renderView();
    await flush();

    expect(within(rowFor("neo")).getByText("REACT")).toBeInTheDocument();
    expect(within(rowFor("nano")).getByText("REACT")).toBeInTheDocument();
    expect(within(rowFor("component")).getByText("MANUFACTURE")).toBeInTheDocument();
    expect(within(rowFor("root")).getByText("MANUFACTURE")).toBeInTheDocument();
  });

  // N. same-stage parallelism -- no implied ordering between peers
  it("two nodes at the same stage render as peers in one section, not sequential sections", async () => {
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["b", "c"]), stage(1, ["root"])],
        nodes: [
          node({ id: "b", occurrenceIds: ["b"], outputTypeName: "B", stage: 0 }),
          node({ id: "c", occurrenceIds: ["c"], outputTypeName: "C", stage: 0 }),
          node({ id: "root", occurrenceIds: ["root"], outputTypeName: "A", stage: 1 }),
        ],
        occurrences: [
          occurrence({ id: "b", nodeId: "b", outputTypeName: "B" }),
          occurrence({ id: "c", nodeId: "c", outputTypeName: "C" }),
          occurrence({ id: "root", nodeId: "root", isRoot: true, outputTypeName: "A", stage: 1 }),
        ],
      }),
    );
    renderView();
    await flush();

    // Exactly one "Stage 1" heading covering both peers.
    expect(screen.getAllByText("Stage 1")).toHaveLength(1);
    const table = rowFor("b").closest("table");
    expect(table).toBe(rowFor("c").closest("table"));
  });

  // O. root final production
  it("the root Build renders under Final Production, never a numbered Stage heading", async () => {
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["root"])],
        nodes: [node({ id: "root", occurrenceIds: ["root"], outputTypeName: "Muninn", stage: 0 })],
        occurrences: [occurrence({ id: "root", nodeId: "root", isRoot: true, outputTypeName: "Muninn", stage: 0 })],
      }),
    );
    renderView();
    await flush();

    expect(screen.getByText("Final Production")).toBeInTheDocument();
    // Root Used By and Required: nothing downstream consumes the final product.
    expect(within(rowFor("root")).getAllByText("—")).toHaveLength(2);
  });

  // P. incomplete cost display
  it("incomplete cost shows 'Incomplete', never 0 ISK, in the Material / Install / Total columns", async () => {
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["root"])],
        nodes: [
          node({
            id: "root",
            occurrenceIds: ["root"],
            outputTypeName: "Root",
            stage: 0,
            costComplete: false,
            totalProductionCost: null,
          }),
        ],
        occurrences: [occurrence({ id: "root", nodeId: "root", isRoot: true, outputTypeName: "Root", stage: 0, costComplete: false })],
      }),
    );
    renderView();
    await flush();

    for (const label of ["Material", "Install", "Total"]) {
      expect(screen.getByRole("columnheader", { name: label })).toBeInTheDocument();
    }
    expect(within(rowFor("root")).getAllByText("Incomplete").length).toBe(3);
    expect(within(rowFor("root")).queryByText(/0(\.00)? ISK/)).not.toBeInTheDocument();
  });

  // Cost remains readable/unclipped when complete -- compact in the cell,
  // exact value available on hover, never a silently-hidden digit.
  it("a complete cost renders compactly in the table with the exact value in the title", async () => {
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["root"])],
        nodes: [
          node({
            id: "root",
            occurrenceIds: ["root"],
            outputTypeName: "Root",
            stage: 0,
            costComplete: true,
            totalProductionCost: "1234567.89",
          }),
        ],
        occurrences: [occurrence({ id: "root", nodeId: "root", isRoot: true, outputTypeName: "Root", stage: 0 })],
      }),
    );
    renderView();
    await flush();

    const costCell = within(rowFor("root")).getByTitle("1,234,567.89 ISK");
    expect(costCell).toBeInTheDocument();
    expect(costCell.textContent).toBe("1.2M"); // compact in the cell -- exact value always on hover
  });

  // Table has no standalone Inventory or Facility column -- a long
  // facility name never crowds the table itself (it only ever appears as
  // an Item subtitle and in the inspector), which is the actual desktop
  // right-side-truncation fix.
  it("a long facility name never widens or truncates the table -- Facility is not a table column", async () => {
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["root"])],
        nodes: [
          node({
            id: "root",
            occurrenceIds: ["root"],
            outputTypeName: "Root",
            stage: 0,
            facilityName: "A Very Long Player-Named Sotiyo Structure In Some Distant Solar System",
          }),
        ],
        occurrences: [occurrence({ id: "root", nodeId: "root", isRoot: true, outputTypeName: "Root", stage: 0 })],
      }),
    );
    renderView();
    await flush();

    expect(screen.queryByRole("columnheader", { name: "Facility" })).not.toBeInTheDocument();
    expect(screen.queryByRole("columnheader", { name: "Inventory" })).not.toBeInTheDocument();
    [
      "Item",
      "Activity",
      "Required",
      "Produce",
      "Runs",
      "Used By",
      "Material",
      "Install",
      "Total",
    ].forEach((label) => {
      expect(screen.getByRole("columnheader", { name: label })).toBeInTheDocument();
    });
  });

  // Q. acquisition shortage section
  it("renders the acquisition shortage section with required/inventory/shortage", async () => {
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["root"])],
        nodes: [node({ id: "root", occurrenceIds: ["root"], outputTypeName: "Root", stage: 0 })],
        occurrences: [occurrence({ id: "root", nodeId: "root", isRoot: true, outputTypeName: "Root", stage: 0 })],
        acquisitions: [
          acquisition({ typeId: 34, typeName: "Tritanium", requiredQuantity: 500, plannedInventoryQuantity: 100, shortageQuantity: 400 }),
        ],
      }),
    );
    renderView();
    await flush();

    const r = within(rowFor("34"));
    expect(r.getByText("500")).toBeInTheDocument();
    expect(r.getByText("100")).toBeInTheDocument();
    expect(r.getByText("400")).toBeInTheDocument();
  });

  // Acquisition row opens the acquisition inspector, with one consumer.
  it("clicking an acquisition row opens the acquisition inspector, showing shortage evidence and its one consumer", async () => {
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["root"])],
        nodes: [node({ id: "root", occurrenceIds: ["root"], outputTypeName: "Ferrogel", stage: 0 })],
        occurrences: [occurrence({ id: "root", nodeId: "root", isRoot: true, outputTypeName: "Ferrogel", stage: 0 })],
        acquisitions: [
          acquisition({
            typeId: 999,
            typeName: "Raw Material X",
            requiredQuantity: 18_000,
            plannedInventoryQuantity: 0,
            shortageQuantity: 18_000,
            availableQuantity: 0,
            consumers: [edge("root", "root", 18_000)],
          }),
        ],
      }),
    );
    renderView();
    await flush();

    fireEvent.click(rowFor("999"));

    const acquisitionSection = within(inspectorSection("Requirement"));
    expect(acquisitionSection.getByText("External shortage")).toBeInTheDocument();
    expect(acquisitionSection.getAllByText("18,000").length).toBeGreaterThan(0);

    const usedBy = within(inspectorSection("Sourcing"));
    expect(usedBy.getByText("For Ferrogel")).toBeInTheDocument();
    expect(acquisitionSection.getByText("Shortage")).toBeInTheDocument();
  });

  // Acquisition Used By with multiple consumers -- the worked example from
  // real Muninn usage (Neo Mercurite Inputs/Raw Material X).
  it("acquisition Used By preserves every consumer's own authoritative quantity, reconciling with the shortage total", async () => {
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["ferrogel"]), stage(1, ["phenolic"]), stage(2, ["other"])],
        nodes: [
          node({ id: "ferrogel", occurrenceIds: ["ferrogel"], outputTypeName: "Ferrogel", stage: 0 }),
          node({ id: "phenolic", occurrenceIds: ["phenolic"], outputTypeName: "Phenolic Composites", stage: 1 }),
          node({ id: "other", occurrenceIds: ["other"], outputTypeName: "Some Other Reaction", stage: 2 }),
        ],
        occurrences: [
          occurrence({ id: "ferrogel", nodeId: "ferrogel", outputTypeName: "Ferrogel" }),
          occurrence({ id: "phenolic", nodeId: "phenolic", outputTypeName: "Phenolic Composites" }),
          occurrence({ id: "other", nodeId: "other", isRoot: true, outputTypeName: "Some Other Reaction" }),
        ],
        acquisitions: [
          acquisition({
            typeId: 999,
            typeName: "Raw Material X",
            requiredQuantity: 18_000,
            shortageQuantity: 18_000,
            consumers: [
              edge("ferrogel", "ferrogel", 8_000),
              edge("phenolic", "phenolic", 6_000),
              edge("other", "other", 4_000),
            ],
          }),
        ],
      }),
    );
    renderView();
    await flush();

    fireEvent.click(rowFor("999"));
    // Sourcing: one switch card per demand edge; Requirement: each edge's
    // own authoritative quantities.
    const usedBy = within(inspectorSection("Sourcing"));
    const requirement = within(inspectorSection("Requirement"));
    for (const name of ["For Ferrogel", "For Phenolic Composites", "For Some Other Reaction"]) {
      expect(usedBy.getByText(name)).toBeInTheDocument();
      expect(requirement.getByText(name)).toBeInTheDocument();
    }
    expect(requirement.getAllByText("8,000").length).toBeGreaterThan(0);
    expect(requirement.getAllByText("6,000").length).toBeGreaterThan(0);
    expect(requirement.getAllByText("4,000").length).toBeGreaterThan(0);
    // 8,000 + 6,000 + 4,000 == 18,000, matching the line's own shortage.
  });

  // R. no zero-shortage acquisitions invented client-side
  it("does not render an Inputs to Source section when the backend sends none", async () => {
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["root"])],
        nodes: [node({ id: "root", occurrenceIds: ["root"], outputTypeName: "Root", stage: 0 })],
        occurrences: [occurrence({ id: "root", nodeId: "root", isRoot: true, outputTypeName: "Root", stage: 0 })],
        acquisitions: [],
      }),
    );
    renderView();
    await flush();
    expect(screen.queryByText("Inputs to Source")).not.toBeInTheDocument();
  });

  // S. warnings render safely without obscuring the rest of the view
  it("renders warnings in a dismissible-detail banner without hiding the stage sections", async () => {
    const warnings: CostWarning[] = [
      { code: "noFacilitySelected", opIndex: 0 },
      { code: "missingFreshPrice", opIndex: 1, traversalIndex: 0, typeId: 34 },
    ];
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["root"])],
        nodes: [node({ id: "root", occurrenceIds: ["root"], outputTypeName: "Root", stage: 0 })],
        occurrences: [occurrence({ id: "root", nodeId: "root", isRoot: true, outputTypeName: "Root", stage: 0 })],
        warnings,
        complete: false,
      }),
    );
    renderView();
    await flush();

    expect(screen.getByText("2 cost warnings")).toBeInTheDocument();
    expect(screen.getByText("Final Production")).toBeInTheDocument();
    expect(within(rowFor("root")).getByText("Root")).toBeInTheDocument();

    fireEvent.click(screen.getByText("Show details"));
    expect(screen.getByText(/no facility selected/i)).toBeInTheDocument();
    expect(screen.getByText(/no fresh market\/manual price/i)).toBeInTheDocument();
  });

  it("does not fetch while inactive", async () => {
    postBuildExecutionPlan.mockResolvedValue(plan());
    render(<BuildStagesView active={false} editor={editorStub()} />);
    await flush();
    expect(postBuildExecutionPlan).not.toHaveBeenCalled();
  });

  it("shows a calculating state before the first response", async () => {
    postBuildExecutionPlan.mockReturnValue(new Promise(() => {}));
    renderView();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(250);
    });
    expect(screen.getByText("Calculating plan...")).toBeInTheDocument();
  });

  // Selection lifecycle -- survives a refresh when the node still exists,
  // refreshing its displayed evidence from the new response.
  it("preserves the inspector selection across a plan refresh when the selected node still exists, refreshing its content", async () => {
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["root"])],
        nodes: [node({ id: "root", occurrenceIds: ["root"], outputTypeName: "Root", stage: 0, requiredQuantity: 100 })],
        occurrences: [occurrence({ id: "root", nodeId: "root", isRoot: true, outputTypeName: "Root", stage: 0 })],
      }),
    );
    const { rerender } = renderView();
    await flush();

    fireEvent.click(rowFor("root"));
    expect(within(inspectorSection("Production")).getByText("100")).toBeInTheDocument();

    // A new overlay (e.g. an unsaved runs edit) triggers a refetch that
    // returns updated evidence for the SAME node id.
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["root"])],
        nodes: [node({ id: "root", occurrenceIds: ["root"], outputTypeName: "Root", stage: 0, requiredQuantity: 500 })],
        occurrences: [occurrence({ id: "root", nodeId: "root", isRoot: true, outputTypeName: "Root", stage: 0 })],
      }),
    );
    rerender(<BuildStagesView active editor={editorStub({ previewKey: JSON.stringify({ ...OVERLAY, runs: 5 }) })} />);
    await flush();

    // Inspector is still open, for the same node, showing the REFRESHED value.
    expect(within(inspectorSection("Production")).getByText("500")).toBeInTheDocument();
    expect(within(inspectorSection("Production")).queryByText("100")).not.toBeInTheDocument();
  });

  // Selection lifecycle -- closes gracefully, never leaving stale data, when
  // the selected node disappears from a freshly returned plan.
  it("closes the inspector gracefully when the selected node disappears from a refreshed plan", async () => {
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["child"]), stage(1, ["root"])],
        nodes: [
          node({ id: "child", occurrenceIds: ["child"], outputTypeName: "Hull Section", stage: 0 }),
          node({ id: "root", occurrenceIds: ["root"], outputTypeName: "Rifter", stage: 1 }),
        ],
        occurrences: [
          occurrence({ id: "child", nodeId: "child", outputTypeName: "Hull Section", stage: 0 }),
          occurrence({ id: "root", nodeId: "root", isRoot: true, outputTypeName: "Rifter", stage: 1 }),
        ],
      }),
    );
    const { rerender } = renderView();
    await flush();

    fireEvent.click(rowFor("child"));
    expect(screen.getByRole("button", { name: /Economics/ })).toBeInTheDocument();

    // The child is now fully inventory-covered and pruned from the plan.
    postBuildExecutionPlan.mockResolvedValue(
      plan({
        stages: [stage(0, ["root"])],
        nodes: [node({ id: "root", occurrenceIds: ["root"], outputTypeName: "Rifter", stage: 0 })],
        occurrences: [occurrence({ id: "root", nodeId: "root", isRoot: true, outputTypeName: "Rifter", stage: 0 })],
      }),
    );
    rerender(<BuildStagesView active editor={editorStub({ previewKey: JSON.stringify({ ...OVERLAY, runs: 99 }) })} />);
    await flush();

    expect(screen.queryByRole("button", { name: /Used By/ })).not.toBeInTheDocument();
  });
});
