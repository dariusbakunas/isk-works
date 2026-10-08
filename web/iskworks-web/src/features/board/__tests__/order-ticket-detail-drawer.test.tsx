import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { OrderTicketDetailDrawer } from "../order-ticket-detail-drawer";
import { ticketFixture } from "./fixtures";

const industryApi = vi.hoisted(() => ({
  updateTicketStatus: vi.fn(),
  recordTicketAcquisition: vi.fn(),
  recordTicketProduction: vi.fn(),
  updateTicketMetadata: vi.fn(),
  deleteTicket: vi.fn(),
}));

vi.mock("../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../api/industry")>("../../../api/industry");
  return { ...actual, ...industryApi };
});

describe("OrderTicketDetailDrawer", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("shows the ticket's identity, status, kind, and cost", () => {
    render(<OrderTicketDetailDrawer onClose={() => {}} ticket={ticketFixture()} />);

    expect(screen.getByText("ISK-2000")).toBeInTheDocument();
    expect(screen.getAllByText("Tritanium").length).toBeGreaterThan(0);
    // "To Do" appears both as the status badge and as the selected <option>.
    expect(screen.getAllByText("To Do").length).toBeGreaterThan(0);
    expect(screen.getByLabelText("Status")).toHaveValue("todo");
    expect(screen.getByText("BUY")).toBeInTheDocument();
  });

  it.each(["acquisition", "manufacturing", "reaction"] as const)(
    "renders captured %s ticket data without a source Build",
    (kind) => {
      render(
        <OrderTicketDetailDrawer
          onClose={() => {}}
          ticket={ticketFixture({
            kind,
            sourceBuildId: null,
            capturedName: `${kind} snapshot`,
          })}
        />,
      );

      expect(screen.getAllByText(`${kind} snapshot`).length).toBeGreaterThan(0);
      expect(screen.getByLabelText("Status")).toHaveValue("todo");
    },
  );

  it("Delete ticket confirms, calls deleteTicket, and closes", async () => {
    industryApi.deleteTicket.mockResolvedValue(undefined);
    const user = userEvent.setup();
    const onClose = vi.fn();
    const onChanged = vi.fn();
    render(
      <OrderTicketDetailDrawer
        onChanged={onChanged}
        onClose={onClose}
        ticket={ticketFixture({ acquisitionRunId: "run-1" })}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Delete ticket" }));
    const dialog = await screen.findByRole("dialog", { name: "Delete this ticket?" });
    expect(dialog).toHaveTextContent("This removes the ticket from the Board");
    expect(dialog).toHaveTextContent("Inventory already recorded from this ticket will be kept");
    expect(dialog).not.toHaveTextContent(/recordings.*removed/i);
    await user.click(within(dialog).getByRole("button", { name: "Delete ticket" }));

    await waitFor(() => expect(industryApi.deleteTicket).toHaveBeenCalledWith("ticket-1"));
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });

  it("renders the Required Materials table from prerequisites", () => {
    render(
      <OrderTicketDetailDrawer
        onClose={() => {}}
        ticket={ticketFixture({
          kind: "manufacturing",
          status: "todo",
          prerequisites: [
            {
              id: "prereq-1",
              ticketId: "ticket-1",
              typeId: 37164,
              capturedName: "Isogen",
              kind: "buy",
              sourceBuildId: null,
              requiredQuantity: 50,
              fulfillmentScope: "full",
              reusedQuantity: 0,
              freshQuantity: 50,
              estimatedUnitCost: "10.0000",
              estimatedLineTotal: "500.0000",
              reusedLineTotal: null,
            },
          ],
        })}
      />,
    );

    expect(screen.getByText("Required Materials")).toBeInTheDocument();
    expect(screen.getByText("Isogen")).toBeInTheDocument();
    // Required and fresh quantity columns both read 50 in this fixture.
    expect(screen.getAllByText("50")).toHaveLength(2);
  });

  it("SOURCE reflects frozen inventory coverage, not the bare sourcing kind", () => {
    render(
      <OrderTicketDetailDrawer
        onClose={() => {}}
        ticket={ticketFixture({
          kind: "manufacturing",
          status: "todo",
          prerequisites: [
            {
              // The reported case: a Buy-kind raw material entirely covered
              // by frozen Model-B inventory reuse (fresh 0). The user buys
              // nothing -- it must not read "BUY".
              id: "prereq-fc",
              ticketId: "ticket-1",
              typeId: 16672,
              capturedName: "Fernite Carbide",
              kind: "buy",
              sourceBuildId: null,
              requiredQuantity: 9853,
              fulfillmentScope: "missing",
              reusedQuantity: 9853,
              freshQuantity: 0,
              estimatedUnitCost: null,
              estimatedLineTotal: null,
              reusedLineTotal: null,
            },
            {
              // Partially covered: 40 from inventory, 60 still to acquire.
              id: "prereq-pc",
              ticketId: "ticket-1",
              typeId: 16680,
              capturedName: "Phenolic Composites",
              kind: "buy",
              sourceBuildId: null,
              requiredQuantity: 100,
              fulfillmentScope: "missing",
              reusedQuantity: 40,
              freshQuantity: 60,
              estimatedUnitCost: null,
              estimatedLineTotal: null,
              reusedLineTotal: null,
            },
          ],
        })}
      />,
    );

    const coveredRow = screen.getByText("Fernite Carbide").closest("div") as HTMLElement;
    expect(within(coveredRow).getByText("9,853")).toBeInTheDocument();
    expect(within(coveredRow).getByText("0")).toBeInTheDocument();
    expect(within(coveredRow).getByText("Use Inventory")).toBeInTheDocument();
    expect(within(coveredRow).queryByText("BUY")).not.toBeInTheDocument();

    const partialRow = screen.getByText("Phenolic Composites").closest("div") as HTMLElement;
    expect(within(partialRow).getByText("60")).toBeInTheDocument();
    expect(within(partialRow).getByText("Buy · Missing (60)")).toBeInTheDocument();

    // The only "BUY" on screen would be the ticket-kind badge; this ticket
    // is manufacturing, so "BUY" must not appear at all.
    expect(screen.queryByText("BUY")).not.toBeInTheDocument();
  });

  it("shows the Blocked-by list with outstanding quantity and fulfilling ticket, independent of workflow status", () => {
    render(
      <OrderTicketDetailDrawer
        onClose={() => {}}
        ticket={ticketFixture({
          kind: "manufacturing",
          // An in-progress ticket that still has an unmet prerequisite --
          // both the workflow status and the derived blocker render.
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
        })}
      />,
    );

    expect(screen.getByText("Blocked by")).toBeInTheDocument();
    expect(screen.getByText("Isogen")).toBeInTheDocument();
    expect(screen.getByText("— ISK-2001")).toBeInTheDocument();
    // The workflow control is unaffected -- still shows the user-chosen lane.
    expect(screen.getByLabelText("Status")).toHaveValue("inProgress");
  });

  it("still shows the Blocked-by section on a Complete ticket", () => {
    render(
      <OrderTicketDetailDrawer
        onClose={() => {}}
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

    expect(screen.getByLabelText("Status")).toHaveValue("complete");
    expect(screen.getByText("Blocked by")).toBeInTheDocument();
    expect(screen.getByText("Isogen")).toBeInTheDocument();
    expect(screen.queryByText(/conflict|out of sync|inconsisten/i)).not.toBeInTheDocument();
  });

  it("does not show the Blocked-by section when no prerequisite is unmet", () => {
    render(<OrderTicketDetailDrawer onClose={() => {}} ticket={ticketFixture()} />);

    expect(screen.queryByText("Blocked by")).not.toBeInTheDocument();
  });
});

describe("OrderTicketDetailDrawer Epic context and editing", () => {
  beforeEach(() => vi.clearAllMocks());

  const orders = [
    { id: "order-1", displayName: "Weekend Ishtar Production" } as never,
    { id: "order-2", displayName: "Weekday Rifter Run" } as never,
  ];

  it("shows the Epic dropdown with the current Epic selected, and Open switches to it", async () => {
    const onOpenEpic = vi.fn();
    render(
      <OrderTicketDetailDrawer
        epicName="Weekend Ishtar Production"
        onClose={() => {}}
        onOpenEpic={onOpenEpic}
        orders={orders}
        ticket={ticketFixture({ orderId: "order-1" })}
      />,
    );

    expect(screen.getByLabelText("Epic")).toHaveValue("order-1");
    await userEvent.click(screen.getByRole("button", { name: "Open" }));

    expect(onOpenEpic).toHaveBeenCalled();
  });

  it("omits the Epic control entirely when no Epic list is supplied", () => {
    render(<OrderTicketDetailDrawer onClose={() => {}} ticket={ticketFixture({ orderId: "order-1" })} />);

    expect(screen.queryByLabelText("Epic")).not.toBeInTheDocument();
  });

  it("changing the Epic dropdown patches metadata only -- no Open button for a standalone ticket", async () => {
    industryApi.updateTicketMetadata.mockResolvedValue({});
    const onChanged = vi.fn();
    render(
      <OrderTicketDetailDrawer
        onChanged={onChanged}
        onClose={() => {}}
        orders={orders}
        ticket={ticketFixture({ orderId: null })}
      />,
    );

    expect(screen.getByLabelText("Epic")).toHaveValue("");
    expect(screen.queryByRole("button", { name: "Open" })).not.toBeInTheDocument();

    await userEvent.selectOptions(screen.getByLabelText("Epic"), "order-2");

    await waitFor(() =>
      expect(industryApi.updateTicketMetadata).toHaveBeenCalledWith("ticket-1", { orderId: "order-2" }),
    );
    expect(onChanged).toHaveBeenCalled();
  });

  it("clearing the Epic dropdown sends an explicit null, not an omitted field", async () => {
    industryApi.updateTicketMetadata.mockResolvedValue({});
    render(
      <OrderTicketDetailDrawer onClose={() => {}} orders={orders} ticket={ticketFixture({ orderId: "order-1" })} />,
    );

    await userEvent.selectOptions(screen.getByLabelText("Epic"), "");

    await waitFor(() =>
      expect(industryApi.updateTicketMetadata).toHaveBeenCalledWith("ticket-1", { orderId: null }),
    );
  });

  it("shows the Assignee dropdown and reassigning patches metadata only", async () => {
    industryApi.updateTicketMetadata.mockResolvedValue({});
    const characters = [
      { connectionId: "char-1", characterName: "Alt One" } as never,
      { connectionId: "char-2", characterName: "Alt Two" } as never,
    ];
    render(
      <OrderTicketDetailDrawer
        characters={characters}
        onClose={() => {}}
        ticket={ticketFixture({ assigneeCharacterId: "char-1" })}
      />,
    );

    expect(screen.getByLabelText("Assignee")).toHaveValue("char-1");

    await userEvent.selectOptions(screen.getByLabelText("Assignee"), "char-2");

    await waitFor(() =>
      expect(industryApi.updateTicketMetadata).toHaveBeenCalledWith("ticket-1", {
        assigneeCharacterId: "char-2",
      }),
    );
  });

  it("re-seeds Title and Notes drafts when the selected ticket identity changes", async () => {
    const ticketA = ticketFixture({
      id: "ticket-A",
      capturedName: "Alpha widget",
      notes: "Alpha notes",
    });
    const ticketB = ticketFixture({
      id: "ticket-B",
      capturedName: "Bravo widget",
      notes: "Bravo notes",
    });

    const { rerender } = render(<OrderTicketDetailDrawer onClose={() => {}} ticket={ticketA} />);

    expect(screen.getByLabelText("Title")).toHaveValue("Alpha widget");
    expect(screen.getByLabelText("Notes")).toHaveValue("Alpha notes");

    // The Board reuses this same mounted component and only swaps the prop.
    rerender(<OrderTicketDetailDrawer onClose={() => {}} ticket={ticketB} />);

    expect(screen.getByLabelText("Title")).toHaveValue("Bravo widget");
    expect(screen.getByLabelText("Notes")).toHaveValue("Bravo notes");

    // ...and back again shows A's persisted values.
    rerender(<OrderTicketDetailDrawer onClose={() => {}} ticket={ticketA} />);

    expect(screen.getByLabelText("Title")).toHaveValue("Alpha widget");
    expect(screen.getByLabelText("Notes")).toHaveValue("Alpha notes");
  });

  it("clears the Notes textarea when switching from a ticket with notes to one with empty notes", () => {
    const withNotes = ticketFixture({ id: "ticket-A", notes: "Remember the reaction inputs" });
    const withoutNotes = ticketFixture({ id: "ticket-B", notes: "" });

    const { rerender } = render(<OrderTicketDetailDrawer onClose={() => {}} ticket={withNotes} />);
    expect(screen.getByLabelText("Notes")).toHaveValue("Remember the reaction inputs");

    rerender(<OrderTicketDetailDrawer onClose={() => {}} ticket={withoutNotes} />);
    expect(screen.getByLabelText("Notes")).toHaveValue("");
  });

  it("keeps an unsaved local draft across a same-ticket refetch (new object, same id)", async () => {
    const { rerender } = render(
      <OrderTicketDetailDrawer onClose={() => {}} ticket={ticketFixture({ id: "ticket-A", notes: "" })} />,
    );

    const notes = screen.getByLabelText("Notes");
    await userEvent.type(notes, "half-typed thought");
    expect(notes).toHaveValue("half-typed thought");

    // A background refetch hands us a brand-new object for the same ticket
    // (e.g. its `updatedAt` moved). Re-rendering with an equal-id prop must
    // not wipe the in-progress edit.
    rerender(
      <OrderTicketDetailDrawer
        onClose={() => {}}
        ticket={ticketFixture({ id: "ticket-A", notes: "", updatedAt: "2026-09-07T00:00:00Z" })}
      />,
    );

    expect(screen.getByLabelText("Notes")).toHaveValue("half-typed thought");
  });

  it("completion of ticket A's pending save cannot overwrite ticket B's local drafts", async () => {
    let resolveSave: (value: unknown) => void = () => {};
    industryApi.updateTicketMetadata.mockImplementation(
      () => new Promise((resolve) => (resolveSave = resolve)),
    );

    const ticketA = ticketFixture({ id: "ticket-A", capturedName: "Alpha", notes: "Alpha notes" });
    const ticketB = ticketFixture({ id: "ticket-B", capturedName: "Bravo", notes: "Bravo notes" });

    const { rerender } = render(<OrderTicketDetailDrawer onClose={() => {}} ticket={ticketA} />);

    // Edit A's notes and blur -- fires an in-flight save that will not
    // resolve until we let it.
    const notesA = screen.getByLabelText("Notes");
    await userEvent.clear(notesA);
    await userEvent.type(notesA, "Alpha edited");
    notesA.blur();
    await waitFor(() => expect(industryApi.updateTicketMetadata).toHaveBeenCalledWith("ticket-A", { notes: "Alpha edited" }));

    // User immediately selects Ticket B.
    rerender(<OrderTicketDetailDrawer onClose={() => {}} ticket={ticketB} />);
    expect(screen.getByLabelText("Notes")).toHaveValue("Bravo notes");
    expect(screen.getByLabelText("Title")).toHaveValue("Bravo");

    // A's save now completes -- it must not apply A's values to B.
    resolveSave({});
    await waitFor(() => expect(screen.getByLabelText("Notes")).toHaveValue("Bravo notes"));
    expect(screen.getByLabelText("Title")).toHaveValue("Bravo");
  });

  it("edits title and notes, committing on blur", async () => {
    industryApi.updateTicketMetadata.mockResolvedValue({});
    render(<OrderTicketDetailDrawer onClose={() => {}} ticket={ticketFixture()} />);

    const title = screen.getByLabelText("Title");
    await userEvent.clear(title);
    await userEvent.type(title, "Renamed ticket");
    title.blur();

    await waitFor(() =>
      expect(industryApi.updateTicketMetadata).toHaveBeenCalledWith("ticket-1", {
        capturedName: "Renamed ticket",
      }),
    );

    const notes = screen.getByLabelText("Notes");
    await userEvent.type(notes, "Handle with care");
    notes.blur();

    await waitFor(() =>
      expect(industryApi.updateTicketMetadata).toHaveBeenCalledWith("ticket-1", {
        notes: "Handle with care",
      }),
    );
  });
});

describe("OrderTicketDetailDrawer workflow status control", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("shows the current lane and moves the ticket forward via updateTicketStatus + onChanged", async () => {
    industryApi.updateTicketStatus.mockResolvedValue(ticketFixture({ status: "inProgress" }));
    const onChanged = vi.fn();
    render(<OrderTicketDetailDrawer onChanged={onChanged} onClose={() => {}} ticket={ticketFixture({ status: "todo" })} />);

    const select = screen.getByLabelText("Status");
    expect(select).toHaveValue("todo");
    await userEvent.selectOptions(select, "inProgress");

    expect(industryApi.updateTicketStatus).toHaveBeenCalledWith("ticket-1", "inProgress");
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
  });

  it("moves the ticket backward just as freely -- Complete -> To Do", async () => {
    industryApi.updateTicketStatus.mockResolvedValue(ticketFixture({ status: "todo" }));
    render(<OrderTicketDetailDrawer onClose={() => {}} ticket={ticketFixture({ status: "complete" })} />);

    await userEvent.selectOptions(screen.getByLabelText("Status"), "todo");

    expect(industryApi.updateTicketStatus).toHaveBeenCalledWith("ticket-1", "todo");
  });

  it("can move the ticket straight to Canceled and back", async () => {
    industryApi.updateTicketStatus.mockResolvedValue(ticketFixture({ status: "canceled" }));
    const { rerender } = render(
      <OrderTicketDetailDrawer onClose={() => {}} ticket={ticketFixture({ status: "todo" })} />,
    );

    await userEvent.selectOptions(screen.getByLabelText("Status"), "canceled");
    expect(industryApi.updateTicketStatus).toHaveBeenCalledWith("ticket-1", "canceled");

    industryApi.updateTicketStatus.mockResolvedValue(ticketFixture({ status: "inProgress" }));
    rerender(<OrderTicketDetailDrawer onClose={() => {}} ticket={ticketFixture({ status: "canceled" })} />);
    await userEvent.selectOptions(screen.getByLabelText("Status"), "inProgress");
    expect(industryApi.updateTicketStatus).toHaveBeenLastCalledWith("ticket-1", "inProgress");
  });

  it("changing the lane uses only the workflow endpoint -- never recording", async () => {
    industryApi.updateTicketStatus.mockResolvedValue(ticketFixture({ status: "complete" }));
    render(<OrderTicketDetailDrawer onClose={() => {}} ticket={ticketFixture({ status: "inProgress" })} />);

    await userEvent.selectOptions(screen.getByLabelText("Status"), "complete");

    await waitFor(() => expect(industryApi.updateTicketStatus).toHaveBeenCalledWith("ticket-1", "complete"));
    expect(industryApi.recordTicketAcquisition).not.toHaveBeenCalled();
    expect(industryApi.recordTicketProduction).not.toHaveBeenCalled();
  });

  it("is not gated by unmet dependencies -- a blocked ticket can be moved to Complete", async () => {
    industryApi.updateTicketStatus.mockResolvedValue(ticketFixture({ status: "complete" }));
    render(
      <OrderTicketDetailDrawer
        onClose={() => {}}
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

    const select = screen.getByLabelText("Status");
    expect(select).not.toBeDisabled();
    await userEvent.selectOptions(select, "complete");

    expect(industryApi.updateTicketStatus).toHaveBeenCalledWith("ticket-1", "complete");
  });

  it("surfaces the error when the status write fails", async () => {
    industryApi.updateTicketStatus.mockRejectedValue(new Error("ticket was archived"));
    render(<OrderTicketDetailDrawer onClose={() => {}} ticket={ticketFixture({ status: "todo" })} />);

    await userEvent.selectOptions(screen.getByLabelText("Status"), "complete");

    expect(await screen.findByText("ticket was archived")).toBeInTheDocument();
  });

  it("hides the status control, with an explanatory note, for a ticket batched into an Acquisition Run", () => {
    render(
      <OrderTicketDetailDrawer
        onClose={() => {}}
        ticket={ticketFixture({ status: "todo", acquisitionRunId: "run-1" })}
      />,
    );

    expect(screen.queryByLabelText("Status")).not.toBeInTheDocument();
    expect(screen.getByText(/Batched into an Acquisition Run/)).toBeInTheDocument();
  });
});

describe("OrderTicketDetailDrawer recording section", () => {
  beforeEach(() => vi.clearAllMocks());

  const recordingFixture = {
    state: "partiallyRecorded" as const,
    requestedQuantity: 100,
    recordedQuantity: 40,
    remainingQuantity: 60,
    surplusQuantity: 0,
  };

  it("renders the RECORDING section from the backend summary, separate from the workflow actions", () => {
    render(
      <OrderTicketDetailDrawer
        onClose={() => {}}
        ticket={ticketFixture({ status: "inProgress", recording: recordingFixture })}
      />,
    );

    const section = screen.getByRole("region", { name: "Recording" });
    expect(within(section).getByText("Partially recorded")).toBeInTheDocument();
    expect(within(section).getByRole("button", { name: "Record acquisition" })).toBeInTheDocument();
  });

  it("does not render a RECORDING section when the ticket carries no summary", () => {
    render(<OrderTicketDetailDrawer onClose={() => {}} ticket={ticketFixture({ recording: null })} />);
    expect(screen.queryByRole("region", { name: "Recording" })).not.toBeInTheDocument();
  });

  it("records an acquisition and calls onChanged without any workflow mutation", async () => {
    industryApi.recordTicketAcquisition.mockResolvedValue({});
    const onChanged = vi.fn();
    render(
      <OrderTicketDetailDrawer
        onChanged={onChanged}
        onClose={() => {}}
        ticket={ticketFixture({ status: "complete", recording: recordingFixture })}
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: "Record acquisition" }));
    await userEvent.click(screen.getByRole("button", { name: "Record" }));

    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    expect(industryApi.recordTicketAcquisition).toHaveBeenCalledWith(
      "ticket-1",
      expect.objectContaining({ quantity: 60 }),
    );
    expect(industryApi.updateTicketStatus).not.toHaveBeenCalled();
  });

  // Recording state and workflow status are independent dimensions -- every
  // combination renders normally, with no warning that one is "out of sync"
  // with the other (see RecordingSection's own doc comment).
  it("Complete + Not recorded renders normally, no warning", () => {
    render(
      <OrderTicketDetailDrawer
        onClose={() => {}}
        ticket={ticketFixture({
          status: "complete",
          recording: { state: "notRecorded", requestedQuantity: 100, recordedQuantity: 0, remainingQuantity: 100, surplusQuantity: 0 },
        })}
      />,
    );

    const section = screen.getByRole("region", { name: "Recording" });
    expect(within(section).getByText("Not recorded")).toBeInTheDocument();
    expect(screen.queryByText(/out of sync|incomplete|warning/i)).not.toBeInTheDocument();
  });

  it("InProgress + Recorded renders normally, no warning", () => {
    render(
      <OrderTicketDetailDrawer
        onClose={() => {}}
        ticket={ticketFixture({
          status: "inProgress",
          recording: { state: "recorded", requestedQuantity: 100, recordedQuantity: 100, remainingQuantity: 0, surplusQuantity: 0 },
        })}
      />,
    );

    screen.getByRole("region", { name: "Recording" });
    expect(within(screen.getByTestId("recording-state")).getByText("Recorded")).toBeInTheDocument();
    expect(screen.queryByText(/out of sync|incomplete|warning/i)).not.toBeInTheDocument();
  });

  it("keeps the Recorded indicator after the ticket is Canceled -- recording is never reverted by cancellation", () => {
    render(
      <OrderTicketDetailDrawer
        onClose={() => {}}
        ticket={ticketFixture({
          status: "canceled",
          recording: { state: "recorded", requestedQuantity: 100, recordedQuantity: 100, remainingQuantity: 0, surplusQuantity: 0 },
        })}
      />,
    );

    screen.getByRole("region", { name: "Recording" });
    expect(within(screen.getByTestId("recording-state")).getByText("Recorded")).toBeInTheDocument();
  });
});
