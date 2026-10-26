import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { ApiError } from "../../../api/workspace";
import { ticketFixture } from "./fixtures";

const industryApi = vi.hoisted(() => ({ combineTickets: vi.fn() }));

vi.mock("../../../api/industry", async () => {
  const actual = await vi.importActual<typeof import("../../../api/industry")>("../../../api/industry");
  return { ...actual, ...industryApi };
});

import { CombineTicketsDialog } from "../combine-tickets-dialog";

function step(id: string, epic: string, quantity: number, runs: number, assignee: string | null = "char-1") {
  return ticketFixture({
    id,
    displayId: `ISK-${id}`,
    kind: "reaction",
    typeId: 16658,
    capturedName: "Hexite",
    quantity,
    orderId: epic,
    assigneeCharacterId: assignee,
    estimatedLineTotal: `${quantity}.0000`,
    executionSnapshot: {
      runs,
      blueprint: null,
      facility: null,
      durationSeconds: null,
      installationCost: null,
      materialValue: null,
    },
    operations: [{ orderId: epic, occurrenceKey: "build:hexite" }],
  });
}

const epicNameById = new Map([
  ["epic-1", "Prowler ×2"],
  ["epic-2", "Wreathe ×2"],
]);
const characterNameById = new Map([
  ["char-1", "Xuesen"],
  ["char-2", "Alt"],
]);

describe("CombineTicketsDialog", () => {
  beforeEach(() => {
    industryApi.combineTickets.mockReset();
  });

  it("summarizes the members and combines them in selection order", async () => {
    const tickets = [step("1205", "epic-1", 400, 2), step("1214", "epic-2", 1200, 6)];
    const combined = step("1300", "epic-1", 1600, 8);
    industryApi.combineTickets.mockResolvedValue(combined);
    const onCombined = vi.fn();
    render(
      <CombineTicketsDialog
        characterNameById={characterNameById}
        epicNameById={epicNameById}
        onCancel={vi.fn()}
        onCombined={onCombined}
        open
        tickets={tickets}
      />,
    );

    const dialog = screen.getByRole("dialog", { name: "Combine 2 tickets" });
    expect(within(dialog).getByText("Hexite")).toBeInTheDocument();
    expect(within(dialog).getByText("×1,600")).toBeInTheDocument();
    expect(within(dialog).getByText("8 runs")).toBeInTheDocument();
    expect(within(dialog).getByText("2 Epics")).toBeInTheDocument();
    expect(within(dialog).getByText("Xuesen")).toBeInTheDocument();
    expect(within(dialog).getByText("Prowler ×2")).toBeInTheDocument();
    expect(within(dialog).getByText("Wreathe ×2")).toBeInTheDocument();

    await userEvent.click(within(dialog).getByRole("button", { name: "Combine" }));

    expect(industryApi.combineTickets).toHaveBeenCalledWith(["1205", "1214"]);
    expect(onCombined).toHaveBeenCalledWith(combined);
  });

  it("says the assignee comes from the first ticket when they differ", () => {
    render(
      <CombineTicketsDialog
        characterNameById={characterNameById}
        epicNameById={epicNameById}
        onCancel={vi.fn()}
        onCombined={vi.fn()}
        open
        tickets={[step("1", "epic-1", 400, 2, null), step("2", "epic-2", 400, 2, "char-2")]}
      />,
    );

    expect(screen.getByText("Unassigned")).toBeInTheDocument();
    expect(screen.getByText(/from ISK-1, the first selected/)).toBeInTheDocument();
  });

  it("shows the server's refusal and stays open", async () => {
    industryApi.combineTickets.mockRejectedValue(
      new ApiError(409, {
        code: "ticket_not_combinable",
        message: "ISK-2 can't be combined: only To Do tickets can be combined",
      }),
    );
    const onCombined = vi.fn();
    render(
      <CombineTicketsDialog
        characterNameById={characterNameById}
        epicNameById={epicNameById}
        onCancel={vi.fn()}
        onCombined={onCombined}
        open
        tickets={[step("1", "epic-1", 400, 2), step("2", "epic-2", 400, 2)]}
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: "Combine" }));

    expect(await screen.findByText(/only To Do tickets can be combined/)).toBeInTheDocument();
    expect(onCombined).not.toHaveBeenCalled();
  });
});
