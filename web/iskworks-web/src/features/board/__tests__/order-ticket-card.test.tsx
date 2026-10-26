import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { isBatchableOrderTicket, OrderTicketCard } from "../order-ticket-card";
import { makeDataTransfer, ticketFixture } from "./fixtures";

describe("OrderTicketCard", () => {
  it("shows matched job progress, and how many runs are started when only some are", () => {
    render(
      <OrderTicketCard
        jobProgress={{ percent: 40, label: "3h 12m", state: "running", jobCount: 2, jobRuns: 8 }}
        ticket={ticketFixture({
          kind: "reaction",
          status: "inProgress",
          recording: {
            state: "notRecorded",
            requestedQuantity: 10,
            recordedQuantity: 0,
            remainingQuantity: 10,
            surplusQuantity: 0,
          },
        })}
      />,
    );

    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "40");
    expect(screen.getByText("8/10 · 3h 12m")).toBeInTheDocument();
  });

  it("shows no job progress bar without a match or in the compact row", () => {
    const { rerender } = render(<OrderTicketCard ticket={ticketFixture({ kind: "reaction", status: "inProgress" })} />);
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
    rerender(
      <OrderTicketCard
        compact
        jobProgress={{ percent: 100, label: "Ready", state: "ready", jobCount: 1, jobRuns: 10 }}
        ticket={ticketFixture({ kind: "reaction", status: "inProgress" })}
      />,
    );
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
  });

  it("shows the ticket's identity, kind, status, and cost", () => {
    render(<OrderTicketCard ticket={ticketFixture()} />);

    expect(screen.getByText("ISK-2000")).toBeInTheDocument();
    expect(screen.getByText("Tritanium")).toBeInTheDocument();
    expect(screen.getByText("To Do")).toBeInTheDocument();
    expect(screen.getByText("BUY")).toBeInTheDocument();
  });

  it("shows a Blocked-by hint naming the fulfilling ticket when one exists", () => {
    render(
      <OrderTicketCard
        ticket={ticketFixture({
          kind: "manufacturing",
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
        })}
      />,
    );

    expect(screen.getByText("Blocked by ISK-2001")).toBeInTheDocument();
  });

  it("falls back to the material name when no fulfilling ticket exists yet", () => {
    render(
      <OrderTicketCard
        ticket={ticketFixture({
          kind: "manufacturing",
          status: "todo",
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
        })}
      />,
    );

    expect(screen.getByText("Blocked by Isogen")).toBeInTheDocument();
  });

  it("shows a +N more suffix when multiple prerequisites are unmet", () => {
    render(
      <OrderTicketCard
        ticket={ticketFixture({
          kind: "manufacturing",
          status: "todo",
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
            {
              prerequisiteId: "prereq-2",
              kind: "buy",
              typeId: 34,
              capturedName: "Tritanium",
              outstandingQuantity: 100,
              representativeFulfillingTicketId: null,
              representativeFulfillingTicketDisplayId: null,
              representativeFulfillingTicketStatus: null,
            },
          ],
        })}
      />,
    );

    expect(screen.getByText("Blocked by Isogen (+1 more)")).toBeInTheDocument();
  });

  it("does not show a Blocked-by hint when no prerequisite is unmet", () => {
    render(<OrderTicketCard ticket={ticketFixture()} />);

    expect(screen.queryByText(/Blocked by/)).not.toBeInTheDocument();
  });

  it("still shows the Blocked-by hint on a Complete ticket -- dependency state is independent of workflow status", () => {
    render(
      <OrderTicketCard
        ticket={ticketFixture({
          kind: "manufacturing",
          status: "complete",
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
        })}
      />,
    );

    expect(screen.getByText("Complete")).toBeInTheDocument();
    expect(screen.getByText("Blocked by Isogen")).toBeInTheDocument();
  });
});

describe("isBatchableOrderTicket", () => {
  it("is true only for a To Do, unbatched Acquisition ticket", () => {
    expect(isBatchableOrderTicket(ticketFixture())).toBe(true);
    expect(isBatchableOrderTicket(ticketFixture({ kind: "manufacturing", status: "todo" }))).toBe(false);
    expect(isBatchableOrderTicket(ticketFixture({ kind: "reaction", status: "todo" }))).toBe(false);
    expect(isBatchableOrderTicket(ticketFixture({ status: "inProgress" }))).toBe(false);
    expect(isBatchableOrderTicket(ticketFixture({ acquisitionRunId: "run-1" }))).toBe(false);
  });
});

describe("OrderTicketCard selection", () => {
  it("shows a checkbox and reports toggles only while selectable", () => {
    const onToggleSelect = vi.fn();
    render(<OrderTicketCard onToggleSelect={onToggleSelect} selectable ticket={ticketFixture()} />);

    const checkbox = screen.getByLabelText("Select ISK-2000");
    expect(checkbox).not.toBeChecked();
  });

  it("checks the box when selected is true", () => {
    render(<OrderTicketCard selectable selected ticket={ticketFixture()} />);

    expect(screen.getByLabelText("Select ISK-2000")).toBeChecked();
  });

  it("disables the checkbox for a Manufacturing/Reaction ticket by default -- never batchable", () => {
    render(<OrderTicketCard selectable ticket={ticketFixture({ kind: "manufacturing" })} />);

    expect(screen.getByLabelText("Select ISK-2000")).toBeDisabled();
  });

  it("follows the Board's eligibility when given -- a production step can be picked to combine", () => {
    const { rerender } = render(
      <OrderTicketCard selectable selectionEnabled ticket={ticketFixture({ kind: "reaction" })} />,
    );
    expect(screen.getByLabelText("Select ISK-2000")).toBeEnabled();

    rerender(<OrderTicketCard selectable selectionEnabled={false} ticket={ticketFixture()} />);
    expect(screen.getByLabelText("Select ISK-2000")).toBeDisabled();
  });

  it("toggling the checkbox does not also open the ticket (no drawer click-through)", async () => {
    const onToggleSelect = vi.fn();
    const onOpen = vi.fn();
    render(<OrderTicketCard onOpen={onOpen} onToggleSelect={onToggleSelect} selectable ticket={ticketFixture()} />);

    await userEvent.click(screen.getByLabelText("Select ISK-2000"));

    expect(onToggleSelect).toHaveBeenCalledWith("ticket-1");
    expect(onOpen).not.toHaveBeenCalled();
  });
});

describe("OrderTicketCard drag", () => {
  it("is draggable for a To Do, unbatched ticket", () => {
    render(<OrderTicketCard ticket={ticketFixture({ status: "todo" })} />);

    expect(screen.getByText("ISK-2000").closest('[draggable]')).toHaveAttribute("draggable", "true");
  });

  it("is draggable for an InProgress, unbatched ticket", () => {
    render(<OrderTicketCard ticket={ticketFixture({ status: "inProgress" })} />);

    expect(screen.getByText("ISK-2000").closest('[draggable]')).toHaveAttribute("draggable", "true");
  });

  it("is draggable for a ticket with unmet dependencies -- blockers never gate a lane move", () => {
    render(
      <OrderTicketCard
        ticket={ticketFixture({
          kind: "manufacturing",
          status: "todo",
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
        })}
      />,
    );

    expect(screen.getByText("ISK-2000").closest('[draggable]')).toHaveAttribute("draggable", "true");
  });

  it("is draggable for a Complete ticket -- moves back to a working lane are reversible", () => {
    render(<OrderTicketCard ticket={ticketFixture({ status: "complete" })} />);

    expect(screen.getByText("ISK-2000").closest('[draggable]')).toHaveAttribute("draggable", "true");
  });

  it("is not draggable for a ticket batched into an Acquisition Run, and explains why", () => {
    render(<OrderTicketCard ticket={ticketFixture({ status: "todo", acquisitionRunId: "run-1" })} />);

    const card = screen.getByText("ISK-2000").closest('[draggable]');
    expect(card).toHaveAttribute("draggable", "false");
    expect(card).toHaveAttribute(
      "title",
      "Batched into an Acquisition Run — start or complete it from the Run instead.",
    );
  });

  it("is not draggable while in selection mode", () => {
    render(<OrderTicketCard selectable ticket={ticketFixture({ status: "todo" })} />);

    expect(screen.getByText("ISK-2000").closest('[draggable]')).toHaveAttribute("draggable", "false");
  });

  it("reports drag start/end to the parent without opening the drawer", () => {
    const onDragStart = vi.fn();
    const onDragEnd = vi.fn();
    const onOpen = vi.fn();
    render(
      <OrderTicketCard
        onDragEnd={onDragEnd}
        onDragStart={onDragStart}
        onOpen={onOpen}
        ticket={ticketFixture({ status: "todo" })}
      />,
    );

    const card = screen.getByText("ISK-2000").closest('[draggable]') as HTMLElement;
    fireEvent.dragStart(card, { dataTransfer: makeDataTransfer() });
    fireEvent.dragEnd(card);

    expect(onDragStart).toHaveBeenCalledWith("ticket-1");
    expect(onDragEnd).toHaveBeenCalled();
    expect(onOpen).not.toHaveBeenCalled();
  });

  it("still opens the drawer on a plain click (drag doesn't swallow clicks)", async () => {
    const onOpen = vi.fn();
    render(<OrderTicketCard onOpen={onOpen} ticket={ticketFixture({ status: "todo" })} />);

    await userEvent.click(screen.getByText("ISK-2000"));

    expect(onOpen).toHaveBeenCalledWith("ticket-1");
  });
});

describe("OrderTicketCard recording indicator", () => {
  it("shows recorded-progress for a recordable ticket, secondary to its workflow status", () => {
    render(
      <OrderTicketCard
        ticket={ticketFixture({
          kind: "manufacturing",
          status: "inProgress",
          recording: {
            state: "partiallyRecorded",
            requestedQuantity: 100,
            recordedQuantity: 40,
            remainingQuantity: 60,
            surplusQuantity: 0,
          },
        })}
      />,
    );

    expect(screen.getByText("40 / 100")).toBeInTheDocument();
    expect(screen.getByText("— Partially recorded")).toBeInTheDocument();
    // The (screen-reader) status is still driven by workflow status alone.
    expect(screen.getByText("In Progress")).toBeInTheDocument();
  });

  it("omits the indicator when the ticket has no recording summary", () => {
    render(<OrderTicketCard ticket={ticketFixture({ recording: null })} />);

    expect(screen.queryByText(/recorded/)).not.toBeInTheDocument();
  });

  it("labels production progress in runs and acquisition progress in bare quantities", () => {
    const { rerender } = render(
      <OrderTicketCard
        ticket={ticketFixture({
          kind: "reaction",
          recording: {
            state: "recorded",
            requestedQuantity: 20,
            recordedQuantity: 20,
            remainingQuantity: 0,
            surplusQuantity: 0,
          },
        })}
      />,
    );
    expect(screen.getByText("20 / 20").parentElement).toHaveAttribute(
      "title",
      expect.stringContaining("runs"),
    );

    rerender(
      <OrderTicketCard
        ticket={ticketFixture({
          kind: "acquisition",
          recording: {
            state: "partiallyRecorded",
            requestedQuantity: 100,
            recordedQuantity: 30,
            remainingQuantity: 70,
            surplusQuantity: 0,
          },
        })}
      />,
    );
    expect(screen.getByText("30 / 100").parentElement).toHaveAttribute(
      "title",
      expect.not.stringContaining("runs"),
    );
  });

  it("compact row keeps identity and quantity but collapses recording to its glyph", () => {
    render(
      <OrderTicketCard
        compact
        ticket={ticketFixture({
          kind: "reaction",
          status: "complete",
          quantity: 5400,
          recording: {
            state: "recorded",
            requestedQuantity: 18,
            recordedQuantity: 18,
            remainingQuantity: 0,
            surplusQuantity: 0,
          },
        })}
      />,
    );

    expect(screen.getByText("ISK-2000")).toBeInTheDocument();
    expect(screen.getByText("×5,400")).toBeInTheDocument();
    expect(screen.queryByText("18 / 18")).not.toBeInTheDocument();
    expect(screen.getByText("●").parentElement).toHaveAttribute("title", expect.stringContaining("18 of 18 runs"));
  });

  it("masks a generic ticket's free-form title from session replay, but not a structured name", () => {
    const { rerender } = render(
      <OrderTicketCard ticket={ticketFixture({ kind: "generic", capturedName: "Move stuff to Amarr" })} />,
    );
    expect(screen.getByText("Move stuff to Amarr")).toHaveAttribute("data-private", "");

    rerender(<OrderTicketCard ticket={ticketFixture({ kind: "manufacturing", capturedName: "Rifter" })} />);
    expect(screen.getByText("Rifter")).not.toHaveAttribute("data-private");
  });
});
