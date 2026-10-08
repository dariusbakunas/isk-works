import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { TicketRecordingSummary, TicketSummary } from "../../../../api/industry";
import { RecordingSection } from "../recording-section";

// The recording forms make network calls; this suite only exercises the
// outer section, so stub the API module wholesale.
vi.mock("../../../../api/industry", async () => {
  const actual =
    await vi.importActual<typeof import("../../../../api/industry")>("../../../../api/industry");
  return { ...actual, recordTicketAcquisition: vi.fn(), recordTicketProduction: vi.fn() };
});

function summary(overrides: Partial<TicketRecordingSummary> = {}): TicketRecordingSummary {
  return {
    state: "notRecorded",
    requestedQuantity: 100,
    recordedQuantity: 0,
    remainingQuantity: 100,
    surplusQuantity: 0,
    ...overrides,
  };
}

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
    recording: summary(),
    ...overrides,
  };
}

function stateChipText(): string {
  return screen.getByTestId("recording-state").textContent ?? "";
}

describe("RecordingSection — state presentation", () => {
  it("renders the Not recorded state with the requested/recorded/remaining rollup and no surplus row", () => {
    render(<RecordingSection onRecorded={() => {}} ticket={ticket()} />);

    expect(stateChipText()).toContain("Not recorded");
    expect(screen.getAllByRole("term").map((dt) => dt.textContent)).toEqual([
      "Requested",
      "Recorded",
      "Remaining",
    ]);
    expect(screen.queryByText("Surplus")).not.toBeInTheDocument();
  });

  it("renders Partially recorded with requested / recorded / remaining values", () => {
    render(
      <RecordingSection
        onRecorded={() => {}}
        ticket={ticket({
          recording: summary({
            state: "partiallyRecorded",
            recordedQuantity: 40,
            remainingQuantity: 60,
          }),
        })}
      />,
    );

    expect(stateChipText()).toContain("Partially recorded");
    const values = screen.getAllByRole("definition").map((dd) => dd.textContent);
    expect(values).toEqual(["100", "40", "60"]);
  });

  it("renders the Surplus row only when surplus > 0", () => {
    render(
      <RecordingSection
        onRecorded={() => {}}
        ticket={ticket({
          recording: summary({
            state: "recorded",
            recordedQuantity: 105,
            remainingQuantity: 0,
            surplusQuantity: 5,
          }),
        })}
      />,
    );

    expect(stateChipText()).toContain("Recorded");
    expect(screen.getByText("Surplus")).toBeInTheDocument();
    const values = screen.getAllByRole("definition").map((dd) => dd.textContent);
    expect(values).toEqual(["100", "105", "0", "5"]);
  });

  it("shows no contradiction when workflow is Complete but recording is Not recorded", () => {
    render(
      <RecordingSection
        onRecorded={() => {}}
        ticket={ticket({ status: "complete", recording: summary({ state: "notRecorded" }) })}
      />,
    );

    expect(stateChipText()).toContain("Not recorded");
    expect(screen.queryByText(/warning/i)).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("renders normally when workflow is Ready but recording is Recorded", () => {
    render(
      <RecordingSection
        onRecorded={() => {}}
        ticket={ticket({
          status: "todo",
          recording: summary({ state: "recorded", recordedQuantity: 100, remainingQuantity: 0 }),
        })}
      />,
    );

    expect(stateChipText()).toContain("Recorded");
    expect(screen.getByRole("button", { name: "Record acquisition" })).toBeInTheDocument();
  });

  it("returns nothing when the ticket has no recording summary", () => {
    const { container } = render(
      <RecordingSection onRecorded={() => {}} ticket={ticket({ recording: null })} />,
    );
    expect(container).toBeEmptyDOMElement();
  });
});

describe("RecordingSection — action wiring", () => {
  it("uses run terminology and a Record production action for a manufacturing ticket", () => {
    render(
      <RecordingSection
        onRecorded={() => {}}
        ticket={ticket({
          kind: "manufacturing",
          recording: summary({
            state: "partiallyRecorded",
            recordedQuantity: 40,
            remainingQuantity: 60,
          }),
        })}
      />,
    );

    expect(screen.getByText("Planned runs")).toBeInTheDocument();
    expect(screen.getByText("Recorded runs")).toBeInTheDocument();
    expect(screen.getByText("Remaining runs")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Record production" })).toBeInTheDocument();
  });

  it("opens the inline editor when the action is clicked", async () => {
    render(<RecordingSection onRecorded={() => {}} ticket={ticket()} />);

    await userEvent.click(screen.getByRole("button", { name: "Record acquisition" }));

    expect(screen.getByRole("form", { name: "Record acquisition" })).toBeInTheDocument();
  });

  it("hides the action for an Acquisition ticket batched into a Run", () => {
    render(<RecordingSection onRecorded={() => {}} ticket={ticket({ acquisitionRunId: "run-1" })} />);

    expect(screen.queryByRole("button", { name: "Record acquisition" })).not.toBeInTheDocument();
    expect(within(screen.getByRole("region", { name: "Recording" })).getByText(/Recorded through its Acquisition Run/)).toBeInTheDocument();
  });
});
