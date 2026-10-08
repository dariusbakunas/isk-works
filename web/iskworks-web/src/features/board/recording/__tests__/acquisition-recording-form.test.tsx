import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { TicketSummary } from "../../../../api/industry";
import { AcquisitionRecordingForm } from "../acquisition-recording-form";

const api = vi.hoisted(() => ({
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

function quantityInput(): HTMLInputElement {
  return screen.getByRole("spinbutton", { name: "Quantity" });
}

describe("AcquisitionRecordingForm", () => {
  beforeEach(() => vi.clearAllMocks());

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
    expect(screen.getByRole("button", { name: "Record" })).toBeDisabled();

    await userEvent.type(quantityInput(), "25");
    expect(screen.getByRole("button", { name: "Record" })).toBeEnabled();
    await userEvent.click(screen.getByRole("button", { name: "Record" }));

    await waitFor(() => expect(api.recordTicketAcquisition).toHaveBeenCalledTimes(1));
    expect(api.recordTicketAcquisition.mock.calls[0][1]).toMatchObject({ quantity: 25 });
  });

  it("records via record-acquisition only and never touches workflow status", async () => {
    api.recordTicketAcquisition.mockResolvedValue({});
    const onRecorded = vi.fn();
    render(
      <AcquisitionRecordingForm onCancel={() => {}} onRecorded={onRecorded} ticket={ticket()} />,
    );

    await userEvent.click(screen.getByRole("button", { name: "Record" }));

    await waitFor(() => expect(onRecorded).toHaveBeenCalled());
    expect(api.recordTicketAcquisition).toHaveBeenCalledWith(
      "ticket-1",
      expect.objectContaining({ quantity: 60, idempotencyKey: expect.any(String) }),
    );
    expect(api.startTicket).not.toHaveBeenCalled();
    expect(api.completeTicket).not.toHaveBeenCalled();
    expect(api.updateTicketStatus).not.toHaveBeenCalled();
  });

  it("keeps the form and entered values on failure", async () => {
    api.recordTicketAcquisition.mockRejectedValue(new Error("a cost is required to record this purchase"));
    render(<AcquisitionRecordingForm onCancel={() => {}} onRecorded={() => {}} ticket={ticket()} />);

    await userEvent.clear(quantityInput());
    await userEvent.type(quantityInput(), "17");
    await userEvent.click(screen.getByRole("button", { name: "Record" }));

    expect(await screen.findByText("a cost is required to record this purchase")).toBeInTheDocument();
    expect(quantityInput()).toHaveValue(17);
    expect(screen.getByRole("button", { name: "Record" })).toBeEnabled();
  });

  it("reuses the idempotency key on retry, then a fresh key for the next recording", async () => {
    api.recordTicketAcquisition
      .mockRejectedValueOnce(new Error("network blip"))
      .mockResolvedValueOnce({})
      .mockResolvedValueOnce({});
    render(<AcquisitionRecordingForm onCancel={() => {}} onRecorded={() => {}} ticket={ticket()} />);

    // First attempt fails.
    await userEvent.click(screen.getByRole("button", { name: "Record" }));
    await screen.findByText("network blip");
    // Retry (same submission intent) succeeds.
    await userEvent.click(screen.getByRole("button", { name: "Record" }));
    await waitFor(() => expect(api.recordTicketAcquisition).toHaveBeenCalledTimes(2));
    // A brand-new recording after success.
    await userEvent.type(quantityInput(), "5");
    await userEvent.click(screen.getByRole("button", { name: "Record" }));
    await waitFor(() => expect(api.recordTicketAcquisition).toHaveBeenCalledTimes(3));

    const keys = api.recordTicketAcquisition.mock.calls.map((call) => call[1].idempotencyKey);
    expect(keys[0]).toBe(keys[1]); // retry reuses the key
    expect(keys[2]).not.toBe(keys[1]); // next recording gets a fresh key
  });
});
