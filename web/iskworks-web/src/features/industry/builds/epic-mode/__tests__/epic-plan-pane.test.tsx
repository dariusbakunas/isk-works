import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { OrderDetail, OrderRequirement, OrderSummary } from "../../../../../api/industry";
import { EpicPlanPane } from "../epic-plan-pane";

vi.mock("../../../../../api/industry", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../../../../api/industry")>()),
  getOrder: vi.fn(),
  getOrderCoverage: vi.fn(),
  createTicketForRequirement: vi.fn(),
  listOrders: vi.fn(),
  listBuilds: vi.fn(),
}));
vi.mock("../../../../../api/industry/orders", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../../../../api/industry/orders")>()),
  getOrder: vi.fn(),
  getOrderCoverage: vi.fn(),
}));
vi.mock("../../../../../api/characters", () => ({ listCharacters: vi.fn() }));

import * as industry from "../../../../../api/industry";
import * as orders from "../../../../../api/industry/orders";
import { listCharacters } from "../../../../../api/characters";

function requirement(id: string, name: string, over: Partial<OrderRequirement> = {}): OrderRequirement {
  return {
    id,
    orderId: "epic-1",
    typeId: 34,
    capturedName: name,
    kind: "buy",
    sourceBuildId: null,
    requiredQuantity: 100,
    fulfillmentScope: "missing",
    reusedQuantity: 0,
    freshQuantity: 100,
    estimatedUnitCost: null,
    estimatedLineTotal: null,
    reusedLineTotal: null,
    state: "needsAction",
    linkedTickets: [],
    operationOccurrenceKey: "root",
    ...over,
  } as OrderRequirement;
}

const EPIC = {
  id: "epic-1",
  displayName: "Manufacture Muninn",
  sourceBuildRevision: 1,
  requirements: [
    requirement("req-buy", "Tritanium"),
    requirement("req-linked", "Pyerite", { state: "linked" }),
    requirement("req-build", "Fernite Carbide", { kind: "build" }),
  ],
  productionPlan: {
    rootOccurrenceKey: "root",
    dependencies: [],
    operations: [{
      id: "op-root",
      occurrenceKey: "root",
      parentOccurrenceKey: null,
      buildId: null,
      productTypeId: 1,
      productName: "Muninn",
      runs: 1,
      producedQuantity: 1,
      stage: 0,
      ticketId: "ticket-root",
      ticketDisplayId: "T-1",
      ticketStatus: "todo",
      servedRequirementIds: [],
    }],
  },
} as unknown as OrderDetail;

const EPIC_SUMMARY = { id: "epic-1", displayName: "Manufacture Muninn", status: "blocked" } as unknown as OrderSummary;

function rowFor(name: string) {
  return within(screen.getByRole("table", { name: "Epic plan" })).getByText(name).closest("tr")!;
}

describe("EpicPlanPane", () => {
  beforeEach(() => {
    vi.mocked(orders.getOrder).mockResolvedValue(EPIC);
    vi.mocked(orders.getOrderCoverage).mockResolvedValue({ orderId: "epic-1", lines: [] });
  });
  afterEach(() => {
    vi.clearAllMocks();
  });

  it("creates a Buy requirement's ticket in the Epic and reloads", async () => {
    vi.mocked(industry.createTicketForRequirement).mockResolvedValue({} as never);
    const user = userEvent.setup();
    render(<EpicPlanPane active buildRevision={1} epicId="epic-1" />);

    await screen.findByRole("table", { name: "Epic plan" });
    expect(within(rowFor("Pyerite")).queryByRole("button")).not.toBeInTheDocument();
    expect(within(rowFor("Fernite Carbide")).queryByRole("button")).not.toBeInTheDocument();

    await user.click(within(rowFor("Tritanium")).getByRole("button", { name: "Create ticket" }));

    expect(industry.createTicketForRequirement).toHaveBeenCalledWith("epic-1", "req-buy");
    await waitFor(() => expect(orders.getOrder).toHaveBeenCalledTimes(2));
  });

  it("opens the ticket editor with the Epic already chosen", async () => {
    vi.mocked(industry.listOrders).mockResolvedValue([EPIC_SUMMARY]);
    vi.mocked(industry.listBuilds).mockResolvedValue([]);
    vi.mocked(listCharacters).mockResolvedValue([]);
    const user = userEvent.setup();
    render(<EpicPlanPane active buildRevision={1} epicId="epic-1" />);

    await screen.findByRole("table", { name: "Epic plan" });
    const toolbarButtons = screen.getAllByRole("button", { name: "Create ticket" });
    // The Plan-level button is the one outside the table.
    const planLevel = toolbarButtons.find((button) => !button.closest("table"))!;
    await user.click(planLevel);

    expect(await screen.findByLabelText("Epic")).toHaveValue("epic-1");
  });
});
