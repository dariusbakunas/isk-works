import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type {
  AcquisitionLine,
  EpicExecutionPlan,
  ExecutionNode,
  ExecutionOccurrence,
  OrderDetail,
} from "../../../../../api/industry";
import type { BuildWorksheetEditorModel } from "../../use-build-worksheet-editor";
import { BuildStagesView } from "../build-stages-view";

// About section contents: sections start expanded here (see the double).
vi.mock("../../../inspector/inspector-collapse", async () => {
  const { expandedInspectorCollapse } = await import("../../../inspector/__tests__/expanded-inspector-collapse");
  return expandedInspectorCollapse();
});


const postBuildExecutionPlan = vi.fn();
const getOrderExecutionPlan = vi.fn();
const getOrder = vi.fn();
const bulkCreateTickets = vi.fn();
const createOperationTicket = vi.fn();
vi.mock("../../../../../api/industry", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  postBuildExecutionPlan: (...args: unknown[]) => postBuildExecutionPlan(...args),
  getOrderExecutionPlan: (...args: unknown[]) => getOrderExecutionPlan(...args),
  getOrder: (...args: unknown[]) => getOrder(...args),
  bulkCreateTickets: (...args: unknown[]) => bulkCreateTickets(...args),
  createOperationTicket: (...args: unknown[]) => createOperationTicket(...args),
}));

function editorStub(revision = 4): BuildWorksheetEditorModel {
  return {
    initialBuild: { id: "build-1", revision },
    previewKey: JSON.stringify({ runs: 1 }),
    linkedBuildsByTypeId: {},
  } as unknown as BuildWorksheetEditorModel;
}

function node(id: string, name: string, stage: number, over: Partial<ExecutionNode> = {}): ExecutionNode {
  return {
    id,
    outputTypeId: 1,
    outputTypeName: name,
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
    projectedOutput: 1,
    projectedRuns: 1,
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
  } as ExecutionNode;
}

function occurrence(id: string, isRoot: boolean, stage: number): ExecutionOccurrence {
  return { id, nodeId: id, isRoot, stage, requirements: [] } as unknown as ExecutionOccurrence;
}

const TRITANIUM: AcquisitionLine = {
  typeId: 34,
  typeName: "Tritanium",
  requiredQuantity: 1_000,
  plannedInventoryQuantity: 600,
  shortageQuantity: 400,
  availableQuantity: 0,
  reservedQuantity: 0,
  sourceStrategy: "buy",
  consumers: [],
  productionMethods: [],
  freshCost: null,
  freshUnitPrice: null,
  freshPriceStale: false,
};

function epicPlan(over: Partial<EpicExecutionPlan["epic"]> = {}): EpicExecutionPlan {
  return {
    plan: {
      rootNodeId: "root",
      stages: [
        { index: 0, nodeIds: ["reaction"] },
        { index: 1, nodeIds: ["root"] },
      ],
      nodes: [node("reaction", "Fernite Carbide", 0), node("root", "Muninn", 1)],
      edges: [],
      occurrences: [occurrence("reaction", false, 0), occurrence("root", true, 1)],
      acquisitions: [TRITANIUM],
      unresolved: [],
      complete: true,
      warnings: [],
      generatedAt: "2026-10-08T10:00:00Z",
      logistics: { destinations: [], totalVolumeM3: "0", volumeComplete: true },
    },
    epic: {
      orderId: "epic-1",
      displayName: "Manufacture Muninn",
      sourceBuildRevision: 4,
      nodes: {
        reaction: {
          ticketId: "t-1",
          ticketDisplayId: "T-7",
          ticketStatus: "complete",
          output: { reserved: 0, consumed: 0, remainingNeed: 0 },
        },
        root: {
          ticketId: "t-2",
          ticketDisplayId: "T-8",
          ticketStatus: "todo",
          output: { reserved: 0, consumed: 0, remainingNeed: 0 },
        },
      },
      acquisitions: { "34": { reserved: 600, consumed: 0, remainingNeed: 400 } },
      ...over,
    },
  };
}

const DETAIL = {
  id: "epic-1",
  requirements: [
    { id: "req-trit", typeId: 34, kind: "buy", state: "needsAction" },
    { id: "req-fernite", typeId: 16673, kind: "build", state: "needsAction" },
  ],
} as unknown as OrderDetail;

function rowFor(id: string): HTMLElement {
  const el = document.querySelector<HTMLElement>(`tr[data-row-key="${id}"]`);
  if (!el) throw new Error(`no rendered row for ${id}`);
  return el;
}

describe("BuildStagesView with an Epic selected", () => {
  beforeEach(() => {
    getOrderExecutionPlan.mockResolvedValue(epicPlan());
    getOrder.mockResolvedValue(DETAIL);
  });
  afterEach(() => {
    vi.clearAllMocks();
  });

  it("renders the Epic's frozen plan in the Plan's own layout", async () => {
    render(<BuildStagesView active editor={editorStub()} epicId="epic-1" />);

    expect(await screen.findByRole("table", { name: "Final Production" })).toBeInTheDocument();
    const headings = [...document.querySelectorAll("h3")].map((el) => el.textContent ?? "");
    expect(headings[0]).toMatch(/^Inputs to Source/);
    expect(headings[1]).toMatch(/^Stage 1/);
    expect(headings[2]).toMatch(/^Final Production/);
    expect(getOrderExecutionPlan).toHaveBeenCalledWith("epic-1");
    expect(postBuildExecutionPlan).not.toHaveBeenCalled();

    // Each step shows its ticket in place of the facility line.
    expect(within(rowFor("reaction")).getByText("T-7 · Done")).toBeInTheDocument();
    expect(within(rowFor("root")).getByText("T-8 · To do")).toBeInTheDocument();

    // An input shows what the Epic holds and what it still needs.
    const tritanium = rowFor("34");
    expect(within(tritanium).getByText("600 reserved · 0 used")).toBeInTheDocument();
    expect(within(tritanium).getByText("400")).toBeInTheDocument();
  });

  it("opens a read-only inspector: details shown, nothing editable", async () => {
    const plan = epicPlan();
    const consumer = {
      nodeId: "root",
      occurrenceId: "root",
      quantity: 400,
      buildId: "build-1",
      dependencyId: "dep:root:16673",
      fulfillmentScope: "missing" as const,
      requiredQuantity: 500,
      plannedInventoryQuantity: 100,
    };
    // Methods a draft would offer a switch for -- the Epic must not.
    plan.plan.nodes[0] = {
      ...plan.plan.nodes[0],
      consumers: [consumer],
      productionMethods: [{ mode: "reaction", reactionFormulaTypeId: 17_960 }],
    };
    plan.plan.acquisitions[0] = {
      ...TRITANIUM,
      consumers: [{ ...consumer, quantity: 400, freshCost: null, freshUnitPrice: null }],
      productionMethods: [{ mode: "manufacturing", blueprintTypeId: 999 }],
    };
    plan.epic.nodes.reaction.output = { reserved: 200, consumed: 300, remainingNeed: 0 };
    getOrderExecutionPlan.mockResolvedValue(plan);
    const user = userEvent.setup();
    render(<BuildStagesView active editor={editorStub()} epicId="epic-1" />);

    await screen.findByRole("table", { name: "Final Production" });
    await user.click(within(rowFor("reaction")).getByText("Fernite Carbide"));

    const close = await screen.findByRole("button", { name: "Close production inspector" });
    const inspector = close.closest("aside, section, [role=dialog], [role=complementary]") ?? document.body;
    const panel = within(inspector as HTMLElement);
    expect(panel.getByText("T-7 · Done")).toBeInTheDocument();
    expect(panel.getByText("Output used")).toBeInTheDocument();
    expect(panel.queryByRole("radiogroup")).not.toBeInTheDocument();
    expect(panel.queryByRole("button", { name: /Create ticket|Edit build settings|Save/ })).not.toBeInTheDocument();
    expect(panel.queryByText("Open producer Build")).not.toBeInTheDocument();
    expect(panel.queryByRole("combobox")).not.toBeInTheDocument();

    // An input's inspector shows what the Epic holds, without switches.
    await user.click(within(rowFor("34")).getByText("Tritanium"));
    const inputClose = await screen.findByRole("button", { name: "Close input inspector" });
    const inputPanel = within(
      (inputClose.closest("aside, section, [role=dialog], [role=complementary]") ?? document.body) as HTMLElement,
    );
    expect(inputPanel.getByText("Still needed")).toBeInTheDocument();
    expect(inputPanel.queryByRole("radiogroup")).not.toBeInTheDocument();
    expect(inputPanel.queryByRole("button", { name: /Produce all/ })).not.toBeInTheDocument();
  });

  it("creates an input's tickets in the Epic and reloads", async () => {
    bulkCreateTickets.mockResolvedValue([]);
    const user = userEvent.setup();
    render(<BuildStagesView active editor={editorStub()} epicId="epic-1" />);

    await screen.findByRole("table", { name: "Final Production" });
    await user.click(within(rowFor("34")).getByRole("button", { name: "Create ticket" }));

    expect(bulkCreateTickets).toHaveBeenCalledWith("epic-1", ["req-trit"]);
    await waitFor(() => expect(getOrderExecutionPlan).toHaveBeenCalledTimes(2));
  });

  it("creates a step's missing ticket from its inspector and reloads", async () => {
    const plan = epicPlan();
    plan.epic.nodes.reaction = {
      ...plan.epic.nodes.reaction,
      ticketId: null,
      ticketDisplayId: null,
      ticketStatus: null,
    };
    getOrderExecutionPlan.mockResolvedValue(plan);
    createOperationTicket.mockResolvedValue({});
    const user = userEvent.setup();
    render(<BuildStagesView active editor={editorStub()} epicId="epic-1" />);

    await screen.findByRole("table", { name: "Final Production" });
    expect(within(rowFor("reaction")).getByText("No ticket")).toBeInTheDocument();
    await user.click(within(rowFor("reaction")).getByText("Fernite Carbide"));
    const close = await screen.findByRole("button", { name: "Close production inspector" });
    const panel = within(
      (close.closest("aside, section, [role=dialog], [role=complementary]") ?? document.body) as HTMLElement,
    );
    await user.click(panel.getByRole("button", { name: "Create ticket" }));

    expect(createOperationTicket).toHaveBeenCalledWith("epic-1", "reaction");
    await waitFor(() => expect(getOrderExecutionPlan).toHaveBeenCalledTimes(2));
  });
});
