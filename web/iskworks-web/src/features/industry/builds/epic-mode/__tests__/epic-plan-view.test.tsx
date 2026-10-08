import { render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { EpicCoverage, OrderDetail, OrderRequirement, PlanOperationView } from "../../../../../api/industry/orders";
import { EpicPlanView, epicPlanSections } from "../epic-plan-view";

vi.mock("../../../../../api/industry/orders", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../../../../api/industry/orders")>()),
  getOrder: vi.fn(),
  getOrderCoverage: vi.fn(),
}));

import { getOrder, getOrderCoverage } from "../../../../../api/industry/orders";

function requirement(id: string, typeId: number, name: string, operation: string, over: Partial<OrderRequirement> = {}): OrderRequirement {
  return {
    id,
    orderId: "epic-1",
    typeId,
    capturedName: name,
    kind: "buy",
    sourceBuildId: null,
    requiredQuantity: 1_000,
    fulfillmentScope: "missing",
    reusedQuantity: 0,
    freshQuantity: 1_000,
    estimatedUnitCost: null,
    estimatedLineTotal: null,
    reusedLineTotal: null,
    state: "needsAction",
    linkedTickets: [],
    operationOccurrenceKey: operation,
    ...over,
  } as OrderRequirement;
}

function operation(key: string, name: string, stage: number, over: Partial<PlanOperationView> = {}): PlanOperationView {
  return {
    id: `op-${key}`,
    occurrenceKey: key,
    parentOccurrenceKey: null,
    buildId: null,
    productTypeId: 1,
    productName: name,
    runs: 4,
    producedQuantity: 4,
    stage,
    ticketId: `ticket-${key}`,
    ticketDisplayId: `T-${stage}`,
    ticketStatus: "todo",
    servedRequirementIds: [],
    ...over,
  } as PlanOperationView;
}

function epic(over: Partial<OrderDetail> = {}): OrderDetail {
  return {
    id: "epic-1",
    displayName: "Manufacture Muninn",
    sourceBuildRevision: 7,
    requirements: [
      requirement("req-trit", 34, "Tritanium", "root", { requiredQuantity: 1_000 }),
      requirement("req-fernite", 16673, "Fernite Carbide", "reaction", { requiredQuantity: 500, kind: "build" }),
    ],
    productionPlan: {
      rootOccurrenceKey: "root",
      dependencies: [],
      operations: [
        operation("reaction", "Fernite Carbide", 0, { ticketStatus: "complete" }),
        operation("root", "Muninn", 1),
      ],
    },
    ...over,
  } as OrderDetail;
}

const COVERAGE: EpicCoverage = {
  orderId: "epic-1",
  lines: [
    { requirementId: "req-trit", reserved: 600, consumed: 0, remainingNeed: 400, freeAvailable: 50, freeCoverable: 50 },
    { requirementId: "req-fernite", reserved: 0, consumed: 500, remainingNeed: 0, freeAvailable: 0, freeCoverable: 0 },
  ],
};

describe("EpicPlanView", () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it("shows each frozen operation's inputs with live reservations and progress", async () => {
    vi.mocked(getOrder).mockResolvedValue(epic());
    vi.mocked(getOrderCoverage).mockResolvedValue(COVERAGE);
    render(<EpicPlanView active buildRevision={7} epicId="epic-1" />);

    const table = await screen.findByRole("region", { name: "Epic plan" });
    const tritanium = within(table).getByText("Tritanium").closest("tr")!;
    expect(within(tritanium).getByText("1,000")).toBeInTheDocument();
    expect(within(tritanium).getByText("600")).toBeInTheDocument();
    expect(within(tritanium).getByText("400")).toBeInTheDocument();
    expect(within(tritanium).getByText("50")).toBeInTheDocument();

    // A finished operation stays visible, marked done.
    expect(within(table).getByText(/T-0 Done/)).toBeInTheDocument();
    expect(within(table).getAllByText("Fernite Carbide").length).toBeGreaterThan(0);

    expect(screen.getByText("Epic: Manufacture Muninn")).toBeInTheDocument();
    expect(screen.queryByText(/changed since this Epic was frozen/)).not.toBeInTheDocument();
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  });

  it("lays operations out by stage in build order, final product last", async () => {
    vi.mocked(getOrder).mockResolvedValue(epic());
    vi.mocked(getOrderCoverage).mockResolvedValue(COVERAGE);
    render(<EpicPlanView active buildRevision={7} epicId="epic-1" />);

    const plan = await screen.findByRole("region", { name: "Epic plan" });
    const headings = within(plan).getAllByRole("heading").map((heading) => heading.textContent);
    expect(headings).toEqual(["Stage 1Earliest production", "Final Production"]);
    expect(within(within(plan).getByRole("table", { name: "Stage 1" })).getByText(/T-0 Done/)).toBeInTheDocument();
    expect(within(within(plan).getByRole("table", { name: "Final Production" })).getByText("Tritanium")).toBeInTheDocument();
  });

  it("numbers stages from the earliest, skipping none", () => {
    const sections = epicPlanSections(
      [
        operation("reaction-a", "Fernite Carbide", 0),
        operation("reaction-b", "Sylramic Fibers", 0),
        operation("component", "Nanoelectrical Microprocessor", 2),
        operation("root", "Muninn", 3),
      ],
      "root",
    );
    expect(sections.map((section) => [section.title, section.operations.map((op) => op.occurrenceKey)])).toEqual([
      ["Stage 1", ["reaction-a", "reaction-b"]],
      ["Stage 2", ["component"]],
      ["Final Production", ["root"]],
    ]);
  });

  it("flags a Build edited after the Epic was frozen", async () => {
    vi.mocked(getOrder).mockResolvedValue(epic({ sourceBuildRevision: 5 }));
    vi.mocked(getOrderCoverage).mockResolvedValue(COVERAGE);
    render(<EpicPlanView active buildRevision={7} epicId="epic-1" />);

    expect(await screen.findByText("The Build has changed since this Epic was frozen")).toBeInTheDocument();
  });

  it("loads only while the Plan tab is active", () => {
    render(<EpicPlanView active={false} buildRevision={7} epicId="epic-1" />);
    expect(getOrder).not.toHaveBeenCalled();
  });
});
