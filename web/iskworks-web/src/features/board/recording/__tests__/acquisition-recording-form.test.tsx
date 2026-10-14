import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { TicketSummary } from "../../../../api/industry";
import { AcquisitionRecordingForm } from "../acquisition-recording-form";

const api = vi.hoisted(() => ({
  previewTicketAcquisition: vi.fn(),
  recordTicketAcquisition: vi.fn(),
  recordTicketProduction: vi.fn(),
  startTicket: vi.fn(),
  completeTicket: vi.fn(),
  updateTicketStatus: vi.fn(),
}));

vi.mock("../../../../api/industry", async () => {
  const actual =
    await vi.importActual<typeof import("../../../../api/industry")>("../../../../api/industry");
  return { ...actual, ...api };
});

function ticket(overrides: Partial<TicketSummary> = {}): TicketSummary {
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
    priceSourceId: null,
    acquisitionRunId: null,
    acquiredQuantity: null,
    executionSnapshot: null,
    createdAt: "2026-09-01T00:00:00Z",
    updatedAt: "2026-09-01T00:00:00Z",
    completedAt: null,
    archivedAt: null,
    blockedBy: [],
    prerequisites: [],
    recording: {
      state: "partiallyRecorded",
      requestedQuantity: 100,
      recordedQuantity: 40,
      remainingQuantity: 60,
      surplusQuantity: 0,
    },
    ...overrides,
  };
}

function balance(quantity: number, total: string, average: string | null) {
  return {
    key: { workspaceId: "workspace-1", ownerId: "owner-1", typeId: 34 },
    typeName: "Tritanium",
    quantity,
    totalHistoricalCost: total,
    averageUnitCost: average,
    revision: 1,
    lastActivityAt: null,
  };
}

const PREVIEW = {
  lines: [
    {
      typeId: 34,
      typeName: "Tritanium",
      effect: "added",
      current: balance(1_000, "4000.0000", "4.0000"),
      resulting: balance(1_060, "4360.0000", "4.1132"),
    },
  ],
};

async function previewThenRecord() {
  await userEvent.click(screen.getByRole("button", { name: "Preview" }));
  await userEvent.click(await screen.findByRole("button", { name: "Record" }));
}

function quantityInput(): HTMLInputElement {
  return screen.getByRole("spinbutton", { name: "Quantity" });
}

describe("AcquisitionRecordingForm", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.previewTicketAcquisition.mockResolvedValue(PREVIEW);
  });

  it("previews the inventory change before recording anything", async () => {
    api.recordTicketAcquisition.mockResolvedValue({});
    render(<AcquisitionRecordingForm onCancel={() => {}} onRecorded={() => {}} ticket={ticket()} />);

    await userEvent.click(screen.getByRole("button", { name: "Preview" }));

    expect(await screen.findByText("Inventory after recording")).toBeInTheDocument();
    expect(
      screen.getByText((_, element) => element?.tagName === "DD" && element.textContent === "1,000 → 1,060"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("average-cost-change")).toBeInTheDocument();
    expect(api.previewTicketAcquisition).toHaveBeenCalledWith(
      "ticket-1",
      expect.objectContaining({ quantity: 60 }),
    );
    expect(api.recordTicketAcquisition).not.toHaveBeenCalled();

    // Back keeps what was entered.
    await userEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(quantityInput()).toHaveValue(60);
  });

  it("defaults the quantity to the remaining amount", () => {
    render(<AcquisitionRecordingForm onCancel={() => {}} onRecorded={() => {}} ticket={ticket()} />);
    expect(quantityInput()).toHaveValue(60);
  });

  it("starts blank when nothing remains, and still allows recording surplus", async () => {
    api.recordTicketAcquisition.mockResolvedValue({});
    render(
      <AcquisitionRecordingForm
        onCancel={() => {}}
        onRecorded={() => {}}
        ticket={ticket({
          recording: {
            state: "recorded",
            requestedQuantity: 100,
            recordedQuantity: 100,
            remainingQuantity: 0,
            surplusQuantity: 0,
          },
        })}
      />,
    );

    expect(quantityInput()).toHaveValue(null);
    expect(screen.getByRole("button", { name: "Preview" })).toBeDisabled();

    await userEvent.type(quantityInput(), "25");
    expect(screen.getByRole("button", { name: "Preview" })).toBeEnabled();
    await previewThenRecord();

    await waitFor(() => expect(api.recordTicketAcquisition).toHaveBeenCalledTimes(1));
    expect(api.recordTicketAcquisition.mock.calls[0][1]).toMatchObject({ quantity: 25 });
  });

  it("records via record-acquisition only and never touches workflow status", async () => {
    api.recordTicketAcquisition.mockResolvedValue({});
    const onRecorded = vi.fn();
    render(
      <AcquisitionRecordingForm onCancel={() => {}} onRecorded={onRecorded} ticket={ticket()} />,
    );

    await previewThenRecord();

    await waitFor(() => expect(onRecorded).toHaveBeenCalled());
    expect(api.recordTicketAcquisition).toHaveBeenCalledWith(
      "ticket-1",
      expect.objectContaining({ quantity: 60, idempotencyKey: expect.any(String) }),
    );
    expect(api.startTicket).not.toHaveBeenCalled();
    expect(api.completeTicket).not.toHaveBeenCalled();
    expect(api.updateTicketStatus).not.toHaveBeenCalled();
  });

  it("keeps the form and entered values when the preview fails", async () => {
    api.previewTicketAcquisition.mockRejectedValue(new Error("a cost is required to record this purchase"));
    render(<AcquisitionRecordingForm onCancel={() => {}} onRecorded={() => {}} ticket={ticket()} />);

    await userEvent.clear(quantityInput());
    await userEvent.type(quantityInput(), "17");
    await userEvent.click(screen.getByRole("button", { name: "Preview" }));

    expect(await screen.findByText("a cost is required to record this purchase")).toBeInTheDocument();
    expect(quantityInput()).toHaveValue(17);
    expect(screen.getByRole("button", { name: "Preview" })).toBeEnabled();
  });

  it("reuses the idempotency key on retry, then a fresh key for the next recording", async () => {
    api.recordTicketAcquisition
      .mockRejectedValueOnce(new Error("network blip"))
      .mockResolvedValueOnce({})
      .mockResolvedValueOnce({});
    render(<AcquisitionRecordingForm onCancel={() => {}} onRecorded={() => {}} ticket={ticket()} />);

    // First attempt fails; the preview stays up with the error.
    await previewThenRecord();
    await screen.findByText("network blip");
    // Retry (same submission intent) succeeds.
    await userEvent.click(screen.getByRole("button", { name: "Record" }));
    await waitFor(() => expect(api.recordTicketAcquisition).toHaveBeenCalledTimes(2));
    // A brand-new recording after success.
    await userEvent.click(screen.getByRole("button", { name: "Back" }));
    await userEvent.type(quantityInput(), "5");
    await previewThenRecord();
    await waitFor(() => expect(api.recordTicketAcquisition).toHaveBeenCalledTimes(3));

    const keys = api.recordTicketAcquisition.mock.calls.map((call) => call[1].idempotencyKey);
    expect(keys[0]).toBe(keys[1]); // retry reuses the key
    expect(keys[2]).not.toBe(keys[1]); // next recording gets a fresh key
  });
});
