import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  revertTicketInventoryRecording,
  type TicketInventoryRecording,
} from "../../../../api/industry";
import { RecordingHistory } from "../recording-history";

vi.mock("../../../../api/industry", async () => {
  const actual =
    await vi.importActual<typeof import("../../../../api/industry")>("../../../../api/industry");
  return { ...actual, revertTicketInventoryRecording: vi.fn() };
});

const revertMock = vi.mocked(revertTicketInventoryRecording);

function recording(
  overrides: Partial<TicketInventoryRecording> = {},
): TicketInventoryRecording {
  return {
    id: "recording-1",
    ticketId: "ticket-1",
    kind: "acquisition",
    recordedQuantity: 120,
    runsCompleted: null,
    installationCost: null,
    outputTypeId: null,
    outputQuantity: null,
    locationNote: "Jita 4-4",
    note: "wrong amount",
    recordedAt: "2026-09-25T14:03:00Z",
    revertedAt: null,
    status: "recorded",
    effects: [
      {
        eventId: "event-1",
        kind: "purchase",
        typeId: 34,
        capturedName: "Tritanium",
        quantityDelta: 120,
        totalCostDelta: "510.0000",
      },
    ],
    ...overrides,
  };
}

describe("RecordingHistory", () => {
  beforeEach(() => {
    revertMock.mockReset();
    revertMock.mockResolvedValue({} as Awaited<ReturnType<typeof revertTicketInventoryRecording>>);
  });

  it("keeps chronological original rows and replaces only the reversed row action with a badge", () => {
    render(
      <RecordingHistory
        onChanged={() => {}}
        recordings={[
          recording(),
          recording({
            id: "recording-2",
            recordedQuantity: 100,
            recordedAt: "2026-09-25T14:06:00Z",
            revertedAt: "2026-09-25T14:05:00Z",
            status: "reversed",
            effects: [
              {
                eventId: "event-2",
                kind: "purchase",
                typeId: 34,
                capturedName: "Tritanium",
                quantityDelta: 100,
                totalCostDelta: "425.0000",
              },
            ],
          }),
        ]}
        ticketId="ticket-1"
      />,
    );

    const rows = screen.getAllByRole("listitem");
    expect(rows).toHaveLength(2);
    expect(within(rows[0]).getByText("120 × Tritanium")).toBeInTheDocument();
    expect(within(rows[0]).getByRole("button", { name: "Revert recording" })).toBeInTheDocument();
    expect(within(rows[1]).getByText("100 × Tritanium")).toBeInTheDocument();
    expect(within(rows[1]).getByText("Reverted")).toBeInTheDocument();
    expect(within(rows[1]).queryByRole("button", { name: "Revert recording" })).toBeNull();
  });

  it("confirms the concrete immutable effects and refreshes after success", async () => {
    const changed = vi.fn();
    render(
      <RecordingHistory
        onChanged={changed}
        recordings={[recording()]}
        ticketId="ticket-1"
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: "Revert recording" }));
    const dialog = screen.getByRole("dialog", { name: "Revert recording?" });
    expect(dialog).toHaveTextContent(
      "This will reverse the inventory change created by this recording.",
    );
    expect(dialog).toHaveTextContent("The original record will remain in history.");
    expect(dialog).toHaveTextContent("+120 × Tritanium");
    expect(dialog).toHaveTextContent("Jita 4-4");

    await userEvent.click(within(dialog).getByRole("button", { name: "Revert recording" }));
    expect(revertMock).toHaveBeenCalledWith("ticket-1", "recording-1");
    expect(changed).toHaveBeenCalledTimes(1);
  });

  it("summarizes every production effect and supports a no-event recording", async () => {
    const production = recording({
      kind: "production",
      recordedQuantity: null,
      runsCompleted: 1,
      outputTypeId: 20185,
      outputQuantity: 36,
      effects: [
        { eventId: "in", kind: "consumption", typeId: 34, capturedName: "Tritanium", quantityDelta: -100, totalCostDelta: "-400.0000" },
        { eventId: "out", kind: "productionOutput", typeId: 20185, capturedName: "Auto-Integrity Preservation Seal", quantityDelta: 36, totalCostDelta: "500.0000" },
      ],
    });
    const { rerender } = render(
      <RecordingHistory onChanged={() => {}} recordings={[production]} ticketId="ticket-1" />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Revert recording" }));
    expect(screen.getByRole("dialog")).toHaveTextContent("This will reverse 2 inventory changes.");
    expect(screen.getByRole("dialog")).toHaveTextContent("-100 × Tritanium");
    expect(screen.getByRole("dialog")).toHaveTextContent("+36 × Auto-Integrity Preservation Seal");

    rerender(
      <RecordingHistory
        onChanged={() => {}}
        recordings={[production, recording({ id: "no-events", effects: [] })]}
        ticketId="ticket-1"
      />,
    );
    const buttons = screen.getAllByRole("button", { name: "Revert recording" });
    await userEvent.click(buttons[1]);
    expect(screen.getByRole("dialog")).toHaveTextContent("No inventory events were posted by this recording.");
  });
});
