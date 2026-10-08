// The Plan inspector's actions -- per-edge sourcing
// changes through the existing write paths, the direct Create ticket, and
// the stage-order display.

import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type {
  ExecutionConsumerRef,
  AcquisitionConsumerRef,
  AcquisitionLine,
  ExecutionNode,
  ExecutionOccurrence,
  ExecutionPlanProjection,
  ExecutionRequirement,
} from "../../../../../api/industry";
import type { BuildWorksheetEditorModel } from "../../use-build-worksheet-editor";
import { BuildStagesView } from "../build-stages-view";

const api = vi.hoisted(() => ({
  postBuildExecutionPlan: vi.fn(),
  getBuild: vi.fn(),
  setComponentResolution: vi.fn(),
  clearComponentResolution: vi.fn(),
  createLinkedBuild: vi.fn(),
  createTicket: vi.fn(),
}));
vi.mock("../../../../../api/industry", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  ...api,
}));

const OVERLAY = { recipe: { mode: "manufacturing", blueprintTypeId: 81_043 }, runs: 1, componentResolutions: [] };
const RCF_FORMULA = { mode: "reaction" as const, reactionFormulaTypeId: 57_497 };
const HYDRO_METHOD = { mode: "reaction" as const, reactionFormulaTypeId: 99_001 };


/** A production consumer edge: `quantity` is its production demand. */
function uses(nodeId: string, occurrenceId: string, quantity: number): ExecutionConsumerRef {
  return {
    nodeId,
    occurrenceId,
    quantity,
    buildId: `build-${nodeId}`,
    dependencyId: `pd:${nodeId}`,
    fulfillmentScope: "missing",
    requiredQuantity: quantity,
    plannedInventoryQuantity: 0,
  };
}

function occurrence(id: string, nodeId: string, over: Partial<ExecutionOccurrence> = {}): ExecutionOccurrence {
  return {
    id,
    nodeId,
    buildId: `build-${nodeId}`,
    revision: 3,
    blueprintSelection: null,
    isRoot: false,
    stage: 0,
    activity: "manufacturing",
    outputTypeId: 1,
    outputTypeName: nodeId,
    blueprintOrFormulaTypeId: 1,
    blueprintOrFormulaName: `${nodeId} Blueprint`,
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

function requirement(over: Partial<ExecutionRequirement> = {}): ExecutionRequirement {
  return {
    typeId: 57_457,
    typeName: "Reinforced Carbon Fiber",
    requiredQuantity: 1,
    plannedInventoryQuantity: 0,
    shortageQuantity: 1,
    fulfillmentScope: "missing",
    resolution: "reaction",
    dependencyId: "pd:rcf",
    producerBuildId: "build-rcf",
    producerNodeId: "rcf",
    ...over,
  };
}

function node(id: string, stage: number, over: Partial<ExecutionNode> = {}): ExecutionNode {
  return {
    id,
    outputTypeId: 1,
    outputTypeName: id,
    activity: "manufacturing",
    stage,
    occurrenceIds: [id],
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

function edge(nodeId: string, quantity: number, buildId: string): AcquisitionConsumerRef {
  return {
    nodeId,
    occurrenceId: nodeId,
    quantity,
    buildId,
    dependencyId: `pd:${nodeId}`,
    fulfillmentScope: "missing",
    requiredQuantity: quantity,
    plannedInventoryQuantity: 0,
    freshCost: null,
    freshUnitPrice: null,
  };
}

/** Squall-shaped canonical plan: Reinforced Carbon Fiber (reaction, stage
 * index 1) feeds Life Support Backup Unit and Auto-Integrity Preservation
 * Seal (stage index 2), which feed the Squall (final). Hydrocarbons are
 * bought by RCF (nested) and Tritanium by the Squall (root). */
function squallPlan(): ExecutionPlanProjection {
  const rcf = node("rcf", 1, {
    outputTypeId: 57_457,
    outputTypeName: "Reinforced Carbon Fiber",
    activity: "reaction",
    projectedRuns: 6,
    productionDemand: 161,
    productionMethods: [RCF_FORMULA],
    consumers: [
      uses("lsbu", "lsbu", 200),
      uses("aips", "aips", 400),
    ],
  });
  const cf = node("cf", 0, { outputTypeName: "Carbon Fiber", activity: "reaction" });
  const lsbu = node("lsbu", 2, {
    outputTypeName: "Life Support Backup Unit",
    productionMethods: [{ mode: "manufacturing", blueprintTypeId: 57_523 }],
    consumers: [{ ...uses("squall", "squall", 20), buildId: "build-root" }],
  });
  const aips = node("aips", 2, { outputTypeName: "Auto-Integrity Preservation Seal" });
  const squall = node("squall", 3, { outputTypeName: "Squall" });
  return {
    rootNodeId: "squall",
    stages: [
      { index: 0, nodeIds: ["cf"] },
      { index: 1, nodeIds: ["rcf"] },
      { index: 2, nodeIds: ["aips", "lsbu"] },
      { index: 3, nodeIds: ["squall"] },
    ],
    nodes: [cf, rcf, aips, lsbu, squall],
    edges: [],
    occurrences: [
      occurrence("cf", "cf", { activity: "reaction" }),
      occurrence("rcf", "rcf", {
        activity: "reaction",
        blueprintOrFormulaTypeId: 57_497,
        requirements: [
          requirement({
            typeId: 20_000,
            typeName: "Carbon Fiber",
            requiredQuantity: 200,
            shortageQuantity: 200,
            producerBuildId: "build-cf",
            producerNodeId: "cf",
          }),
        ],
      }),
      occurrence("lsbu", "lsbu", {
        requirements: [requirement({ requiredQuantity: 54, shortageQuantity: 54 })],
      }),
      occurrence("aips", "aips", {
        projectedRuns: 12,
        requirements: [requirement({ requiredQuantity: 107, shortageQuantity: 107 })],
      }),
      occurrence("squall", "squall", {
        isRoot: true,
        buildId: "build-root",
        requirements: [
          requirement({
            typeId: 57_524,
            typeName: "Life Support Backup Unit",
            requiredQuantity: 20,
            shortageQuantity: 20,
            resolution: "build",
            dependencyId: "pd:lsbu",
            producerBuildId: "build-lsbu",
            producerNodeId: "lsbu",
          }),
          requirement({
            typeId: 34,
            typeName: "Tritanium",
            requiredQuantity: 540_000,
            shortageQuantity: 540_000,
            resolution: "buy",
            dependencyId: "pd:tritanium",
            producerBuildId: null,
            producerNodeId: null,
          }),
        ],
      }),
    ],
    acquisitions: [
      acquisition(16_633, "Hydrocarbons", [edge("rcf", 1_200, "build-rcf")], [HYDRO_METHOD]),
      acquisition(34, "Tritanium", [edge("squall", 540_000, "build-root")], []),
    ],
    unresolved: [],
    complete: true,
    warnings: [],
    generatedAt: "2026-01-01T00:00:00Z",
    logistics: { destinations: [], totalVolumeM3: "0", volumeComplete: true },
  };
}

function acquisition(
  typeId: number,
  typeName: string,
  consumers: AcquisitionConsumerRef[],
  productionMethods: AcquisitionLine["productionMethods"],
): AcquisitionLine {
  const total = consumers.reduce((sum, consumer) => sum + consumer.quantity, 0);
  return {
    typeId,
    typeName,
    requiredQuantity: total,
    plannedInventoryQuantity: 0,
    shortageQuantity: total,
    availableQuantity: 0,
    sourceStrategy: "buy",
    consumers,
    productionMethods,
    freshCost: null,
    freshUnitPrice: null,
    freshPriceStale: false,
  };
}

const setComponentResolutions = vi.fn();
const bumpPreview = vi.fn();
const openBuildSettings = vi.fn();
const closeInspector = vi.fn();

function renderPlan() {
  const editor = {
    initialBuild: { id: "build-root" },
    previewKey: JSON.stringify(OVERLAY),
    linkedBuildsByTypeId: {},
    setComponentResolutions,
    bumpPreview,
    openBuildSettings,
    closeInspector,
    inspectorMode: { kind: "closed" },
  } as unknown as BuildWorksheetEditorModel;
  return render(
    <MemoryRouter>
      <BuildStagesView active editor={editor} />
    </MemoryRouter>,
  );
}

async function flush() {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(300);
  });
}

function rowFor(id: string): HTMLElement {
  const row = document.querySelector<HTMLElement>(`tr[data-row-key="${id}"]`);
  if (!row) throw new Error(`no row ${id}`);
  return row;
}

beforeEach(() => {
  vi.useFakeTimers();
  for (const fn of Object.values(api)) fn.mockReset();
  setComponentResolutions.mockReset();
  bumpPreview.mockReset();
  openBuildSettings.mockReset();
  closeInspector.mockReset();
  api.getBuild.mockImplementation(async (id: string) => ({ id, revision: 7 }));
  api.setComponentResolution.mockResolvedValue({});
  api.clearComponentResolution.mockResolvedValue({});
  api.createLinkedBuild.mockResolvedValue({});
});
afterEach(() => vi.useRealTimers());

describe("Plan stage order display", () => {
  it("renders every producer in an earlier section than its consumers (Squall)", async () => {
    api.postBuildExecutionPlan.mockResolvedValue(squallPlan());
    renderPlan();
    await flush();

    const headings = screen
      .getAllByRole("heading", { level: 3 })
      .map((heading) => heading.textContent ?? "");
    const index = (prefix: string) => headings.findIndex((text) => text.startsWith(prefix));
    expect(index("Inputs to Source")).toBe(0);
    expect(index("Stage 2")).toBeLessThan(index("Stage 3"));
    expect(index("Stage 3")).toBeLessThan(index("Final Production"));

    const stage2 = screen.getByRole("table", { name: "Stage 2" });
    const stage3 = screen.getByRole("table", { name: "Stage 3" });
    expect(within(stage2).getByText("Reinforced Carbon Fiber")).toBeInTheDocument();
    expect(within(stage3).getByText("Life Support Backup Unit")).toBeInTheDocument();
    expect(within(stage3).getByText("Auto-Integrity Preservation Seal")).toBeInTheDocument();
    expect(within(stage3).queryByText("Reinforced Carbon Fiber")).toBeNull();
  });
});

describe("Plan operation requirements", () => {
  it("keeps each consumer's RCF requirement distinct and navigates to the shared producer", async () => {
    api.postBuildExecutionPlan.mockResolvedValue(squallPlan());
    renderPlan();
    await flush();

    fireEvent.click(rowFor("aips"));
    let section = screen.getByRole("button", { name: /Requirements/ }).closest("section")!;
    expect(within(section).getAllByText("107")).toHaveLength(2);
    expect(within(section).getByText(/Total planned 161/)).toBeInTheDocument();
    fireEvent.click(within(section).getByRole("button", { name: /Reinforced Carbon Fiber/ }));
    expect(screen.getByRole("heading", { name: "Reinforced Carbon Fiber" })).toBeInTheDocument();
    const production = screen.getByRole("button", { name: /Production$/ }).closest("section")!;
    expect(within(production).getByText("161")).toBeInTheDocument();

    fireEvent.click(rowFor("lsbu"));
    section = screen.getByRole("button", { name: /Requirements/ }).closest("section")!;
    expect(within(section).getAllByText("54")).toHaveLength(2);
    expect(within(section).queryByText("107")).not.toBeInTheDocument();
  });

  it("navigates bought inputs and renders reaction and root direct requirements", async () => {
    api.postBuildExecutionPlan.mockResolvedValue(squallPlan());
    renderPlan();
    await flush();

    fireEvent.click(rowFor("rcf"));
    let section = screen.getByRole("button", { name: /Requirements/ }).closest("section")!;
    expect(within(section).getByText("Carbon Fiber")).toBeInTheDocument();
    expect(within(section).queryByText("Auto-Integrity Preservation Seal")).not.toBeInTheDocument();

    fireEvent.click(rowFor("squall"));
    section = screen.getByRole("button", { name: /Requirements/ }).closest("section")!;
    expect(within(section).getByText("Life Support Backup Unit")).toBeInTheDocument();
    fireEvent.click(within(section).getByRole("button", { name: /Tritanium/ }));
    expect(screen.getByRole("heading", { name: "Tritanium" })).toBeInTheDocument();
  });

});

describe("Plan inspector sourcing", () => {
  function sourcingFor(consumer: string) {
    return within(screen.getByRole("radiogroup", { name: `Sourcing for ${consumer}` }));
  }

  it("an input bought by a descendant shows Buy / Reaction and switches only that edge, then re-plans", async () => {
    api.postBuildExecutionPlan.mockResolvedValue(squallPlan());
    renderPlan();
    await flush();
    expect(api.postBuildExecutionPlan).toHaveBeenCalledTimes(1);

    fireEvent.click(rowFor("16633"));
    // Sourcing is the inspector's first section.
    expect(screen.getAllByRole("button", { name: /^(Sourcing|Acquisition)/ })[0]).toHaveTextContent(
      "Sourcing",
    );
    const sourcing = sourcingFor("Reinforced Carbon Fiber");
    expect(sourcing.getAllByRole("radio").map((radio) => radio.textContent)).toEqual(["Buy", "Reaction"]);
    expect(sourcing.getByRole("radio", { name: "Buy" })).toHaveAttribute("aria-checked", "true");

    fireEvent.click(sourcing.getByRole("radio", { name: "Reaction" }));
    await flush(); // the write chain
    await flush(); // the debounced re-projection

    expect(api.getBuild).toHaveBeenCalledWith("build-rcf");
    expect(api.setComponentResolution).toHaveBeenCalledWith("build-rcf", {
      componentTypeId: 16_633,
      recipe: HYDRO_METHOD,
      expectedRevision: 7,
    });
    expect(api.createLinkedBuild).toHaveBeenCalledWith("build-rcf", { componentTypeId: 16_633 });
    expect(setComponentResolutions).not.toHaveBeenCalled();
    // Every projection re-plans through the editor's preview key -- the
    // persistent Build economics, Plan, Logistics and Graph together.
    expect(bumpPreview).toHaveBeenCalledTimes(1);
  });

  it("an input the root consumes offers Buy / Build and changes through the root editor overlay", async () => {
    const plan = squallPlan();
    plan.acquisitions[1].productionMethods = [{ mode: "manufacturing", blueprintTypeId: 1234 }];
    api.postBuildExecutionPlan.mockResolvedValue(plan);
    renderPlan();
    await flush();

    fireEvent.click(rowFor("34"));
    fireEvent.click(sourcingFor("Squall").getByRole("radio", { name: "Build" }));
    await flush();

    expect(api.setComponentResolution).not.toHaveBeenCalled();
    expect(setComponentResolutions).toHaveBeenCalledTimes(1);
    const updater = setComponentResolutions.mock.calls[0][0] as (current: object) => object;
    expect(updater({})).toEqual({ 34: { recipe: { mode: "manufacturing", blueprintTypeId: 1234 } } });
  });

  it("a Buy-only input explains why production is unavailable", async () => {
    api.postBuildExecutionPlan.mockResolvedValue(squallPlan());
    renderPlan();
    await flush();

    fireEvent.click(rowFor("34"));
    expect(screen.getByText(/Buy only -- no published blueprint or reaction formula produces Tritanium/)).toBeInTheDocument();
    expect(sourcingFor("Squall").getByRole("radio", { name: "Buy" })).toBeDisabled();
  });

  it("a shared producer shows one Used by / Sourcing card per consumer; one consumer -> Buy leaves the other", async () => {
    api.postBuildExecutionPlan.mockResolvedValue(squallPlan());
    renderPlan();
    await flush();

    fireEvent.click(rowFor("rcf"));
    const lsbu = sourcingFor("Life Support Backup Unit");
    const aips = sourcingFor("Auto-Integrity Preservation Seal");
    expect(lsbu.getByRole("radio", { name: "Reaction" })).toHaveAttribute("aria-checked", "true");
    expect(aips.getByRole("radio", { name: "Reaction" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByText(/changing one leaves the others on this operation/)).toBeInTheDocument();

    fireEvent.click(lsbu.getByRole("radio", { name: "Buy" }));
    await flush(); // the write chain
    await flush(); // the debounced re-projection

    expect(api.clearComponentResolution).toHaveBeenCalledTimes(1);
    expect(api.clearComponentResolution).toHaveBeenCalledWith("build-lsbu", 57_457, 7);
    expect(api.setComponentResolution).not.toHaveBeenCalled();
    expect(bumpPreview).toHaveBeenCalledTimes(1);
  });

  it("a single-consumer producer shows its one sourcing switch at the top", async () => {
    api.postBuildExecutionPlan.mockResolvedValue(squallPlan());
    renderPlan();
    await flush();

    fireEvent.click(rowFor("lsbu"));
    // LSBU is consumed only by the Squall root: one card, one switch, no
    // multi-consumer note.
    expect(screen.getByText(/One consumer uses this manufacturing operation/)).toBeInTheDocument();
    expect(screen.getAllByRole("radiogroup")).toHaveLength(1);
  });

  it("the root has no Buy / Build switch -- it links to Build settings instead", async () => {
    api.postBuildExecutionPlan.mockResolvedValue(squallPlan());
    renderPlan();
    await flush();

    fireEvent.click(rowFor("squall"));
    expect(screen.queryByRole("radiogroup")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Edit build settings" }));
    expect(openBuildSettings).toHaveBeenCalledTimes(1);
  });

  it("Produce all is an explicit multi-consumer action; each switch changes only its own edge", async () => {
    const plan = squallPlan();
    // Phenolic-shaped: two consumers buy the same item.
    plan.acquisitions[0] = acquisition(
      16_633,
      "Hydrocarbons",
      [edge("rcf", 180, "build-rcf"), edge("cf", 6_448, "build-cf")],
      [HYDRO_METHOD],
    );
    api.postBuildExecutionPlan.mockResolvedValue(plan);
    renderPlan();
    await flush();

    fireEvent.click(rowFor("16633"));
    fireEvent.click(sourcingFor("Reinforced Carbon Fiber").getByRole("radio", { name: "Reaction" }));
    await flush();
    await flush();
    expect(api.setComponentResolution).toHaveBeenCalledTimes(1);
    expect(api.setComponentResolution).toHaveBeenCalledWith("build-rcf", expect.anything());

    api.setComponentResolution.mockClear();
    const produceAll = screen.getByRole("button", { name: "Produce all by Reaction" });
    expect(produceAll.parentElement?.parentElement).toHaveTextContent("Total to buy6,628");
    fireEvent.click(produceAll);
    await flush();
    await flush();
    await flush();
    expect(api.setComponentResolution.mock.calls.map((call) => call[0])).toEqual(["build-rcf", "build-cf"]);
  });

  it("an item both bought and produced shows the split in both inspectors", async () => {
    const plan = squallPlan();
    const producer = node("hydro", 0, {
      outputTypeId: 16_633,
      outputTypeName: "Hydrocarbons",
      activity: "reaction",
      requiredQuantity: 180,
      productionDemand: 180,
      consumers: [uses("rcf", "rcf", 180)],
    });
    plan.nodes.push(producer);
    plan.occurrences.push(occurrence("hydro", "hydro", { activity: "reaction", outputTypeId: 16_633 }));
    plan.stages[0].nodeIds.push("hydro");
    plan.acquisitions[0] = {
      ...acquisition(16_633, "Hydrocarbons", [edge("cf", 6_448, "build-cf")], [HYDRO_METHOD]),
      sourceStrategy: "mixed",
    };
    api.postBuildExecutionPlan.mockResolvedValue(plan);
    renderPlan();
    await flush();

    fireEvent.click(rowFor("16633"));
    const split = within(screen.getByRole("group", { name: "Sourcing split" }));
    expect(split.getByText("6,628")).toBeInTheDocument();
    expect(split.getByText("180")).toBeInTheDocument();
    expect(split.getByText("6,448")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Close input inspector" }));
    fireEvent.click(rowFor("hydro"));
    const producerSplit = within(screen.getByRole("group", { name: "Sourcing split" }));
    expect(producerSplit.getByText("6,628")).toBeInTheDocument();
    expect(producerSplit.getByText(/bought for Carbon Fiber/)).toBeInTheDocument();
  });
});

describe("Plan economics", () => {
  it("rows show Material / Install / Total and the inspector the full operation economics", async () => {
    const plan = squallPlan();
    const rcf = plan.nodes.find((node) => node.id === "rcf")!;
    Object.assign(rcf, {
      materialComponentCost: "700000.0000",
      ownInstallationCost: "20000.0000",
      totalProductionCost: "720000.0000",
      unitProductionCost: "3600.0000",
      retainedSurplusQuantity: 1,
      retainedSurplusCost: "3600.0000",
      projectedOutput: 200,
    });
    api.postBuildExecutionPlan.mockResolvedValue(plan);
    renderPlan();
    await flush();

    const row = within(rowFor("rcf"));
    expect(row.getByText("700K")).toBeInTheDocument();
    expect(row.getByText("20K")).toBeInTheDocument();
    expect(row.getByText("720K")).toBeInTheDocument();

    fireEvent.click(rowFor("rcf"));
    const economics = screen.getByRole("button", { name: /Economics/ }).closest("section")!;
    for (const label of [
      "Material/component",
      "Own installation",
      "Total production",
      "Unit production cost",
      "Retained surplus basis",
    ]) {
      expect(within(economics).getByText(label)).toBeInTheDocument();
    }
  });

  it("inputs show unit price and estimated cost, and 'Unpriced' rather than a fabricated 0", async () => {
    const plan = squallPlan();
    Object.assign(plan.acquisitions[0], { freshCost: "4500.0000", freshUnitPrice: "3.7500" });
    api.postBuildExecutionPlan.mockResolvedValue(plan);
    renderPlan();
    await flush();

    expect(screen.getByRole("columnheader", { name: "Unit Price" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Est. Cost" })).toBeInTheDocument();
    expect(within(rowFor("16633")).getByText("4.5K")).toBeInTheDocument();
    expect(within(rowFor("34")).getByText("Unpriced")).toBeInTheDocument();
  });
});

describe("Plan direct ticket", () => {
  it("creates ONE standalone ticket for a shared operation at its projected runs", async () => {
    api.postBuildExecutionPlan.mockResolvedValue(squallPlan());
    api.createTicket.mockResolvedValue({ id: "ticket-1", displayId: "ISK-1001" });
    renderPlan();
    await flush();

    fireEvent.click(rowFor("rcf"));
    fireEvent.click(screen.getByRole("button", { name: "Create ticket" }));
    await flush();

    expect(api.createTicket).toHaveBeenCalledTimes(1);
    expect(api.createTicket).toHaveBeenCalledWith({ kind: "reaction", buildId: "build-rcf", runs: 6 });
    expect(screen.getByRole("link", { name: "ISK-1001" })).toHaveAttribute(
      "href",
      "/board?ticket=ticket-1",
    );
  });

  it("does not show one operation's created ticket after selecting another operation", async () => {
    api.postBuildExecutionPlan.mockResolvedValue(squallPlan());
    api.createTicket.mockResolvedValue({ id: "ticket-1", displayId: "ISK-1001" });
    renderPlan();
    await flush();

    fireEvent.click(rowFor("rcf"));
    fireEvent.click(screen.getByRole("button", { name: "Create ticket" }));
    await flush();
    expect(screen.getByRole("link", { name: "ISK-1001" })).toBeInTheDocument();

    fireEvent.click(rowFor("cf"));

    expect(screen.queryByRole("link", { name: "ISK-1001" })).not.toBeInTheDocument();
  });
});
