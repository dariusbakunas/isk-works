import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, useLocation } from "react-router";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { OrderDetail, OrderSummary, TicketSummary } from "../../../api/industry";
import { ApiError } from "../../../api/workspace";

const industryApi = vi.hoisted(() => ({
  listTickets: vi.fn(),
  listAcquisitionRuns: vi.fn(),
  listOrders: vi.fn(),
  listPriceSources: vi.fn(),
  listMarketRegions: vi.fn(),
  listMarketRegionLocations: vi.fn(),
  createAcquisitionRun: vi.fn(),
  getAcquisitionRun: vi.fn(),
  updateTicketStatus: vi.fn(),
  startTicket: vi.fn(),
  completeTicket: vi.fn(),
  cancelTicket: vi.fn(),
  recordTicketProduction: vi.fn(),
  recordTicketAcquisition: vi.fn(),
  getOrder: vi.fn(),
  startOrder: vi.fn(),
  completeOrder: vi.fn(),
  cancelOrder: vi.fn(),
  archiveOrder: vi.fn(),
  restoreOrder: vi.fn(),
  createTicketForRequirement: vi.fn(),
  bulkCreateTickets: vi.fn(),
  createTicket: vi.fn(),
  updateTicketMetadata: vi.fn(),
  listBuilds: vi.fn(),
}));

const charactersApi = vi.hoisted(() => ({
  listCharacters: vi.fn(),
}));

function dragTicketTo(ticketDisplayId: string, laneTestId: string) {
  const card = screen.getByText(ticketDisplayId).closest("[draggable]") as HTMLElement;
  const lane = screen.getByTestId(laneTestId);
  fireEvent.dragStart(card, { dataTransfer: makeDataTransfer() });
  fireEvent.dragOver(lane, { dataTransfer: makeDataTransfer() });
  fireEvent.drop(lane, { dataTransfer: makeDataTransfer() });
  fireEvent.dragEnd(card);
}

vi.mock("../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../api/industry")>("../../../api/industry");
  return { ...actual, ...industryApi };
});

vi.mock("../../../api/characters", async () => {
  const actual = await vi.importActual<typeof import("../../../api/characters")>("../../../api/characters");
  return { ...actual, ...charactersApi };
});

import { BoardPage } from "../board-page";
import { makeDataTransfer, runFixture } from "./fixtures";

function ticketFixture(overrides: Partial<TicketSummary> = {}): TicketSummary {
  return {
    id: "ticket-1",
    workspaceId: "workspace-1",
    ownerId: "owner-1",
    displayId: "ISK-2000",
    kind: "acquisition",
    typeId: 34,
    capturedName: "Tritanium",
    quantity: 100,
    orderId: null,
    notes: "",
    assigneeCharacterId: null,
    sourceBuildId: null,
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
    status: "blocked",
    rollup: { satisfied: 0, needsAction: 1, inProgress: 0, total: 1 },
    ...overrides,
  };
}

function orderDetailFixture(overrides: Partial<OrderDetail> = {}): OrderDetail {
  return { ...orderFixture(), requirements: [], ...overrides };
}

function renderPage() {
  return render(
    <MemoryRouter>
      <BoardPage />
    </MemoryRouter>,
  );
}

function renderPageAt(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <BoardPage />
    </MemoryRouter>,
  );
}

function LocationProbe() {
  const location = useLocation();
  return <output data-testid="location">{location.pathname + location.search}</output>;
}

// Surfaces the current URL (pathname + search) alongside the Board, for
// tests asserting that a filter change updates the URL -- without
// navigating away or remounting.
function renderPageWithLocation(path?: string) {
  return render(
    <MemoryRouter initialEntries={path ? [path] : undefined}>
      <BoardPage />
      <LocationProbe />
    </MemoryRouter>,
  );
}

// Scopes a query to the lane grid, excluding the toolbar's Epic filter
// `<select>` -- its `<option>`s repeat every Epic's display name, which
// would otherwise make a bare `screen.getByText(order.displayName)`
// ambiguous between the option and the actual Epic card. Async because the
// lane grid itself only exists once the Board's initial load resolves; by
// the time it's in the DOM, everything inside it (cards) is too, so a
// single await here covers the whole test's later synchronous queries.
async function boardLanes() {
  return within(await screen.findByTestId("board-lanes"));
}

// Scopes a query to the filter toolbar itself -- an open Ticket/Epic
// Inspector or the TicketEditor can render their own "Assignee"/"Epic"
// controls (e.g. the Ticket drawer's own Assignee reassignment select),
// which would otherwise make a bare `screen.getByLabelText("Assignee")`
// ambiguous with the Board's own filter control of the same name.
async function boardToolbar() {
  return within(await screen.findByTestId("board-toolbar"));
}

describe("BoardPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    industryApi.getAcquisitionRun.mockResolvedValue({ items: [] });
    industryApi.listOrders.mockResolvedValue([]);
    industryApi.listPriceSources.mockResolvedValue([]);
    industryApi.listMarketRegions.mockResolvedValue([
      { regionId: 10_000_002, regionName: "The Forge" },
      { regionId: 10_000_043, regionName: "Domain" },
    ]);
    industryApi.listMarketRegionLocations.mockImplementation((regionId: number) =>
      Promise.resolve(
        regionId === 10_000_002
          ? [{ locationId: 60_003_760, locationName: "Jita IV - Moon 4 - Caldari Navy Assembly Plant" }]
          : [{ locationId: 60_008_494, locationName: "Amarr VIII (Oris) - Emperor Family Academy" }],
      ),
    );
    charactersApi.listCharacters.mockResolvedValue([]);
    industryApi.listTickets.mockResolvedValue([]);
    industryApi.listAcquisitionRuns.mockResolvedValue([]);
    industryApi.listBuilds.mockResolvedValue([]);
  });

  it("renders the workflow lanes (To Do / In Progress / Complete) with tickets grouped by status", async () => {
    // Manufacturing/Reaction tickets so this test (about lanes, not ACQ
    // grouping) keeps asserting on individually-rendered cards.
    industryApi.listTickets.mockResolvedValue([
      ticketFixture({ id: "t1", displayId: "ISK-1000", status: "todo", kind: "manufacturing" }),
      ticketFixture({
        id: "t2",
        displayId: "ISK-1001",
        status: "inProgress",
        capturedName: "Isogen",
        kind: "manufacturing",
      }),
    ]);

    renderPage();

    expect(await screen.findByText("ISK-1000")).toBeInTheDocument();
    expect(screen.getByText("ISK-1001")).toBeInTheDocument();
    expect(screen.getByText("Isogen")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Board" })).toBeInTheDocument();
    // Only the three workflow lanes exist -- no Blocked or Ready lane.
    expect(screen.getByTestId("board-lane-todo")).toBeInTheDocument();
    expect(screen.getByTestId("board-lane-inProgress")).toBeInTheDocument();
    expect(screen.getByTestId("board-lane-complete")).toBeInTheDocument();
    expect(screen.queryByTestId("board-lane-blocked")).not.toBeInTheDocument();
    expect(screen.queryByTestId("board-lane-ready")).not.toBeInTheDocument();
    expect(within(screen.getByTestId("board-lane-todo")).getByText("ISK-1000")).toBeInTheDocument();
    expect(within(screen.getByTestId("board-lane-inProgress")).getByText("ISK-1001")).toBeInTheDocument();
  });

  it("uses Epic/run terminology in the header and footer, not Order/batch", async () => {
    industryApi.listTickets.mockResolvedValue([
      ticketFixture({ displayId: "ISK-1000", kind: "manufacturing" }),
    ]);

    renderPage();

    expect(await screen.findByText("ISK-1000")).toBeInTheDocument();
    expect(
      screen.getByText(/Operational work across every Epic and Acquisition Run\./i),
    ).toBeInTheDocument();
    expect(screen.queryByText(/across every Order/i)).not.toBeInTheDocument();
    // Footer counter: "<n> runs · <n> tickets · <n> epics" -- no "batches"/"orders".
    expect(screen.getByText(/\d+ runs · \d+ tickets · \d+ epics/)).toBeInTheDocument();
    expect(screen.queryByText(/batches/i)).not.toBeInTheDocument();
  });

  it("keeps a ticket with unmet dependencies in its own workflow lane, showing only a derived blocker indicator", async () => {
    industryApi.listTickets.mockResolvedValue([
      ticketFixture({
        id: "t1",
        displayId: "ISK-1000",
        kind: "manufacturing",
        // User-chosen lane is In Progress; the unmet prerequisite must not
        // move it anywhere or override its status badge.
        status: "inProgress",
        blockedBy: [
          {
            prerequisiteId: "prereq-1",
            kind: "buy",
            typeId: 37164,
            capturedName: "Isogen",
            outstandingQuantity: 50,
            representativeFulfillingTicketId: "ticket-2",
            representativeFulfillingTicketDisplayId: "ISK-2001",
            representativeFulfillingTicketStatus: "inProgress",
          },
        ],
      }),
    ]);

    renderPage();
    await screen.findByText("ISK-1000");

    // Lives in the In Progress lane it was assigned to -- not a Blocked lane
    // (there is none) and not forced back to To Do.
    expect(within(screen.getByTestId("board-lane-inProgress")).getByText("ISK-1000")).toBeInTheDocument();
    expect(within(screen.getByTestId("board-lane-todo")).queryByText("ISK-1000")).not.toBeInTheDocument();
    // The card still carries the workflow status badge and, separately, a
    // compact derived blocker hint.
    const card = screen.getByText("ISK-1000").closest("[draggable]") as HTMLElement;
    expect(within(card).getByText("In Progress")).toBeInTheDocument();
    expect(within(card).getByText(/Blocked by/)).toBeInTheDocument();
    expect(within(card).getByText(/ISK-2001/)).toBeInTheDocument();
  });

  it("opens a ticket's drawer from a ?ticket= deep-link (e.g. Inventory's Reservations tab), and clears it on close", async () => {
    industryApi.listTickets.mockResolvedValue([
      ticketFixture({ id: "ticket-1", displayId: "ISK-1852", capturedName: "Tritanium" }),
    ]);

    renderPageAt("/board?ticket=ticket-1");

    expect(await screen.findByRole("complementary", { name: "Tritanium" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Close ticket details" }));
    expect(screen.queryByRole("complementary", { name: "Tritanium" })).not.toBeInTheDocument();
  });

  it("folds an Epic whose requirements are unmet into the To Do lane (no Blocked lane)", async () => {
    industryApi.listOrders.mockResolvedValue([
      orderFixture({
        id: "order-1",
        displayName: "Manufacture Ishtar",
        status: "blocked",
        rollup: { satisfied: 0, needsAction: 11, inProgress: 0, total: 11 },
      }),
    ]);

    renderPage();

    expect((await boardLanes()).getByText("Manufacture Ishtar")).toBeInTheDocument();
    expect(screen.getByText("0/11 satisfied")).toBeInTheDocument();
    expect(screen.getByText("11 need action")).toBeInTheDocument();
    // The Epic-lifecycle "blocked" status has no lane of its own -- it maps
    // onto To Do like every other pre-start state.
    expect(within(screen.getByTestId("board-lane-todo")).getByText("Manufacture Ishtar")).toBeInTheDocument();
  });

  // Core UX rule: clicking an Epic card opens the Epic Inspector in the
  // Board's own right rail -- it must never navigate to a separate Order
  // page.
  it("opens the Epic Inspector (not the old Order page) when an Epic card is clicked, with organizational lifecycle controls only", async () => {
    industryApi.listOrders.mockResolvedValue([
      orderFixture({ id: "order-1", displayName: "Manufacture Ishtar", status: "ready" }),
    ]);
    industryApi.getOrder.mockResolvedValue(orderDetailFixture({ id: "order-1", displayName: "Manufacture Ishtar", status: "ready" }));

    renderPage();
    const lanes = await boardLanes();
    lanes.getByText("Manufacture Ishtar");
    expect(screen.queryByRole("link")).not.toBeInTheDocument();

    await userEvent.click(lanes.getByText("Manufacture Ishtar"));

    expect(await screen.findByRole("complementary", { name: "Manufacture Ishtar" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start Epic" })).toBeInTheDocument();
    // Never execution-engine language (reservation/allocation).
    expect(screen.queryByText(/reserved/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/allocat/i)).not.toBeInTheDocument();
  });

  it("switches from the Epic Inspector to the Ticket Inspector when a linked ticket row is clicked, and Back restores the Epic", async () => {
    industryApi.listOrders.mockResolvedValue([
      orderFixture({ id: "order-1", displayName: "Manufacture Ishtar", sourceBuildId: "build-1" }),
    ]);
    industryApi.listTickets.mockResolvedValue([
      ticketFixture({
        id: "ticket-1",
        displayId: "ISK-1000",
        orderId: "order-1",
        sourceBuildId: "build-1",
        kind: "manufacturing",
      }),
    ]);
    industryApi.getOrder.mockResolvedValue(
      orderDetailFixture({ id: "order-1", displayName: "Manufacture Ishtar", sourceBuildId: "build-1" }),
    );

    renderPage();
    await userEvent.click((await boardLanes()).getByText("Manufacture Ishtar"));
    expect(await screen.findByRole("complementary", { name: "Manufacture Ishtar" })).toBeInTheDocument();

    // The root Manufacturing ticket is associated via its explicit
    // `orderId`, not by inferring from sourceBuildId or its title. Scoped
    // to the inspector -- the same ticket also renders its own Board card.
    const inspector = screen.getByRole("complementary", { name: "Manufacture Ishtar" });
    await userEvent.click(within(inspector).getByText("ISK-1000"));

    expect(await screen.findByRole("heading", { name: "Tritanium" })).toBeInTheDocument();
    expect(screen.queryByRole("complementary", { name: "Manufacture Ishtar" })).not.toBeInTheDocument();
    const backButton = screen.getByRole("button", { name: "Back to Epic" });

    await userEvent.click(backButton);

    expect(await screen.findByRole("complementary", { name: "Manufacture Ishtar" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Tritanium" })).not.toBeInTheDocument();
  });

  it("fully closes (no Back button, no re-opened Epic) when a ticket opened directly from a Board card is closed", async () => {
    industryApi.listTickets.mockResolvedValue([
      ticketFixture({ id: "ticket-1", displayId: "ISK-1000", kind: "manufacturing" }),
    ]);

    renderPage();
    await userEvent.click(await screen.findByText("ISK-1000"));

    expect(await screen.findByRole("heading", { name: "Tritanium" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Back to Epic" })).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Close ticket details" }));

    expect(screen.queryByRole("heading", { name: "Tritanium" })).not.toBeInTheDocument();
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
  });

  it("preserves Board search/filter state across opening and closing the Epic Inspector", async () => {
    industryApi.listOrders.mockResolvedValue([
      orderFixture({ id: "order-1", displayName: "Manufacture Ishtar" }),
      orderFixture({ id: "order-2", displayName: "Manufacture Cerberus" }),
    ]);
    // A ticket under order-1 only -- once Search is active, an Epic card
    // needs >=1 matching child Ticket to stay visible (see
    // board-filters.ts's `boardEpicCardIsVisible`). Matches via the
    // Epic-title search field (its own capturedName doesn't mention
    // "Ishtar" at all), doubling as coverage for that field.
    industryApi.listTickets.mockResolvedValue([
      ticketFixture({ id: "ticket-1", orderId: "order-1", capturedName: "Some component" }),
    ]);
    industryApi.getOrder.mockResolvedValue(orderDetailFixture({ id: "order-1", displayName: "Manufacture Ishtar" }));

    renderPage();
    const lanesBefore = await boardLanes();
    lanesBefore.getByText("Manufacture Ishtar");

    await userEvent.type(screen.getByLabelText("Search tickets"), "Ishtar");
    expect(lanesBefore.queryByText("Manufacture Cerberus")).not.toBeInTheDocument();

    await userEvent.click(lanesBefore.getByText("Manufacture Ishtar"));
    expect(await screen.findByRole("complementary", { name: "Manufacture Ishtar" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Close Epic inspector" }));

    // The search box and its filtered result set are untouched -- the Board
    // itself never remounted.
    expect(screen.getByLabelText("Search tickets")).toHaveValue("Ishtar");
    expect(lanesBefore.queryByText("Manufacture Cerberus")).not.toBeInTheDocument();
    expect(lanesBefore.getByText("Manufacture Ishtar")).toBeInTheDocument();
  });

  // Canonical manual Ticket creation, driven from the Board itself.
  it("Create ticket opens the canonical editor, creates a Generic ticket, and opens its Inspector -- Board state preserved", async () => {
    industryApi.listTickets.mockResolvedValue([]);
    charactersApi.listCharacters.mockResolvedValue([
      { connectionId: "char-1", characterName: "Alt One" } as never,
    ]);
    industryApi.createTicket.mockResolvedValue(
      ticketFixture({ id: "new-ticket", kind: "generic", capturedName: "Move blueprints to C-J6MT", typeId: null, quantity: null }),
    );

    renderPage();
    await screen.findByRole("heading", { name: "Board" });

    // The Board's own filters/search are set before opening the editor --
    // they must survive the whole create flow.
    await userEvent.type(screen.getByLabelText("Search tickets"), "unrelated search text");

    await userEvent.click(screen.getByRole("button", { name: "Create ticket" }));
    expect(screen.getByRole("heading", { name: "Create ticket" })).toBeInTheDocument();
    // Generic is the default Type -- no item/quantity fields shown.
    expect(screen.queryByLabelText("Search item")).not.toBeInTheDocument();

    await userEvent.type(screen.getByPlaceholderText("e.g. Move blueprints to C-J6MT"), "Move blueprints to C-J6MT");
    industryApi.listTickets.mockResolvedValue([
      ticketFixture({ id: "new-ticket", kind: "generic", capturedName: "Move blueprints to C-J6MT", typeId: null, quantity: null }),
    ]);
    const editor = screen.getByRole("complementary", { name: "Create ticket" });
    await userEvent.click(within(editor).getByRole("button", { name: "Create ticket" }));

    expect(await screen.findByRole("heading", { name: "Move blueprints to C-J6MT" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Create ticket" })).not.toBeInTheDocument();
    // Board's own state (URL/route, search box) is untouched.
    expect(screen.getByLabelText("Search tickets")).toHaveValue("unrelated search text");
  });

  it("opens the Epic Inspector from a ?epic= deep-link", async () => {
    industryApi.listOrders.mockResolvedValue([orderFixture({ id: "order-1", displayName: "Manufacture Ishtar" })]);
    industryApi.getOrder.mockResolvedValue(orderDetailFixture({ id: "order-1", displayName: "Manufacture Ishtar" }));

    renderPageAt("/board?epic=order-1");

    expect(await screen.findByRole("complementary", { name: "Manufacture Ishtar" })).toBeInTheDocument();
  });

  // Reverse Ticket -> Epic context: clicking the Epic name inside the
  // Ticket Inspector switches to that Epic's own Inspector, staying on
  // Board -- never a navigation to /orders/:id.
  it("switches from the Ticket Inspector to the Epic Inspector when its Epic context is clicked", async () => {
    industryApi.listOrders.mockResolvedValue([
      orderFixture({ id: "order-1", displayName: "Manufacture Ishtar" }),
    ]);
    industryApi.listTickets.mockResolvedValue([
      ticketFixture({ id: "ticket-1", displayId: "ISK-1000", orderId: "order-1" }),
    ]);
    industryApi.getOrder.mockResolvedValue(
      orderDetailFixture({ id: "order-1", displayName: "Manufacture Ishtar" }),
    );

    renderPageAt("/board?ticket=ticket-1");

    expect(await screen.findByRole("heading", { name: "Tritanium" })).toBeInTheDocument();
    // The Ticket Inspector's Epic control is an editable dropdown; "Open"
    // is the distinct affordance that switches to that Epic's Inspector.
    await userEvent.click(screen.getByRole("button", { name: "Open" }));

    expect(await screen.findByRole("complementary", { name: "Manufacture Ishtar" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Tritanium" })).not.toBeInTheDocument();
  });

  it("hides archived Orders by default and reveals them with Show archived", async () => {
    industryApi.listOrders.mockResolvedValue([
      orderFixture({ id: "order-1", displayName: "Manufacture Ishtar" }),
      orderFixture({ id: "order-2", displayName: "Manufacture Cerberus", archivedAt: "2026-08-21T02:00:00Z" }),
    ]);

    renderPage();

    const lanes = await boardLanes();
    lanes.getByText("Manufacture Ishtar");
    expect(lanes.queryByText("Manufacture Cerberus")).not.toBeInTheDocument();
    expect(screen.getByText("1 archived hidden")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Show archived" }));

    expect(await lanes.findByText("Manufacture Cerberus")).toBeInTheDocument();
    expect(lanes.getByText("Manufacture Ishtar")).toBeInTheDocument();
  });

  it("never renders a canceled Order, regardless of the archived filter", async () => {
    industryApi.listOrders.mockResolvedValue([orderFixture({ status: "canceled" })]);

    renderPage();

    await screen.findByRole("heading", { name: "Board" });
    expect(screen.queryByText("Manufacture Ishtar")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Show archived" }));

    expect(screen.queryByText("Manufacture Ishtar")).not.toBeInTheDocument();
  });

  it("keeps other ticket cards clickable while the detail drawer is open (push-in layout, no blocking overlay)", async () => {
    industryApi.listTickets.mockResolvedValue([
      ticketFixture({ id: "t1", displayId: "ISK-1000", status: "todo", kind: "manufacturing" }),
      ticketFixture({ id: "t2", displayId: "ISK-1001", status: "todo", capturedName: "Isogen", kind: "manufacturing" }),
    ]);

    renderPage();
    await screen.findByText("ISK-1000");

    await userEvent.click(screen.getByText("ISK-1000"));
    expect(await screen.findByRole("heading", { name: "Tritanium" })).toBeInTheDocument();

    // The board's own ticket cards (not just the drawer's own controls)
    // must still be clickable with the drawer open -- no full-screen
    // overlay should sit between them.
    await userEvent.click(screen.getByText("ISK-1001"));

    expect(await screen.findByRole("heading", { name: "Isogen" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Tritanium" })).not.toBeInTheDocument();
    // The board underneath is still fully rendered, not hidden behind a
    // backdrop.
    expect(screen.getByText("ISK-1000")).toBeInTheDocument();
  });

  it("groups unbatched Acquisition tickets into one ACQ GROUP card by price source, and batched ones render via the Run card instead", async () => {
    industryApi.listTickets.mockResolvedValue([
      ticketFixture({ id: "t1", displayId: "ISK-1000", status: "todo", acquisitionRunId: "run-1" }),
      ticketFixture({ id: "t2", displayId: "ISK-1001", status: "todo", capturedName: "Isogen" }),
    ]);
    industryApi.listAcquisitionRuns.mockResolvedValue([runFixture()]);
    industryApi.listPriceSources.mockResolvedValue([{ id: "source-1", name: "Jita 4-4" } as never]);

    renderPage();

    // The batched ticket's own card is hidden; the Run card represents it.
    await screen.findByText("Isogen");
    expect(screen.queryByText("ISK-1000")).not.toBeInTheDocument();
    expect(screen.getByText("Tuesday Jita Run")).toBeInTheDocument();
    // The unbatched ticket groups under an ACQ GROUP card by price source.
    expect(screen.getByText("ACQ GROUP")).toBeInTheDocument();
    expect(screen.getAllByText(/Jita 4-4/).length).toBeGreaterThan(0);
  });

  it("labels a market-scoped ACQ group by its market scope, never 'Unknown source'", async () => {
    industryApi.listTickets.mockResolvedValue([
      // A Build-generated acquisition ticket: market scope carried,
      // priceSourceId legitimately null.
      ticketFixture({
        id: "t1",
        displayId: "ISK-1000",
        status: "todo",
        capturedName: "Tritanium",
        marketRegionId: 10_000_002,
        marketLocationId: 60_003_760,
        priceSourceId: null,
      }),
    ]);

    renderPage();

    await screen.findByText("ACQ GROUP");
    expect(screen.queryByText("Unknown source")).not.toBeInTheDocument();
    expect(
      screen.getAllByText("Priced at Jita IV - Moon 4 - Caldari Navy Assembly Plant").length,
    ).toBeGreaterThan(0);
  });

  it("keeps acquisition tickets priced in different regions in separate ACQ groups", async () => {
    industryApi.listTickets.mockResolvedValue([
      ticketFixture({
        id: "forge",
        displayId: "ISK-1000",
        status: "todo",
        capturedName: "Tritanium",
        marketRegionId: 10_000_002,
        marketLocationId: 60_003_760,
        priceSourceId: null,
      }),
      ticketFixture({
        id: "domain",
        displayId: "ISK-1001",
        status: "todo",
        capturedName: "Pyerite",
        marketRegionId: 10_000_043,
        marketLocationId: 60_008_494,
        priceSourceId: null,
      }),
    ]);

    renderPage();

    expect(
      (await screen.findAllByText("Priced at Jita IV - Moon 4 - Caldari Navy Assembly Plant")).length,
    ).toBeGreaterThan(0);
    expect(
      screen.getAllByText("Priced at Amarr VIII (Oris) - Emperor Family Academy").length,
    ).toBeGreaterThan(0);
    // Two distinct ACQ GROUP cards in the To Do lane, not one merged bucket.
    expect(within(screen.getByTestId("board-lane-todo")).getAllByText("ACQ GROUP")).toHaveLength(2);
  });

  it("does not let a no-pricing-source ACQ group start a Run", async () => {
    industryApi.listTickets.mockResolvedValue([
      ticketFixture({
        id: "t1",
        displayId: "ISK-1000",
        status: "todo",
        marketRegionId: null,
        marketLocationId: null,
        priceSourceId: null,
      }),
    ]);

    renderPage();
    await screen.findByText("ACQ GROUP");
    expect(screen.getAllByText("No pricing source").length).toBeGreaterThan(0);

    await userEvent.click(screen.getByRole("button", { name: "Select" }));
    await userEvent.click(screen.getByRole("button", { name: /Expand No pricing source To Do group/ }));

    // No select-all, no per-row checkbox -- the group can't be batched.
    expect(screen.queryByRole("button", { name: /Select \d+ actionable/ })).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Select ISK-1000")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Create Acquisition Run" })).not.toBeInTheDocument();
  });

  it("labels a market-scoped Acquisition Run card by its market scope, never 'Unknown source'", async () => {
    industryApi.listTickets.mockResolvedValue([
      ticketFixture({
        id: "t1",
        displayId: "ISK-1000",
        status: "todo",
        acquisitionRunId: "run-1",
        marketRegionId: 10_000_002,
        marketLocationId: 60_003_760,
        priceSourceId: null,
      }),
    ]);
    industryApi.listAcquisitionRuns.mockResolvedValue([
      runFixture({ marketRegionId: 10_000_002, marketLocationId: 60_003_760, priceSourceId: null }),
    ]);

    renderPage();

    await screen.findByText("Tuesday Jita Run");
    expect(screen.queryByText("Unknown source")).not.toBeInTheDocument();
    expect(
      screen.getAllByText("Priced at Jita IV - Moon 4 - Caldari Navy Assembly Plant").length,
    ).toBeGreaterThan(0);
  });

  it("selects tickets in an expanded group and creates an Acquisition Run", async () => {
    industryApi.listTickets.mockResolvedValue([
      ticketFixture({ id: "t1", displayId: "ISK-1000", status: "todo" }),
      ticketFixture({ id: "t2", displayId: "ISK-1001", status: "todo", capturedName: "Isogen" }),
    ]);
    industryApi.listPriceSources.mockResolvedValue([{ id: "source-1", name: "Jita 4-4" } as never]);
    industryApi.createAcquisitionRun.mockResolvedValue(runFixture());

    renderPage();
    await screen.findByText("ACQ GROUP");

    await userEvent.click(screen.getByRole("button", { name: "Select" }));
    await userEvent.click(screen.getByRole("button", { name: /Expand .*Jita 4-4 To Do group/ }));
    await userEvent.click(screen.getByLabelText("Select ISK-1000"));
    await userEvent.click(screen.getByLabelText("Select ISK-1001"));

    expect(screen.getByText("2 order tickets selected")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Create Acquisition Run" }));
    await userEvent.click(screen.getByRole("button", { name: "Create Run" }));

    expect(industryApi.createAcquisitionRun).toHaveBeenCalledWith({
      name: undefined,
      ticketIds: ["t1", "t2"],
    });
  });

  it("opens the Acquisition Run drawer with that Run's full ticket group when its card is clicked", async () => {
    industryApi.listTickets.mockResolvedValue([
      ticketFixture({ id: "t1", displayId: "ISK-1000", status: "todo", acquisitionRunId: "run-1" }),
    ]);
    industryApi.listAcquisitionRuns.mockResolvedValue([runFixture()]);
    industryApi.listPriceSources.mockResolvedValue([{ id: "source-1", name: "Jita 4-4" } as never]);

    renderPage();
    await screen.findByText("Tuesday Jita Run");

    await userEvent.click(screen.getByText("Tuesday Jita Run"));

    expect(await screen.findByRole("heading", { name: "Tuesday Jita Run" })).toBeInTheDocument();
    expect(screen.getAllByText("ACQ-0042").length).toBeGreaterThan(0);

    await userEvent.click(screen.getByRole("button", { name: "Close acquisition run details" }));
    expect(screen.queryByRole("heading", { name: "Tuesday Jita Run" })).not.toBeInTheDocument();
  });

  it("opens the ticket detail drawer with the blocked-by list when a ticket with unmet dependencies is clicked", async () => {
    industryApi.listTickets.mockResolvedValue([
      ticketFixture({
        id: "t1",
        displayId: "ISK-1012",
        kind: "manufacturing",
        // A To Do ticket that still has an unmet prerequisite -- the two are
        // independent, and the drawer shows both.
        status: "todo",
        blockedBy: [
          {
            prerequisiteId: "prereq-1",
            kind: "buy",
            typeId: 37164,
            capturedName: "Isogen",
            outstandingQuantity: 50,
            representativeFulfillingTicketId: "ticket-2",
            representativeFulfillingTicketDisplayId: "ISK-2001",
            representativeFulfillingTicketStatus: "inProgress",
          },
        ],
      }),
    ]);

    renderPage();
    await screen.findByText("ISK-1012");

    await userEvent.click(screen.getByText("ISK-1012"));

    expect(await screen.findByRole("heading", { name: "Tritanium" })).toBeInTheDocument();
    expect(screen.getByText("Blocked by")).toBeInTheDocument();
    expect(screen.getByText("Isogen")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Close ticket details" }));
    expect(screen.queryByRole("heading", { name: "Tritanium" })).not.toBeInTheDocument();
  });

  it("surfaces the server's rejection (e.g. an incompatible acquisition location) inline in the dialog", async () => {
    industryApi.listTickets.mockResolvedValue([
      ticketFixture({ id: "t1", displayId: "ISK-1000", status: "todo" }),
      ticketFixture({ id: "t2", displayId: "ISK-1001", status: "todo", capturedName: "Isogen" }),
    ]);
    industryApi.listPriceSources.mockResolvedValue([{ id: "source-1", name: "Jita 4-4" } as never]);
    industryApi.createAcquisitionRun.mockRejectedValue(
      new ApiError(409, {
        code: "acquisition_run_crosses_incompatible_location",
        message: "selected tickets must share the same acquisition location",
      }),
    );

    renderPage();
    await screen.findByText("ACQ GROUP");

    await userEvent.click(screen.getByRole("button", { name: "Select" }));
    await userEvent.click(screen.getByRole("button", { name: /Expand .*Jita 4-4 To Do group/ }));
    await userEvent.click(screen.getByLabelText("Select ISK-1000"));
    await userEvent.click(screen.getByLabelText("Select ISK-1001"));
    await userEvent.click(screen.getByRole("button", { name: "Create Acquisition Run" }));
    await userEvent.click(screen.getByRole("button", { name: "Create Run" }));

    expect(await screen.findByText("selected tickets must share the same acquisition location")).toBeInTheDocument();
    // The dialog stays open with the selection intact so the user can adjust it.
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });
});

describe("BoardPage ticket drag-and-drop", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    industryApi.getAcquisitionRun.mockResolvedValue({ items: [] });
    industryApi.listOrders.mockResolvedValue([]);
    industryApi.listPriceSources.mockResolvedValue([]);
    charactersApi.listCharacters.mockResolvedValue([]);
    industryApi.listAcquisitionRuns.mockResolvedValue([]);
    industryApi.listBuilds.mockResolvedValue([]);
  });

  // A lane move is a bare `PATCH /api/tickets/:id { status }` and nothing
  // else. It never calls the /start, /complete, or /cancel domain commands,
  // so it has no inventory or cascade side effect, and it works in both
  // directions and regardless of any unmet dependency.

  it("moves a To Do ticket into In Progress with only a bare status PATCH", async () => {
    industryApi.listTickets
      .mockResolvedValueOnce([ticketFixture({ id: "t1", displayId: "ISK-1000", status: "todo", kind: "manufacturing" })])
      .mockResolvedValueOnce([
        ticketFixture({ id: "t1", displayId: "ISK-1000", status: "inProgress", kind: "manufacturing" }),
      ]);
    industryApi.updateTicketStatus.mockResolvedValue(
      ticketFixture({ id: "t1", displayId: "ISK-1000", status: "inProgress", kind: "manufacturing" }),
    );

    renderPage();
    await screen.findByText("ISK-1000");
    expect(within(screen.getByTestId("board-lane-todo")).getByText("ISK-1000")).toBeInTheDocument();

    dragTicketTo("ISK-1000", "board-lane-inProgress");

    await waitFor(() => expect(industryApi.updateTicketStatus).toHaveBeenCalledWith("t1", "inProgress"));
    expect(industryApi.startTicket).not.toHaveBeenCalled();
    expect(industryApi.completeTicket).not.toHaveBeenCalled();
    expect(industryApi.cancelTicket).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(within(screen.getByTestId("board-lane-inProgress")).getByText("ISK-1000")).toBeInTheDocument(),
    );
    expect(within(screen.getByTestId("board-lane-todo")).queryByText("ISK-1000")).not.toBeInTheDocument();
  });

  it("moves an In Progress ticket into Complete with only a bare status PATCH", async () => {
    industryApi.listTickets
      .mockResolvedValueOnce([
        ticketFixture({ id: "t1", displayId: "ISK-1000", status: "inProgress", kind: "manufacturing" }),
      ])
      .mockResolvedValueOnce([
        ticketFixture({ id: "t1", displayId: "ISK-1000", status: "complete", kind: "manufacturing" }),
      ]);
    industryApi.updateTicketStatus.mockResolvedValue(
      ticketFixture({ id: "t1", displayId: "ISK-1000", status: "complete", kind: "manufacturing" }),
    );

    renderPage();
    await screen.findByText("ISK-1000");

    dragTicketTo("ISK-1000", "board-lane-complete");

    await waitFor(() => expect(industryApi.updateTicketStatus).toHaveBeenCalledWith("t1", "complete"));
    expect(industryApi.completeTicket).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(within(screen.getByTestId("board-lane-complete")).getByText("ISK-1000")).toBeInTheDocument(),
    );
  });

  it("moves a Complete ticket backward into In Progress -- reversible, no history rewrite", async () => {
    industryApi.listTickets
      .mockResolvedValueOnce([
        ticketFixture({ id: "t1", displayId: "ISK-1000", status: "complete", kind: "manufacturing" }),
      ])
      .mockResolvedValueOnce([
        ticketFixture({ id: "t1", displayId: "ISK-1000", status: "inProgress", kind: "manufacturing" }),
      ]);
    industryApi.updateTicketStatus.mockResolvedValue(
      ticketFixture({ id: "t1", displayId: "ISK-1000", status: "inProgress", kind: "manufacturing" }),
    );

    renderPage();
    await screen.findByText("ISK-1000");
    expect(within(screen.getByTestId("board-lane-complete")).getByText("ISK-1000")).toBeInTheDocument();

    dragTicketTo("ISK-1000", "board-lane-inProgress");

    await waitFor(() => expect(industryApi.updateTicketStatus).toHaveBeenCalledWith("t1", "inProgress"));
    expect(industryApi.startTicket).not.toHaveBeenCalled();
    expect(industryApi.completeTicket).not.toHaveBeenCalled();
    expect(industryApi.cancelTicket).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(within(screen.getByTestId("board-lane-inProgress")).getByText("ISK-1000")).toBeInTheDocument(),
    );
    expect(within(screen.getByTestId("board-lane-complete")).queryByText("ISK-1000")).not.toBeInTheDocument();
  });

  it("moves a Complete ticket straight back to To Do", async () => {
    industryApi.listTickets
      .mockResolvedValueOnce([
        ticketFixture({ id: "t1", displayId: "ISK-1000", status: "complete", kind: "manufacturing" }),
      ])
      .mockResolvedValueOnce([
        ticketFixture({ id: "t1", displayId: "ISK-1000", status: "todo", kind: "manufacturing" }),
      ]);
    industryApi.updateTicketStatus.mockResolvedValue(
      ticketFixture({ id: "t1", displayId: "ISK-1000", status: "todo", kind: "manufacturing" }),
    );

    renderPage();
    await screen.findByText("ISK-1000");

    dragTicketTo("ISK-1000", "board-lane-todo");

    await waitFor(() => expect(industryApi.updateTicketStatus).toHaveBeenCalledWith("t1", "todo"));
    await waitFor(() =>
      expect(within(screen.getByTestId("board-lane-todo")).getByText("ISK-1000")).toBeInTheDocument(),
    );
  });

  it("highlights every other rendered lane while dragging (both directions)", async () => {
    industryApi.listTickets.mockResolvedValue([
      ticketFixture({ id: "t1", displayId: "ISK-1000", status: "inProgress", kind: "manufacturing" }),
    ]);

    renderPage();
    await screen.findByText("ISK-1000");

    const card = screen.getByText("ISK-1000").closest("[draggable]") as HTMLElement;
    fireEvent.dragStart(card, { dataTransfer: makeDataTransfer() });

    expect(screen.getByTestId("board-lane-todo")).toHaveAttribute("data-drag-eligible", "true");
    expect(screen.getByTestId("board-lane-complete")).toHaveAttribute("data-drag-eligible", "true");
    // The card's own lane is never a target.
    expect(screen.getByTestId("board-lane-inProgress")).toHaveAttribute("data-drag-eligible", "false");

    fireEvent.dragEnd(card);
  });

  it("allows a ticket with unmet dependencies to be dragged -- blockers never gate a move", async () => {
    const blocked = ticketFixture({
      id: "t1",
      displayId: "ISK-1000",
      status: "todo",
      kind: "manufacturing",
      blockedBy: [
        {
          prerequisiteId: "prereq-1",
          kind: "buy",
          typeId: 37164,
          capturedName: "Isogen",
          outstandingQuantity: 50,
          representativeFulfillingTicketId: null,
          representativeFulfillingTicketDisplayId: null,
          representativeFulfillingTicketStatus: null,
        },
      ],
    });
    industryApi.listTickets
      .mockResolvedValueOnce([blocked])
      .mockResolvedValueOnce([{ ...blocked, status: "complete" }]);
    industryApi.updateTicketStatus.mockResolvedValue({ ...blocked, status: "complete" });

    renderPage();
    await screen.findByText("ISK-1000");

    const card = screen.getByText("ISK-1000").closest("[draggable]") as HTMLElement;
    expect(card).toHaveAttribute("draggable", "true");

    dragTicketTo("ISK-1000", "board-lane-complete");

    await waitFor(() => expect(industryApi.updateTicketStatus).toHaveBeenCalledWith("t1", "complete"));
    await waitFor(() =>
      expect(within(screen.getByTestId("board-lane-complete")).getByText("ISK-1000")).toBeInTheDocument(),
    );
  });

  it("reverts the card to its original lane and shows the error when the status PATCH fails", async () => {
    industryApi.listTickets.mockResolvedValue([
      ticketFixture({ id: "t1", displayId: "ISK-1000", status: "todo", kind: "manufacturing" }),
    ]);
    industryApi.updateTicketStatus.mockRejectedValue(
      new ApiError(404, { code: "ticket_not_found", message: "ticket is no longer available" }),
    );

    renderPage();
    await screen.findByText("ISK-1000");

    dragTicketTo("ISK-1000", "board-lane-inProgress");

    expect(await screen.findByText("ticket is no longer available")).toBeInTheDocument();
    expect(within(screen.getByTestId("board-lane-todo")).getByText("ISK-1000")).toBeInTheDocument();
    expect(within(screen.getByTestId("board-lane-inProgress")).queryByText("ISK-1000")).not.toBeInTheDocument();
  });

  it("never renders a run-owned ticket as an individually draggable card", async () => {
    industryApi.listTickets.mockResolvedValue([
      ticketFixture({ id: "t1", displayId: "ISK-1000", status: "todo", acquisitionRunId: "run-1" }),
    ]);
    industryApi.listAcquisitionRuns.mockResolvedValue([runFixture()]);
    industryApi.listPriceSources.mockResolvedValue([{ id: "source-1", name: "Jita 4-4" } as never]);

    renderPage();
    await screen.findByText("Tuesday Jita Run");

    // The ticket only appears folded into the Run card (a <button>, not
    // draggable) -- its own card never renders on the Board, so it can't
    // bypass the Run's own lifecycle via drag.
    expect(screen.queryByText("ISK-1000")).not.toBeInTheDocument();
  });

  it("moves an unbatched Acquisition ticket dragged out of its ACQ group card with a bare status PATCH", async () => {
    // Every unbatched Acquisition ticket renders inside an ACQ GROUP card
    // (deriveOrderAcquisitionGroups groups even a single ticket), never as
    // a standalone OrderTicketCard -- so this is the realistic path for
    // the dominant ticket kind, not an edge case.
    industryApi.listTickets
      .mockResolvedValueOnce([
        ticketFixture({ id: "t1", displayId: "ISK-1000", status: "todo", priceSourceId: "source-1" }),
      ])
      .mockResolvedValueOnce([
        ticketFixture({ id: "t1", displayId: "ISK-1000", status: "inProgress", priceSourceId: "source-1" }),
      ]);
    industryApi.listPriceSources.mockResolvedValue([{ id: "source-1", name: "Jita 4-4" } as never]);
    industryApi.updateTicketStatus.mockResolvedValue(
      ticketFixture({ id: "t1", displayId: "ISK-1000", status: "inProgress", priceSourceId: "source-1" }),
    );

    renderPage();
    await screen.findByText("ACQ GROUP");
    await userEvent.click(screen.getByText("ACQ GROUP"));
    await screen.findByText("ISK-1000");

    dragTicketTo("ISK-1000", "board-lane-inProgress");

    await waitFor(() => expect(industryApi.updateTicketStatus).toHaveBeenCalledWith("t1", "inProgress"));
    expect(industryApi.startTicket).not.toHaveBeenCalled();
    // The moved ticket re-derives into a *new* ACQ GROUP card keyed by its
    // new lane (source-1:inProgress), which renders collapsed by default --
    // its capturedName shows in the collapsed preview even though its
    // displayId only shows once expanded.
    await waitFor(() =>
      expect(within(screen.getByTestId("board-lane-inProgress")).getByText("Tritanium")).toBeInTheDocument(),
    );
    expect(within(screen.getByTestId("board-lane-todo")).queryByText("Tritanium")).not.toBeInTheDocument();
  });

  it("re-renders every ticket from the reloaded list after a move, without predicting a cascade", async () => {
    // The generic status PATCH does not cascade dependent tickets
    // server-side, and the Board never predicts one client-side -- it just
    // faithfully repaints whatever the follow-up listTickets returns. Here
    // the dependent keeps its own workflow lane (To Do) and its derived
    // blocker indicator throughout.
    const dependent = ticketFixture({
      id: "t2",
      displayId: "ISK-1001",
      status: "todo",
      capturedName: "Isogen",
      kind: "manufacturing",
      blockedBy: [
        {
          prerequisiteId: "prereq-1",
          kind: "build",
          typeId: 34,
          capturedName: "Tritanium",
          outstandingQuantity: 100,
          representativeFulfillingTicketId: "t1",
          representativeFulfillingTicketDisplayId: "ISK-1000",
          representativeFulfillingTicketStatus: "inProgress",
        },
      ],
    });
    industryApi.listTickets
      .mockResolvedValueOnce([
        ticketFixture({ id: "t1", displayId: "ISK-1000", status: "inProgress", kind: "manufacturing" }),
        dependent,
      ])
      .mockResolvedValueOnce([
        ticketFixture({ id: "t1", displayId: "ISK-1000", status: "complete", kind: "manufacturing" }),
        dependent,
      ]);
    industryApi.updateTicketStatus.mockResolvedValue(
      ticketFixture({ id: "t1", displayId: "ISK-1000", status: "complete", kind: "manufacturing" }),
    );

    renderPage();
    await screen.findByText("ISK-1000");
    expect(within(screen.getByTestId("board-lane-todo")).getByText("Isogen")).toBeInTheDocument();

    dragTicketTo("ISK-1000", "board-lane-complete");

    await waitFor(() => expect(industryApi.updateTicketStatus).toHaveBeenCalledWith("t1", "complete"));
    await waitFor(() =>
      expect(within(screen.getByTestId("board-lane-complete")).getByText("ISK-1000")).toBeInTheDocument(),
    );
    // The dependent's own workflow lane is untouched -- no client-side cascade.
    expect(within(screen.getByTestId("board-lane-todo")).getByText("Isogen")).toBeInTheDocument();
  });
});

describe("BoardPage explicit recording integration", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    industryApi.getAcquisitionRun.mockResolvedValue({ items: [] });
    industryApi.listOrders.mockResolvedValue([]);
    industryApi.listPriceSources.mockResolvedValue([]);
    charactersApi.listCharacters.mockResolvedValue([]);
    industryApi.listAcquisitionRuns.mockResolvedValue([]);
    industryApi.listBuilds.mockResolvedValue([]);
  });

  const snapshot = {
    runs: 100,
    blueprint: null,
    facility: null,
    durationSeconds: 7200,
    installationCost: {
      complete: true,
      estimatedItemValue: null,
      systemCostIndex: null,
      unmodifiedSystemIndexCost: null,
      jobCostReductionPercent: "0",
      systemIndexCost: null,
      facilityTax: null,
      sccSurcharge: null,
      allianceSurcharge: null,
      fixedSupplementalCost: "0",
      total: "1000.0000",
      warnings: [],
      formulaVersion: "test",
    },
    materialValue: "40000.0000",
  };

  const mfgTicket = (recording: TicketSummary["recording"]): TicketSummary =>
    ticketFixture({
      id: "mfg-1",
      displayId: "ISK-1000",
      kind: "manufacturing",
      typeId: 20185,
      capturedName: "Crystalline Carbonide Armor Plate",
      quantity: 1000,
      sourceBuildId: "build-1",
      status: "inProgress",
      executionSnapshot: snapshot,
      prerequisites: [
        {
          id: "prereq-1",
          ticketId: "mfg-1",
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
        },
      ],
      recording,
    });

  it("records production from the inspector, updating the summary and card without moving the ticket's lane", async () => {
    industryApi.listTickets
      .mockResolvedValueOnce([
        mfgTicket({
          state: "partiallyRecorded",
          requestedQuantity: 100,
          recordedQuantity: 40,
          remainingQuantity: 60,
          surplusQuantity: 0,
        }),
      ])
      .mockResolvedValue([
        mfgTicket({
          state: "recorded",
          requestedQuantity: 100,
          recordedQuantity: 100,
          remainingQuantity: 0,
          surplusQuantity: 0,
        }),
      ]);
    industryApi.recordTicketProduction.mockResolvedValue({});

    renderPage();

    // Card indicator before recording.
    const cardBefore = await screen.findByText("ISK-1000");
    expect(
      within(screen.getByTestId("board-lane-inProgress")).getByText("40 / 100"),
    ).toBeInTheDocument();

    // Open the inspector.
    await userEvent.click(cardBefore);
    const section = await screen.findByRole("region", { name: "Recording" });
    expect(within(section).getByText("Partially recorded")).toBeInTheDocument();
    expect(within(section).getAllByRole("definition").map((dd) => dd.textContent)).toEqual([
      "100",
      "40",
      "60",
    ]);

    // Open Record production; the form defaults to the remaining 60 runs.
    await userEvent.click(within(section).getByRole("button", { name: "Record production" }));
    const runsField = screen.getByRole("spinbutton", { name: "Runs completed" });
    expect(runsField).toHaveValue(60);

    // Edit an actual input and the installation cost, then submit.
    const material = screen.getByRole("spinbutton", { name: "Consumed quantity for Tritanium" });
    await userEvent.clear(material);
    await userEvent.type(material, "615");
    const install = screen.getByRole("textbox", { name: "Installation cost (ISK)" });
    await userEvent.clear(install);
    await userEvent.type(install, "50");
    await userEvent.tab();
    await userEvent.click(screen.getByRole("button", { name: "Record" }));

    await waitFor(() => expect(industryApi.recordTicketProduction).toHaveBeenCalledTimes(1));
    expect(industryApi.recordTicketProduction.mock.calls[0][1]).toMatchObject({
      runsCompleted: 60,
      output: { typeId: 20185, quantity: 600 },
      inputs: [{ typeId: 34, quantity: 615 }],
      installationCost: "50",
    });

    // Summary now reads Recorded; the card indicator follows.
    const updatedSection = await screen.findByRole("region", { name: "Recording" });
    await waitFor(() =>
      expect(within(updatedSection).getByText("Recorded")).toBeInTheDocument(),
    );
    expect(
      within(screen.getByTestId("board-lane-inProgress")).getByText("100 / 100"),
    ).toBeInTheDocument();

    // Workflow status is unchanged: still In Progress, still in that lane.
    expect(
      within(screen.getByTestId("board-lane-inProgress")).getByText("ISK-1000"),
    ).toBeInTheDocument();
    expect(industryApi.completeTicket).not.toHaveBeenCalled();
    expect(industryApi.startTicket).not.toHaveBeenCalled();
    expect(industryApi.updateTicketStatus).not.toHaveBeenCalled();
  });
});

describe("BoardPage filters + URL state", () => {
  const orderA = orderFixture({ id: "order-a", displayName: "Weekend Production" });
  const orderB = orderFixture({ id: "order-b", displayName: "Ion Batch" });
  const valka = { connectionId: "char-valka", characterName: "Valka" } as never;
  const freya = { connectionId: "char-freya", characterName: "Freya" } as never;

  const tAcqA = ticketFixture({
    id: "t-acq-a",
    displayId: "ISK-9001",
    kind: "acquisition",
    orderId: "order-a",
    assigneeCharacterId: "char-valka",
    capturedName: "Tritanium",
  });
  const tManA = ticketFixture({
    id: "t-man-a",
    displayId: "ISK-9002",
    kind: "manufacturing",
    typeId: 645,
    quantity: 1,
    orderId: "order-a",
    assigneeCharacterId: "char-freya",
    capturedName: "Ishtar Hull",
  });
  const tGenA = ticketFixture({
    id: "t-gen-a",
    displayId: "ISK-9003",
    kind: "generic",
    typeId: null,
    quantity: null,
    orderId: "order-a",
    assigneeCharacterId: null,
    capturedName: "Move blueprints",
  });
  const tManB = ticketFixture({
    id: "t-man-b",
    displayId: "ISK-9004",
    kind: "manufacturing",
    typeId: 24,
    quantity: 1,
    orderId: "order-b",
    assigneeCharacterId: "char-valka",
    capturedName: "Cerberus Hull",
  });
  const tGenStandalone = ticketFixture({
    id: "t-gen-standalone",
    displayId: "ISK-9005",
    kind: "generic",
    typeId: null,
    quantity: null,
    orderId: null,
    assigneeCharacterId: "char-valka",
    capturedName: "Standalone task",
  });

  beforeEach(() => {
    vi.clearAllMocks();
    industryApi.getAcquisitionRun.mockResolvedValue({ items: [] });
    industryApi.listPriceSources.mockResolvedValue([]);
    industryApi.listAcquisitionRuns.mockResolvedValue([]);
    industryApi.listBuilds.mockResolvedValue([]);
    industryApi.listOrders.mockResolvedValue([orderA, orderB]);
    industryApi.listTickets.mockResolvedValue([tAcqA, tManA, tGenA, tManB, tGenStandalone]);
    charactersApi.listCharacters.mockResolvedValue([valka, freya]);
  });

  // -- URL state -----------------------------------------------------------

  it("initializes every control from the URL on load", async () => {
    renderPageWithLocation("/board?epicFilter=order-a&assigneeFilter=char-valka&typeFilter=manufacturing&q=hull");
    await boardLanes();

    expect(screen.getByLabelText("Epic")).toHaveValue("order-a");
    expect(screen.getByLabelText("Assignee")).toHaveValue("char-valka");
    expect(screen.getByLabelText("Ticket type")).toHaveValue("manufacturing");
    expect(screen.getByLabelText("Search tickets")).toHaveValue("hull");
  });

  it("updates the URL as each control changes, without remounting the Board", async () => {
    renderPageWithLocation();
    await boardLanes();

    await userEvent.selectOptions(screen.getByLabelText("Epic"), "order-a");
    await waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent("epicFilter=order-a"));

    await userEvent.selectOptions(screen.getByLabelText("Epic"), "none");
    await waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent("epicFilter=none"));

    await userEvent.selectOptions(screen.getByLabelText("Assignee"), "unassigned");
    await waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent("assigneeFilter=unassigned"));

    await userEvent.selectOptions(screen.getByLabelText("Ticket type"), "reaction");
    await waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent("typeFilter=reaction"));

    await userEvent.type(screen.getByLabelText("Search tickets"), "ion");
    await waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent("q=ion"));

    // Every param landed together -- nothing overwrote a sibling filter.
    const location = screen.getByTestId("location").textContent ?? "";
    expect(location).toContain("epicFilter=none");
    expect(location).toContain("assigneeFilter=unassigned");
    expect(location).toContain("typeFilter=reaction");
    expect(location).toContain("q=ion");
  });

  it("Clear filters resets every control and removes every filter param from the URL", async () => {
    renderPageWithLocation("/board?epicFilter=order-a&assigneeFilter=char-valka&typeFilter=manufacturing&q=hull");
    await boardLanes();

    // Both the toolbar's own Clear filters and the empty-filter-result
    // banner's are on screen at once here (these filters match nothing) --
    // either does the same thing.
    await userEvent.click(screen.getAllByRole("button", { name: "Clear filters" })[0]);

    expect(screen.getByLabelText("Epic")).toHaveValue("");
    expect(screen.getByLabelText("Assignee")).toHaveValue("");
    expect(screen.getByLabelText("Ticket type")).toHaveValue("");
    expect(screen.getByLabelText("Search tickets")).toHaveValue("");
    const location = screen.getByTestId("location").textContent ?? "";
    expect(location).not.toContain("epicFilter");
    expect(location).not.toContain("assigneeFilter");
    expect(location).not.toContain("typeFilter");
    expect(location).not.toContain("q=");
    expect(screen.queryByRole("button", { name: "Clear filters" })).not.toBeInTheDocument();
  });

  it("the legacy ?epic= Epic-Inspector deep-link keeps opening the Epic Inspector, untouched by the new distinct filter params", async () => {
    industryApi.getOrder.mockResolvedValue(orderDetailFixture({ id: "order-a", displayName: "Weekend Production" }));
    renderPageWithLocation("/board?epic=order-a");

    expect(await screen.findByRole("complementary", { name: "Weekend Production" })).toBeInTheDocument();
    // Not interpreted as a Board filter -- every ticket still shows.
    expect(screen.getByLabelText("Epic")).toHaveValue("");
    expect(screen.queryByRole("button", { name: "Clear filters" })).not.toBeInTheDocument();
  });

  // -- Combined filtering + Epic-card behavior ------------------------------

  it("combines Epic + Assignee + Type + Search with AND semantics", async () => {
    renderPage();
    const lanes = await boardLanes();
    lanes.getByText("Tritanium"); // sanity: visible unfiltered

    await userEvent.selectOptions(screen.getByLabelText("Epic"), "order-a");
    await userEvent.selectOptions(screen.getByLabelText("Assignee"), "char-valka");

    expect(lanes.getByText("Tritanium")).toBeInTheDocument();
    expect(lanes.queryByText("Ishtar Hull")).not.toBeInTheDocument();
    expect(lanes.queryByText("Move blueprints")).not.toBeInTheDocument();
    expect(lanes.queryByText("Cerberus Hull")).not.toBeInTheDocument();
    expect(lanes.queryByText("Standalone task")).not.toBeInTheDocument();

    await userEvent.selectOptions(screen.getByLabelText("Ticket type"), "manufacturing");
    expect(lanes.queryByText("Tritanium")).not.toBeInTheDocument();
    expect(screen.getByText("No tickets match these filters.")).toBeInTheDocument();

    await userEvent.selectOptions(screen.getByLabelText("Ticket type"), "");
    await userEvent.type(screen.getByLabelText("Search tickets"), "trit");
    expect(lanes.getByText("Tritanium")).toBeInTheDocument();
    expect(lanes.queryByText("Move blueprints")).not.toBeInTheDocument();
  });

  it("Assignee filtering shows an Epic card only for Epics with a matching child Ticket, and No Epic shows none", async () => {
    renderPage();
    const lanes = await boardLanes();

    await userEvent.selectOptions(screen.getByLabelText("Assignee"), "char-valka");
    // Both Epic A (t-acq-a) and Epic B (t-man-b) have a Valka ticket.
    expect(lanes.getByText("Weekend Production")).toBeInTheDocument();
    expect(lanes.getByText("Ion Batch")).toBeInTheDocument();

    await userEvent.selectOptions(screen.getByLabelText("Assignee"), "char-freya");
    // Only Epic A has a Freya ticket (t-man-a).
    expect(lanes.getByText("Weekend Production")).toBeInTheDocument();
    expect(lanes.queryByText("Ion Batch")).not.toBeInTheDocument();

    await userEvent.selectOptions(screen.getByLabelText("Assignee"), "unassigned");
    await userEvent.selectOptions(screen.getByLabelText("Epic"), "none");
    // No Epic shows no Epic cards at all, even though a standalone
    // Unassigned Ticket exists -- Epic cards never satisfy "no Epic".
    expect(lanes.queryByText("Weekend Production")).not.toBeInTheDocument();
    expect(lanes.queryByText("Ion Batch")).not.toBeInTheDocument();
  });

  // -- Persistence across Board operations ----------------------------------

  it("keeps filters unchanged across Epic -> Ticket -> Back -> Close", async () => {
    industryApi.getOrder.mockResolvedValue(orderDetailFixture({ id: "order-a", displayName: "Weekend Production" }));
    renderPage();
    const lanes = await boardLanes();
    const toolbar = await boardToolbar();

    await userEvent.selectOptions(toolbar.getByLabelText("Assignee"), "char-valka");
    await userEvent.click(lanes.getByText("Weekend Production"));
    expect(await screen.findByRole("complementary", { name: "Weekend Production" })).toBeInTheDocument();

    const inspector = screen.getByRole("complementary", { name: "Weekend Production" });
    await userEvent.click(within(inspector).getByText("ISK-9001"));
    expect(await screen.findByRole("complementary", { name: "Tritanium" })).toBeInTheDocument();
    expect(toolbar.getByLabelText("Assignee")).toHaveValue("char-valka");

    await userEvent.click(screen.getByRole("button", { name: "Back to Epic" }));
    expect(await screen.findByRole("complementary", { name: "Weekend Production" })).toBeInTheDocument();
    expect(toolbar.getByLabelText("Assignee")).toHaveValue("char-valka");

    await userEvent.click(screen.getByRole("button", { name: "Close Epic inspector" }));
    expect(toolbar.getByLabelText("Assignee")).toHaveValue("char-valka");
  });

  it("reassigning a ticket away from the active Assignee filter keeps the Inspector open and the card disappears, filter unchanged", async () => {
    const reassigned = { ...tGenStandalone, assigneeCharacterId: "char-freya" };
    industryApi.listTickets.mockResolvedValueOnce([tAcqA, tManA, tGenA, tManB, tGenStandalone]).mockResolvedValue([
      tAcqA,
      tManA,
      tGenA,
      tManB,
      reassigned,
    ]);
    industryApi.updateTicketMetadata.mockResolvedValue(reassigned);

    renderPage();
    const toolbar = await boardToolbar();
    await userEvent.selectOptions(toolbar.getByLabelText("Assignee"), "char-valka");
    const lanes = await boardLanes();
    await userEvent.click(await lanes.findByText("Standalone task"));

    const inspector = await screen.findByRole("complementary", { name: "Standalone task" });
    await userEvent.selectOptions(within(inspector).getByLabelText("Assignee"), "char-freya");
    await waitFor(() => expect(industryApi.updateTicketMetadata).toHaveBeenCalledWith("t-gen-standalone", { assigneeCharacterId: "char-freya" }));

    // Inspector stays open -- filtering never auto-closes it.
    expect(screen.getByRole("complementary", { name: "Standalone task" })).toBeInTheDocument();
    // Card disappears from the filtered lanes once the reload lands.
    await waitFor(() => expect(lanes.queryByText("Standalone task")).not.toBeInTheDocument());
    // The URL filter itself is untouched.
    expect(toolbar.getByLabelText("Assignee")).toHaveValue("char-valka");
  });

  it("creating a standalone Ticket under an active Epic filter opens its Inspector without rewriting the filter, and the new card stays filtered out", async () => {
    const created = ticketFixture({
      id: "new-ticket",
      displayId: "ISK-9999",
      kind: "generic",
      typeId: null,
      quantity: null,
      orderId: null,
      capturedName: "New standalone ticket",
    });
    industryApi.listTickets
      .mockResolvedValueOnce([tAcqA, tManA, tGenA, tManB, tGenStandalone])
      .mockResolvedValue([tAcqA, tManA, tGenA, tManB, tGenStandalone, created]);
    industryApi.createTicket.mockResolvedValue(created);

    renderPage();
    const toolbar = await boardToolbar();
    await userEvent.selectOptions(toolbar.getByLabelText("Epic"), "order-a");
    const lanes = await boardLanes();

    await userEvent.click(toolbar.getByRole("button", { name: "Create ticket" }));
    const editor = screen.getByRole("complementary", { name: "Create ticket" });
    await userEvent.type(
      within(editor).getByPlaceholderText("e.g. Move blueprints to C-J6MT"),
      "New standalone ticket",
    );
    await userEvent.click(within(editor).getByRole("button", { name: "Create ticket" }));

    expect(await screen.findByRole("complementary", { name: "New standalone ticket" })).toBeInTheDocument();
    // The Epic filter was never silently rewritten to make the new
    // (No-Epic) ticket visible.
    expect(toolbar.getByLabelText("Epic")).toHaveValue("order-a");
    expect(lanes.queryByText("New standalone ticket")).not.toBeInTheDocument();
  });

  // -- Empty state -----------------------------------------------------------

  it("shows a dedicated empty-filter message (not a generic empty-board message) only when filters exclude everything", async () => {
    renderPage();
    await boardLanes();
    expect(screen.queryByText("No tickets match these filters.")).not.toBeInTheDocument();

    await userEvent.selectOptions(screen.getByLabelText("Epic"), "order-a");
    await userEvent.selectOptions(screen.getByLabelText("Assignee"), "char-freya");
    await userEvent.selectOptions(screen.getByLabelText("Ticket type"), "acquisition");

    expect(await screen.findByText("No tickets match these filters.")).toBeInTheDocument();
    await userEvent.click(screen.getAllByRole("button", { name: "Clear filters" })[0]);
    expect(screen.queryByText("No tickets match these filters.")).not.toBeInTheDocument();
  });
});
