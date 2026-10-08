import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { describe, expect, it, vi } from "vitest";

import type { OrderDetail, OrderRequirement, OrderSummary, TicketSummary } from "../../../api/industry";

const industryApi = vi.hoisted(() => ({
  getOrder: vi.fn(),
  startOrder: vi.fn(),
  completeOrder: vi.fn(),
  cancelOrder: vi.fn(),
  archiveOrder: vi.fn(),
  restoreOrder: vi.fn(),
  deleteOrder: vi.fn(),
  createTicketForRequirement: vi.fn(),
  bulkCreateTickets: vi.fn(),
}));

vi.mock("../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../api/industry")>("../../../api/industry");
  return { ...actual, ...industryApi };
});

import { EpicInspector } from "../epic-inspector";

function orderFixture(overrides: Partial<OrderSummary> = {}): OrderSummary {
  return {
    id: "order-1",
    workspaceId: "workspace-1",
    ownerId: "owner-1",
    sourceBuildId: "build-1",
    sourceBuildRevision: 1,
    displayName: "Manufacture Ishtar",
    runs: 1,
    recipeFingerprint: "fp",
    priceSnapshotId: "snapshot-1",
    estimatedMaterialCost: "1000.0000",
    expectedRevenue: null,
    estimatedMargin: null,
    missingPriceCount: 0,
    createdAt: "2026-08-21T00:00:00Z",
    updatedAt: "2026-08-21T00:00:00Z",
    startedAt: null,
    completedAt: null,
    canceledAt: null,
    archivedAt: null,
    status: "inProgress",
    rollup: { satisfied: 0, needsAction: 0, inProgress: 0, total: 0 },
    ...overrides,
  };
}

function requirementFixture(overrides: Partial<OrderRequirement> = {}): OrderRequirement {
  return {
    id: "req-1",
    orderId: "order-1",
    typeId: 34,
    capturedName: "Tritanium",
    kind: "buy",
    sourceBuildId: null,
    requiredQuantity: 1000,
    fulfillmentScope: "full",
    reusedQuantity: 0,
    freshQuantity: 1000,
    estimatedUnitCost: "5.0000",
    estimatedLineTotal: "5000.0000",
    reusedLineTotal: null,
    state: "needsAction",
    linkedTickets: [],
    ...overrides,
  };
}

function orderDetailFixture(overrides: Partial<OrderDetail> = {}): OrderDetail {
  return { ...orderFixture(), requirements: [], ...overrides };
}

function ticketFixture(overrides: Partial<TicketSummary> = {}): TicketSummary {
  return {
    id: "ticket-1",
    workspaceId: "workspace-1",
    ownerId: "owner-1",
    displayId: "ISK-2000",
    kind: "manufacturing",
    typeId: 34,
    capturedName: "Tritanium",
    quantity: 100,
    orderId: null,
    notes: "",
    assigneeCharacterId: null,
    sourceBuildId: "build-1",
    status: "todo",
    estimatedUnitCost: "5.0000",
    estimatedLineTotal: "500.0000",
    actualUnitCost: null,
    actualLineTotal: null,
    marketRegionId: null,
    marketLocationId: null,
    priceSourceId: "source-1",
    acquisitionRunId: null,
    acquiredQuantity: null,
    executionSnapshot: null,
    createdAt: "2026-08-22T00:00:00Z",
    updatedAt: "2026-08-22T00:00:00Z",
    archivedAt: null,
    blockedBy: [],
    prerequisites: [],
    recording: null,
    ...overrides,
  };
}

function renderInspector({
  order = orderFixture(),
  tickets = [],
  onOpenTicket = vi.fn(),
  onCreateTicket,
}: {
  order?: OrderSummary;
  tickets?: TicketSummary[];
  onOpenTicket?: (ticketId: string) => void;
  onCreateTicket?: () => void;
} = {}) {
  const onClose = vi.fn();
  const onChanged = vi.fn();
  render(
    <MemoryRouter>
      <EpicInspector
        onChanged={onChanged}
        onClose={onClose}
        onCreateTicket={onCreateTicket}
        onOpenTicket={onOpenTicket}
        order={order}
        tickets={tickets}
      />
    </MemoryRouter>,
  );
  return { onClose, onChanged, onOpenTicket };
}

describe("EpicInspector", () => {
  it("shows only organizational Details fields -- never reserved/allocation/consumption language", async () => {
    industryApi.getOrder.mockResolvedValue(orderDetailFixture());
    renderInspector();

    expect(await screen.findByText("Details")).toBeInTheDocument();
    expect(screen.getByText("Runs")).toBeInTheDocument();
    expect(screen.getByText("Source Build revision")).toBeInTheDocument();
    expect(screen.queryByText(/reserved/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/allocat/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/consum/i)).not.toBeInTheDocument();
  });

  it("derives Progress from linked tickets, independent of the Epic's own status, with no mismatch warning", async () => {
    industryApi.getOrder.mockResolvedValue(
      orderDetailFixture({
        status: "complete",
        completedAt: "2026-09-01T00:00:00Z",
        requirements: [requirementFixture({ state: "linked", linkedTickets: [{ id: "ticket-1", displayId: "ISK-2000", status: "todo", allocatedQuantity: 1000 }] })],
      }),
    );
    // A Complete Epic with a linked ticket that is still To Do / not
    // recorded -- both facts render side by side, no warning that they
    // "disagree".
    renderInspector({
      order: orderFixture({ status: "complete", completedAt: "2026-09-01T00:00:00Z" }),
      tickets: [
        ticketFixture({
          orderId: "order-1",
          status: "todo",
          recording: { state: "notRecorded", requestedQuantity: 1, recordedQuantity: 0, remainingQuantity: 1, surplusQuantity: 0 },
        }),
      ],
    });

    expect(await screen.findByText("0/1 tickets complete")).toBeInTheDocument();
    expect(screen.getByText("0/1 recorded")).toBeInTheDocument();
    expect(screen.queryByText(/mismatch/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/inconsisten/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/out of sync/i)).not.toBeInTheDocument();
  });

  it("creates a ticket for a single Needs Action requirement via the existing per-requirement endpoint", async () => {
    industryApi.getOrder.mockResolvedValue(
      orderDetailFixture({ requirements: [requirementFixture({ id: "req-1", state: "needsAction" })] }),
    );
    industryApi.createTicketForRequirement.mockResolvedValue(ticketFixture({ id: "new-ticket" }));
    const user = userEvent.setup();
    const { onChanged } = renderInspector();

    await screen.findByText("Needs Action");
    await user.click(screen.getByRole("button", { name: "Create ticket" }));

    await waitFor(() => expect(industryApi.createTicketForRequirement).toHaveBeenCalledWith("order-1", "req-1"));
    expect(onChanged).toHaveBeenCalled();
  });

  it("bulk-creates tickets for every Needs Action requirement at once when there is more than one", async () => {
    industryApi.getOrder.mockResolvedValue(
      orderDetailFixture({
        requirements: [
          requirementFixture({ id: "req-1", state: "needsAction" }),
          requirementFixture({ id: "req-2", capturedName: "Pyerite", state: "needsAction" }),
        ],
      }),
    );
    industryApi.bulkCreateTickets.mockResolvedValue([]);
    const user = userEvent.setup();
    renderInspector();

    await user.click(await screen.findByRole("button", { name: "Create 2 tickets" }));

    await waitFor(() => expect(industryApi.bulkCreateTickets).toHaveBeenCalledWith("order-1", ["req-1", "req-2"]));
  });

  it("Open build is a real navigation link, distinct from every other row's inspect-in-place click", async () => {
    industryApi.getOrder.mockResolvedValue(orderDetailFixture());
    renderInspector();

    const openBuild = await screen.findByRole("link", { name: "Open build" });
    expect(openBuild).toHaveAttribute("href", "/builds/build-1");
  });

  it("keeps frozen Epic economics readable when the source Build is gone", async () => {
    industryApi.getOrder.mockResolvedValue(
      orderDetailFixture({
        sourceBuildId: null,
        estimatedMaterialCost: "4321.0000",
        productionPlan: {
          rootOccurrenceKey: "root:deleted",
          dependencies: [],
          operations: [
            {
              id: "operation-1",
              occurrenceKey: "root:deleted",
              parentOccurrenceKey: null,
              buildId: null,
              productTypeId: 5876,
              productName: "Rifter",
              runs: 1,
              producedQuantity: 1,
              materialComponentCost: "42.0000",
              ownInstallationCost: "8.0000",
              totalProductionCost: "50.0000",
              complete: true,
              consumedQuantity: null,
              surplusQuantity: null,
              surplusRetainedBasis: null,
              stage: 0,
              ticketId: null,
              ticketDisplayId: null,
              ticketStatus: null,
              servedRequirementIds: [],
            },
          ],
        },
      }),
    );
    renderInspector({ order: orderFixture({ sourceBuildId: null, estimatedMaterialCost: "4321.0000" }) });

    expect(await screen.findByText("4,321 ISK")).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Open build" })).not.toBeInTheDocument();
  });

  it("reworded lifecycle controls use Epic language and never call ticket recording", async () => {
    industryApi.getOrder.mockResolvedValue(orderDetailFixture());
    industryApi.cancelOrder.mockResolvedValue(orderDetailFixture({ status: "canceled" }));
    const user = userEvent.setup();
    renderInspector({ order: orderFixture({ status: "inProgress" }) });

    expect(await screen.findByRole("button", { name: "Mark Epic complete" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Cancel Epic" }));

    const dialog = await screen.findByRole("dialog", { name: "Cancel this Epic?" });
    await user.click(within(dialog).getByRole("button", { name: "Cancel Epic" }));

    await waitFor(() => expect(industryApi.cancelOrder).toHaveBeenCalledWith("order-1"));
  });

  it("Delete Epic confirms, calls deleteOrder, and closes", async () => {
    industryApi.getOrder.mockResolvedValue(orderDetailFixture());
    industryApi.deleteOrder.mockResolvedValue(undefined);
    const user = userEvent.setup();
    const { onChanged, onClose } = renderInspector();

    await user.click(await screen.findByRole("button", { name: "Delete Epic" }));
    const dialog = await screen.findByRole("dialog", { name: "Delete this Epic?" });
    expect(dialog).toHaveTextContent("This removes the Epic and its frozen plan");
    expect(dialog).toHaveTextContent("Its tickets will remain on the Board");
    expect(dialog).toHaveTextContent("Inventory already recorded from those tickets will be kept");
    expect(dialog).not.toHaveTextContent(/recordings.*removed/i);
    await user.click(within(dialog).getByRole("button", { name: "Delete Epic" }));

    await waitFor(() => expect(industryApi.deleteOrder).toHaveBeenCalledWith("order-1"));
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });

  // Explicit organizational membership -- the Epic Inspector's Tickets list
  // is driven entirely by `ticket.orderId`, never by resolving requirement
  // fulfillments or matching `sourceBuildId`.
  it("shows only tickets whose explicit orderId is this Epic, not other Epics' or standalone tickets", async () => {
    industryApi.getOrder.mockResolvedValue(orderDetailFixture({ id: "epic-1" }));
    renderInspector({
      order: orderFixture({ id: "epic-1" }),
      tickets: [
        ticketFixture({ id: "a", displayId: "ISK-1", orderId: "epic-1", capturedName: "Ticket A" }),
        ticketFixture({ id: "b", displayId: "ISK-2", orderId: "epic-1", capturedName: "Ticket B" }),
        ticketFixture({ id: "c", displayId: "ISK-3", orderId: "epic-2", capturedName: "Ticket C" }),
        ticketFixture({ id: "d", displayId: "ISK-4", orderId: null, capturedName: "Ticket D" }),
      ],
    });

    expect(await screen.findByText("Ticket A")).toBeInTheDocument();
    expect(screen.getByText("Ticket B")).toBeInTheDocument();
    expect(screen.queryByText("Ticket C")).not.toBeInTheDocument();
    expect(screen.queryByText("Ticket D")).not.toBeInTheDocument();
    // Only the 2 Epic-1-owned tickets count toward Progress -- Ticket C
    // (another Epic) and Ticket D (standalone) are excluded entirely.
    expect(screen.getByText("0/2 tickets complete")).toBeInTheDocument();
  });

  it("counts only workflow-Complete tickets toward Progress -- not In Progress, and (there being no such status) not blocked/ready", async () => {
    industryApi.getOrder.mockResolvedValue(orderDetailFixture({ id: "epic-1" }));
    renderInspector({
      order: orderFixture({ id: "epic-1" }),
      tickets: [
        ticketFixture({ id: "a", displayId: "ISK-1", orderId: "epic-1", status: "complete", capturedName: "Done" }),
        ticketFixture({ id: "b", displayId: "ISK-2", orderId: "epic-1", status: "inProgress", capturedName: "Working" }),
        ticketFixture({
          id: "c",
          displayId: "ISK-3",
          orderId: "epic-1",
          status: "todo",
          capturedName: "Waiting",
          kind: "manufacturing",
          blockedBy: [
            {
              prerequisiteId: "prereq-1",
              kind: "buy",
              typeId: 34,
              capturedName: "Tritanium",
              outstandingQuantity: 100,
              representativeFulfillingTicketId: null,
              representativeFulfillingTicketDisplayId: null,
              representativeFulfillingTicketStatus: null,
            },
          ],
        }),
      ],
    });

    // 1 of 3 -- only the Complete ticket. The In Progress ticket and the
    // To Do ticket that still has an unmet prerequisite do not count.
    expect(await screen.findByText("1/3 tickets complete")).toBeInTheDocument();
  });

  // Critical acceptance behavior: two root tickets sharing a sourceBuildId
  // (the pre-order_id transitional weakness) must each surface only in
  // their own Epic's inspector, distinguished purely by orderId.
  it("shows only its own root ticket when two Epics share the same sourceBuildId", async () => {
    industryApi.getOrder.mockResolvedValue(orderDetailFixture({ id: "epic-1", sourceBuildId: "build-shared" }));
    const tickets = [
      ticketFixture({
        id: "root-a",
        displayId: "ISK-100",
        kind: "manufacturing",
        orderId: "epic-1",
        sourceBuildId: "build-shared",
        capturedName: "Manufacture Ishtar (A)",
      }),
      ticketFixture({
        id: "root-b",
        displayId: "ISK-200",
        kind: "manufacturing",
        orderId: "epic-2",
        sourceBuildId: "build-shared",
        capturedName: "Manufacture Ishtar (B)",
      }),
    ];

    renderInspector({ order: orderFixture({ id: "epic-1", sourceBuildId: "build-shared" }), tickets });

    expect(await screen.findByText("Manufacture Ishtar (A)")).toBeInTheDocument();
    expect(screen.queryByText("Manufacture Ishtar (B)")).not.toBeInTheDocument();
  });

  // Epic Inspector's own "Create ticket" opens the same canonical editor
  // prefilled with this Epic -- it never builds a second Epic-specific
  // creation form; the actual editor is the parent's (Board's)
  // responsibility, this component only surfaces the entry point.
  it("shows a Create ticket action that delegates to the parent, and omits it when not provided", async () => {
    industryApi.getOrder.mockResolvedValue(orderDetailFixture());
    const onCreateTicket = vi.fn();
    renderInspector({ onCreateTicket });

    await userEvent.click(await screen.findByRole("button", { name: "+ Create ticket" }));
    expect(onCreateTicket).toHaveBeenCalled();
  });

  it("omits the Create ticket action when the caller doesn't support it", async () => {
    industryApi.getOrder.mockResolvedValue(orderDetailFixture());
    renderInspector();

    await screen.findByText("Details");
    expect(screen.queryByRole("button", { name: "+ Create ticket" })).not.toBeInTheDocument();
  });
});
