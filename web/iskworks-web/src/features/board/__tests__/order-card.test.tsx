import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { describe, expect, it, vi } from "vitest";

import type { OrderSummary } from "../../../api/industry";
import { OrderCard } from "../order-card";

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

function renderCard(order: OrderSummary, onOpen?: (orderId: string) => void) {
  return render(
    <MemoryRouter>
      <OrderCard onOpen={onOpen} order={order} />
    </MemoryRouter>,
  );
}

describe("OrderCard", () => {
  // Regression: an Epic card must open the Epic Inspector in place, never
  // navigate to a separate Order page -- the Board is the
  // workspace and inspecting stays on it.
  it("opens the Epic Inspector instead of navigating, and shows EPIC (not ORDER)", async () => {
    const onOpen = vi.fn();
    const user = userEvent.setup();
    renderCard(orderFixture(), onOpen);

    expect(screen.queryByRole("link")).not.toBeInTheDocument();
    expect(screen.getByText("EPIC")).toBeInTheDocument();

    await user.click(screen.getByRole("button"));

    expect(onOpen).toHaveBeenCalledWith("order-1");
  });

  it("shows needs-action count only when greater than zero", () => {
    renderCard(orderFixture({ rollup: { satisfied: 3, needsAction: 0, inProgress: 0, total: 3 } }));

    expect(screen.getByText("3/3 satisfied")).toBeInTheDocument();
    expect(screen.queryByText(/need action/)).not.toBeInTheDocument();
  });

  it("shows a completion summary instead of a progress bar once complete", () => {
    renderCard(
      orderFixture({
        status: "complete",
        rollup: { satisfied: 4, needsAction: 0, inProgress: 0, total: 4 },
      }),
    );

    expect(screen.getByText("All 4 dependencies satisfied")).toBeInTheDocument();
  });
});
