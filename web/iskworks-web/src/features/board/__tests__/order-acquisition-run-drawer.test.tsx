import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { AcquisitionRun, MarketScope, Ticket } from "../../../api/industry";

const industryApi = vi.hoisted(() => ({
  getAcquisitionRun: vi.fn(),
  startAcquisitionRun: vi.fn(),
  recordAcquisitionProgress: vi.fn(),
  completeAcquisitionRun: vi.fn(),
}));

vi.mock("../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../api/industry")>("../../../api/industry");
  return { ...actual, ...industryApi };
});

vi.mock("../../../hooks/use-market-scope-label", () => ({
  useMarketScopeLabel: (scope: MarketScope | null) =>
    scope === null
      ? { regionName: "", locationName: "" }
      : {
          regionName: "The Forge",
          locationName:
            scope.locationId === undefined
              ? "All locations"
              : "Jita IV - Moon 4 - Caldari Navy Assembly Plant",
        },
}));

import { OrderAcquisitionRunDrawer } from "../order-acquisition-run-drawer";
import { runFixture } from "./fixtures";

function ticketFixture(overrides: Partial<Ticket> = {}): Ticket {
  return {
    id: "ticket-1",
    workspaceId: "workspace-1",
    ownerId: "owner-1",
    displayId: "ISK-2000",
    kind: "acquisition",
    typeId: 16_272,
    capturedName: "Zydrine",
    quantity: 1_000,
    orderId: null,
    notes: "",
    assigneeCharacterId: null,
    sourceBuildId: null,
    status: "todo",
    estimatedUnitCost: "990.0000",
    estimatedLineTotal: "990000.0000",
    actualUnitCost: null,
    actualLineTotal: null,
    marketRegionId: null,
    marketLocationId: null,
    priceSourceId: "source-1",
    acquisitionRunId: "run-1",
    acquiredQuantity: 0,
    executionSnapshot: null,
    createdAt: "2026-08-22T00:00:00Z",
    updatedAt: "2026-08-22T00:00:00Z",
    archivedAt: null,
    ...overrides,
  };
}

function renderDrawer(
  run: AcquisitionRun,
  tickets: Ticket[],
  overrides: { onClose?: () => void; onChanged?: () => void } = {},
) {
  return render(
    <OrderAcquisitionRunDrawer
      onChanged={overrides.onChanged ?? vi.fn()}
      onClose={overrides.onClose ?? vi.fn()}
      priceSourceName="Jita 4-4"
      run={run}
      tickets={tickets}
    />,
  );
}

describe("OrderAcquisitionRunDrawer", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    industryApi.getAcquisitionRun.mockResolvedValue({ items: [] });
  });

  it("shows the resolved manual price list", () => {
    renderDrawer(runFixture(), [ticketFixture()]);

    expect(screen.getByText("Price list: Jita 4-4")).toBeInTheDocument();
  });

  it("shows a market-scoped Run's scope, never 'Unknown source'", () => {
    renderDrawer(
      runFixture({ marketRegionId: 10_000_002, marketLocationId: 60_003_760, priceSourceId: null }),
      [ticketFixture()],
    );

    expect(screen.getByText("Priced at Jita IV - Moon 4 - Caldari Navy Assembly Plant")).toBeInTheDocument();
    expect(screen.queryByText("Unknown source")).not.toBeInTheDocument();
  });

  it("starts the Run directly with no preview/confirm step", async () => {
    industryApi.startAcquisitionRun.mockResolvedValue(runFixture({ status: "inProgress" }));
    const onChanged = vi.fn();

    renderDrawer(runFixture({ status: "ready" }), [ticketFixture()], { onChanged });

    await userEvent.click(screen.getByRole("button", { name: "Start Run" }));

    // No confirmation dialog/button ever appears -- Start calls the API
    // immediately, with no preview/confirm step.
    expect(screen.queryByRole("button", { name: /Confirm/ })).not.toBeInTheDocument();
    expect(industryApi.startAcquisitionRun).toHaveBeenCalledWith("run-1");
    expect(onChanged).toHaveBeenCalled();
  });

  it("allows recording progress above what's required, without clamping", async () => {
    industryApi.recordAcquisitionProgress.mockResolvedValue(runFixture({ status: "inProgress" }));
    const onChanged = vi.fn();

    renderDrawer(runFixture({ status: "inProgress" }), [ticketFixture({ quantity: 1_000, acquiredQuantity: 0 })], {
      onChanged,
    });

    const input = screen.getByLabelText("Acquired Zydrine");
    await userEvent.clear(input);
    await userEvent.type(input, "1500");
    await userEvent.click(screen.getByRole("button", { name: "Save Progress" }));

    expect(industryApi.recordAcquisitionProgress).toHaveBeenCalledWith("run-1", [
      { typeId: 16_272, acquiredQuantity: 1500 },
    ]);
    expect(onChanged).toHaveBeenCalled();
  });

  it("shows partial delivery as remaining-to-buy, not as complete", () => {
    renderDrawer(runFixture({ status: "inProgress" }), [
      ticketFixture({ quantity: 1_000, acquiredQuantity: 400 }),
    ]);

    expect(screen.getByText("Remaining to buy 600")).toBeInTheDocument();
  });

  it("shows surplus beyond what's needed as stock, not discarded", () => {
    renderDrawer(runFixture({ status: "inProgress" }), [
      ticketFixture({ quantity: 1_000, acquiredQuantity: 1_200 }),
    ]);

    expect(screen.getByText("On delivery: reserve 1,000 · to stock 200")).toBeInTheDocument();
  });

  it("completes the Run", async () => {
    industryApi.completeAcquisitionRun.mockResolvedValue(runFixture({ status: "complete" }));
    const onChanged = vi.fn();

    renderDrawer(runFixture({ status: "inProgress" }), [ticketFixture({ quantity: 1_000, acquiredQuantity: 1_000 })], {
      onChanged,
    });

    await userEvent.click(screen.getByRole("button", { name: "Complete Run" }));

    expect(industryApi.completeAcquisitionRun).toHaveBeenCalledWith("run-1");
    expect(onChanged).toHaveBeenCalled();
  });

  it("calls onClose when Close is clicked", async () => {
    const onClose = vi.fn();
    renderDrawer(runFixture({ status: "ready" }), [ticketFixture()], { onClose });

    await userEvent.click(screen.getByRole("button", { name: "Close acquisition run details" }));

    expect(onClose).toHaveBeenCalled();
  });
});
